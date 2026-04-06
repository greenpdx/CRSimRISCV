// ─────────────────────────────────────────────────────────────────────────────
// ID Stage Netlist — Decode
//
// Internal components: Decoder, ImmGen, Register File, Forwarding Unit,
//                      Branch Target Adder (moved from EX for 1-cycle penalty)
//
// Internal nets:
//   id_inst_fields  → Decoder extracts fields from instruction
//   id_imm          → ImmGen sign-extends immediate
//   id_rs1_orig     → RegFile rs1 read (before forwarding)
//   id_rs2_orig     → RegFile rs2 read (before forwarding)
//   id_rs1_fwd      → Forwarding unit output (forwarded rs1)
//   id_rs2_fwd      → Forwarding unit output (forwarded rs2)
//   id_branch_tgt   → Branch Target Adder output (PC + imm)
//
// External interface:
//   Inputs ← IF/ID register:
//     if_id_pc, if_id_inst
//
//   Outputs → ID/EX register:
//     id_ex_pc, id_ex_rs1_addr, id_ex_rs2_addr, id_ex_rd_addr
//     id_ex_rs1_val, id_ex_rs2_val, id_ex_imm
//     id_ex_funct3, id_ex_alu_op, id_ex_opcode, id_ex_exception
//
//   Output → IF (feedback, 1-cycle):
//     fb_branch_target (already in IF netlist)
//
//   Inputs ← WB (register writeback):
//     fb_wb_rd, fb_wb_rd_addr, fb_wb_wr_rd
//
//   Inputs ← EX/MEM, MEM/WB (forwarding):
//     fwd_ex_rd_addr, fwd_ex_wr_rd, fwd_ex_rd_valid, fwd_ex_rd
//     fwd_mem_rd_addr, fwd_mem_wr_rd, fwd_mem_rd
//
//   Output ← FWD unit:
//     fwd_force_stall
// ─────────────────────────────────────────────────────────────────────────────

import type { ComponentDescriptor, Net } from '../types/cpu'

export const ID_COMPONENTS: ComponentDescriptor[] = [
  {
    id: 'id_decoder', label: 'Decoder',
    ports: [
      { componentId: 'id_decoder', name: 'inst',       direction: 'in',  bits: 32 },
      { componentId: 'id_decoder', name: 'pc_in',      direction: 'in',  bits: 32 },
      { componentId: 'id_decoder', name: 'rs1_addr',   direction: 'out', bits: 5  },
      { componentId: 'id_decoder', name: 'rs2_addr',   direction: 'out', bits: 5  },
      { componentId: 'id_decoder', name: 'rd_addr',    direction: 'out', bits: 5  },
      { componentId: 'id_decoder', name: 'funct3',     direction: 'out', bits: 3  },
      { componentId: 'id_decoder', name: 'alu_op',     direction: 'out', bits: 14 },
      { componentId: 'id_decoder', name: 'opcode',     direction: 'out', bits: 11 },
      { componentId: 'id_decoder', name: 'exception',  direction: 'out', bits: 4  },
      { componentId: 'id_decoder', name: 'pc_out',     direction: 'out', bits: 32 },
    ],
  },
  {
    id: 'id_immgen', label: 'Immediate Generator',
    ports: [
      { componentId: 'id_immgen', name: 'inst',    direction: 'in',  bits: 32 },
      { componentId: 'id_immgen', name: 'opcode',  direction: 'in',  bits: 11 },
      { componentId: 'id_immgen', name: 'imm_out', direction: 'out', bits: 32 },
    ],
  },
  {
    id: 'id_regfile', label: 'Register File',
    ports: [
      { componentId: 'id_regfile', name: 'rs1_addr',   direction: 'in',  bits: 5  },
      { componentId: 'id_regfile', name: 'rs2_addr',   direction: 'in',  bits: 5  },
      { componentId: 'id_regfile', name: 'rd_addr',    direction: 'in',  bits: 5  },  // from WB
      { componentId: 'id_regfile', name: 'rd_data',    direction: 'in',  bits: 32 },  // from WB
      { componentId: 'id_regfile', name: 'wr_en',      direction: 'in',  bits: 1  },  // from WB
      { componentId: 'id_regfile', name: 'rs1_data',   direction: 'out', bits: 32 },
      { componentId: 'id_regfile', name: 'rs2_data',   direction: 'out', bits: 32 },
    ],
  },
  {
    id: 'id_fwd', label: 'Forwarding Unit',
    ports: [
      { componentId: 'id_fwd', name: 'rs1_orig',       direction: 'in',  bits: 32 },
      { componentId: 'id_fwd', name: 'rs2_orig',       direction: 'in',  bits: 32 },
      { componentId: 'id_fwd', name: 'rs1_addr',       direction: 'in',  bits: 5  },
      { componentId: 'id_fwd', name: 'rs2_addr',       direction: 'in',  bits: 5  },
      // From EX/MEM stage (forwarding)
      { componentId: 'id_fwd', name: 'ex_rd_addr',     direction: 'in',  bits: 5  },
      { componentId: 'id_fwd', name: 'ex_wr_rd',       direction: 'in',  bits: 1  },
      { componentId: 'id_fwd', name: 'ex_rd_valid',    direction: 'in',  bits: 1  },
      { componentId: 'id_fwd', name: 'ex_rd',          direction: 'in',  bits: 32 },
      // From MEM/WB stage (forwarding)
      { componentId: 'id_fwd', name: 'mem_rd_addr',    direction: 'in',  bits: 5  },
      { componentId: 'id_fwd', name: 'mem_wr_rd',      direction: 'in',  bits: 1  },
      { componentId: 'id_fwd', name: 'mem_rd',         direction: 'in',  bits: 32 },
      // Outputs
      { componentId: 'id_fwd', name: 'rs1_fwd',        direction: 'out', bits: 32 },
      { componentId: 'id_fwd', name: 'rs2_fwd',        direction: 'out', bits: 32 },
      { componentId: 'id_fwd', name: 'force_stall',    direction: 'out', bits: 1  },
    ],
  },
  {
    id: 'id_branch_adder', label: 'Branch Target Adder',
    ports: [
      { componentId: 'id_branch_adder', name: 'pc_in',  direction: 'in',  bits: 32 },
      { componentId: 'id_branch_adder', name: 'imm_in', direction: 'in',  bits: 32 },
      { componentId: 'id_branch_adder', name: 'target',  direction: 'out', bits: 32 },
    ],
  },
]

export const ID_NETS: Net[] = [
  // ── Inputs from IF/ID register ──
  {
    id: 'if_id_inst', bits: 32,
    from: { componentId: 'rom', portName: 'inst_out' },  // through IF/ID reg
    to: [
      { componentId: 'id_decoder', portName: 'inst' },
      { componentId: 'id_immgen',  portName: 'inst' },
    ],
  },
  {
    id: 'if_id_pc', bits: 32,
    from: { componentId: 'if_pc_reg', portName: 'pc_out' },  // through IF/ID reg
    to: [
      { componentId: 'id_decoder',       portName: 'pc_in' },
      { componentId: 'id_branch_adder',  portName: 'pc_in' },
    ],
  },

  // ── Internal: Decoder → RegFile ──
  {
    id: 'id_rs1_addr', bits: 5,
    from: { componentId: 'id_decoder', portName: 'rs1_addr' },
    to: [
      { componentId: 'id_regfile', portName: 'rs1_addr' },
      { componentId: 'id_fwd',    portName: 'rs1_addr' },
    ],
  },
  {
    id: 'id_rs2_addr', bits: 5,
    from: { componentId: 'id_decoder', portName: 'rs2_addr' },
    to: [
      { componentId: 'id_regfile', portName: 'rs2_addr' },
      { componentId: 'id_fwd',    portName: 'rs2_addr' },
    ],
  },

  // ── Internal: Decoder → ImmGen ──
  {
    id: 'id_opcode', bits: 11,
    from: { componentId: 'id_decoder', portName: 'opcode' },
    to:   [{ componentId: 'id_immgen', portName: 'opcode' }],
  },

  // ── Internal: ImmGen → Branch Target Adder ──
  {
    id: 'id_imm', bits: 32,
    from: { componentId: 'id_immgen', portName: 'imm_out' },
    to: [
      { componentId: 'id_branch_adder', portName: 'imm_in' },
      // Also goes to ID/EX register
    ],
  },

  // ── Internal: RegFile → FWD unit ──
  {
    id: 'id_rs1_orig', bits: 32,
    from: { componentId: 'id_regfile', portName: 'rs1_data' },
    to:   [{ componentId: 'id_fwd', portName: 'rs1_orig' }],
  },
  {
    id: 'id_rs2_orig', bits: 32,
    from: { componentId: 'id_regfile', portName: 'rs2_data' },
    to:   [{ componentId: 'id_fwd', portName: 'rs2_orig' }],
  },

  // ── Internal: FWD unit outputs (forwarded values → ID/EX) ──
  {
    id: 'id_rs1_fwd', bits: 32,
    from: { componentId: 'id_fwd', portName: 'rs1_fwd' },
    to:   [],  // → ID/EX register
  },
  {
    id: 'id_rs2_fwd', bits: 32,
    from: { componentId: 'id_fwd', portName: 'rs2_fwd' },
    to:   [],  // → ID/EX register
  },

  // ── Internal: Branch Target Adder output ──
  // (fb_branch_target already defined in IF netlist as the feedback net)

  // ── Internal: Decoder passthrough outputs → ID/EX ──
  {
    id: 'id_rd_addr', bits: 5,
    from: { componentId: 'id_decoder', portName: 'rd_addr' },
    to:   [],  // → ID/EX register
  },
  {
    id: 'id_funct3', bits: 3,
    from: { componentId: 'id_decoder', portName: 'funct3' },
    to:   [],  // → ID/EX register
  },
  {
    id: 'id_alu_op', bits: 14,
    from: { componentId: 'id_decoder', portName: 'alu_op' },
    to:   [],  // → ID/EX register
  },
  {
    id: 'id_pc_out', bits: 32,
    from: { componentId: 'id_decoder', portName: 'pc_out' },
    to:   [],  // → ID/EX register
  },
  {
    id: 'id_exception', bits: 4,
    from: { componentId: 'id_decoder', portName: 'exception' },
    to:   [],  // → ID/EX register
  },

  // ── Feedback: WB → RegFile (register writeback) ──
  {
    id: 'fb_wb_rd', bits: 32,
    from: { componentId: 'wb_mux', portName: 'out' },  // from WB stage
    to:   [{ componentId: 'id_regfile', portName: 'rd_data' }],
  },
  {
    id: 'fb_wb_rd_addr', bits: 5,
    from: { componentId: 'wb_mux', portName: 'rd_addr' },  // from WB stage
    to:   [{ componentId: 'id_regfile', portName: 'rd_addr' }],
  },
  {
    id: 'fb_wb_wr_rd', bits: 1,
    from: { componentId: 'wb_mux', portName: 'wr_rd' },  // from WB stage
    to:   [{ componentId: 'id_regfile', portName: 'wr_en' }],
  },

  // ── FWD unit: force_stall output ──
  {
    id: 'id_force_stall', bits: 1,
    from: { componentId: 'id_fwd', portName: 'force_stall' },
    to:   [],  // → pipeline control
  },
]
