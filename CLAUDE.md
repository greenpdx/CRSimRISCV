# RISC-V CPU Visualizer — Claude Code Guide

## Project

Vue 3 + TypeScript + Pinia educational CPU visualizer for RV32I single-cycle datapath.
Designed for classroom use — animated signal propagation, switchable 8-register / 32-register view,
scrollable ROM/RAM tables, full netlist-driven wiring.

---

## Tech Stack

| Layer | Choice |
|-------|--------|
| Framework | Vue 3 (Composition API, `<script setup>`) |
| Language | TypeScript (strict) |
| State | Pinia |
| Styling | Scoped `<style scoped>` per component, no CSS framework |
| Font | JetBrains Mono (monospace throughout) |
| Wiring | SVG `<BusLine>` / `<ControlLine>` driven by netlist |
| Build | Vite (to be scaffolded) |

Scaffold command (run once):
```bash
npm create vite@latest cpu-vis -- --template vue-ts
cd cpu-vis
npm install pinia
npm install -D @types/node
```

Then copy all `src/` files from this repo into the scaffolded project.

---

## Directory Layout

```
src/
  types/
    cpu.ts                    # Shared interfaces: Register, MemoryWord,
                              # BusState, ControlSignalState, Port, Net,
                              # ComponentDescriptor, Netlist

  data/
    registerData.ts           # All 32 RV32I register definitions (name, group,
                              # savedBy, description). Exports REGISTER_DEFS,
                              # REGISTER_DEFS_8 (8-reg subset), REG_BY_NUM,
                              # REG_BY_NAME, GROUP_COLORS
    aluData.ts                # 11 ALU ops table + aluCompute() pure function
    aluControlData.ts         # ALU control truth table + computeAluControl()
    controlData.ts            # 9-opcode decoder table, ControlSignals interface,
                              # PC_SEL_LABEL, WB_SEL_LABEL, BRANCH_CONDITION
    immData.ts                # 5 immediate formats + extractImmediate() pure fn
    netlist.ts                # Complete RV32I netlist: 15 components, 28 nets.
                              # NET_BY_ID, COMPONENT_BY_ID, netsFrom(), netsTo()

  stores/
    cpuState.ts               # Pinia store: full single-cycle step() execution,
                              # all register/memory/bus/control reactive state

  components/cpu/
    rows/
      RegisterRow.vue         # Single <tr> for register file table
      MemoryRow.vue           # Single <tr> for ROM/RAM tables

    tables/
      RegisterFile.vue        # 32-register scrollable table, 8/32-reg toggle
      RomBlock.vue            # Instruction memory scrollable table, auto-scroll to PC
      RamBlock.vue            # Data memory scrollable table, read/write highlight

    units/
      PcRegister.vue          # PC display: current + next, branch badge
      PcAdder.vue             # +4 adder display
      AluUnit.vue             # ALU: operands, operation, result, ZNVC flags
      AluControlUnit.vue      # 2-bit ALUOp + funct3 + funct7[30] → 4-bit aluControl
      DecoderControl.vue      # Instruction decoder, all control signal badges
      ImmGenerator.vue        # Immediate extraction display (I/S/B/U/J formats)
      LoadStore.vue           # Load/store unit: byte/halfword/word, read/write
      IoBlock.vue             # I/O peripheral: 8-LED bit display

    wiring/
      BusLine.vue             # SVG animated data/address bus wire
      ControlLine.vue         # SVG animated 1-bit control signal wire
      ControlSignalBadge.vue  # Inline pill badge for control signal display

    MuxUnit.vue               # Generic N-input MUX component
```

---

## Key Design Decisions

### Netlist-driven architecture
`src/data/netlist.ts` defines every component port and net (wire) as static data.
The Pinia store (`cpuState.ts`) maintains a parallel `busStates{}` and `controlStates{}`
record keyed by `netId`. Components receive state as props — they never import the store
directly except the top-level layout view.

Animation: `BusLine` and `ControlLine` watch their `state`/`asserted` props.
When the store sets `busStates['alu_result'].state = 'active'`, the `BusLine`
bound to `netId="alu_result"` fires its `stroke-dashoffset` CSS animation.

### Pure compute functions in data files
`aluCompute()`, `extractImmediate()`, `computeAluControl()` live in `src/data/`,
not in Vue components. The store and components both import and call them.
No duplication of logic.

### PCSel is 2-bit (not separate branch + jump)
```
00 → PC + 4              (sequential)
01 → PC + imm            (branches if taken, JAL unconditional)
10 → (rs1 + imm) & ~1   (JALR only)
```
For conditional branches `pcSel=01` is gated at runtime in `step()` using
`BRANCH_CONDITION[funct3]` which maps each branch funct3 to a flag check
(zero / !zero / negative / !negative / carry / !carry).

### RegisterFile has 8-reg / 32-reg toggle
The toggle is internal state inside `RegisterFile.vue`. In 8-reg mode it shows only:
`zero, ra, sp, t0, t1, s0, a0, a1` (defined in `REGISTER_DEFS_8`).
The parent passes all 32 registers — the component filters the display.

### mux_wb has 3 inputs
```
in0 (WBSel=00) ← alu_result      R-type, I-ALU, LUI, AUIPC
in1 (WBSel=01) ← mem_read_data   Load instructions
in2 (WBSel=10) ← pc_plus4        JAL, JALR (link address)
```

### Memory addressing
- ROM: starts at `0x00000000`
- RAM: starts at `0x00010000`
- I/O: starts at `0x40020000` — `isIoAddress()` in store detects accesses above this

---

## Pinia Store API (`useCpuState`)

```ts
// State
registers: Register[]          // 32 registers, active/readA/readB flags
pc: number                     // current PC
nextPc: number                 // next PC (after step)
romWords: MemoryWord[]         // instruction memory
ramWords: MemoryWord[]         // data memory
ioData: number                 // I/O output register
ioOp: 'idle'|'read'|'write'
busStates: Record<netId, BusState>
controlStates: Record<netId, ControlSignalState>
cycleCount: number
halted: boolean

// Actions
step()                                          // execute one instruction
reset()                                         // reset all state
loadProgram(words: number[], base?)             // load binary into ROM
loadProgramWithAsm(entries: {value,asm}[], base?) // load with disasm labels
setIoData(value: number)                        // write I/O from outside
```

---

## Component Props Quick Reference

```ts
// Rows (used inside tables — render as <tr>)
RegisterRow   { reg: Register, def?: RegisterDef, stripe?: boolean }
MemoryRow     { word: MemoryWord, kind: 'rom'|'ram', stripe?: boolean }

// Tables
RegisterFile  { registers: Register[] }
RomBlock      { words: MemoryWord[], pc: number }
RamBlock      { words: MemoryWord[], lastOp: RamOp, lastAddress: number, lastValue: number }

// Units
PcRegister    { pc: number, nextPc: number, active?: boolean }
PcAdder       { pc: number, active?: boolean }
AluUnit       { aluControl: number, operandA: number, operandB: number, active?: boolean }
AluControlUnit{ aluOp: 0|1|2, funct3: number, funct7b30: 0|1, isLui?: boolean, active?: boolean }
DecoderControl{ opcode: number, active?: boolean }
ImmGenerator  { instruction: number, immSel: ImmSel, active?: boolean }
LoadStore     { op: LoadStoreOp, address: number, width: 'b'|'h'|'w',
                loadSign?: 'signed'|'unsigned', writeData?: number, readData?: number }
IoBlock       { baseAddress: number, address: number, data: number, op: IoOp }

// Wiring (SVG — used inside an <svg> element in the layout view)
BusLine       { netId: string, points: Point[], bits: number, value: number,
                state: SignalState, showLabel?: boolean }
ControlLine   { netId: string, points: Point[], asserted: boolean, showLabel?: boolean }

// Generic
MuxUnit       { label: string, inputs: MuxInput[], select: number, active?: boolean }
```

---

## What Still Needs Building

1. **`CpuLayout.vue`** — top-level composition view. Places all components in an SVG+HTML
   canvas. Reads position data from a layout config (or hardcoded coordinates).
   Renders `BusLine` / `ControlLine` for every net in `RV32I_NETLIST.nets`.
   The SVG file (`cpu_layout_v2.svg`) being edited by Shaun defines block positions.

2. **`main.ts` / `App.vue`** — wire up Pinia, import `useCpuState`, pass props down.

3. **Clock controls** — Step button, Run (auto-step with interval), Reset, speed slider.

4. **Program loader** — textarea or file input to load hex/assembly into ROM.

5. **`BranchAdder.vue`** — small adder unit component (PC + immediate = branch target).
   Structurally identical to `PcAdder.vue` but takes `pc` and `offset` as separate props.

6. **Level 1 / Level 2 layout toggle** — Level 1 hides `ImmGenerator`, `AluControlUnit`,
   shows `RegisterFile` in 8-reg mode. Level 2 reveals them and switches to 32-reg mode.
   Controlled by a top-level `level` ref in `App.vue`.

---

## Netlist Summary

### Components (15)
`pc`, `pc_adder`, `rom`, `decoder`, `imm_gen`, `alu_ctrl`, `regfile`,
`mux_alusrc`, `alu`, `loadstore`, `ram`, `io`, `mux_wb`, `mux_pc`, `branch_adder`

### Nets (28)
| Net ID | Bits | From → To |
|--------|------|-----------|
| `pc_out` | 32 | pc → rom, pc_adder, branch_adder |
| `pc_plus4` | 32 | pc_adder → mux_pc.in0, mux_wb.in2 |
| `next_pc` | 32 | mux_pc → pc |
| `instr` | 32 | rom → decoder, imm_gen, alu_ctrl, regfile |
| `ctrl_reg_write` | 1 | decoder → regfile |
| `ctrl_alu_src` | 1 | decoder → mux_alusrc.sel |
| `ctrl_alu_op` | 2 | decoder → alu_ctrl |
| `ctrl_mem_read` | 1 | decoder → ram, loadstore |
| `ctrl_mem_write` | 1 | decoder → ram, loadstore, io |
| `ctrl_wb_sel` | 2 | decoder → mux_wb.sel |
| `ctrl_pc_sel` | 2 | decoder → mux_pc.sel |
| `ctrl_imm_sel` | 3 | decoder → imm_gen |
| `immediate` | 32 | imm_gen → mux_alusrc.in1, branch_adder |
| `alu_control` | 4 | alu_ctrl → alu |
| `rs1_data` | 32 | regfile → alu.A |
| `rs2_data` | 32 | regfile → mux_alusrc.in0, ram, loadstore, io |
| `alu_b` | 32 | mux_alusrc → alu.B |
| `alu_result` | 32 | alu → mux_wb.in0, ram.addr, loadstore, io.addr |
| `zero_flag` | 1 | alu → (step() runtime only) |
| `mem_read_data` | 32 | ram → mux_wb.in1 |
| `io_read_data` | 32 | io → mux_wb.in1 |
| `branch_target` | 32 | branch_adder → mux_pc.in1 |
| `jalr_target` | 32 | alu → mux_pc.in2 |
| `write_data` | 32 | mux_wb → regfile |
| `ctrl_alu_op` | 2 | decoder → alu_ctrl |

### Control nets (bits < 32): `ctrl_reg_write`, `ctrl_alu_src`, `ctrl_mem_read`, `ctrl_mem_write`, `zero_flag` (1-bit each) · `ctrl_alu_op`, `ctrl_wb_sel`, `ctrl_pc_sel` (2-bit) · `ctrl_imm_sel` (3-bit) · `alu_control` (4-bit)

---

## ALU Operations (4-bit aluControl)

| Code | Op | Instructions |
|------|----|-------------|
| 0000 | AND | AND, ANDI |
| 0001 | OR | OR, ORI |
| 0010 | ADD | ADD, ADDI, LW, SW, JALR, AUIPC |
| 0011 | XOR | XOR, XORI |
| 0100 | SLL | SLL, SLLI |
| 0101 | SRL | SRL, SRLI |
| 0110 | SUB | SUB, BEQ, BNE, BLT, BGE, BLTU, BGEU |
| 0111 | SLT | SLT, SLTI |
| 1000 | SLTU | SLTU, SLTIU |
| 1001 | SRA | SRA, SRAI |
| 1010 | LUI | LUI (pass B through) |

---

## Colour Conventions (consistent across all components and wires)

| Colour | Hex | Meaning |
|--------|-----|---------|
| Blue | `#0099cc` | Instruction bus / address |
| Amber | `#cc8800` | Data path |
| Red | `#cc2222` | Control signals |
| Green | `#009977` | PC feedback loop |
| Teal header | `#2aacbe` | Component header background |
| Teal body | `#4ec3d5` | Table component body background |
| White | `#ffffff` | Unit component background |
