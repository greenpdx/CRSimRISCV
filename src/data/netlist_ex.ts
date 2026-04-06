// ─────────────────────────────────────────────────────────────────────────────
// EX Stage Netlist — Execute
//
// Internal components: MUX B, ALU, Branch Logic, JALR Target, RD MUX,
//                      Write Control
//
// No MUX A — ALU gets rs1 directly
// No Branch/Jump Adder — moved to ID stage
// JALR target computed here (needs rs1 value)
// Branch condition resolved here (needs ALU compare)
// ─────────────────────────────────────────────────────────────────────────────

import type { ComponentDescriptor, Net } from '../types/cpu'

export const EX_COMPONENTS: ComponentDescriptor[] = [
  {
    id: 'ex_mux_b', label: 'MUX B (rs2/imm)',
    ports: [
      { componentId: 'ex_mux_b', name: 'in0',  direction: 'in',  bits: 32 },  // rs2
      { componentId: 'ex_mux_b', name: 'in1',  direction: 'in',  bits: 32 },  // imm
      { componentId: 'ex_mux_b', name: 'sel',  direction: 'in',  bits: 1  },  // opcode-derived
      { componentId: 'ex_mux_b', name: 'out',  direction: 'out', bits: 32 },
    ],
  },
  {
    id: 'ex_alu', label: 'ALU',
    ports: [
      { componentId: 'ex_alu', name: 'a',       direction: 'in',  bits: 32 },  // rs1 direct
      { componentId: 'ex_alu', name: 'b',       direction: 'in',  bits: 32 },  // from MUX B
      { componentId: 'ex_alu', name: 'alu_op',  direction: 'in',  bits: 14 },
      { componentId: 'ex_alu', name: 'y',       direction: 'out', bits: 32 },  // ALU result
    ],
  },
  {
    id: 'ex_branch', label: 'Branch Logic',
    ports: [
      { componentId: 'ex_branch', name: 'y_bit0',     direction: 'in',  bits: 1  },
      { componentId: 'ex_branch', name: 'opcode',     direction: 'in',  bits: 11 },
      { componentId: 'ex_branch', name: 'branch_taken',direction: 'out', bits: 1  },
      { componentId: 'ex_branch', name: 'flush',      direction: 'out', bits: 1  },
    ],
  },
  {
    id: 'ex_jalr', label: 'JALR Target',
    ports: [
      { componentId: 'ex_jalr', name: 'rs1',    direction: 'in',  bits: 32 },
      { componentId: 'ex_jalr', name: 'imm',    direction: 'in',  bits: 32 },
      { componentId: 'ex_jalr', name: 'target', direction: 'out', bits: 32 },
    ],
  },
  {
    id: 'ex_rd_mux', label: 'RD Value MUX',
    ports: [
      { componentId: 'ex_rd_mux', name: 'alu_result', direction: 'in',  bits: 32 },
      { componentId: 'ex_rd_mux', name: 'imm',        direction: 'in',  bits: 32 },  // LUI
      { componentId: 'ex_rd_mux', name: 'pc_plus4',   direction: 'in',  bits: 32 },  // JAL/JALR link
      { componentId: 'ex_rd_mux', name: 'opcode',     direction: 'in',  bits: 11 },
      { componentId: 'ex_rd_mux', name: 'rd_out',     direction: 'out', bits: 32 },
    ],
  },
  {
    id: 'ex_wr_ctrl', label: 'Write Control',
    ports: [
      { componentId: 'ex_wr_ctrl', name: 'opcode',    direction: 'in',  bits: 11 },
      { componentId: 'ex_wr_ctrl', name: 'funct3',    direction: 'in',  bits: 3  },
      { componentId: 'ex_wr_ctrl', name: 'wr_rd',     direction: 'out', bits: 1  },
      { componentId: 'ex_wr_ctrl', name: 'rd_valid',  direction: 'out', bits: 1  },
    ],
  },
]

export const EX_NETS: Net[] = [
  // ── MUX B ──
  {
    id: 'ex_mux_b_out', bits: 32,
    from: { componentId: 'ex_mux_b', portName: 'out' },
    to:   [{ componentId: 'ex_alu',  portName: 'b' }],
  },

  // ── ALU ──
  {
    id: 'ex_alu_result', bits: 32,
    from: { componentId: 'ex_alu', portName: 'y' },
    to: [
      { componentId: 'ex_branch',  portName: 'y_bit0' },
      { componentId: 'ex_rd_mux', portName: 'alu_result' },
      // Also → EX/MEM register
    ],
  },

  // ── Branch Logic output ──
  {
    id: 'ex_branch_taken', bits: 1,
    from: { componentId: 'ex_branch', portName: 'branch_taken' },
    to:   [],  // → IF PC MUX select + pipeline flush
  },
  {
    id: 'ex_flush', bits: 1,
    from: { componentId: 'ex_branch', portName: 'flush' },
    to:   [],  // → flush IF/ID
  },

  // ── JALR target ──
  // (fb_jalr_target already defined in IF netlist)

  // ── RD MUX output ──
  {
    id: 'ex_rd_val', bits: 32,
    from: { componentId: 'ex_rd_mux', portName: 'rd_out' },
    to:   [],  // → EX/MEM register
  },

  // ── Write control outputs ──
  {
    id: 'ex_wr_rd', bits: 1,
    from: { componentId: 'ex_wr_ctrl', portName: 'wr_rd' },
    to:   [],  // → EX/MEM register
  },
  {
    id: 'ex_rd_valid', bits: 1,
    from: { componentId: 'ex_wr_ctrl', portName: 'rd_valid' },
    to:   [],  // → EX/MEM register + forwarding unit
  },
]
