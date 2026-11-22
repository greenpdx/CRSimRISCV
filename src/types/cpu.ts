// ─────────────────────────────────────────────────────────────────────────────
// Signal state
// ─────────────────────────────────────────────────────────────────────────────

export type SignalState = 'idle' | 'active' | 'asserted'

// ─────────────────────────────────────────────────────────────────────────────
// Port — one connection point on a component
//   direction:
//     'in'  — input only (receives signal)
//     'out' — output only (drives signal)
//     'bi'  — bidirectional (e.g. memory data bus)
//     'tri' — tristate output (can be disabled / high-Z; multiple drivers ok)
// ─────────────────────────────────────────────────────────────────────────────

export type PortDirection = 'in' | 'out' | 'bi' | 'tri'

export interface Port {
  /** Port name on this component, e.g. 'rs1_addr', 'alu_result', 'pc_out' */
  name: string
  direction: PortDirection
  /** Bit width: 1 for control signals, 5 for register addresses, 32 for data */
  bits: number
  /** ID of the component that owns this port */
  componentId: string
}

// ─────────────────────────────────────────────────────────────────────────────
// PortRef — pointer to a specific port on a specific component
// ─────────────────────────────────────────────────────────────────────────────

export interface PortRef {
  componentId: string
  portName: string
}

// ─────────────────────────────────────────────────────────────────────────────
// Net — one wire or bus in the netlist
//   One driver (from), one or more receivers (to[])
// ─────────────────────────────────────────────────────────────────────────────

export interface Net {
  /** Unique net name, e.g. 'pc_out', 'alu_result', 'regwrite_ctrl' */
  id: string
  bits: number
  from: PortRef
  to: PortRef[]
}

// ─────────────────────────────────────────────────────────────────────────────
// BusState — runtime state of a Net during simulation
// ─────────────────────────────────────────────────────────────────────────────

export interface BusState {
  netId: string
  bits: number
  value: number
  state: SignalState
}

// ─────────────────────────────────────────────────────────────────────────────
// ControlSignalState — 1-bit asserted/deasserted line
// ─────────────────────────────────────────────────────────────────────────────

export interface ControlSignalState {
  netId: string
  asserted: boolean
}

// ─────────────────────────────────────────────────────────────────────────────
// ComponentDescriptor — static port list for a component
// ─────────────────────────────────────────────────────────────────────────────

export interface ComponentDescriptor {
  id: string
  label: string
  ports: Port[]
}

// ─────────────────────────────────────────────────────────────────────────────
// Register
// ─────────────────────────────────────────────────────────────────────────────

export interface Register {
  name: string
  xNum: number
  value: number
  active?: boolean
  readA?: boolean
  readB?: boolean
}

// ─────────────────────────────────────────────────────────────────────────────
// Memory word
// ─────────────────────────────────────────────────────────────────────────────

export interface MemoryWord {
  address: number
  value: number
  asm?: string
  active?: boolean
  /** Byte lanes a sub-word access touched (bit i = byte i); unset = whole word */
  lanes?: number
}

// ─────────────────────────────────────────────────────────────────────────────
// Netlist — full static circuit topology
//   Built once at app init, never mutated.
//   Simulation state (BusState[], ControlSignalState[]) lives in Pinia.
// ─────────────────────────────────────────────────────────────────────────────

export interface Netlist {
  components: ComponentDescriptor[]
  nets: Net[]
}
