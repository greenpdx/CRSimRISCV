// ─────────────────────────────────────────────────────────────────────────────
// IF Stage Netlist — Fetch
//
// Internal components: PC MUX, PC Register, +4 Adder
// External: ROM (Instruction Memory)
//
// Internal nets:
//   mux_out     → PC MUX output to PC Register
//   pc_val      → PC Register output (fans out to ROM, +4, IF/ID)
//   pc_plus4    → +4 Adder output back to PC MUX input 0
//
// External interface (to other stages):
//   Outputs → IF/ID register:
//     if_id_pc    (32) — PC value
//     if_id_inst  (32) — instruction from ROM
//
//   Inputs ← feedback from later stages:
//     branch_target  (32) ← ID stage (PC + imm, 1-cycle feedback)
//     branch_taken   (1)  ← EX stage (branch condition resolved)
//     jalr_target    (32) ← EX stage (rs1 + imm for JALR)
//     trap_pc        (32) ← WB stage (mtvec or mepc)
//     trap_change_pc (1)  ← WB stage
//
//   External memory:
//     pc_addr     (32) → ROM
//     inst_from_rom (32) ← ROM
//
//   Pipeline control:
//     stall_if    (1)  ← pipeline control
//     flush_if    (1)  ← ID/EX stage
// ─────────────────────────────────────────────────────────────────────────────

import type { ComponentDescriptor, Net } from '../types/cpu'

export const IF_COMPONENTS: ComponentDescriptor[] = [
  {
    id: 'if_pc_mux', label: 'PC MUX',
    ports: [
      { componentId: 'if_pc_mux', name: 'in0',  direction: 'in',  bits: 32 },  // PC+4
      { componentId: 'if_pc_mux', name: 'in1',  direction: 'in',  bits: 32 },  // branch_target (from ID)
      { componentId: 'if_pc_mux', name: 'in2',  direction: 'in',  bits: 32 },  // JALR_target (from EX)
      { componentId: 'if_pc_mux', name: 'in3',  direction: 'in',  bits: 32 },  // trap_pc (from WB)
      { componentId: 'if_pc_mux', name: 'sel',  direction: 'in',  bits: 2  },
      { componentId: 'if_pc_mux', name: 'out',  direction: 'out', bits: 32 },
    ],
  },
  {
    id: 'if_pc_reg', label: 'PC Register',
    ports: [
      { componentId: 'if_pc_reg', name: 'in',     direction: 'in',  bits: 32 },
      { componentId: 'if_pc_reg', name: 'pc_out', direction: 'out', bits: 32 },
    ],
  },
  {
    id: 'if_adder', label: '+4 Adder',
    ports: [
      { componentId: 'if_adder', name: 'pc_in',    direction: 'in',  bits: 32 },
      { componentId: 'if_adder', name: 'pc_plus4', direction: 'out', bits: 32 },
    ],
  },
  {
    id: 'rom', label: 'Instruction Memory',
    ports: [
      { componentId: 'rom', name: 'addr',     direction: 'in',  bits: 32 },
      { componentId: 'rom', name: 'inst_out', direction: 'out', bits: 32 },
    ],
  },
]

export const IF_NETS: Net[] = [
  // ── Internal nets ──
  {
    id: 'if_mux_out', bits: 32,
    from: { componentId: 'if_pc_mux', portName: 'out' },
    to:   [{ componentId: 'if_pc_reg', portName: 'in' }],
  },
  {
    id: 'if_pc_val', bits: 32,
    from: { componentId: 'if_pc_reg', portName: 'pc_out' },
    to: [
      { componentId: 'rom',       portName: 'addr' },
      { componentId: 'if_adder',  portName: 'pc_in' },
      // Also goes to IF/ID register (external)
    ],
  },
  {
    id: 'if_pc_plus4', bits: 32,
    from: { componentId: 'if_adder', portName: 'pc_plus4' },
    to:   [{ componentId: 'if_pc_mux', portName: 'in0' }],
  },

  // ── External: ROM ──
  {
    id: 'if_inst', bits: 32,
    from: { componentId: 'rom', portName: 'inst_out' },
    to:   [],  // → IF/ID register (connected when ID is added)
  },

  // ── Feedback inputs (from later stages) ──
  {
    id: 'fb_branch_target', bits: 32,
    from: { componentId: 'id_branch_adder', portName: 'target' },  // from ID stage
    to:   [{ componentId: 'if_pc_mux', portName: 'in1' }],
  },
  {
    id: 'fb_jalr_target', bits: 32,
    from: { componentId: 'ex_jalr', portName: 'target' },  // from EX stage
    to:   [{ componentId: 'if_pc_mux', portName: 'in2' }],
  },
  {
    id: 'fb_trap_pc', bits: 32,
    from: { componentId: 'wb_trap', portName: 'trap_pc' },  // from WB stage
    to:   [{ componentId: 'if_pc_mux', portName: 'in3' }],
  },
]
