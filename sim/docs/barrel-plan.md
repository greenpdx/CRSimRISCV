# Barrel Processor Simulator (B1–B7)

An 8-thread RV32I barrel processor with a 6-stage pipeline, two-class
instruction memory hierarchy, shared L1 caches, store buffer, and
configurable scheduler. Built as a separate simulator alongside the
existing single-cycle and 5-stage Rust simulators in this crate; the
existing pipelines were never modified.

## Architecture summary

- **8 hardware thread contexts**, each with its own PC, register file,
  scheduler state, and per-thread CSRs.
- **6-stage pipeline**: `LT → IF → DI → EX → MA → WB`. The new **LT**
  (Load Thread) stage runs the scheduler and selects the next thread
  to dispatch.
- **No forwarding / no hazards required when ≥ 2 threads are active**:
  the same-thread distance in the pipeline is at least N=8 cycles >
  pipeline depth 6, so RAW dependencies are naturally resolved by
  the time the consumer reaches DI. Forwarding paths (EX/MA → EX
  and MA/WB → EX) and load-use stall logic are present for the
  single-thread degenerate fallback.
- **Branch resolution per-thread**: a taken branch in EX overrides
  only its own thread's PC. No squash of in-flight instructions
  because no younger same-thread instruction can be in the pipeline.
- **Two-class instruction memory hierarchy**:
  - Slots 0/1/2 have a private **16 KiB I-SRAM** mapped at
    `0xF000_0000 + slot*0x4000`. Single-cycle, never misses, only
    fetchable by the owning slot.
  - Slots 0..7 share a 4 KiB direct-mapped **I-L1** for any external
    instruction fetches.
  - Slots accessing a foreign I-SRAM range raise an
    `instr_access_fault` and are deactivated.
- **Shared 4 KiB direct-mapped D-L1** for all data accesses.
- **Store buffer**: 4-deep FIFO, write-through, drained by the bus.
  Store-to-load forwarding from the buffer when the load address
  matches a pending store.
- **Single shared external memory bus**, fixed-priority arbiter
  (D-L1 fill > I-L1 fill > store drain), 8-cycle line fill latency.
- **Custom-0 ISA extension** at opcode `0x0b`: FORK / JOIN / DONE /
  YIELD / TID / FORKRET — hardware thread management with 1-cycle
  FORK and 0-cycle JOIN.
- **Configurable scheduler**: RoundRobin, Priority, AgedPriority,
  TwoClass.
- **IRQ controller (B7)**: `inject_irq(slot)` activates a slot;
  hardware interrupt = wake the pre-loaded handler thread.

## File layout

```
sim/src/sim/barrel/
  mod.rs          — BarrelState::step(), bus_tick(), reset(),
                    plus the load-use stall logic
  state.rs        — Thread struct, BarrelState, all pipeline regs,
                    address-space helpers, prewarm_icache(),
                    inject_irq(), per-thread CSR R/W
  scheduler.rs    — SchedMode enum + 4 implementations
  cache.rs        — direct-mapped Cache (I-L1 / D-L1)
  store_buffer.rs — write-through store FIFO
  bus.rs          — single-bus arbiter state machine
  stage_lt.rs     — Load Thread (calls scheduler)
  stage_if.rs     — Instruction Fetch (3-way: ISRAM / I-L1 / fault)
  stage_di.rs     — Decode + per-thread regfile read + custom-0
  stage_ex.rs     — Execute + ALU + branch + same-thread forwarding
  stage_ma.rs     — Memory Access + D-L1 + store buffer + custom-0
  stage_wb.rs     — Write Back to per-thread regfile
```

The 5-stage pipeline at `sim/src/sim/pipeline/` was not modified.
The two simulators coexist; choose between them in the wasm API
by instantiating `JsPipelineState` vs `JsBarrelState`.

## Phasing

- **B1** — skeleton: 6 stages, 8 threads, round-robin, no caches,
  no custom-0. 6 tests.
- **B2** — custom-0 instructions (FORK/JOIN/DONE/YIELD/TID/FORKRET).
  7 tests.
- **B3** — local 16 KiB I-SRAMs for slots 0/1/2 + access faults.
  7 tests.
- **B4** — I-L1 + D-L1 caches + store buffer + bus arbiter.
  7 tests.
- **B5** — Priority / AgedPriority / TwoClass scheduler modes +
  EX/MA → EX and MA/WB → EX forwarding + load-use stall for
  single-thread fallback. 7 tests.
- **B6** — wasm `JsBarrelState` + Vue `MicroStepBarrelView` with
  thread cards, stage cards, scheduler timeline, scheduler mode
  dropdown, cache stats.
- **B7** — per-thread privileged state (mstatus, mtvec, mepc,
  mcause, mtval, mscratch, mip, mie, satp), `inject_irq(slot)`,
  click-to-inject IRQ button per thread card, per-thread CSR
  display strip in the visualizer. 4 tests.

## Test count

| Suite             | Tests |
|-------------------|------:|
| barrel_smoke (B1) |     6 |
| barrel_custom0 (B2)|    7 |
| barrel_isram (B3) |     7 |
| barrel_caches (B4)|     7 |
| barrel_b5 (B5)    |     7 |
| barrel_b7 (B7)    |     4 |
| **Barrel total**  |  **38** |

## Address map

| Range                       | Purpose                   |
|-----------------------------|---------------------------|
| `0xF000_0000..0xF000_3FFF`  | Slot 0 local I-SRAM       |
| `0xF000_4000..0xF000_7FFF`  | Slot 1 local I-SRAM       |
| `0xF000_8000..0xF000_BFFF`  | Slot 2 local I-SRAM       |
| `0x0000_0000..`             | External instruction RAM  |
|                             | (ext_imem, cached by I-L1)|
| `0x1000_0000..`             | External data RAM         |
|                             | (ext_dmem, cached by D-L1)|

## Custom-0 instructions

R-type encoding, opcode `0x0b`. Distinguished by `funct7`:

| funct7 | Mnemonic         | Operands           | Action |
|-------:|------------------|--------------------|--------|
|  0x00  | `FORK rd,rs1,rs2`| rs1=start_pc, rs2=arg, rd←slot | spawn thread, return slot or -1 |
|  0x01  | `JOIN rs1`       | rs1=TID            | block until target done |
|  0x02  | `DONE`           | —                  | wake joiners, deactivate self |
|  0x03  | `YIELD`          | —                  | skip one scheduler round |
|  0x04  | `TID rd`         | —                  | rd ← current TID |
|  0x05  | `FORKRET rd`     | rd is *source*     | DONE + pass rd as return value |

## Scheduler modes

- **RoundRobin** (B1): pick `(current+1) mod N`, skipping stalled.
- **Priority** (B5): strict, RR within priority class.
- **AgedPriority** (B5): `priority + min(age, 8)`, anti-starvation.
- **TwoClass** (B5): pinned (slots 0/1/2) > general (3..7), RR within.

## Known limitations

1. **Single-thread + cache miss + load-use is broken**. The PC
   rollback for the cache miss compounds with the rollback for
   the load-use stall. Multi-thread mode is unaffected. Pre-install
   the d-cache line in tests if you need single-thread cache hits.
2. **Sync trap delivery (ECALL/EBREAK/illegal)** is not implemented
   for the barrel CPU. Per-thread CSR storage is in place but the
   trap path is not wired. The barrel design's IRQ-via-thread-wake
   model means async interrupts don't need a trap path.
3. **`SchedMode::Predict`** is reserved but not implemented; the
   barrel design doesn't use branch prediction (no flushes).
4. **Per-thread Sv32 translation** is reserved; `satp` is per-thread
   storage but the walker is not yet routed through it.
5. **No store-buffer-full back-pressure on stores**: a store that
   can't push to the buffer rolls back and re-issues, wasting
   dispatch cycles until the buffer drains. With 4 entries and
   1-cycle drains, this is rare in practice.

## Where to look next

- Read `sim/src/sim/barrel/mod.rs` for the per-cycle step() flow.
- Read `sim/tests/barrel_smoke.rs` for the simplest dispatch trace.
- Read `sim/tests/barrel_custom0.rs` for the FORK/JOIN/DONE patterns.
- Read `src/views/MicroStepBarrelView.vue` for the visualizer.
