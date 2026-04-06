// ─────────────────────────────────────────────────────────────────────────────
// MA Stage Netlist — Memory Access
//
// Internal components: Store Aligner, Memory Bus, Load Extractor, CSR Unit
// External: RAM / I/O (Data Memory)
// ─────────────────────────────────────────────────────────────────────────────

import type { ComponentDescriptor, Net } from '../types/cpu'

export const MA_COMPONENTS: ComponentDescriptor[] = [
  {
    id: 'ma_store_align', label: 'Store Aligner',
    ports: [
      { componentId: 'ma_store_align', name: 'rs2_data',   direction: 'in',  bits: 32 },
      { componentId: 'ma_store_align', name: 'addr_low',   direction: 'in',  bits: 2  },
      { componentId: 'ma_store_align', name: 'funct3',     direction: 'in',  bits: 3  },
      { componentId: 'ma_store_align', name: 'data_out',   direction: 'out', bits: 32 },
      { componentId: 'ma_store_align', name: 'byte_sel',   direction: 'out', bits: 4  },
    ],
  },
  {
    id: 'ma_mem_bus', label: 'Memory Bus Interface',
    ports: [
      { componentId: 'ma_mem_bus', name: 'addr',       direction: 'out', bits: 32 },
      { componentId: 'ma_mem_bus', name: 'data_out',   direction: 'out', bits: 32 },
      { componentId: 'ma_mem_bus', name: 'we',         direction: 'out', bits: 1  },
      { componentId: 'ma_mem_bus', name: 'sel',        direction: 'out', bits: 4  },
      { componentId: 'ma_mem_bus', name: 'stb',        direction: 'out', bits: 1  },
      { componentId: 'ma_mem_bus', name: 'data_in',    direction: 'in',  bits: 32 },
      { componentId: 'ma_mem_bus', name: 'ack',        direction: 'in',  bits: 1  },
    ],
  },
  {
    id: 'ma_load_ext', label: 'Load Extractor',
    ports: [
      { componentId: 'ma_load_ext', name: 'raw_data',   direction: 'in',  bits: 32 },
      { componentId: 'ma_load_ext', name: 'addr_low',   direction: 'in',  bits: 2  },
      { componentId: 'ma_load_ext', name: 'funct3',     direction: 'in',  bits: 3  },
      { componentId: 'ma_load_ext', name: 'data_load',  direction: 'out', bits: 32 },
    ],
  },
  {
    id: 'ma_csr', label: 'CSR Unit',
    ports: [
      { componentId: 'ma_csr', name: 'csr_addr',   direction: 'in',  bits: 12 },
      { componentId: 'ma_csr', name: 'rs1_val',    direction: 'in',  bits: 32 },
      { componentId: 'ma_csr', name: 'funct3',     direction: 'in',  bits: 3  },
      { componentId: 'ma_csr', name: 'opcode',     direction: 'in',  bits: 11 },
      { componentId: 'ma_csr', name: 'exception',  direction: 'in',  bits: 4  },
      { componentId: 'ma_csr', name: 'csr_out',    direction: 'out', bits: 32 },
      { componentId: 'ma_csr', name: 'go_to_trap', direction: 'out', bits: 1  },
      { componentId: 'ma_csr', name: 'ret_from_trap', direction: 'out', bits: 1 },
      { componentId: 'ma_csr', name: 'trap_addr',  direction: 'out', bits: 32 },
      { componentId: 'ma_csr', name: 'ret_addr',   direction: 'out', bits: 32 },
    ],
  },
  {
    id: 'ram', label: 'Data Memory (RAM/IO)',
    ports: [
      { componentId: 'ram', name: 'addr',     direction: 'in',  bits: 32 },
      { componentId: 'ram', name: 'data_in',  direction: 'in',  bits: 32 },
      { componentId: 'ram', name: 'we',       direction: 'in',  bits: 1  },
      { componentId: 'ram', name: 'sel',      direction: 'in',  bits: 4  },
      { componentId: 'ram', name: 'data_out', direction: 'out', bits: 32 },
      { componentId: 'ram', name: 'ack',      direction: 'out', bits: 1  },
    ],
  },
]

export const MA_NETS: Net[] = [
  // ── Store path ──
  {
    id: 'ma_store_data', bits: 32,
    from: { componentId: 'ma_store_align', portName: 'data_out' },
    to:   [{ componentId: 'ma_mem_bus', portName: 'data_out' }],
  },
  {
    id: 'ma_byte_sel', bits: 4,
    from: { componentId: 'ma_store_align', portName: 'byte_sel' },
    to:   [{ componentId: 'ma_mem_bus', portName: 'sel' }],
  },

  // ── Memory bus → external RAM ──
  {
    id: 'ma_mem_addr', bits: 32,
    from: { componentId: 'ma_mem_bus', portName: 'addr' },
    to:   [{ componentId: 'ram', portName: 'addr' }],
  },
  {
    id: 'ma_mem_data_to_ram', bits: 32,
    from: { componentId: 'ma_mem_bus', portName: 'data_out' },
    to:   [{ componentId: 'ram', portName: 'data_in' }],
  },
  {
    id: 'ma_mem_we', bits: 1,
    from: { componentId: 'ma_mem_bus', portName: 'we' },
    to:   [{ componentId: 'ram', portName: 'we' }],
  },

  // ── RAM → Memory bus → Load extractor ──
  {
    id: 'ma_mem_data_from_ram', bits: 32,
    from: { componentId: 'ram', portName: 'data_out' },
    to: [
      { componentId: 'ma_mem_bus', portName: 'data_in' },
      { componentId: 'ma_load_ext', portName: 'raw_data' },
    ],
  },
  {
    id: 'ma_mem_ack', bits: 1,
    from: { componentId: 'ram', portName: 'ack' },
    to:   [{ componentId: 'ma_mem_bus', portName: 'ack' }],
  },

  // ── Load extractor output → MEM/WB ──
  {
    id: 'ma_data_load', bits: 32,
    from: { componentId: 'ma_load_ext', portName: 'data_load' },
    to:   [],  // → MEM/WB register
  },

  // ── CSR output → MEM/WB ──
  {
    id: 'ma_csr_out', bits: 32,
    from: { componentId: 'ma_csr', portName: 'csr_out' },
    to:   [],  // → MEM/WB register → WB MUX
  },

  // ── CSR trap signals → WB ──
  {
    id: 'ma_go_to_trap', bits: 1,
    from: { componentId: 'ma_csr', portName: 'go_to_trap' },
    to:   [],  // → WB trap handler
  },
  {
    id: 'ma_ret_from_trap', bits: 1,
    from: { componentId: 'ma_csr', portName: 'ret_from_trap' },
    to:   [],  // → WB trap handler
  },
  {
    id: 'ma_trap_addr', bits: 32,
    from: { componentId: 'ma_csr', portName: 'trap_addr' },
    to:   [],  // → WB trap handler
  },
  {
    id: 'ma_ret_addr', bits: 32,
    from: { componentId: 'ma_csr', portName: 'ret_addr' },
    to:   [],  // → WB trap handler
  },
]
