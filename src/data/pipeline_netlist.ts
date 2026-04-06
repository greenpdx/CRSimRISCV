// ─────────────────────────────────────────────────────────────────────────────
// 5-Stage Pipeline RV32I — Complete Netlist
// Built incrementally: IF → ID → EX → MA → WB
// ─────────────────────────────────────────────────────────────────────────────

import type { Netlist } from '../types/cpu'
import { IF_COMPONENTS, IF_NETS } from './netlist_if'
import { ID_COMPONENTS, ID_NETS } from './netlist_id'
import { EX_COMPONENTS, EX_NETS } from './netlist_ex'
import { MA_COMPONENTS, MA_NETS } from './netlist_ma'
import { WB_COMPONENTS, WB_NETS } from './netlist_wb'

export const PIPELINE_NETLIST: Netlist = {
  components: [
    ...IF_COMPONENTS,
    ...ID_COMPONENTS,
    ...EX_COMPONENTS,
    ...MA_COMPONENTS,
    ...WB_COMPONENTS,
  ],
  nets: [
    ...IF_NETS,
    ...ID_NETS,
    ...EX_NETS,
    ...MA_NETS,
    ...WB_NETS,
  ],
}

// ── Quick lookup helpers ────────────────────────────────────────────────────

export const PIPE_NET_BY_ID = new Map(
  PIPELINE_NETLIST.nets.map(n => [n.id, n])
)

export const PIPE_COMPONENT_BY_ID = new Map(
  PIPELINE_NETLIST.components.map(c => [c.id, c])
)
