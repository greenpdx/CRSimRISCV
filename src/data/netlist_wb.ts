// ─────────────────────────────────────────────────────────────────────────────
// WB Stage Netlist — Writeback
//
// Internal components: WB MUX, Trap Handler
// No external blocks — all outputs are feedback to earlier stages
//
// Outputs:
//   → RegFile in ID: wb_rd, wb_rd_addr, wb_wr_rd
//   → IF PC MUX: trap_pc, trap_change_pc
//   → FWD unit in ID: rd_addr, wr_rd, wb_rd
//   → Pipeline control: stall_wb, flush_all
// ─────────────────────────────────────────────────────────────────────────────

import type { ComponentDescriptor, Net } from '../types/cpu'

export const WB_COMPONENTS: ComponentDescriptor[] = [
  {
    id: 'wb_mux', label: 'WB MUX',
    ports: [
      { componentId: 'wb_mux', name: 'alu_result', direction: 'in',  bits: 32 },  // from EX (rd_val)
      { componentId: 'wb_mux', name: 'data_load',  direction: 'in',  bits: 32 },  // from MA (loaded data)
      { componentId: 'wb_mux', name: 'csr_out',    direction: 'in',  bits: 32 },  // from MA CSR
      { componentId: 'wb_mux', name: 'opcode',     direction: 'in',  bits: 11 },  // select
      { componentId: 'wb_mux', name: 'out',        direction: 'out', bits: 32 },  // final rd value
      { componentId: 'wb_mux', name: 'rd_addr',    direction: 'out', bits: 5  },  // passthrough
      { componentId: 'wb_mux', name: 'wr_rd',      direction: 'out', bits: 1  },  // passthrough
    ],
  },
  {
    id: 'wb_trap', label: 'Trap Handler',
    ports: [
      { componentId: 'wb_trap', name: 'pc',              direction: 'in',  bits: 32 },
      { componentId: 'wb_trap', name: 'go_to_trap',      direction: 'in',  bits: 1  },
      { componentId: 'wb_trap', name: 'ret_from_trap',   direction: 'in',  bits: 1  },
      { componentId: 'wb_trap', name: 'trap_addr_in',    direction: 'in',  bits: 32 },  // mtvec
      { componentId: 'wb_trap', name: 'ret_addr_in',     direction: 'in',  bits: 32 },  // mepc
      { componentId: 'wb_trap', name: 'trap_pc',         direction: 'out', bits: 32 },  // → IF PC MUX
      { componentId: 'wb_trap', name: 'change_pc',       direction: 'out', bits: 1  },
      { componentId: 'wb_trap', name: 'flush_all',       direction: 'out', bits: 1  },
    ],
  },
]

export const WB_NETS: Net[] = [
  // ── WB MUX output → RegFile writeback (feedback to ID) ──
  {
    id: 'wb_rd_val', bits: 32,
    from: { componentId: 'wb_mux', portName: 'out' },
    to: [
      { componentId: 'id_regfile', portName: 'rd_data' },   // → RegFile write
      { componentId: 'id_fwd',    portName: 'mem_rd' },     // → FWD unit (MEM/WB forwarding)
    ],
  },
  {
    // wb_rd_addr and wb_wr_rd are passthrough — already defined as fb_wb_rd_addr
    // and fb_wb_wr_rd in ID netlist. These just confirm the source.
    id: 'wb_wr_rd_out', bits: 1,
    from: { componentId: 'wb_mux', portName: 'wr_rd' },
    to: [
      { componentId: 'id_regfile', portName: 'wr_en' },
      { componentId: 'id_fwd',    portName: 'mem_wr_rd' },
    ],
  },
  {
    id: 'wb_rd_addr_out', bits: 5,
    from: { componentId: 'wb_mux', portName: 'rd_addr' },
    to: [
      { componentId: 'id_regfile', portName: 'rd_addr' },
      { componentId: 'id_fwd',    portName: 'mem_rd_addr' },
    ],
  },

  // ── Trap Handler → IF (feedback) ──
  // fb_trap_pc already defined in IF netlist

  // ── Trap Handler outputs ──
  {
    id: 'wb_change_pc', bits: 1,
    from: { componentId: 'wb_trap', portName: 'change_pc' },
    to:   [],  // → IF PC MUX select + pipeline flush
  },
  {
    id: 'wb_flush_all', bits: 1,
    from: { componentId: 'wb_trap', portName: 'flush_all' },
    to:   [],  // → flush all pipeline registers
  },
]
