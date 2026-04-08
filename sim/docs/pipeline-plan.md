# 5-Stage Pipeline Simulator — Plan & Single-Cycle Comparison

This document is the design plan for the 5-stage pipeline RISC-V simulator
that lives in `sim/src/sim/pipeline/`. It also explains exactly how the
pipelined model differs from the existing single-cycle simulator in
`sim/src/sim/cpu_state.rs`.

---

## 1. Why two simulators

The single-cycle simulator answers "what does the CPU do when it executes
**one** instruction?" — it walks the datapath through 9 fine-grained
phases for a single instruction at a time. It's simple to follow and the
visualizer can light up one bus per phase.

The pipelined simulator answers "what does the CPU do when **five**
instructions are running at the same time?" — at any cycle, one
instruction is in IF, another is in ID, another is in EX, another is in
MA, and another is in WB. The user-visible state of the CPU at any
moment includes the contents of all five pipeline registers plus the
register file, RAM, ROM, and CSRs.

The two simulators **share** all of the underlying data: the same RV32I
ISA tables, the same ALU, the same control decoder, the same immediate
generator, the same RAM/ROM. They differ only in how those building
blocks are *wired together* in time.

---

## 2. Goals and non-goals

### Goals

1. **Implement load-use hazard detection** — insert a single-cycle
   bubble when a load is followed by a dependent instruction.
2. **Implement forwarding (bypassing)** — route results from EX/MA and
   MA/WB back to the EX stage's input mux to avoid stalls on most RAW
   hazards.
3. **No branch prediction** — on a taken branch, squash the two
   instructions in IF and ID and redirect the PC. Branch penalty is 2
   cycles per taken branch.
4. **Run real RV32I programs correctly** — same arithmetic, same
   load/store semantics as the single-cycle simulator.
5. **Step one cycle at a time**, exposing the full pipeline-register
   state and the active component set per stage.
6. **Designed for trap support, but traps disabled in v1.** The data
   structures and the step function should make it cheap to add
   precise exceptions later, but the v1 simulator does not handle
   them.
7. **Per-stage Rust file** so each stage's logic is isolated and
   readable.

### Non-goals

- **Branch prediction** (would be a v2 enhancement).
- **Trap and interrupt handling** (designed-in but disabled — v2).
- **Out-of-order execution, superscalar, multi-issue** — way out of
  scope.
- **Exact cycle-accurate timing** — propagation delay and clock skew
  are not modeled. The simulator runs at one logical cycle per call.
- **Cache modelling** — RAM and ROM respond in one cycle, like the
  single-cycle simulator.

---

## 3. File layout

```
sim/src/sim/pipeline/
├── mod.rs          # public API; re-exports + step() entry point
├── state.rs        # PipelineState struct + the 4 pipeline-register structs
├── stage_if.rs     # IF stage logic
├── stage_id.rs     # ID stage logic + hazard detection
├── stage_ex.rs     # EX stage logic + forwarding unit + branch resolution
├── stage_ma.rs     # MA stage logic
└── stage_wb.rs     # WB stage logic
```

Each stage file exports a single function `compute_<stage>_outputs(...)`
that takes the current pipeline register inputs and produces the next
stage's register inputs (plus any side effects on RAM/regfile/etc.).

`mod.rs::step()` orchestrates them in **reverse** order (WB first, then
MA, then EX, then ID, then IF) so that earlier stages see the *new*
register values written by later stages this cycle. This mirrors how
real hardware works: the writes happen on the rising edge, but the
combinational logic for stage N reads pipeline register inputs that
were *latched* on the previous edge, while stage N+1 reads what stage
N is *producing this cycle*. Computing in reverse order makes the data
flow easy to follow.

---

## 4. Pipeline register contents

Each pipeline register is a struct holding the partial work of an
in-flight instruction. In addition to the data fields, every register
has a `valid` bit — when `false`, that slot holds a bubble (NOP) and
all downstream stages treat it as a no-op.

### `IfId`
```rust
pub struct IfId {
    pub valid: bool,
    pub pc: u32,
    pub instr: u32,
    pub pc_plus4: u32,
}
```

### `IdEx`
```rust
pub struct IdEx {
    pub valid: bool,
    pub pc: u32,
    pub pc_plus4: u32,
    pub rs1_data: u32,
    pub rs2_data: u32,
    pub imm: u32,
    pub rs1_num: u8,
    pub rs2_num: u8,
    pub rd_num: u8,
    pub funct3: u8,
    pub funct7b30: u8,
    pub opcode: u8,
    pub ctrl: ControlBits,    // alu_src, alu_op, mem_read, mem_write,
                              // reg_write, wb_sel, pc_sel, branch, jump
}
```

### `ExMa`
```rust
pub struct ExMa {
    pub valid: bool,
    pub alu_result: u32,
    pub rs2_data: u32,         // for stores
    pub pc_plus4: u32,         // for JAL/JALR link
    pub rd_num: u8,
    pub funct3: u8,            // for load width
    pub branch_taken: bool,
    pub branch_target: u32,
    pub ctrl: ControlBits,     // mem_read, mem_write, reg_write, wb_sel
}
```

### `MaWb`
```rust
pub struct MaWb {
    pub valid: bool,
    pub alu_result: u32,
    pub mem_data: u32,
    pub pc_plus4: u32,
    pub rd_num: u8,
    pub ctrl: ControlBits,     // reg_write, wb_sel
}
```

`ControlBits` is a small struct shared by all four registers; each
register only consults the bits relevant to its downstream stages.

---

## 5. The step function

`PipelineState::step()` advances the simulator by **one clock cycle**.
Logical order:

```text
1. Compute WB stage outputs from MA/WB    (combinational; writes regfile)
2. Compute MA stage outputs from EX/MA    (combinational; reads/writes RAM)
3. Compute EX stage outputs from ID/EX    (combinational; uses forwarding)
4. Detect EX→ID forwarding sources        (forward_unit)
5. Compute ID stage outputs from IF/ID    (combinational; reads regfile)
6. Detect load-use hazard                 (hazard_unit; sets stall flag)
7. Compute IF stage outputs from PC       (combinational; reads ROM)
8. Latch new pipeline registers           (with NOPs for stalls and flushes)
9. Update PC                               (or hold on stall)
10. cycle_count += 1
```

Steps 1–3 read the current pipeline register inputs (latched on the
previous edge) and produce the *next* values for the registers in
front of them. Steps 4–6 implement the hazard / forwarding logic.
Step 7 reads the current PC and the new IF/ID inputs. Step 8 latches
everything atomically — the "rising clock edge".

A bubble (NOP slot) is inserted into ID/EX whenever:
- the hazard unit detected a load-use stall, **or**
- the EX stage detected a taken branch (so the instruction in ID/EX
  becomes invalid because it was fetched speculatively after the
  branch).

A flush is implemented by setting `valid = false` on the affected
pipeline registers.

---

## 6. Forwarding (bypassing)

The `forward_unit` runs combinationally during EX. For each ALU input,
it looks at the current EX/MA and MA/WB pipeline registers and decides
which value to feed in:

```text
forward_a:
  if EX_MA.valid && EX_MA.ctrl.reg_write && EX_MA.rd_num != 0
                  && EX_MA.rd_num == ID_EX.rs1_num:
      → EX_MA.alu_result
  elif MA_WB.valid && MA_WB.ctrl.reg_write && MA_WB.rd_num != 0
                  && MA_WB.rd_num == ID_EX.rs1_num:
      → MA_WB write_data
  else:
      → ID_EX.rs1_data (the value read from the regfile in ID)
```

Same for `forward_b` against `ID_EX.rs2_num`.

Forwarding happens before the `mux_alusrc` mux. After forwarding,
`mux_alusrc` chooses between the (possibly forwarded) rs2 and the
immediate.

---

## 7. Hazard detection (load-use stall)

The `hazard_unit` runs combinationally during ID. It checks whether
the instruction currently in ID/EX is a load **and** its destination
register matches one of the source registers of the instruction
currently in IF/ID:

```text
load_use_hazard =
    ID_EX.valid && ID_EX.ctrl.mem_read &&
    (ID_EX.rd_num == IF_ID.rs1 || ID_EX.rd_num == IF_ID.rs2)
```

When this fires:
- The PC is **frozen** (do not increment).
- The IF/ID register is **frozen** (the next instruction stays in IF/ID).
- A **bubble** is inserted into ID/EX (instead of the real new value).

Net effect: one extra cycle, and the pipeline picks up where it left
off.

---

## 8. Branch resolution

In v1, branches are **resolved in EX**. When the EX stage detects a
taken branch, it asserts `branch_taken` and produces a `branch_target`
value. The next cycle:

- IF/ID is invalidated (the instruction that was just fetched after
  the branch is wrong).
- ID/EX is invalidated (the instruction that was in ID is also wrong —
  it's the one *right after* the branch in program order).
- PC is set to `branch_target`.

Branch penalty: **2 cycles** per taken branch (the two squashed
instructions). Not-taken branches have zero penalty because the
sequentially fetched instructions are the right ones.

JAL is always taken (penalty 2). JALR is always taken (penalty 2;
target comes from the ALU).

---

## 9. The 5 stages — what each computes

### Stage IF (`stage_if.rs`)
- Reads `pc`, ROM at `pc`, computes `pc + 4`.
- Writes `IfId { valid: true, pc, instr, pc_plus4 }` (unless
  flushed/stalled).
- Always-active components: `pc`, `rom`, `pc_adder`, `if_id_reg`.

### Stage ID (`stage_id.rs`)
- Reads `IfId.instr` from the previous cycle's IF/ID register.
- Decodes the opcode to get the control vector.
- Generates the immediate.
- Reads `rs1`/`rs2` from the regfile (combinational).
- Detects load-use hazards (using `IdEx` from the current cycle).
- Writes `IdEx { ... }` (unless stalled/flushed → bubble).
- Always-active components: `decoder`, `imm_gen`, `regfile` (read),
  `hazard_unit`, `id_ex_reg`.

### Stage EX (`stage_ex.rs`)
- Reads `IdEx`.
- Runs `forward_unit` to pick rs1/rs2 sources.
- Computes `alu_a` from forwarded rs1, `alu_b` from forwarded rs2 or
  immediate.
- Calls `alu_compute(alu_ctrl_code, alu_a, alu_b)`.
- For branches: computes `branch_target = pc + imm` and decides
  taken/not.
- For JALR: target = `(rs1 + imm) & ~1`.
- Writes `ExMa { alu_result, rs2_data, pc_plus4, rd_num, funct3,
  branch_taken, branch_target, ctrl }`.
- Always-active components: `forward_unit`, `mux_fwd_a`, `mux_fwd_b`,
  `alu_ctrl`, `mux_alusrc`, `alu`, `branch_adder`, `ex_ma_reg`.

### Stage MA (`stage_ma.rs`)
- Reads `ExMa`.
- For loads: reads RAM (or IO) at `alu_result`, applies sign/zero
  extension.
- For stores: writes RAM (or IO) at `alu_result` with `rs2_data`.
- Writes `MaWb { alu_result, mem_data, pc_plus4, rd_num, ctrl }`.
- Always-active components: `ma_wb_reg`, plus `ram` or `io` and
  `loadstore` if memory access.

### Stage WB (`stage_wb.rs`)
- Reads `MaWb`.
- Selects the write-back source via the `mux_wb` selector.
- Writes the result to the regfile (if `reg_write && rd_num != 0`).
- Always-active components: `mux_wb`, `regfile` (write).

---

## 10. Differences from the single-cycle simulator

| Dimension | Single-cycle (`cpu_state.rs`) | 5-stage pipeline (`pipeline/`) |
|---|---|---|
| **State per cycle** | One in-flight instruction | Five in-flight instructions |
| **Phases** | 9 fine-grained phases per instruction (`Fetch`, `Decode`, `Control`, `Imm`, `RegRead`, `Alu`, `Mem`, `Wb`, `PcUpdate`) | 5 stages (`IF`, `ID`, `EX`, `MA`, `WB`), each holding a different instruction |
| **Step semantics** | One step = one instruction. Phases run sequentially within. | One step = one **cycle**. Five things happen at once. |
| **Sequential elements** | PC, regfile, RAM | PC, regfile, RAM, **plus 4 pipeline registers** |
| **Combinational blocks** | decoder, immgen, ALU, RAM read, mux_wb, mux_pc, mux_alusrc | Same set, **plus** forwarding muxes + forward_unit + hazard_unit |
| **Hazards** | None — each instruction completes before the next starts | Load-use hazards detected and resolved with a bubble |
| **Forwarding** | N/A | EX/MA → EX and MA/WB → EX bypass paths |
| **Branch resolution** | Resolved in PcUpdate at the end of the same cycle | Resolved in EX, two cycles after fetch |
| **Branch penalty** | 0 cycles | 2 cycles per taken branch |
| **Throughput** | 1 instruction per cycle (the cycle is long) | Up to 1 instruction per cycle (the cycle is short) |
| **Latency** | 1 cycle per instruction | 5 cycles per instruction |
| **Critical path** | Sum of all 9 phases (worst case `lw`) | Longest single stage (probably MA) |
| **Wasm step API** | `step_traced()` returns activity per phase | `step_cycle()` returns activity per stage |
| **CSR / trap support** | Yes (feature-flagged via `traps_enabled`) | Designed for, **disabled in v1** |
| **Code organization** | One big file (`cpu_state.rs`) | Per-stage files in `pipeline/` |

### Conceptual differences

1. **Pipeline registers are first-class state.** In single-cycle, the
   only state across cycles is `pc`, `registers`, `ram`, `rom`. In
   pipelined, you also have the four pipeline registers, each holding
   a partial instruction. The visualizer needs to display all of them.

2. **Multiple instructions are simultaneously "active".** At cycle N,
   instruction A is being written back (WB), instruction B is in
   memory (MA), instruction C is in execute (EX), instruction D is
   being decoded (ID), instruction E is being fetched (IF). All five
   are valid and meaningful.

3. **The activity table changes shape.** Single-cycle has
   `activity[phase] → component_set` — one phase, one set. Pipelined
   has `activity[stage] → component_set` — one set per stage, but
   *every* stage may be active in the same cycle. The user-visible
   "what's the CPU doing" is now the union of all five stage sets.

4. **Hazards are observable.** When the hazard unit fires, the user
   sees a bubble propagate through ID/EX → EX/MA → MA/WB on subsequent
   cycles. When the forward unit fires, the user sees the alu_a or
   alu_b value come from a non-regfile source. These are events the
   single-cycle simulator can never produce.

5. **Branches squash work.** When a branch is taken, two instructions
   are invalidated mid-pipeline. In the visualizer, you'll see two
   slots become bubbles on the next cycle. Single-cycle never has to
   throw away work.

---

## 11. Wasm exposure (planned)

The pipeline simulator will be exposed via a new wasm class
`JsPipelineState`, parallel to `JsCpuState`. Methods:

```rust
pub fn new() -> Self
pub fn reset(&mut self)
pub fn load_program(&mut self, words: &[u32])
pub fn step_cycle(&mut self) -> JsValue   // returns the activity snapshot
pub fn snapshot(&self) -> JsValue          // returns full state for the table view
pub fn read_register(&self, idx: u8) -> u32
pub fn poke_register(&mut self, idx: u8, val: u32)
pub fn poke_ram(&mut self, addr: u32, val: u32)
pub fn poke_rom(&mut self, addr: u32, val: u32)
```

The `snapshot` returns:

```ts
{
  cycle: number,
  pc: number,
  registers: number[],
  if_id:  { valid, pc, instr, pc_plus4 },
  id_ex:  { valid, pc, rs1_data, rs2_data, imm, rs1_num, rs2_num,
            rd_num, funct3, ctrl, ... },
  ex_ma:  { valid, alu_result, rs2_data, pc_plus4, rd_num, funct3,
            branch_taken, ctrl },
  ma_wb:  { valid, alu_result, mem_data, pc_plus4, rd_num, ctrl },
  hazard: { stall, flush },
  forward_a: 0|1|2,    // 0=regfile, 1=EX/MA, 2=MA/WB
  forward_b: 0|1|2,
  active_components: { IF: [...], ID: [...], EX: [...], MA: [...], WB: [...] },
}
```

That snapshot is everything the Vue table view needs to render the
whole pipeline state per cycle.

---

## 12. Implementation order

1. ✅ This plan document (you're reading it)
2. `state.rs` — pipeline-register structs
3. `stage_if.rs` — fetch
4. `stage_id.rs` — decode + regfile read + hazard detection
5. `stage_ex.rs` — execute + forwarding + branch resolution
6. `stage_ma.rs` — memory access
7. `stage_wb.rs` — write-back
8. `mod.rs` — `PipelineState::step()` orchestration + active-component
   matrix
9. `tests/pipeline.rs` — small programs to verify hazards, forwarding,
   branches
10. Wasm exposure (`JsPipelineState`)
11. Vue table view (`MicroStepPipelineView.vue`)

Steps 1–9 land us at "the simulator runs RV32I correctly with hazards
and forwarding". Steps 10–11 give us the visualizer.

---

## 13. Tests to write

| test | what it verifies |
|---|---|
| `pipeline_basic_addi` | a single ADDI runs correctly through all 5 stages |
| `pipeline_two_adds_no_hazard` | back-to-back ADDs with no dependency complete in 6 cycles |
| `pipeline_raw_hazard_forwarding` | `add x5; sub x6, x5` → forwarding resolves it, no stall |
| `pipeline_load_use_stall` | `lw x5; add x6, x5` → 1 bubble inserted |
| `pipeline_branch_taken_squash` | `beq x5, x5, +8; nop; addi x10, x0, 99` → x10 not written, branch squashes |
| `pipeline_branch_not_taken` | `bne x5, x5, +8; addi x10, x0, 99` → x10 = 99, no penalty |
| `pipeline_jal` | call a function, observe link address in ra |
| `pipeline_load_store_round_trip` | `sw t0, 0(t1); lw t2, 0(t1)` → t2 == t0 |
| `pipeline_50_instr_throughput` | 50 hazard-free instructions complete in 50 + 4 cycles |
