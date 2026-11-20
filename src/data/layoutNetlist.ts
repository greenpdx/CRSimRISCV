// ─────────────────────────────────────────────────────────────────────────────
// Layout netlist — every drawn line in the datapath panel and what it connects
//
// Extracted from MulticycleLayout.vue (identical to CpuLayout.vue at base0).
// Unlike netlist.ts (logical topology used by the simulator), this records the
// *drawing*: block positions, each line's waypoints, and which component port
// (or which other line) each end attaches to — enough to recreate the wiring.
//
// Coordinates are layout pixels (the .cpu-layout / SVG origin, 1600×850).
//
// Line ends:
//   { component, port, x, y }  attaches to a component's port
//   { tap, x, y }              branches off another line (tap = that line's id)
//   { junction: true, x, y }   ends at a junction that other lines tap
//
// Expression fields (value, active, phaseExpr, showIf) are the Vue template
// bindings as source text, evaluated in the layout component's scope.
// ─────────────────────────────────────────────────────────────────────────────

export type LayoutComponentId =
  | 'rom' | 'ifpc' | 'pc' | 'mux_pc' | 'pc_adder' | 'mux_pc_offset'
  | 'decoder' | 'imm_gen' | 'alu_ctrl' | 'regfile' | 'csr'
  | 'mux_alusrc' | 'alu' | 'mux_wb' | 'loadstore' | 'ram' | 'io'

export interface LayoutComponent {
  id: LayoutComponentId
  label: string
  /** Vue component rendered in the block */
  vue: string
  /** Top-left position, as passed to px(x, y) */
  x: number
  y: number
  /** Rendered size in px (measured in Chromium; content-sized blocks may vary) */
  w: number
  h: number
  /** Template v-if condition, when the block is optional */
  showIf?: string
}

export type LineEnd =
  | { component: LayoutComponentId; port: string; x: number; y: number }
  | { tap: string; x: number; y: number }
  | { junction: true; x: number; y: number }

export interface LayoutLine {
  /** net-id in the template */
  id: string
  kind: 'bus' | 'control'
  bits: number
  /** Template comment describing the line */
  note?: string
  from: LineEnd
  to: LineEnd
  /** Waypoints, first = from end, last = to end */
  points: [number, number][]
  /** Bus value binding (buses only) */
  value?: string
  /** Highlight binding: bus :state or control :asserted */
  active: string
  /** Fixed animation phase … */
  phase?: string
  /** … or a phase computed at runtime */
  phaseExpr?: string
  showLabel: boolean
  /** Template v-if condition, when the line is optional */
  showIf?: string
}

// ── Components ──────────────────────────────────────────────────────────────

export const LAYOUT_COMPONENTS: LayoutComponent[] = [
  { id: 'rom',           label: 'Instruction Memory', vue: 'RomBlock',       x: 0,    y: 20,  w: 288, h: 406 },
  { id: 'ifpc',          label: 'IF / PC',            vue: 'IfPcBox',        x: 296,  y: 20,  w: 95,  h: 93  },
  { id: 'pc',            label: 'PC',                 vue: 'PcRegister',     x: 467,  y: 148, w: 89,  h: 62  },
  { id: 'mux_pc',        label: 'PC Sel',             vue: 'MuxTrapezoid',   x: 584,  y: 131, w: 113, h: 75  },
  { id: 'pc_adder',      label: 'PC Adder',           vue: 'PcAdder',        x: 753,  y: 129, w: 40,  h: 40  },
  { id: 'mux_pc_offset', label: '+4/Imm',             vue: 'MuxTrapezoid',   x: 831,  y: 111, w: 113, h: 75  },
  { id: 'decoder',       label: 'Decoder / Control',  vue: 'DecoderControl', x: 410,  y: 20,  w: 685, h: 83  },
  { id: 'imm_gen',       label: 'Imm Gen',            vue: 'ImmGenerator',   x: 524,  y: 255, w: 109, h: 52  },
  { id: 'alu_ctrl',      label: 'ALU Control',        vue: 'AluControlUnit', x: 913,  y: 403, w: 100, h: 70  },
  { id: 'regfile',       label: 'Register File',      vue: 'RegisterFile',   x: 368,  y: 353, w: 260, h: 293 },
  { id: 'csr',           label: 'CSR Registers',      vue: 'CsrUnit',        x: 1103, y: 20,  w: 156, h: 144, showIf: 'config.enableCsr' },
  { id: 'mux_alusrc',    label: 'ALU Src',            vue: 'MuxTrapezoid',   x: 688,  y: 528, w: 113, h: 75  },
  { id: 'alu',           label: 'ALU',                vue: 'AluUnit',        x: 886,  y: 485, w: 160, h: 120 },
  { id: 'mux_wb',        label: 'WB Sel',             vue: 'MuxTrapezoid',   x: 1009, y: 288, w: 113, h: 98  }, // h 120 with CSR input
  { id: 'loadstore',     label: 'Load / Store',       vue: 'LoadStore',      x: 1280, y: 540, w: 139, h: 90  },
  { id: 'ram',           label: 'Data Memory',        vue: 'RamBlock',       x: 1280, y: 20,  w: 187, h: 388 },
  { id: 'io',            label: 'I/O',                vue: 'IoBlock',        x: 1280, y: 434, w: 200, h: 95  },
]

// ── Lines ───────────────────────────────────────────────────────────────────

export const LAYOUT_LINES: LayoutLine[] = [
  {
    id: 'ctrl_reg_write', kind: 'control', bits: 1,
    note: 'regWrite → Register File',
    from: { component: 'decoder', port: 'reg_write', x: 444, y: 105 },
    to:   { component: 'regfile', port: 'reg_write', x: 444, y: 353 },
    points: [[444, 105], [444, 353]],
    active: "cpu.controlStates['ctrl_reg_write']?.asserted ?? false",
    phaseExpr: 'wbPhase', showLabel: true,
  },
  {
    id: 'ctrl_imm_sel', kind: 'control', bits: 3,
    note: 'immSel → Imm Generator: only when aluSrc=imm',
    from: { component: 'decoder', port: 'imm_sel', x: 569, y: 105 },
    to:   { component: 'imm_gen', port: 'imm_sel', x: 569, y: 255 },
    points: [[569, 105], [569, 255]],
    active: "busVal('ctrl_imm_sel') !== 5",
    phaseExpr: 'immChainPhase', showLabel: true,
  },
  {
    id: 'ctrl_is_jalr', kind: 'control', bits: 1,
    note: 'ctrl_is_jalr → PC Sel MUX',
    from: { component: 'decoder', port: 'is_jalr', x: 640, y: 105 },
    to:   { component: 'mux_pc', port: 'sel', x: 640, y: 131 },
    points: [[640, 105], [640, 131]],
    active: "cpu.controlStates['ctrl_is_jalr']?.asserted ?? false",
    phase: 'pcupdate', showLabel: true,
  },
  {
    id: 'ctrl_alu_src', kind: 'control', bits: 1,
    note: 'aluSrc → ALU Src MUX',
    from: { component: 'decoder', port: 'alu_src', x: 744, y: 105 },
    to:   { component: 'mux_alusrc', port: 'sel', x: 744, y: 528 },
    points: [[744, 105], [744, 528]],
    active: "cpu.controlStates['ctrl_alu_src']?.asserted ?? false",
    phase: 'regread', showLabel: true,
  },
  {
    id: 'ctrl_offset_sel', kind: 'control', bits: 1,
    note: 'ctrl_offset_sel → +4/Imm MUX',
    from: { component: 'decoder', port: 'offset_sel', x: 887, y: 105 },
    to:   { component: 'mux_pc_offset', port: 'sel', x: 887, y: 111 },
    points: [[887, 105], [887, 111]],
    active: "cpu.controlStates['ctrl_offset_sel']?.asserted ?? false",
    phase: 'pcupdate', showLabel: true,
  },
  {
    id: 'ctrl_alu_op', kind: 'control', bits: 2,
    note: 'aluOp → ALU Control',
    from: { component: 'decoder', port: 'alu_op', x: 963, y: 105 },
    to:   { component: 'alu_ctrl', port: 'alu_op', x: 963, y: 403 },
    points: [[963, 105], [963, 403]],
    active: "busVal('ctrl_alu_op') !== 0",
    phase: 'execute', showLabel: true,
  },
  {
    id: 'ctrl_wb_sel', kind: 'control', bits: 2,
    note: 'wbSel → WB Sel MUX',
    from: { component: 'decoder', port: 'wb_sel', x: 1065, y: 105 },
    to:   { component: 'mux_wb', port: 'sel', x: 1065, y: 288 },
    points: [[1065, 105], [1065, 288]],
    active: "busVal('ctrl_wb_sel') !== 0",
    phaseExpr: 'wbPhase', showLabel: true,
  },
  {
    id: 'pc_out', kind: 'bus', bits: 32,
    note: 'PC addr → ROM: straight left from PC into the side of instruction memory',
    from: { component: 'pc', port: 'pc_out', x: 467, y: 168 },
    to:   { component: 'rom', port: 'addr', x: 288, y: 168 },
    points: [[467, 168], [288, 168]],
    value: 'cpu.pc', active: "busActive('pc_out') ? 'active' : 'idle'",
    phase: 'fetch', showLabel: true,
  },
  {
    id: 'next_pc', kind: 'bus', bits: 32,
    note: 'PC Sel (next_pc) → PC: straight from mux output to PC right side',
    from: { component: 'mux_pc', port: 'out', x: 584, y: 168 },
    to:   { component: 'pc', port: 'next_pc', x: 557, y: 168 },
    points: [[584, 168], [557, 168]],
    value: 'cpu.nextPc', active: "busActive('next_pc') ? 'active' : 'idle'",
    phase: 'pcupdate', showLabel: true,
  },
  {
    id: 'pc_out_adder', kind: 'bus', bits: 32,
    note: 'PC → Adder: down from PC bottom, right along bus, up into adder bottom',
    from: { tap: 'pc_out', x: 427, y: 168 },
    to:   { component: 'pc_adder', port: 'pc_in', x: 773, y: 169 },
    points: [[427, 168], [427, 244], [773, 244], [773, 169]],
    value: 'cpu.pc', active: "busActive('pc_out') ? 'active' : 'idle'",
    phase: 'pcupdate', showLabel: false,
  },
  {
    id: 'pc_adder_result', kind: 'bus', bits: 32,
    note: 'Adder output → MUX PC Sel input',
    from: { component: 'pc_adder', port: 'result', x: 754, y: 150 },
    to:   { component: 'mux_pc', port: 'in0', x: 697, y: 150 },
    points: [[754, 150], [697, 150]],
    value: "busVal('pc_adder_result')", active: "busActive('pc_adder_result') ? 'active' : 'idle'",
    phase: 'pcupdate', showLabel: false,
  },
  {
    id: 'instr_immgen', kind: 'bus', bits: 32,
    note: 'Instruction bus branch: out of IF/PC box bottom, down to ImmGen (only when aluSrc=imm)',
    from: { component: 'ifpc', port: 'instr_out', x: 380, y: 113 },
    to:   { component: 'imm_gen', port: 'instr', x: 522, y: 287 },
    points: [[380, 113], [380, 287], [522, 287]],
    value: "busVal('instr')", active: "busActive('instr') ? 'active' : 'idle'",
    phaseExpr: 'immChainPhase', showLabel: false,
  },
  {
    id: 'immediate', kind: 'bus', bits: 32,
    note: 'ImmGen output trunk (only when aluSrc=imm)',
    from: { component: 'imm_gen', port: 'imm', x: 632, y: 287 },
    to:   { junction: true, x: 675, y: 287 },
    points: [[632, 287], [675, 287]],
    value: "busVal('immediate')", active: "busActive('immediate') ? 'active' : 'idle'",
    phaseExpr: 'immChainPhase', showLabel: true,
  },
  {
    id: 'immediate_alusrc', kind: 'bus', bits: 32,
    note: 'ImmGen → ALU Src mux (imm input): only when aluSrc=imm',
    from: { tap: 'immediate', x: 675, y: 287 },
    to:   { component: 'mux_alusrc', port: 'in1', x: 688, y: 574 },
    points: [[675, 287], [675, 574], [688, 574]],
    value: "busVal('immediate')", active: "busActive('immediate') ? 'active' : 'idle'",
    phaseExpr: "aluSrcSel === 1 ? 'regread' : 'idle'", showLabel: false,
  },
  {
    id: 'immediate_pcoffset', kind: 'bus', bits: 32,
    note: 'ImmGen → +4/Imm mux (imm input): right, up, short left into mux',
    from: { tap: 'immediate', x: 675, y: 287 },
    to:   { component: 'mux_pc_offset', port: 'in1', x: 943, y: 160 },
    points: [[675, 287], [953, 287], [953, 160], [943, 160]],
    value: "busVal('immediate')", active: "busActive('immediate') ? 'active' : 'idle'",
    phase: 'pcupdate', showLabel: false,
  },
  {
    id: 'alu_control', kind: 'bus', bits: 4,
    note: 'ALU Control → ALU',
    from: { component: 'alu_ctrl', port: 'ctrl', x: 960, y: 473 },
    to:   { component: 'alu', port: 'alu_control', x: 960, y: 495 },
    points: [[960, 473], [960, 495]],
    value: "busVal('alu_control')", active: "busActive('alu_control') ? 'active' : 'idle'",
    phase: 'execute', showLabel: false,
  },
  {
    id: 'alu_b', kind: 'bus', bits: 32,
    note: 'ALU Src output → ALU B input',
    from: { component: 'mux_alusrc', port: 'out', x: 800, y: 565 },
    to:   { component: 'alu', port: 'b', x: 886, y: 565 },
    points: [[800, 565], [886, 565]],
    value: "busVal('alu_b')", active: "busActive('alu_b') ? 'active' : 'idle'",
    phase: 'regread', showLabel: false,
  },
  {
    id: 'write_data', kind: 'bus', bits: 32,
    note: 'WB Sel → RegFile write data',
    from: { component: 'mux_wb', port: 'out', x: 1009, y: 337 },
    to:   { component: 'regfile', port: 'write_data', x: 520, y: 353 },
    points: [[1009, 337], [520, 337], [520, 353]],
    value: "busVal('write_data')", active: "busActive('write_data') ? 'active' : 'idle'",
    phaseExpr: 'wbPhase', showLabel: false,
  },
  {
    id: 'alu_result_horiz', kind: 'bus', bits: 32,
    note: 'ALU result: horizontal from ALU to corner',
    from: { component: 'alu', port: 'result', x: 1046, y: 555 },
    to:   { junction: true, x: 1170, y: 555 },
    points: [[1046, 555], [1170, 555]],
    value: "busVal('alu_result')", active: "busActive('alu_result') ? 'active' : 'idle'",
    phaseExpr: 'aluResultTrunkPhase', showLabel: false,
  },
  {
    id: 'alu_result_vert_lower', kind: 'bus', bits: 32,
    note: 'ALU result: vertical lower (corner to WB/LS junction)',
    from: { tap: 'alu_result_horiz', x: 1170, y: 545 },
    to:   { junction: true, x: 1170, y: 310 },
    points: [[1170, 545], [1170, 310]],
    value: "busVal('alu_result')", active: "busActive('alu_result') ? 'active' : 'idle'",
    phaseExpr: 'aluResultTrunkPhase', showLabel: false,
  },
  {
    id: 'alu_result_to_wb', kind: 'bus', bits: 32,
    note: 'ALU result: junction → WB Sel input 0',
    from: { tap: 'alu_result_vert_lower', x: 1170, y: 310 },
    to:   { component: 'mux_wb', port: 'in0', x: 1121, y: 310 },
    points: [[1170, 310], [1121, 310]],
    value: "busVal('alu_result')", active: "busActive('alu_result') ? 'active' : 'idle'",
    phase: 'execute', showLabel: false,
  },
  {
    id: 'alu_result_vert_upper', kind: 'bus', bits: 32,
    note: 'ALU result: vertical upper (junction to JALR tap)',
    from: { tap: 'alu_result_vert_lower', x: 1170, y: 310 },
    to:   { junction: true, x: 1170, y: 198 },
    points: [[1170, 310], [1170, 198]],
    value: "busVal('alu_result')", active: "busActive('alu_result') ? 'active' : 'idle'",
    phaseExpr: 'jalrPhase', showLabel: false,
  },
  {
    id: 'jalr_target', kind: 'bus', bits: 32,
    note: 'JALR target → PC Sel mux input 1',
    from: { tap: 'alu_result_vert_upper', x: 1170, y: 198 },
    to:   { component: 'mux_pc', port: 'in1', x: 696, y: 198 },
    points: [[1170, 198], [696, 198]],
    value: "busVal('jalr_target')", active: "busActive('jalr_target') ? 'active' : 'idle'",
    phaseExpr: 'jalrPhase', showLabel: false,
  },
  {
    id: 'mem_read_data', kind: 'bus', bits: 32,
    note: 'LoadStore load data → WB Sel input 1',
    from: { component: 'loadstore', port: 'load_data', x: 1280, y: 585 },
    to:   { component: 'mux_wb', port: 'in1', x: 1121, y: 330 },
    points: [[1280, 585], [1210, 585], [1210, 330], [1121, 330]],
    value: "busVal('mem_read_data')", active: "busActive('mem_read_data') ? 'active' : 'idle'",
    phaseExpr: 'memPhase', showLabel: false,
  },
  {
    id: 'pc_adder_result_wb', kind: 'bus', bits: 32,
    note: 'PC Adder result → WB Sel input 2 (PC+4 link for JAL/JALR only)',
    from: { tap: 'pc_adder_result', x: 734, y: 150 },
    to:   { component: 'mux_wb', port: 'in2', x: 1121, y: 350 },
    points: [[734, 150], [734, 260], [1145, 260], [1145, 350], [1121, 350]],
    value: "busVal('pc_adder_result')", active: "busActive('pc_adder_result') ? 'active' : 'idle'",
    phaseExpr: "wbSel === 2 ? 'memwb' : 'idle'", showLabel: false,
  },
  {
    id: 'csr_read_data', kind: 'bus', bits: 32,
    note: 'CSR read data → WB Sel input 3 (only when Zicsr enabled)',
    from: { component: 'csr', port: 'read_data', x: 1180, y: 166 },
    to:   { component: 'mux_wb', port: 'in3', x: 1121, y: 388 },
    points: [[1180, 166], [1180, 388], [1121, 388]],
    value: "busVal('csr_read_data')", active: "busActive('csr_read_data') ? 'active' : 'idle'",
    phaseExpr: "wbSel === 3 ? 'memwb' : 'idle'", showLabel: false,
    showIf: 'config.enableCsr',
  },
  {
    id: 'ctrl_mem_read', kind: 'control', bits: 1,
    note: 'ctrl_mem_read → LoadStore',
    from: { component: 'decoder', port: 'mem_read', x: 1080, y: 20 },
    to:   { component: 'loadstore', port: 'mem_read', x: 1280, y: 545 },
    points: [[1080, 20], [1080, 8], [1150, 8], [1210, 8], [1274, 8], [1274, 545], [1280, 545]],
    active: "cpu.controlStates['ctrl_mem_read']?.asserted ?? false",
    phaseExpr: 'memPhase', showLabel: true,
  },
  {
    id: 'ctrl_mem_write', kind: 'control', bits: 1,
    note: 'ctrl_mem_write → LoadStore',
    from: { component: 'decoder', port: 'mem_write', x: 1088, y: 20 },
    to:   { component: 'loadstore', port: 'mem_write', x: 1280, y: 555 },
    points: [[1088, 20], [1088, 13], [1110, 13], [1140, 13], [1269, 13], [1269, 555], [1280, 555]],
    active: "cpu.controlStates['ctrl_mem_write']?.asserted ?? false",
    phaseExpr: 'memPhase', showLabel: true,
  },
  {
    id: 'alu_result_ls', kind: 'bus', bits: 32,
    note: 'ALU result → LoadStore addr input',
    from: { tap: 'alu_result_vert_lower', x: 1170, y: 310 },
    to:   { component: 'loadstore', port: 'addr', x: 1280, y: 555 },
    points: [[1170, 310], [1170, 555], [1280, 555]],
    value: "busVal('alu_result')", active: "busActive('alu_result') ? 'active' : 'idle'",
    phaseExpr: 'memPhase', showLabel: false,
  },
  {
    id: 'rs2_data_ls', kind: 'bus', bits: 32,
    note: 'rs2 store data → LoadStore store data input',
    from: { tap: 'rs2_data', x: 650, y: 548 },
    to:   { component: 'loadstore', port: 'store_data', x: 1280, y: 620 },
    points: [[650, 548], [650, 620], [1280, 620]],
    value: "busVal('rs2_data')", active: "busActive('rs2_data') ? 'active' : 'idle'",
    phaseExpr: 'memPhase', showLabel: false,
  },
  {
    id: 'ls_addr_ram', kind: 'bus', bits: 32,
    note: 'LoadStore addr → RAM (only for RAM addresses)',
    from: { component: 'loadstore', port: 'mem_addr', x: 1420, y: 620 },
    to:   { component: 'ram', port: 'addr', x: 1450, y: 75 },
    points: [[1420, 620], [1526, 620], [1526, 75], [1450, 75]],
    value: "busVal('alu_result')", active: "busActive('alu_result') ? 'active' : 'idle'",
    phaseExpr: 'ramPhase', showLabel: false,
  },
  {
    id: 'ls_data_ram', kind: 'bus', bits: 32,
    note: 'LoadStore data → RAM (only for RAM addresses)',
    from: { component: 'loadstore', port: 'mem_data', x: 1420, y: 600 },
    to:   { component: 'ram', port: 'data', x: 1450, y: 85 },
    points: [[1420, 600], [1516, 600], [1516, 85], [1450, 85]],
    value: "busVal('rs2_data')", active: "busActive('rs2_data') ? 'active' : 'idle'",
    phaseExpr: 'ramPhase', showLabel: false,
  },
  {
    id: 'ls_addr_io', kind: 'bus', bits: 32,
    note: 'LoadStore addr → I/O (only for I/O addresses)',
    from: { component: 'loadstore', port: 'io_addr', x: 1420, y: 570 },
    to:   { component: 'io', port: 'addr', x: 1470, y: 450 },
    points: [[1420, 570], [1506, 570], [1506, 450], [1470, 450]],
    value: "busVal('alu_result')", active: "busActive('alu_result') ? 'active' : 'idle'",
    phaseExpr: 'ioPhase', showLabel: false,
  },
  {
    id: 'ls_data_io', kind: 'bus', bits: 32,
    note: 'LoadStore data → I/O (only for I/O addresses)',
    from: { component: 'loadstore', port: 'io_data', x: 1420, y: 555 },
    to:   { component: 'io', port: 'data', x: 1470, y: 460 },
    points: [[1420, 555], [1496, 555], [1496, 460], [1470, 460]],
    value: "busVal('rs2_data')", active: "busActive('rs2_data') ? 'active' : 'idle'",
    phaseExpr: 'ioPhase', showLabel: false,
  },
  {
    id: 'rs1_data', kind: 'bus', bits: 32,
    note: 'RegFile rs1 → ALU A input',
    from: { component: 'regfile', port: 'rs1_data', x: 630, y: 515 },
    to:   { component: 'alu', port: 'a', x: 886, y: 515 },
    points: [[630, 515], [886, 515]],
    value: "busVal('rs1_data')", active: "busActive('rs1_data') ? 'active' : 'idle'",
    phase: 'regread', showLabel: false,
  },
  {
    id: 'rs2_data', kind: 'bus', bits: 32,
    note: 'RegFile rs2 → ALU Src mux: only when aluSrc=rs2',
    from: { component: 'regfile', port: 'rs2_data', x: 630, y: 548 },
    to:   { component: 'mux_alusrc', port: 'in0', x: 688, y: 548 },
    points: [[630, 548], [688, 548]],
    value: "busVal('rs2_data')", active: "busActive('rs2_data') ? 'active' : 'idle'",
    phaseExpr: "aluSrcSel === 0 ? 'regread' : 'idle'", showLabel: false,
  },
  {
    id: 'instr_aluctrl', kind: 'bus', bits: 32,
    note: 'Instruction bus branch: down to ALU Control',
    from: { tap: 'instr_immgen', x: 380, y: 287 },
    to:   { component: 'alu_ctrl', port: 'instr', x: 930, y: 403 },
    points: [[380, 287], [380, 313], [930, 313], [930, 403]],
    value: "busVal('instr')", active: "busActive('instr') ? 'active' : 'idle'",
    phase: 'regread', showLabel: false,
  },
  {
    id: 'instr_rom_ifpc', kind: 'bus', bits: 32,
    note: 'Instruction bus: ROM → IF/PC box',
    from: { component: 'rom', port: 'instr', x: 288, y: 40 },
    to:   { component: 'ifpc', port: 'instr_in', x: 296, y: 40 },
    points: [[288, 40], [296, 40]],
    value: "busVal('instr')", active: "busActive('instr') ? 'active' : 'idle'",
    phase: 'fetch', showLabel: false,
  },
  {
    id: 'instr', kind: 'bus', bits: 32,
    note: 'Instruction bus: IF/PC box → Decoder',
    from: { component: 'ifpc', port: 'instr_out', x: 391, y: 40 },
    to:   { component: 'decoder', port: 'instr', x: 410, y: 40 },
    points: [[391, 40], [410, 40]],
    value: "busVal('instr')", active: "busActive('instr') ? 'active' : 'idle'",
    phase: 'fetch', showLabel: true,
  },
  {
    id: 'pc_offset', kind: 'bus', bits: 32,
    note: '+4/Imm MUX output → Adder right input',
    from: { component: 'mux_pc_offset', port: 'out', x: 832, y: 150 },
    to:   { component: 'pc_adder', port: 'offset', x: 794, y: 150 },
    points: [[832, 150], [794, 150]],
    value: "busVal('pc_offset')", active: "busActive('pc_offset') ? 'active' : 'idle'",
    phase: 'pcupdate', showLabel: false,
  },
]

// ── Lookups ─────────────────────────────────────────────────────────────────

export const LAYOUT_COMPONENT_BY_ID: Record<LayoutComponentId, LayoutComponent> =
  Object.fromEntries(LAYOUT_COMPONENTS.map(c => [c.id, c])) as Record<LayoutComponentId, LayoutComponent>

export const LAYOUT_LINE_BY_ID: Record<string, LayoutLine> =
  Object.fromEntries(LAYOUT_LINES.map(l => [l.id, l]))

/** Lines whose from or to end attaches directly to the given component */
export function linesOf(id: LayoutComponentId): LayoutLine[] {
  const on = (e: LineEnd) => 'component' in e && e.component === id
  return LAYOUT_LINES.filter(l => on(l.from) || on(l.to))
}
