<template>
  <!-- height = canvas + the fixed clock bar, so the page scrolls only when it does not fit -->
  <div class="cpu-layout" :style="{ height: H + clockBarH + 'px' }">

    <!-- ── SVG wiring layer ── -->
    <svg class="cpu-wires" :width="W" :height="H">
      <!--
        Layout: memories down the left (ROM / I/O / RAM), register file on the
        right, decoder across the middle. Fetch and next-PC sit above the
        decoder (their controls go up), execute below it (controls go down).
        Channels: x 380/395 left of the decoder (instr, imm), x 1110–1150
        right of it (ALU result, PC+4, CSR → WB Sel).
      -->

      <!-- ── Control lines: up from the decoder top ── -->

      <!-- regWrite → Register File (left side) -->
      <ControlLine
        net-id="ctrl_reg_write"
        :points="[{ x: DEC_PORT_X.reg_write!, y: DEC_T }, { x: DEC_PORT_X.reg_write!, y: RF_PORT.we }, { x: RF_X, y: RF_PORT.we }]"
        :asserted="cpu.controlStates['ctrl_reg_write']?.asserted ?? false"
        :show-label="true"
        :phase="wbPhase"
        />

      <!-- immSel → Imm Generator (bottom) -->
      <ControlLine
        net-id="ctrl_imm_sel"
        :points="[{ x: DEC_PORT_X.imm_sel!, y: DEC_T }, { x: DEC_PORT_X.imm_sel!, y: DEC_T - 12 }, { x: IMM_X + 45, y: DEC_T - 12 }, { x: IMM_X + 45, y: IMM_Y + 52 }]"
        :asserted="busVal('ctrl_imm_sel') !== 5"
        :show-label="true"
        :phase="immChainPhase"
        />

      <!-- ctrl_is_jalr → PC Sel MUX (bottom) -->
      <ControlLine
        net-id="ctrl_is_jalr"
        :points="[{ x: DEC_PORT_X.is_jalr!, y: DEC_T }, { x: DEC_PORT_X.is_jalr!, y: pcsY + (config.enableCsr ? 118 : 74) }]"
        :asserted="cpu.controlStates['ctrl_is_jalr']?.asserted ?? false"
        :show-label="true"
        phase="pcupdate"
        />

      <!-- ctrl_offset_sel → +4/Imm MUX (bottom) -->
      <ControlLine
        net-id="ctrl_offset_sel"
        :points="[{ x: DEC_PORT_X.offset_sel!, y: DEC_T }, { x: 887, y: OFS_Y + 72 }]"
        :asserted="cpu.controlStates['ctrl_offset_sel']?.asserted ?? false"
        :show-label="true"
        phase="pcupdate"
        />

      <!-- ── Control lines: down from the decoder bottom ── -->

      <!-- aluA → ALU A MUX (PC for AUIPC) -->
      <ControlLine
        net-id="ctrl_alu_a"
        label-text="alu_a"
        :points="[{ x: DEC_PORT_X.alu_a!, y: DEC_B }, { x: AMX + 56, y: AM_Y }]"
        :label-at="{ x: AMX + 56, y: AM_Y - 14 }"
        :asserted="isAuipc"
        :show-label="true"
        phase="regread"
        />

      <!-- aluSrc → ALU Src MUX: down beside the ALU A mux, in between the two -->
      <ControlLine
        net-id="ctrl_alu_src"
        :points="[{ x: DEC_PORT_X.alu_src!, y: DEC_B }, { x: DEC_PORT_X.alu_src!, y: AS_Y - 5 }, { x: AMX + 56, y: AS_Y - 5 }, { x: AMX + 56, y: AS_Y }]"
        :label-at="{ x: DEC_PORT_X.alu_src!, y: 520 }"
        :asserted="cpu.controlStates['ctrl_alu_src']?.asserted ?? false"
        :show-label="true"
        phase="regread"
        />

      <!-- aluOp → ALU Control -->
      <ControlLine
        net-id="ctrl_alu_op"
        :points="[{ x: DEC_PORT_X.alu_op!, y: DEC_B }, { x: 963, y: 476 }]"
        :asserted="busVal('ctrl_alu_op') !== 0"
        :show-label="true"
        phase="execute"
        />

      <!-- wbSel → WB Sel MUX (top) -->
      <ControlLine
        net-id="ctrl_wb_sel"
        :points="[{ x: DEC_PORT_X.wb_sel!, y: DEC_B }, { x: DEC_PORT_X.wb_sel!, y: 437 }, { x: 1226, y: 437 }, { x: 1226, y: WB_Y + 8 }]"
        :asserted="busVal('ctrl_wb_sel') !== 0"
        :show-label="true"
        :phase="wbPhase"
        />

      <!-- ctrl_mem_read → Load / Store (top) -->
      <ControlLine
        net-id="ctrl_mem_read"
        :points="[{ x: DEC_PORT_X.mem_read!, y: DEC_B }, { x: DEC_PORT_X.mem_read!, y: LS_Y - 26 }, { x: 345, y: LS_Y - 26 }, { x: 345, y: LS_Y }]"
        :label-at="{ x: 412, y: LS_Y - 26 }"
        :asserted="cpu.controlStates['ctrl_mem_read']?.asserted ?? false"
        :show-label="true"
        :phase="memPhase"
        />

      <!-- ctrl_mem_write → Load / Store (top) -->
      <ControlLine
        net-id="ctrl_mem_write"
        :points="[{ x: DEC_PORT_X.mem_write!, y: DEC_B }, { x: DEC_PORT_X.mem_write!, y: DEC_B + 34 }, { x: 650, y: DEC_B + 34 }, { x: 650, y: LS_Y - 10 }, { x: 365, y: LS_Y - 10 }, { x: 365, y: LS_Y }]"
        :label-at="{ x: 452, y: LS_Y - 10 }"
        :asserted="cpu.controlStates['ctrl_mem_write']?.asserted ?? false"
        :show-label="true"
        :phase="memPhase"
        />

      <!-- ── Fetch / next-PC (top band) ── -->

      <!-- PC addr → ROM: straight left into the side of instruction memory -->
      <BusLine
        net-id="pc_out"
        :points="[{ x: 467, y: PC_OUT }, { x: MEM_X + 288, y: PC_OUT }]"
        :bits="32"
        :value="cpu.pc"
        :state="busActive('pc_out') ? 'active' : 'idle'"
        :show-label="true"
        phase="fetch"
        />

      <!-- PC Sel (next_pc) → PC -->
      <BusLine
        net-id="next_pc"
        :points="[{ x: 584, y: PC_OUT }, { x: 557, y: PC_OUT }]"
        :bits="32"
        :value="cpu.nextPc"
        :state="busActive('next_pc') ? 'active' : 'idle'"
        :show-label="true"
        phase="pcupdate"
        />

      <!-- PC → Adder -->
      <BusLine
        net-id="pc_out_adder"
        :points="[{ x: 427, y: PC_OUT }, { x: 427, y: PCFB_Y }, { x: 773, y: PCFB_Y }, { x: 773, y: ADD_Y + 40 }]"
        :bits="32"
        :value="cpu.pc"
        :state="busActive('pc_out') ? 'active' : 'idle'"
        :show-label="false"
        phase="pcupdate"
        />

      <!-- Adder output → PC Sel input 0 -->
      <BusLine
        net-id="pc_adder_result"
        :points="[{ x: 754, y: ADD_Y + 21 }, { x: 716, y: ADD_Y + 21 }, { x: 716, y: pcIn(0) }, { x: 697, y: pcIn(0) }]"
        :bits="32"
        :value="busVal('pc_adder_result')"
        :state="busActive('pc_adder_result') ? 'active' : 'idle'"
        :show-label="false"
        phase="pcupdate"
        />

      <!-- +4/Imm MUX output → Adder -->
      <BusLine
        net-id="pc_offset"
        :points="[{ x: 832, y: ADD_Y + 21 }, { x: 794, y: ADD_Y + 21 }]"
        :bits="32"
        :value="busVal('pc_offset')"
        :state="busActive('pc_offset') ? 'active' : 'idle'"
        :show-label="false"
        phase="pcupdate"
        />

      <!-- Trap: mtvec → PC Sel input 2; MRET: mepc → input 3 (Zicsr) -->
      <template v-if="config.enableCsr">
        <BusLine
          net-id="mtvec"
          :points="[{ x: CSR_X, y: pcIn(2) }, { x: 697, y: pcIn(2) }]"
          :bits="32"
          :value="cpu.csrRegisters[0x305] ?? 0"
          :state="pcSel === 2 ? 'active' : 'idle'"
          :show-label="false"
          :phase="trapPhase"
          />
        <BusLine
          net-id="mepc"
          :points="[{ x: CSR_X, y: CSR_Y + CSR_ROW.mepc }, { x: 708, y: CSR_Y + CSR_ROW.mepc }, { x: 708, y: pcIn(3) }, { x: 697, y: pcIn(3) }]"
          :bits="32"
          :value="cpu.csrRegisters[0x341] ?? 0"
          :state="pcSel === 3 ? 'active' : 'idle'"
          :show-label="false"
          :phase="mretPhase"
          />
      </template>

      <!-- ── Instruction ── -->

      <!-- ROM → IF/PC box -->
      <BusLine
        net-id="instr_rom_ifpc"
        :points="[{ x: MEM_X + 288, y: IFPC_Y + 20 }, { x: 296, y: IFPC_Y + 20 }]"
        :bits="32"
        :value="busVal('instr')"
        :state="busActive('instr') ? 'active' : 'idle'"
        :show-label="false"
        phase="fetch"
        />

      <!-- IF/PC box → down the left channel → Decoder -->
      <BusLine
        net-id="instr"
        :points="[{ x: 380, y: IFPC_Y + 78 }, { x: 380, y: DEC_T + 46 }, { x: DEC_X, y: DEC_T + 46 }]"
        :bits="32"
        :value="busVal('instr')"
        :state="busActive('instr') ? 'active' : 'idle'"
        :show-label="true"
        phase="fetch"
        />

      <!-- Instruction → Imm Gen: a branch of the IF output -->
      <BusLine
        net-id="instr_immgen"
        :points="[{ x: 380, y: IMM_Y + 32 }, { x: IMM_X - 2, y: IMM_Y + 32 }]"
        :bits="32"
        :value="busVal('instr')"
        :state="busActive('instr') ? 'active' : 'idle'"
        :show-label="false"
        :phase="immChainPhase"
        />

      <!-- Instruction → ALU Control -->
      <BusLine
        net-id="instr_aluctrl"
        :points="[{ x: 380, y: DEC_T + 46 }, { x: 380, y: DEC_B + 16 }, { x: 930, y: DEC_B + 16 }, { x: 930, y: 476 }]"
        :bits="32"
        :value="busVal('instr')"
        :state="busActive('instr') ? 'active' : 'idle'"
        :show-label="false"
        phase="regread"
        />

      <!-- ── Immediate ── -->

      <BusLine
        net-id="immediate"
        :points="[{ x: IMM_X + 108, y: IMM_Y + 32 }, { x: IMM_TAP, y: IMM_Y + 32 }]"
        :label-at="{ x: IMM_TAP + 70, y: IMM_Y + 32 }"
        :bits="32"
        :value="busVal('immediate')"
        :state="busActive('immediate') ? 'active' : 'idle'"
        :show-label="true"
        :phase="immChainPhase"
        />

      <!-- Imm → +4/Imm mux -->
      <BusLine
        net-id="immediate_pcoffset"
        :points="[{ x: IMM_TAP, y: IMM_Y + 32 }, { x: 953, y: IMM_Y + 32 }, { x: 953, y: OFS_Y + 48 }, { x: 943, y: OFS_Y + 48 }]"
        :bits="32"
        :value="busVal('immediate')"
        :state="busActive('immediate') ? 'active' : 'idle'"
        :show-label="false"
        phase="pcupdate"
        />

      <!-- Imm → down the left channel → ALU Src imm input -->
      <BusLine
        net-id="immediate_alusrc"
        :points="[{ x: IMM_TAP, y: IMM_Y + 32 }, { x: IMM_TAP, y: AS_Y + 48 }, { x: AMX, y: AS_Y + 48 }]"
        :bits="32"
        :value="busVal('immediate')"
        :state="busActive('immediate') ? 'active' : 'idle'"
        :show-label="false"
        :phase="aluSrcSel === 1 ? 'regread' : 'idle'"
        />

      <!-- ── Register reads (register file → execute) ── -->

      <!-- rs1 → ALU A mux input 0 -->
      <BusLine
        net-id="rs1_data"
        :points="[{ x: RF_X, y: RF_PORT.rs1 }, { x: 1290, y: RF_PORT.rs1 }, { x: 1290, y: 466 }, { x: 700, y: 466 }, { x: 700, y: AM_Y + 27 }, { x: AMX, y: AM_Y + 27 }]"
        :bits="32"
        :value="busVal('rs1_data')"
        :state="busActive('rs1_data') ? 'active' : 'idle'"
        :show-label="false"
        phase="regread"
        />

      <!-- rs2 → ALU Src rs2 input -->
      <BusLine
        net-id="rs2_data"
        :points="[{ x: RF_X, y: RF_PORT.rs2 }, { x: 1280, y: RF_PORT.rs2 }, { x: 1280, y: 455 }, { x: 676, y: 455 }, { x: 676, y: AS_Y + 27 }, { x: AMX, y: AS_Y + 27 }]"
        :bits="32"
        :value="busVal('rs2_data')"
        :state="busActive('rs2_data') ? 'active' : 'idle'"
        :show-label="false"
        :phase="aluSrcSel === 0 || isMemOp ? 'regread' : 'idle'"
        />

      <!-- ── Execute ── -->

      <!-- ALU Control → ALU -->
      <BusLine
        net-id="alu_control"
        :points="[{ x: 960, y: 546 }, { x: 960, y: 576 }]"
        :bits="4"
        :value="busVal('alu_control')"
        :state="busActive('alu_control') ? 'active' : 'idle'"
        :show-label="false"
        phase="execute"
        />

      <!-- ALU Src → ALU B -->
      <BusLine
        net-id="alu_b"
        :points="[{ x: AMX + 112, y: AS_Y + 37 }, { x: 848, y: AS_Y + 37 }, { x: 848, y: 646 }, { x: 886, y: 646 }]"
        :bits="32"
        :value="busVal('alu_b')"
        :state="busActive('alu_b') ? 'active' : 'idle'"
        :show-label="false"
        phase="regread"
        />

      <!-- PC output line → down past the decoder's left end → under it → ALU A mux input 1 (AUIPC) -->
      <BusLine
        net-id="pc_alua"
        :points="[{ x: 408, y: PC_OUT }, { x: 408, y: DEC_B + 24 }, { x: 688, y: DEC_B + 24 }, { x: 688, y: AM_Y + 48 }, { x: AMX, y: AM_Y + 48 }]"
        :bits="32"
        :value="cpu.pc"
        :state="isAuipc && busActive('alu_b') ? 'active' : 'idle'"
        :show-label="false"
        color="#0099cc"
        :phase="isAuipc ? 'regread' : 'idle'"
        />

      <!-- ALU A mux → ALU A -->
      <BusLine
        net-id="alu_a"
        :points="[{ x: AMX + 112, y: 596 }, { x: 886, y: 596 }]"
        :bits="32"
        :value="aluA"
        :state="busActive('alu_b') ? 'active' : 'idle'"
        :show-label="false"
        phase="regread"
        />

      <!-- MUL block (Zmmul): ALU A mux output / rs2 taps in, result → WB Sel input 4 -->
      <template v-if="config.enableZmmul">
        <BusLine
          net-id="alu_a_mul"
          :points="[{ x: 866, y: 596 }, { x: 866, y: MUL_Y + 12 }, { x: 886, y: MUL_Y + 12 }]"
          :bits="32"
          :value="aluA"
          :state="isMul && busActive('rs1_data') ? 'active' : 'idle'"
          :show-label="false"
          :phase="isMul ? 'regread' : 'idle'"
          />
        <BusLine
          net-id="rs2_data"
          :points="[{ x: 676, y: AS_Y + 27 }, { x: 676, y: MUL_Y + 34 }, { x: 886, y: MUL_Y + 34 }]"
          :bits="32"
          :value="busVal('rs2_data')"
          :state="isMul && busActive('rs2_data') ? 'active' : 'idle'"
          :show-label="false"
          :phase="isMul ? 'regread' : 'idle'"
          />
        <BusLine
          net-id="mul_result"
          :points="[{ x: 1056, y: MUL_Y + 23 }, { x: 1100, y: MUL_Y + 23 }, { x: 1100, y: wbIn(4) }, { x: WB_X, y: wbIn(4) }]"
          :bits="32"
          :value="busVal('mul_result')"
          :state="busActive('mul_result') ? 'active' : 'idle'"
          :show-label="false"
          :phase="isMul ? 'execute' : 'idle'"
          />
      </template>

      <!-- ALU result → WB Sel input 0 -->
      <BusLine
        net-id="alu_result_horiz"
        :points="[{ x: 1046, y: 636 }, { x: 1110, y: 636 }]"
        :bits="32"
        :value="busVal('alu_result')"
        :state="busActive('alu_result') ? 'active' : 'idle'"
        :show-label="false"
        :phase="aluResultTrunkPhase"
        />
      <BusLine
        net-id="alu_result_to_wb"
        :points="[{ x: 1110, y: 636 }, { x: WB_X, y: wbIn(0) }]"
        :bits="32"
        :value="busVal('alu_result')"
        :state="busActive('alu_result') ? 'active' : 'idle'"
        :show-label="false"
        phase="execute"
        />

      <!-- ALU result → up the right channel → PC Sel JALR input -->
      <BusLine
        net-id="alu_result_vert_upper"
        :points="[{ x: 1110, y: 636 }, { x: 1110, y: JT_Y }]"
        :bits="32"
        :value="busVal('alu_result')"
        :state="busActive('alu_result') ? 'active' : 'idle'"
        :show-label="false"
        :phase="jalrPhase"
        />
      <BusLine
        net-id="jalr_target"
        :points="[{ x: 1110, y: JT_Y }, { x: 720, y: JT_Y }, { x: 720, y: pcIn(1) }, { x: 697, y: pcIn(1) }]"
        :bits="32"
        :value="busVal('jalr_target')"
        :state="busActive('jalr_target') ? 'active' : 'idle'"
        :show-label="false"
        :phase="jalrPhase"
        />

      <!-- ALU result → Load / Store addr -->
      <BusLine
        net-id="alu_result_ls"
        :points="[{ x: 1080, y: 636 }, { x: 1080, y: LS_Y + 45 }, { x: LS_R, y: LS_Y + 45 }]"
        :bits="32"
        :value="busVal('alu_result')"
        :state="busActive('alu_result') ? 'active' : 'idle'"
        :show-label="false"
        :phase="memPhase"
        />

      <!-- rs2 store data → Load / Store -->
      <BusLine
        net-id="rs2_data_ls"
        :points="[{ x: 676, y: AS_Y + 27 }, { x: 676, y: LS_Y + 21 }, { x: LS_R, y: LS_Y + 21 }]"
        :bits="32"
        :value="busVal('rs2_data')"
        :state="busActive('rs2_data') ? 'active' : 'idle'"
        :show-label="false"
        :phase="memPhase"
        />

      <!-- Load data → WB Sel input 1 -->
      <BusLine
        net-id="mem_read_data"
        :points="[{ x: LS_R, y: LS_Y + 69 }, { x: 1150, y: LS_Y + 69 }, { x: 1150, y: wbIn(1) }, { x: WB_X, y: wbIn(1) }]"
        :bits="32"
        :value="busVal('mem_read_data')"
        :state="busActive('mem_read_data') ? 'active' : 'idle'"
        :show-label="false"
        :phase="memPhase"
        />

      <!-- PC+4 → WB Sel input 2 (JAL/JALR link) -->
      <BusLine
        net-id="pc_adder_result_wb"
        :points="[{ x: 734, y: ADD_Y + 21 }, { x: 734, y: P4_Y }, { x: 1140, y: P4_Y }, { x: 1140, y: wbIn(2) }, { x: WB_X, y: wbIn(2) }]"
        :bits="32"
        :value="busVal('pc_adder_result')"
        :state="busActive('pc_adder_result') ? 'active' : 'idle'"
        :show-label="false"
        :phase="wbSel === 2 ? 'memwb' : 'idle'"
        />

      <!-- CSR read data → WB Sel input 3 (Zicsr) -->
      <BusLine
        v-if="config.enableCsr"
        net-id="csr_read_data"
        :points="[{ x: CSR_X + 77, y: CSR_Y + 144 }, { x: CSR_X + 77, y: P4_Y - 12 }, { x: 1125, y: P4_Y - 12 }, { x: 1125, y: wbIn(3) }, { x: WB_X, y: wbIn(3) }]"
        :bits="32"
        :value="busVal('csr_read_data')"
        :state="busActive('csr_read_data') ? 'active' : 'idle'"
        :show-label="false"
        :phase="wbSel === 3 ? 'memwb' : 'idle'"
        />

      <!-- WB Sel → Register File write data -->
      <BusLine
        net-id="write_data"
        :points="[{ x: WB_X + 113, y: wbOutY }, { x: 1300, y: wbOutY }, { x: 1300, y: RF_PORT.wd }, { x: RF_X, y: RF_PORT.wd }]"
        :bits="32"
        :value="busVal('write_data')"
        :state="busActive('write_data') ? 'active' : 'idle'"
        :show-label="false"
        :phase="wbPhase"
        />

      <!-- ── Load / Store → RAM and I/O (left column) ── -->

      <BusLine
        net-id="ls_addr_ram"
        :points="[{ x: LS_X, y: LS_Y + 58 }, { x: MEM_X + 200, y: LS_Y + 58 }]"
        :bits="32"
        :value="busVal('alu_result')"
        :state="busActive('alu_result') ? 'active' : 'idle'"
        :show-label="false"
        :phase="ramPhase"
        />
      <BusLine
        net-id="ls_data_ram"
        :points="[{ x: LS_X, y: LS_Y + 68 }, { x: MEM_X + 200, y: LS_Y + 68 }]"
        :bits="32"
        :value="busVal('rs2_data')"
        :state="busActive('rs2_data') ? 'active' : 'idle'"
        :show-label="false"
        :phase="ramPhase"
        />
      <BusLine
        net-id="ls_addr_io"
        :points="[{ x: LS_X, y: LS_Y + 22 }, { x: 229, y: LS_Y + 22 }, { x: 229, y: IO_Y + IO_H }]"
        :bits="32"
        :value="busVal('alu_result')"
        :state="busActive('alu_result') ? 'active' : 'idle'"
        :show-label="false"
        :phase="ioPhase"
        />
      <BusLine
        net-id="ls_data_io"
        :points="[{ x: LS_X, y: LS_Y + 32 }, { x: 217, y: LS_Y + 32 }, { x: 217, y: IO_Y + IO_H }]"
        :bits="32"
        :value="busVal('rs2_data')"
        :state="busActive('rs2_data') ? 'active' : 'idle'"
        :show-label="false"
        :phase="ioPhase"
        />

      <!-- ── Junction dots (where a wire branches) and wire names, drawn on top ── -->
      <circle v-for="j in junctions" :key="`j${j.x},${j.y}`" :cx="j.x" :cy="j.y" r="4" :fill="j.color ?? '#cc8800'" />
      <text
        v-for="l in wireNames"
        :key="l.text + l.x"
        :x="l.x" :y="l.y"
        :text-anchor="l.anchor ?? 'start'"
        class="bus-name"
        :style="l.color ? { fill: l.color } : undefined"
      >{{ l.text }}</text>

    </svg>

    <!-- ══ Left column: instruction memory / I/O / data memory ══ -->

    <!-- ── Instruction memory (top left) ── -->
    <div class="cpu-block" :style="px(MEM_X, 20)" @mouseover="onRomHover" @mouseleave="onRomLeave">
      <RomBlock
        :words="cpu.romWords"
        :pc="cpu.pc"
        :base="romBase"
      />
    </div>

    <!-- ── I/O (between the memories) ── -->
    <div class="cpu-block cpu-block--io" :style="px(IO_X, IO_Y)">
      <IoBlock
        :base-address="cpu.ioBaseAddress"
        :address="busVal('alu_result')"
        :data="cpu.ioData"
        :op="cpu.ioOp"
        show-offset
      />
    </div>

    <!-- ── Data memory (bottom left) ── -->
    <div class="cpu-block cpu-block--ram" :style="px(MEM_X, RAM_Y)">
      <RamBlock
        :words="cpu.ramWords"
        :last-op="ramOp"
        :last-address="busVal('alu_result')"
        :last-value="ramLastValue"
        :base="ramBase"
      />
    </div>

    <!-- ── Load / Store (beside data memory; memory side faces left) ── -->
    <div class="cpu-block" :style="px(LS_X, LS_Y)">
      <LoadStore
        :op="loadStoreOp"
        :address="busVal('alu_result')"
        :width="lsWidth"
        :load-sign="funct3 & 4 ? 'unsigned' : 'signed'"
        :write-data="busVal('rs2_data')"
        :read-data="busVal('mem_read_data')"
        mirror
      />
    </div>

    <!-- ══ Top band: fetch and next PC ══ -->

    <div class="cpu-block" :style="px(296, IFPC_Y)">
      <IfPcBox
        title="IF"
        :show-pc="false"
        :pc="cpu.pc"
        :instr="fetchWord?.value"
        :asm="currentAsm"
        :active="busActive('pc_out') || busActive('instr')"
      />
    </div>

    <div class="cpu-block" :style="px(467, PC_OUT - 20)">
      <PcRegister
        :pc="cpu.pc"
        :next-pc="cpu.nextPc"
        :active="busActive('pc_out')"
      />
    </div>

    <div class="cpu-block" :style="px(584, pcsY)">
      <MuxTrapezoid
        label="PC Sel"
        output-label="New PC"
        :inputs="muxPcInputs"
        :select="pcSel"
        :active="busActive('next_pc')"
      />
    </div>

    <div class="cpu-block" :style="px(753, ADD_Y)">
      <PcAdder
        :pc="cpu.pc"
        :offset="busVal('pc_offset')"
        :active="busActive('pc_adder_result')"
      />
    </div>

    <div class="cpu-block" :style="px(831, OFS_Y)">
      <MuxTrapezoid
        label="+4/Imm"
        output-label="offset"
        :inputs="muxPcOffsetInputs"
        :select="offsetSel"
        :active="busActive('pc_offset')"
      />
    </div>

    <div class="cpu-block" :style="px(IMM_X, IMM_Y)">
      <ImmGenerator
        :instruction="fetched ? cpu.lastInstr : 0"
        :imm-sel="fetched ? immSel : 5"
        :active="busActive('immediate')"
      />
    </div>

    <div v-if="config.enableCsr" class="cpu-block" :style="px(CSR_X, CSR_Y)">
      <CsrUnit />
    </div>

    <!-- ══ Middle: decoder / control ══ -->
    <div class="cpu-block" :style="{ ...px(DEC_X, DEC_T), width: DEC_W + 'px' }">
      <DecoderControl
        :opcode="fetched ? opcode : 0"
        :active="phases.currentPhase === 'decode'"
        :asm-text="decodedAsm"
        :phase-text="phases.phaseLabel"
        :signals-override="fetched ? decoderOverride : undefined"
        :split="DEC_SPLIT"
        :badge-x="decBadgeX"
        @asm-hover="(e: MouseEvent) => hoverInstrPopup(decodedAsm, e.currentTarget as HTMLElement, 'below')"
        @asm-leave="instrPopup?.leave()"
      />
    </div>

    <!-- ══ Bottom band: execute ══ -->

    <div class="cpu-block" :style="px(913, 476)">
      <AluControlUnit
        :alu-op="aluOp"
        :funct3="funct3"
        :funct7b30="funct7b30"
        :is-lui="isLui"
        :active="busActive('alu_control')"
      />
    </div>

    <div class="cpu-block" :style="px(AMX, AM_Y)">
      <MuxTrapezoid
        label="A Src"
        output-label="ALU A"
        direction="right"
        :inputs="muxAluAInputs"
        :select="isAuipc ? 1 : 0"
        :active="busActive('alu_b')"
      />
    </div>

    <div class="cpu-block" :style="px(AMX, AS_Y)">
      <MuxTrapezoid
        label="ALU Src"
        output-label="ALU B"
        direction="right"
        :inputs="muxAluSrcInputs"
        :select="aluSrcSel"
        :active="busActive('alu_b')"
      />
    </div>

    <div class="cpu-block" :style="px(886, 566)">
      <AluUnit
        :alu-control="busVal('alu_control')"
        :operand-a="aluA"
        :operand-b="busVal('alu_b')"
        :active="busActive('alu_result')"
      />
    </div>

    <div v-if="config.enableZmmul" class="cpu-block" :style="px(886, MUL_Y)">
      <MulBlock
        :a="aluA"
        :b="busVal('rs2_data')"
        :op="funct3 & 3"
        :result="busVal('mul_result')"
        :used="isMul"
        :active="busActive('mul_result')"
      />
    </div>

    <div class="cpu-block" :style="px(WB_X, WB_Y)">
      <MuxTrapezoid
        label="WB Sel"
        output-label="write data"
        direction="right"
        :inputs="muxWbInputs"
        :min-inputs="config.enableZmmul ? 5 : undefined"
        :select="wbSel"
        :active="busActive('write_data')"
      />
    </div>

    <!-- ══ Right column: register file ══ -->
    <div class="cpu-block cpu-block--regs" :style="px(RF_X, RF_Y)">
      <RegisterFile
        :registers="cpu.registers"
        :short-rows="16"
        compact
      />
    </div>

    <!-- ── Clock Controls ── -->
    <div ref="clockBar" class="clock-bar">
      <!-- Views -->
      <button class="clock-btn clock-btn--edit" @click="openEditor">Edit</button>
      <!-- Panel select (Memory and OoO are reached from the Pipeline view) -->
      <div class="clock-menu">
        <button class="clock-btn clock-btn--edit" @click.stop="toggleMenu('views')">Panels ▴</button>
        <div v-if="openMenu === 'views'" class="clock-menu__list">
          <button class="clock-menu__item clock-menu__item--current" disabled>Single-cycle</button>
          <button class="clock-menu__item" @click="emit('switch-microstep-pipeline')">Pipeline</button>
          <button class="clock-menu__item" @click="emit('switch-microstep-barrel')">Barrel</button>
          <button class="clock-menu__item" @click="emit('switch-microstep')">μStep table (single-cycle)</button>
        </div>
      </div>

      <span class="clock-divider">|</span>

      <!-- File menu: load / save / examples -->
      <div class="clock-menu">
        <button class="clock-btn clock-btn--file" @click.stop="toggleMenu('file')">File ▴</button>
        <div v-if="openMenu === 'file'" class="clock-menu__list">
          <button class="clock-menu__item" @click="loadProgram">Load…</button>
          <button class="clock-menu__item" @click="saveProgram">Save</button>
          <div class="clock-menu__head">Examples</div>
          <button
            v-for="ex in examplePrograms"
            :key="ex.name"
            class="clock-menu__item"
            @click="loadExample(ex)"
          >{{ ex.name }}</button>
        </div>
      </div>
      <button class="clock-btn clock-btn--file" @click="showConfig = true">Config</button>

      <span class="clock-divider">|</span>

      <!-- Execution controls -->
      <button class="clock-btn" title="Advance one phase of the instruction" @click="phases.microStep()" :disabled="phases.animating">Phase</button>
      <button class="clock-btn" title="Run to the next instruction" @click="phases.animatedStep()" :disabled="phases.animating">Next</button>
      <button class="clock-btn" @click="phases.toggleAutoRun()">{{ phases.autoRunning ? 'Stop' : 'Run' }}</button>
      <button class="clock-btn clock-btn--reset" @click="phases.reset()">Reset</button>
      <input
        type="range"
        class="clock-speed"
        min="100"
        max="2000"
        step="50"
        v-model.number="phases.animDuration"
        :title="`Animation speed: ${phases.animDuration} ms per phase`"
      />
      <span class="clock-phase">{{ phases.phaseLabel }}</span>
      <span class="clock-label">Cycle: {{ cpu.cycleCount }}</span>

      <!-- Traps (Config → Traps): mode, in-trap flag, interrupt lines -->
      <template v-if="cpu.trapsEnabled">
        <span class="clock-divider">|</span>
        <span class="clock-label clock-tag" :class="'mode-' + cpu.privMode">mode: {{ cpu.privMode }}</span>
        <span v-if="cpu.trapDepth > 0" class="clock-label clock-tag clock-tag--trap">IN_TRAP</span>
        <span
          v-if="irqPending.names"
          class="clock-label clock-tag"
          :class="irqPending.masked ? 'clock-tag--masked' : 'clock-tag--trap'"
          :title="irqPending.masked ? irqPending.why : 'Taken at the start of the next instruction'"
        >irq: {{ irqPending.names }}{{ irqPending.tag }}</span>
        <div class="clock-menu">
          <button class="clock-btn" @click.stop="toggleMenu('irq')">IRQ ▴</button>
          <div v-if="openMenu === 'irq'" class="clock-menu__list clock-menu__list--right">
            <button class="clock-menu__item" @click="cpu.raiseIrq('mti')">MTI — machine timer</button>
            <button class="clock-menu__item" @click="cpu.raiseIrq('mei')">MEI — machine external</button>
            <button class="clock-menu__item" @click="cpu.raiseIrq('sei')">SEI — supervisor external</button>
            <button class="clock-menu__item" @click="cpu.clearIrqs()">Clear IRQs</button>
          </div>
        </div>
      </template>
    </div>

    <!-- Config Modal -->
    <div v-if="showConfig" class="modal-overlay" @click.self="showConfig = false">
      <div class="modal">
        <div class="modal__header">
          <span class="modal__title">Configuration</span>
          <button class="modal__close" @click="showConfig = false">X</button>
        </div>
        <div class="modal__body">
          <div class="modal__section">General</div>
          <div class="modal__row">
            <label class="modal__label">I/O Base Address</label>
            <input class="modal__input" v-model="configIoBase" placeholder="0x40020000" />
          </div>
          <div class="modal__row">
            <label class="modal__label">ROM Size (words)</label>
            <input class="modal__input" type="number" v-model.number="configRomSize" min="4" max="256" />
          </div>
          <div class="modal__row">
            <label class="modal__label">RAM Size (words)</label>
            <input class="modal__input" type="number" v-model.number="configRamSize" min="4" max="256" />
          </div>
          <div class="modal__row">
            <label class="modal__label">Register Mode</label>
            <select class="modal__input" v-model="configRegMode">
              <option value="8">8 registers</option>
              <option value="32">32 registers</option>
            </select>
          </div>

          <div class="modal__section">Datapath</div>
          <div class="modal__row">
            <label class="modal__label">Multicycle</label>
            <label class="modal__toggle modal__toggle--off">
              <input type="checkbox" :checked="false" disabled />
              <span>Not available</span>
            </label>
          </div>
          <div class="modal__row">
            <label class="modal__label">Traps (ECALL / EBREAK / MRET, interrupts)</label>
            <label class="modal__toggle">
              <input type="checkbox" v-model="cpu.trapsEnabled" />
              <span>{{ cpu.trapsEnabled ? 'Enabled' : 'Disabled' }}</span>
            </label>
          </div>

          <div class="modal__section">Extensions</div>
          <div class="modal__row">
            <label class="modal__label">Zicsr (CSR registers){{ cpu.trapsEnabled ? ' — needed by traps' : '' }}</label>
            <label class="modal__toggle" :class="{ 'modal__toggle--off': cpu.trapsEnabled }">
              <input type="checkbox" v-model="config.enableCsr" :disabled="cpu.trapsEnabled" />
              <span>{{ config.enableCsr ? 'Enabled' : 'Disabled' }}</span>
            </label>
          </div>
          <div class="modal__row">
            <label class="modal__label">Zmmul (MUL block, single-cycle)</label>
            <label class="modal__toggle">
              <input type="checkbox" v-model="config.enableZmmul" />
              <span>{{ config.enableZmmul ? 'Enabled' : 'Disabled' }}</span>
            </label>
          </div>
        </div>
        <div class="modal__footer">
          <button class="modal__btn modal__btn--apply" @click="applyConfig">Apply</button>
          <button class="modal__btn" @click="showConfig = false">Cancel</button>
        </div>
      </div>
    </div>

    <!-- Instruction description popup -->
    <InstrPopup ref="instrPopup" />

    <!-- Hidden file input for loading -->
    <input
      ref="fileInput"
      type="file"
      accept=".json,.hex,.txt"
      style="display: none"
      @change="onFileLoad"
    />

  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useCpuState } from '../stores/cpuState'
import { useCpuPhases } from '../stores/cpuPhases'
import { useCpuConfig } from '../stores/cpuConfig'
import type { ImmSel } from '../data/immData'

import RomBlock from './RomBlock.vue'
import RamBlock from './RamBlock.vue'
import RegisterFile from './RegisterFile.vue'
import PcRegister from './PcRegister.vue'
import IfPcBox from './IfPcBox.vue'
import PcAdder from './PcAdder.vue'
import AluUnit from './AluUnit.vue'
import AluControlUnit from './AluControlUnit.vue'
import DecoderControl from './DecoderControl.vue'
import ImmGenerator from './ImmGenerator.vue'
import LoadStore from './LoadStore.vue'
import MulBlock from './MulBlock.vue'
import type { ControlSignals } from '../data/controlData'
import IoBlock from './IoBlock.vue'
import MuxTrapezoid from './MuxTrapezoid.vue'
import CsrUnit from './CsrUnit.vue'
import BusLine from './BusLine.vue'
import ControlLine from './ControlLine.vue'
import InstrPopup from './InstrPopup.vue'

// ── Store ────────────────────────────────────────────────────────────────────

const cpu = useCpuState()

const phases = useCpuPhases()
const config = useCpuConfig()
const instrPopup = ref<InstanceType<typeof InstrPopup> | null>(null)

/** Raised interrupt lines, and why they are not being taken (if so) */
const IRQ_BITS: [number, string][] = [[11, 'MEI'], [3, 'MSI'], [7, 'MTI'], [9, 'SEI'], [1, 'SSI'], [5, 'STI']]
const irqPending = computed(() => {
  void cpu.pc; void cpu.cycleCount; void cpu.csrRegisters
  const p = cpu.pendingIrq
  const names = IRQ_BITS.filter(([b]) => (p >>> b) & 1).map(([, n]) => n).join(' ')
  if (!names) return { names: '', masked: false, why: '', tag: '' }
  const mstatus = cpu.csrRead(0x300), mie = cpu.csrRead(0x304)
  const globalOn = cpu.privMode !== 'M' || ((mstatus >>> 3) & 1) === 1
  const enabled = (mie & p) !== 0
  const inHandler = cpu.trapDepth > 0
  const why = !enabled ? 'Not taken: its enable bit in mie is 0 (the program must set it)'
    : !globalOn ? (inHandler
        ? 'Held off: the CPU cleared mstatus.MIE on entering the handler; mret restores it'
        : 'Not taken: mstatus.MIE is 0 (the program must set it)') : ''
  const tag = !enabled || !globalOn ? (enabled && inHandler ? ' — held off in handler' : ' — masked') : ''
  return { names, masked: !enabled || !globalOn, why, tag }
})

// Traps use mtvec / mepc / mcause: with traps on, the CSR box and PC Sel's
// trap and mret inputs are shown too.
watch(() => cpu.trapsEnabled, on => { if (on) config.enableCsr = true }, { immediate: true })

/** Instruction info on hover: beside (right of) or below the element hovered */
function hoverInstrPopup(asm: string, el: HTMLElement, where: 'right' | 'below') {
  if (!asm || asm === 'nop') { instrPopup.value?.leave(); return }
  const r = el.getBoundingClientRect()
  if (where === 'right') instrPopup.value?.hover(asm.toUpperCase(), r.right + 6, r.top - 4)
  else instrPopup.value?.hover(asm.toUpperCase(), r.left, r.bottom + 4)
}

/** Instruction memory: the row under the pointer */
let romHoverRow: Element | null = null
function onRomHover(event: MouseEvent) {
  const row = (event.target as HTMLElement).closest('tr')
  if (row === romHoverRow) return
  romHoverRow = row
  const asm = row?.querySelector('.mem-row__annotation')?.textContent?.trim() ?? ''
  if (row && asm) hoverInstrPopup(asm, row as HTMLElement, 'right')
  else instrPopup.value?.leave()
}
function onRomLeave() { romHoverRow = null; instrPopup.value?.leave() }

// ── Program Load / Save ─────────────────────────────────────────────────────

const fileInput = ref<HTMLInputElement | null>(null)
const showConfig = ref(false)
/** Control-bar drop-up menu that is open (one at a time; any click closes it) */
const openMenu = ref<null | 'views' | 'file' | 'irq'>(null)
function toggleMenu(m: 'views' | 'file' | 'irq') { openMenu.value = openMenu.value === m ? null : m }
const closeMenu = () => { openMenu.value = null }
onMounted(() => window.addEventListener('click', closeMenu))
onBeforeUnmount(() => window.removeEventListener('click', closeMenu))
const configIoBase = ref('0x40020000')
const configRomSize = ref(32)
const configRamSize = ref(32)
const configRegMode = ref('8')

const emit = defineEmits<{
  'switch-tab': []
  'switch-pipeline': []
  'switch-memory': []
  'switch-ooo': []
  'switch-microstep': []
  'switch-microstep-pipeline': [],
  'switch-microstep-barrel': []
}>()

function openEditor() {
  emit('switch-tab')
}

function applyConfig() {
  const ioBase = parseInt(configIoBase.value, 16)
  if (!isNaN(ioBase)) {
    cpu.ioBaseAddress = ioBase
  }
  // ROM/RAM size and reg mode would need store changes — placeholder for now
  showConfig.value = false
  phases.reset()
}

interface ProgramEntry { hex: string; asm: string }
interface ProgramFile { name: string; program: ProgramEntry[] }

const examplePrograms: ProgramFile[] = [
  // Turn on Config → Traps, run it, then raise MEI from the IRQ menu. The line
  // stays raised (the handler runs again and again) until IRQ → Clear IRQs.
  {
    name: 'Interrupt (sets mtvec)',
    program: [
      { hex: '0x04000293', asm: 'addi x5, x0, 0x40' },  // handler address
      { hex: '0x30529073', asm: 'csrrw x0, mtvec, x5' },  // mtvec = 0x40: every trap jumps there
      { hex: '0x00100293', asm: 'addi x5, x0, 1' },
      { hex: '0x00b29293', asm: 'slli x5, x5, 11' },  // x5 = 0x800, the MEIE bit
      { hex: '0x3042a073', asm: 'csrrs x0, mie, x5' },  // mie.MEIE = 1: external interrupt enabled
      { hex: '0x30046073', asm: 'csrrsi x0, mstatus, 8' },  // mstatus.MIE = 1: interrupts on
      { hex: '0x00150513', asm: 'addi x10, x10, 1' },  // main loop: x10 counts
      { hex: '0xffdff06f', asm: 'jal x0, -4' },  // back to the loop
      { hex: '0x00000013', asm: 'nop' },
      { hex: '0x00000013', asm: 'nop' },
      { hex: '0x00000013', asm: 'nop' },
      { hex: '0x00000013', asm: 'nop' },
      { hex: '0x00000013', asm: 'nop' },
      { hex: '0x00000013', asm: 'nop' },
      { hex: '0x00000013', asm: 'nop' },
      { hex: '0x00000013', asm: 'nop' },
      { hex: '0x34202373', asm: 'csrrs x6, mcause, x0' },  // handler @ 0x40: x6 = cause
      { hex: '0x00158593', asm: 'addi x11, x11, 1' },  // x11 counts interrupts
      { hex: '0x30200073', asm: 'mret' },  // back to the interrupted instruction
    ],
  },
  // Vectored mode: mtvec's low bit is 1, so each interrupt jumps to its own
  // slot in a table (base + 4 x cause). Exceptions still go to the base.
  // Turn on Config → Traps, run it, raise MTI or MEI, then IRQ → Clear IRQs.
  {
    name: 'Interrupt table (vectored mtvec)',
    program: [
      { hex: '0x02100293', asm: 'addi x5, x0, 0x21' },  // table at 0x20, low bit 1 = vectored mode
      { hex: '0x30529073', asm: 'csrrw x0, mtvec, x5' },  // mtvec = 0x21
      { hex: '0x01100293', asm: 'addi x5, x0, 0x11' },
      { hex: '0x00729293', asm: 'slli x5, x5, 7' },  // x5 = 0x880: MTIE (bit 7) and MEIE (bit 11)
      { hex: '0x3042a073', asm: 'csrrs x0, mie, x5' },  // enable timer and external interrupts
      { hex: '0x30046073', asm: 'csrrsi x0, mstatus, 8' },  // mstatus.MIE = 1: interrupts on
      { hex: '0x00150513', asm: 'addi x10, x10, 1' },  // main loop: x10 counts
      { hex: '0xffdff06f', asm: 'jal x0, -4' },  // back to the loop
      { hex: '0x0400006f', asm: 'jal x0, 64' },  // table @ 0x20, slot 0: all exceptions
      { hex: '0x00000013', asm: 'nop' },
      { hex: '0x00000013', asm: 'nop' },
      { hex: '0x00000013', asm: 'nop' },
      { hex: '0x00000013', asm: 'nop' },
      { hex: '0x00000013', asm: 'nop' },
      { hex: '0x00000013', asm: 'nop' },
      { hex: '0x0140006f', asm: 'jal x0, 20' },  // slot 7 @ 0x3c: machine timer (0x20 + 4*7)
      { hex: '0x00000013', asm: 'nop' },
      { hex: '0x00000013', asm: 'nop' },
      { hex: '0x00000013', asm: 'nop' },
      { hex: '0x00c0006f', asm: 'jal x0, 12' },  // slot 11 @ 0x4c: machine external (0x20 + 4*11)
      { hex: '0x00158593', asm: 'addi x11, x11, 1' },  // timer handler @ 0x50: x11 counts
      { hex: '0x30200073', asm: 'mret' },
      { hex: '0x00160613', asm: 'addi x12, x12, 1' },  // external handler @ 0x58: x12 counts
      { hex: '0x30200073', asm: 'mret' },
      { hex: '0x00168693', asm: 'addi x13, x13, 1' },  // exception handler @ 0x60: x13 counts
      { hex: '0x0000006f', asm: 'jal x0, 0' },  // stop here
    ],
  },
  {
    name: 'Add & Store',
    program: [
      { hex: '0x00700193', asm: 'addi x3, x0, 7' },
      { hex: '0x00518213', asm: 'addi x4, x3, 5' },
      { hex: '0x000102b7', asm: 'lui  x5, 0x10' },
      { hex: '0x0042a223', asm: 'sw   x4, 4(x5)' },
    ],
  },
  {
    name: 'Count to 5',
    program: [
      { hex: '0x00500193', asm: 'addi x3, x0, 5' },
      { hex: '0x00100213', asm: 'addi x4, x0, 1' },
      { hex: '0x00420213', asm: 'addi x4, x4, 0' },
      { hex: '0xfff18193', asm: 'addi x3, x3, -1' },
      { hex: '0xfe019ce3', asm: 'bne  x3, x0, -8' },
    ],
  },
  {
    name: 'NOP sled',
    program: [],
  },
  {
    name: 'RV32-Scramble (all ops)',
    program: [
      { hex: '0x00010337', asm: 'lui x6, 0x10' },
      { hex: '0x00000397', asm: 'auipc x7, 0' },
      { hex: '0x04700193', asm: 'addi x3, x0, 0x47' },
      { hex: '0x05a06213', asm: 'ori x4, x0, 0x5a' },
      { hex: '0x00519293', asm: 'slli x5, x3, 5' },
      { hex: '0x0031d093', asm: 'srli x1, x3, 3' },
      { hex: '0x0012c1b3', asm: 'xor x3, x5, x1' },
      { hex: '0x004181b3', asm: 'add x3, x3, x4' },
      { hex: '0x4041d293', asm: 'srai x5, x3, 4' },
      { hex: '0xfff2c293', asm: 'xori x5, x5, -1' },
      { hex: '0x0051f0b3', asm: 'and x1, x3, x5' },
      { hex: '0x0070e1b3', asm: 'or x3, x1, x7' },
      { hex: '0x404182b3', asm: 'sub x5, x3, x4' },
      { hex: '0x00f2f113', asm: 'andi x2, x5, 0xf' },
      { hex: '0x002190b3', asm: 'sll x1, x3, x2' },
      { hex: '0x0021d2b3', asm: 'srl x5, x3, x2' },
      { hex: '0x4020d1b3', asm: 'sra x3, x1, x2' },
      { hex: '0x0041a2b3', asm: 'slt x5, x3, x4' },
      { hex: '0x003230b3', asm: 'sltu x1, x4, x3' },
      { hex: '0x0641a113', asm: 'slti x2, x3, 100' },
      { hex: '0x0c81b393', asm: 'sltiu x7, x3, 200' },
      { hex: '0x008000ef', asm: 'jal x1, 8' },
      { hex: '0x00000063', asm: 'beq x0, x0, 0' },
      { hex: '0x00332023', asm: 'sw x3, 0(x6)' },
      { hex: '0x00531223', asm: 'sh x5, 4(x6)' },
      { hex: '0x00230423', asm: 'sb x2, 8(x6)' },
      { hex: '0x00032183', asm: 'lw x3, 0(x6)' },
      { hex: '0x00431203', asm: 'lh x4, 4(x6)' },
      { hex: '0x00830283', asm: 'lb x5, 8(x6)' },
      { hex: '0x00435103', asm: 'lhu x2, 4(x6)' },
      { hex: '0x00834383', asm: 'lbu x7, 8(x6)' },
      { hex: '0x00008067', asm: 'jalr x0, x1, 0' },
    ],
  },
  {
    name: 'All Opcodes',
    program: [
      { hex: '0x00700193', asm: 'addi x3, x0, 7' },
      { hex: '0x00300213', asm: 'addi x4, x0, 3' },
      { hex: '0x004182b3', asm: 'add x5, x3, x4' },
      { hex: '0x404182b3', asm: 'sub x5, x3, x4' },
      { hex: '0x0041f2b3', asm: 'and x5, x3, x4' },
      { hex: '0x0041e2b3', asm: 'or x5, x3, x4' },
      { hex: '0x0041c2b3', asm: 'xor x5, x3, x4' },
      { hex: '0x004192b3', asm: 'sll x5, x3, x4' },
      { hex: '0x0041d2b3', asm: 'srl x5, x3, x4' },
      { hex: '0x003222b3', asm: 'slt x5, x4, x3' },
      { hex: '0x0031f293', asm: 'andi x5, x3, 3' },
      { hex: '0x0101e293', asm: 'ori x5, x3, 16' },
      { hex: '0xfff1c293', asm: 'xori x5, x3, -1' },
      { hex: '0x00219293', asm: 'slli x5, x3, 2' },
      { hex: '0x0011d293', asm: 'srli x5, x3, 1' },
      { hex: '0x00a1a293', asm: 'slti x5, x3, 10' },
      { hex: '0x00010337', asm: 'lui x6, 0x10' },
      { hex: '0x00000397', asm: 'auipc x7, 0' },
      { hex: '0x00332023', asm: 'sw x3, 0(x6)' },
      { hex: '0x00032283', asm: 'lw x5, 0(x6)' },
      { hex: '0x00200193', asm: 'addi x3, x0, 2' },
      { hex: '0xfff18193', asm: 'addi x3, x3, -1' },
      { hex: '0xfe019ee3', asm: 'bne x3, x0, -4' },
      { hex: '0x00c000ef', asm: 'jal x1, 12' },
      { hex: '0x02a00293', asm: 'addi x5, x0, 42' },
      { hex: '0x00c0006f', asm: 'jal x0, 12' },
      { hex: '0x06300193', asm: 'addi x3, x0, 99' },
      { hex: '0x00008067', asm: 'jalr x0, x1, 0' },
      { hex: '0x40020337', asm: 'lui x6, 0x40020' },
      { hex: '0x00532023', asm: 'sw x5, 0(x6)' },
      { hex: '0x0000006f', asm: 'jal x0, 0' },
    ],
  },
  {
    name: 'Byte / half lanes',
    program: [
      { hex: '0x000103b7', asm: 'lui  x7, 0x10' },
      { hex: '0x80ff80b7', asm: 'lui  x1, 0x80ff8' },
      { hex: '0xf0108093', asm: 'addi x1, x1, -255' },
      { hex: '0x0013a023', asm: 'sw   x1, 0(x7)' },
      { hex: '0x00038103', asm: 'lb   x2, 0(x7)' },
      { hex: '0x00338183', asm: 'lb   x3, 3(x7)' },
      { hex: '0x0033c203', asm: 'lbu  x4, 3(x7)' },
      { hex: '0x00239283', asm: 'lh   x5, 2(x7)' },
      { hex: '0x0023d303', asm: 'lhu  x6, 2(x7)' },
      { hex: '0x0ab00093', asm: 'addi x1, x0, 0xab' },
      { hex: '0x001380a3', asm: 'sb   x1, 1(x7)' },
      { hex: '0x00139323', asm: 'sh   x1, 6(x7)' },
      { hex: '0x0003a103', asm: 'lw   x2, 0(x7)' },
      { hex: '0x0043a183', asm: 'lw   x3, 4(x7)' },
      { hex: '0x40020237', asm: 'lui  x4, 0x40020' },
      { hex: '0x00120023', asm: 'sb   x1, 0(x4)' },
      { hex: '0x0000006f', asm: 'jal  x0, 0' },
    ],
  },
  {
    name: 'Multiply (Zmmul)',
    program: [
      { hex: '0x00700093', asm: 'addi x1, x0, 7' },
      { hex: '0x00600113', asm: 'addi x2, x0, 6' },
      { hex: '0x022081b3', asm: 'mul  x3, x1, x2' },
      { hex: '0xffd00093', asm: 'addi x1, x0, -3' },
      { hex: '0x02208233', asm: 'mul  x4, x1, x2' },
      { hex: '0x022092b3', asm: 'mulh x5, x1, x2' },
      { hex: '0x0220a333', asm: 'mulhsu x6, x1, x2' },
      { hex: '0x0220b3b3', asm: 'mulhu x7, x1, x2' },
      { hex: '0x00100093', asm: 'addi x1, x0, 1' },
      { hex: '0x00500113', asm: 'addi x2, x0, 5' },
      { hex: '0x022080b3', asm: 'mul  x1, x1, x2' },
      { hex: '0xfff10113', asm: 'addi x2, x2, -1' },
      { hex: '0xfe011ce3', asm: 'bne  x2, x0, loop' },
      { hex: '0x000101b7', asm: 'lui  x3, 0x10' },
      { hex: '0x0011a023', asm: 'sw   x1, 0(x3)' },
      { hex: '0x0000006f', asm: 'jal  x0, 0' },
    ],
  },
]

function loadExample(ex: ProgramFile) {
  if (ex.program.length === 0) {
    cpu.loadProgram([])
    localStorage.setItem('cpuvis_asm', '')
    return
  }
  cpu.loadProgramWithAsm(
    ex.program.map(e => ({ value: parseInt(e.hex, 16), asm: e.asm }))
  )
  // Copy asm to editor pane via localStorage
  const asmText = ex.program
    .map(e => '          ' + e.asm.toUpperCase())
    .join('\n')
  localStorage.setItem('cpuvis_asm', asmText)
}

function loadProgram() {
  fileInput.value?.click()
}

function onFileLoad(event: Event) {
  const file = (event.target as HTMLInputElement).files?.[0]
  if (!file) return

  const reader = new FileReader()
  reader.onload = () => {
    const text = reader.result as string
    try {
      // Try JSON format first
      const json = JSON.parse(text) as ProgramFile
      if (json.program && Array.isArray(json.program)) {
        cpu.loadProgramWithAsm(
          json.program.map(e => ({ value: parseInt(e.hex, 16), asm: e.asm }))
        )
        const asmText = json.program
          .map(e => '          ' + e.asm.toUpperCase())
          .join('\n')
        localStorage.setItem('cpuvis_asm', asmText)
        return
      }
    } catch {
      // Not JSON — try hex-per-line format
    }

    // Hex per line
    const lines = text.trim().split('\n').filter(l => l.trim())
    const words = lines.map(l => parseInt(l.trim(), 16)).filter(n => !isNaN(n))
    if (words.length > 0) {
      cpu.loadProgram(words)
    }
  }
  reader.readAsText(file)

  // Reset input so same file can be loaded again
  ;(event.target as HTMLInputElement).value = ''
}

function saveProgram() {
  const program: ProgramEntry[] = cpu.romWords
    .filter(w => w.value !== 0x00000013 || w.asm !== 'nop')
    .map(w => ({
      hex: '0x' + (w.value >>> 0).toString(16).padStart(8, '0'),
      asm: w.asm ?? 'unknown',
    }))

  const data: ProgramFile = { name: 'program', program }
  const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = 'program.json'
  a.click()
  URL.revokeObjectURL(url)
}

// ── Position helper (direct pixel coordinates) ──────────────────────────────

function px(x: number, y: number) {
  return { left: `${x}px`, top: `${y}px` }
}

// ── Clock bar: the canvas leaves exactly its height free at the bottom ───────
const clockBar = ref<HTMLDivElement | null>(null)
const clockBarH = ref(46)
let clockBarObs: ResizeObserver | null = null
onMounted(() => {
  if (!clockBar.value) return
  clockBarObs = new ResizeObserver(() => { clockBarH.value = clockBar.value?.offsetHeight ?? 46 })
  clockBarObs.observe(clockBar.value)
})
onBeforeUnmount(() => clockBarObs?.disconnect())

// ── Junction dots and wire names ─────────────────────────────────────────────

const PC_BLUE = '#0077a8', PC_GREEN = '#007a5e'
/** Where one wire branches into several (all ends carry the same value) */
const junctions = computed(() => {
  const j: { x: number; y: number; color?: string }[] = [
    { x: 427, y: PC_OUT, color: '#0099cc' },        // pc → ROM addr / adder
    { x: 408, y: PC_OUT, color: '#0099cc' },        // pc → ALU A mux (AUIPC)
    { x: 734, y: ADD_Y.value + 21, color: '#009977' },    // adder → PC Sel / WB (PC+4)
    { x: 380, y: IMM_Y.value + 32 },                      // instr → Imm Gen
    { x: 380, y: DEC_T + 46 },                      // instr → decoder / ALU Control
    { x: IMM_TAP, y: IMM_Y.value + 32 },                      // imm → +4/Imm / ALU Src
    { x: 676, y: AS_Y + 27 },                       // rs2 → ALU Src / store data
    { x: 1080, y: 636 },                            // ALU result → Load / Store addr
    { x: 1110, y: 636 },                            // ALU result → WB Sel / JALR target
  ]
  if (config.enableZmmul) j.push(
    { x: 866, y: 596 },                             // ALU A mux → ALU A / MUL a
    { x: 676, y: MUL_Y + 34 },                      // rs2 → MUL b
  )
  return j
})
/** Wire names (x, y = text baseline) */
const wireNames = computed(() => {
  const n: { text: string; x: number; y: number; anchor?: 'start' | 'end'; color?: string }[] = [
    { text: 'pc', x: 300, y: PC_OUT - 4, color: PC_BLUE },
    { text: 'pc', x: 600, y: PCFB_Y.value - 4, color: PC_BLUE },
    { text: 'instr', x: 376, y: 312, anchor: 'end' },
    { text: 'imm', x: IMM_TAP - 4, y: 540, anchor: 'end' },
    { text: 'rs1', x: 704, y: 540 },
    { text: 'rs2', x: 672, y: 526, anchor: 'end' },
    { text: 'pc', x: 560, y: DEC_B + 36, color: PC_BLUE },
    { text: 'alu out', x: 1049, y: 631 },
    { text: 'jalr target', x: 962, y: JT_Y.value - 3 },
    { text: 'pc+4', x: 962, y: P4_Y.value - 3, color: PC_GREEN },
    { text: 'store data', x: 450, y: LS_Y + 17 },
    { text: 'addr', x: 900, y: LS_Y + 41 },
    { text: 'load data', x: 900, y: LS_Y + 65 },
    { text: 'wdata', x: 1296, y: 560, anchor: 'end' },
  ]
  if (config.enableCsr) n.push(
    { text: 'mtvec', x: 760, y: pcIn(2) - 4 },
    { text: 'mepc', x: 760, y: CSR_Y.value + CSR_ROW.mepc - 4 },
    { text: 'csr data', x: CSR_X + 84, y: P4_Y.value - 16 },
  )
  if (config.enableZmmul) n.push({ text: 'mul', x: 1060, y: MUL_Y + 19 })
  return n
})

// ── Bus / control helpers ────────────────────────────────────────────────────

function busVal(id: string): number {
  return cpu.busStates[id]?.value ?? 0
}

function busActive(id: string): boolean {
  return cpu.busStates[id]?.state === 'active'
}

// ── Decoded instruction fields ───────────────────────────────────────────────

const opcode   = computed(() => cpu.lastInstr & 0x7f)
const funct3   = computed(() => (cpu.lastInstr >> 12) & 0x07)
const funct7b30 = computed(() => ((cpu.lastInstr >> 30) & 0x1) as 0 | 1)
const isLui    = computed(() => opcode.value === 0b0110111)
/** R-type with funct7 = 0000001: MUL / MULH / MULHSU / MULHU */
const isMulInstr = computed(() => opcode.value === 0b0110011 && (cpu.lastInstr >>> 25) === 1)
/** …running on the MUL block (Zmmul on) */
const isMul = computed(() => config.enableZmmul && isMulInstr.value)
/** Decoder badges: MUL selects WB input 4; with Zmmul off a MUL is illegal (all off) */
const decoderOverride = computed<Partial<ControlSignals> | undefined>(() => {
  if (!isMulInstr.value) return undefined
  if (isMul.value) return { wbSel: 4 }
  return { regWrite: false, aluOp: 0, wbSel: 0, immSel: 5 }
})
/** Load / store width from funct3 */
const lsWidth = computed(() => (['b', 'h', 'w', 'w'] as const)[funct3.value & 3] ?? 'w')
// ── Layout geometry (canvas px) ─────────────────────────────────────────────
const W = 1600, H = 434 + 388 + 12
/** Decoder top / bottom edge (control lines leave from both) */
/** Decoder: top port row, title, instruction row, bottom port row (73 px) */
const DEC_T = 292, DEC_B = DEC_T + 73
/** Decoder left edge / width: its left end leaves a channel for instr, pc and imm (x 380–590) */
const DEC_X = 600, DEC_W = 490
/** Decoder ports: top row (lines go up) and bottom row (lines go down), canvas x of each line */
const DEC_SPLIT = { top: ['imm_sel', 'is_jalr', 'offset_sel', 'reg_write'], bottom: ['mem_read', 'mem_write', 'alu_a', 'alu_src', 'alu_op', 'wb_sel'] }
/** Imm Gen sits under the PC (x 467); its ImmSel port is 45 px in */
const IMM_X = 467
/** imm branches just right of Imm Gen: right to +4/Imm, straight down the decoder's left side to ALU Src */
const IMM_TAP = 590
const DEC_PORT_X: Record<string, number> = {
  imm_sel: 632, is_jalr: 690, offset_sel: 887, reg_write: 1060,
  mem_read: 632, mem_write: 680, alu_a: 768, alu_src: 836, alu_op: 963, wb_sel: 1065,
}
/** ALU input muxes, one above the other: A (rs1 / PC) straight into ALU A (y 596);
    ALU Src (rs2 / imm) below it, its output jogs up to ALU B (y 646) */
const AMX = 712, AM_Y = 559, AS_Y = 644
/** Badge offsets inside the decoder (its grids start 9 px in: border + padding) */
const decBadgeX = Object.fromEntries(Object.entries(DEC_PORT_X).map(([k, x]) => [k, x - DEC_X - 9]))
/** Left column: I/O and RAM tops; Load / Store beside RAM */
/** 3 px gap left of the memories */
const MEM_X = 3
/** 8 px between instruction memory (y 20–426), I/O and data memory */
/** Data memory under instruction memory; I/O to its right, tops level */
const RAM_Y = 434, IO_X = MEM_X + 200 + 8, IO_Y = RAM_Y, IO_H = 62
/** MUL just under the ALU; Load / Store lanes pass under it */
const MUL_Y = 700
/** Load / Store right side, top to bottom: store data, addr, load data (no crossings) */
const LS_X = 240, LS_Y = RAM_Y + 388 - 90, LS_R = LS_X + 139
/** Right column: register file */
const RF_X = 1320, RF_Y = 240
/** Register file ports on its left edge: write enable, rs2 / rs1 out, write data in.
    rs1 / rs2 drop down x 1290 / 1280 to the execute lanes (y 600 / 584). */
const RF_PORT = { we: 280, rs2: 292, rs1: 304, wd: 316 }
/** WB Sel (inputs on the left, output right toward the register file) */
const WB_X = 1170
/** WB Sel inputs: ALU, Mem, PC+4 (+ CSR slot with Zicsr or Zmmul, + MUL with Zmmul) */
const wbSlots = computed(() => config.enableZmmul ? 5 : config.enableCsr ? 4 : 3)
/** Mux rows are 20.5 px apart, centred in a box ~75 + 22.6·(n−2) tall */
const wbH = computed(() => 75 + 22.6 * (wbSlots.value - 2))
const wbRow = (k: number) => wbH.value / 2 + (k - (wbSlots.value - 1) / 2) * 20.5
/** WB Sel top: input 0 lines up with the ALU result (y 636) */
const WB_Y = computed(() => Math.round(636 - wbRow(0)))
const wbIn = (k: number) => Math.round(WB_Y.value + wbRow(k))
/** Output sits at the middle of the box */
const wbOutY = computed(() => Math.round(WB_Y.value + wbH.value / 2))
/** PC row: PC, PC Sel, the adder and +4/Imm sit on one line (offset → adder → PC Sel → PC) */
const PC_OUT = 100
/** PC Sel: with Zicsr it has 4 inputs (trap, mret) and moves up so its output stays on the PC row */
const pcsY = computed(() => PC_OUT - (config.enableCsr ? 60 : 38))
/** pc → adder line: just below the PC box, clear of PC Sel (taller with Zicsr) */
const PCFB_Y = computed(() => config.enableCsr ? pcsY.value + 128 : PC_OUT + 50)
/** pc+4, then jalr target under it, run just below the imm line (IMM_Y + 32) */
const P4_Y = computed(() => PCFB_Y.value + 66)
const JT_Y = computed(() => PCFB_Y.value + 80)
/** Imm Gen: under the PC, below the pc → adder line (and its instr input clear of the IF box) */
const IMM_Y = computed(() => PCFB_Y.value + 20)
/** Adder and +4/Imm: outputs in line with PC Sel input 0 (4BR), so that wire runs straight */
const ADD_Y = computed(() => pcIn(0) - 21)
const OFS_Y = computed(() => ADD_Y.value - 17)
/** IF box just below the PC → ROM line; its one output leaves the bottom and branches to Imm Gen */
const IFPC_Y = PC_OUT + 12
/** CSR box: right of the +4/Imm mux and the imm line (x 953), on the next-PC row */
/** CSR row centres in the box (mtvec, mepc) */
const CSR_ROW = { mtvec: 95, mepc: 114 }
/** CSR top: the mtvec row lines up with PC Sel's trap input, so that wire runs straight */
const CSR_X = 970
const CSR_Y = computed(() => pcIn(2) - CSR_ROW.mtvec)
const pcIn = (k: number) => config.enableCsr ? Math.round(pcsY.value + 29 + 20.5 * k) : pcsY.value + 27 + 21 * k
/** Base addresses shown in the memory headers (rows show offsets) */
const romBase = computed(() => cpu.romWords[0]?.address ?? 0)
const ramBase = computed(() => cpu.ramWords[0]?.address ?? 0)

const fetchWord = computed(() => cpu.romWords.find(w => w.address === cpu.pc))
const currentAsm = computed(() => fetchWord.value?.asm ?? '')
/**
 * The instruction the decoder is working on: nothing until it has been
 * fetched (Ready and the fetch phase), then the fetched word. Not the word at
 * the PC, which already points at the next instruction in the PC-update phase.
 */
/** The instruction has been fetched: the decoder and the immediate generator have something to work on */
const fetched = computed(() => phases.currentPhase !== 'idle' && phases.currentPhase !== 'fetch')
const decodedAsm = computed(() => {
  if (!fetched.value) return ''
  return cpu.romWords.find(w => w.value === cpu.lastInstr)?.asm ?? ''
})

// ── Control signal values ────────────────────────────────────────────────────

const aluOp    = computed(() => (busVal('ctrl_alu_op') as 0 | 1 | 2))
const immSel   = computed(() => busVal('ctrl_imm_sel') as ImmSel)
const aluSrcSel = computed(() => cpu.controlStates['ctrl_alu_src']?.asserted ? 1 : 0)
const wbSel    = computed(() => busVal('ctrl_wb_sel'))
const offsetSel = computed(() => cpu.controlStates['ctrl_offset_sel']?.asserted ? 1 : 0)
const isJalrSel = computed(() => cpu.controlStates['ctrl_is_jalr']?.asserted ? 1 : 0)
const isBranchTaken = computed(() => offsetSel.value === 1)
const isRegWrite = computed(() => cpu.controlStates['ctrl_reg_write']?.asserted ?? false)

/** Phase for WB buses: memwb only if regWrite is asserted */
const wbPhase = computed(() => isRegWrite.value ? 'memwb' as const : 'idle' as const)

/** Phase for ALU result trunk: highlights in execute, memwb(if memOp), or pcupdate(if JALR) */
const aluResultTrunkPhase = computed(() => {
  const p = phases.currentPhase
  if (p === 'execute') return 'execute' as const
  if (p === 'memwb' && isMemOp.value) return 'memwb' as const
  if (p === 'pcupdate' && isJalrSel.value === 1) return 'pcupdate' as const
  return 'idle' as const
})

/** Phase for JALR-only segments */
const jalrPhase = computed(() =>
  (phases.currentPhase === 'pcupdate' && isJalrSel.value === 1) ? 'pcupdate' as const : 'idle' as const
)

/** Phase for imm chain: regread if aluSrc=imm, OR pcupdate if branch taken */
const immChainPhase = computed(() => {
  if (aluSrcSel.value === 1) return 'regread' as const
  if (isBranchTaken.value) return 'pcupdate' as const
  return 'idle' as const
})

// ── MUX input arrays ─────────────────────────────────────────────────────────

const muxPcOffsetInputs = computed(() => [
  { name: '+4',  value: 4,                    bits: 32 },
  { name: 'imm', value: busVal('immediate'),  bits: 32 },
])

const muxPcInputs = computed(() => {
  const inputs = [
    { name: '4BR',  value: busVal('pc_adder_result'), bits: 32 },
    { name: 'JALR', value: busVal('jalr_target'),     bits: 32 },
  ]
  if (config.enableCsr) {
    inputs.push({ name: 'trap', value: cpu.csrRegisters[0x305] ?? 0, bits: 32 })
    inputs.push({ name: 'mret', value: cpu.csrRegisters[0x341] ?? 0, bits: 32 })
  }
  return inputs
})
/** MRET: next PC comes from mepc */
const isMret = computed(() => cpu.lastInstr >>> 0 === 0x30200073)
/** A trap was taken this instruction: next PC comes from mtvec */
const trapTaken = computed(() => { void cpu.pc; void cpu.lastInstr; return cpu.micro()?.trap_taken ?? false })
/** PC Sel select: 0 PC+4/branch, 1 JALR, 2 trap → mtvec, 3 mret → mepc */
const pcSel = computed(() => config.enableCsr && trapTaken.value ? 2 : config.enableCsr && isMret.value ? 3 : isJalrSel.value)
/** Phases for the trap / mret paths */
const trapPhase = computed(() => pcSel.value === 2 ? 'pcupdate' as const : 'idle' as const)
const mretPhase = computed(() => pcSel.value === 3 ? 'pcupdate' as const : 'idle' as const)

/** AUIPC: ALU A = PC */
const isAuipc = computed(() => opcode.value === 0b0010111)
/** ALU input A as the core computed it (rs1, or the PC for AUIPC) */
const aluA = computed(() => {
  void cpu.pc; void cpu.lastInstr; void phases.currentPhase
  return (cpu.micro()?.alu_a ?? busVal('rs1_data')) >>> 0
})
const muxAluAInputs = computed(() => [
  { name: 'rs1', value: busVal('rs1_data'), bits: 32 },
  { name: 'pc',  value: cpu.pc,             bits: 32 },
])

const muxAluSrcInputs = computed(() => [
  { name: 'rs2',  value: busVal('rs2_data'),  bits: 32 },
  { name: 'imm',  value: busVal('immediate'), bits: 32 },
])

const muxWbInputs = computed(() => {
  const inputs = [
    { name: 'ALU',  value: busVal('alu_result'),       bits: 32 },
    { name: 'Mem',  value: busVal('mem_read_data'),    bits: 32 },
    { name: 'PC+4', value: busVal('pc_adder_result'),  bits: 32 },
  ]
  if (config.enableCsr || config.enableZmmul) {
    inputs.push({ name: config.enableCsr ? 'CSR' : '—', value: busVal('csr_read_data'), bits: 32 })
  }
  if (config.enableZmmul) {
    inputs.push({ name: 'MUL', value: busVal('mul_result'), bits: 32 })
  }
  return inputs
})

// ── Load / Store derived state ───────────────────────────────────────────────

const isMemOp = computed(() =>
  cpu.controlStates['ctrl_mem_read']?.asserted || cpu.controlStates['ctrl_mem_write']?.asserted
)

const isIoAddr = computed(() =>
  isMemOp.value && cpu.isIoAddress(busVal('alu_result'))
)

/** Phase for memory-only buses: memwb if mem op, otherwise idle */
const memPhase = computed(() => isMemOp.value ? 'memwb' as const : 'idle' as const)

/** Phase for RAM buses: only if mem op AND not I/O address */
const ramPhase = computed(() => (isMemOp.value && !isIoAddr.value) ? 'memwb' as const : 'idle' as const)

/** Phase for I/O buses: only if mem op AND I/O address */
const ioPhase = computed(() => (isMemOp.value && isIoAddr.value) ? 'memwb' as const : 'idle' as const)

const loadStoreOp = computed(() => {
  if (cpu.controlStates['ctrl_mem_read']?.asserted)  return 'load' as const
  if (cpu.controlStates['ctrl_mem_write']?.asserted) return 'store' as const
  return 'idle' as const
})

const ramOp = computed(() => {
  if (cpu.controlStates['ctrl_mem_read']?.asserted)  return 'read' as const
  if (cpu.controlStates['ctrl_mem_write']?.asserted) return 'write' as const
  return 'idle' as const
})

const ramLastValue = computed(() => {
  if (ramOp.value === 'read')  return busVal('mem_read_data')
  if (ramOp.value === 'write') {
    const a = (busVal('alu_result') & ~3) >>> 0
    return cpu.ramWords.find(w => w.address === a)?.value ?? busVal('rs2_data')
  }
  return 0
})
</script>

<style scoped>
.cpu-layout {
  position: relative;
  width: 1600px;
  height: 834px;
  font-family: 'JetBrains Mono', monospace;
  background: #fafafa;
  overflow: auto;
}

.cpu-wires {
  position: absolute;
  top: 0;
  left: 0;
  pointer-events: none;
  z-index: 2;
}

.cpu-block {
  position: absolute;
  z-index: 1;
}
/* Wire name beside a wire */
.bus-name { font-family: Inter, 'Segoe UI', Roboto, Arial, sans-serif; font-size: 9px; font-weight: 700; fill: #92400e; paint-order: stroke; stroke: #fafafa; stroke-width: 3px; }
/* I/O box: tighter lines */
.cpu-block--io :deep(.io) { line-height: 1.15; }
.cpu-block--io :deep(.io__header) { padding: 2px 8px; }
.cpu-block--io :deep(.io__body) { padding: 2px 10px; gap: 1px; }
.cpu-block--io :deep(.io__leds) { padding: 2px 0; }
.cpu-block--io :deep(.io__led-col) { gap: 1px; }
.cpu-block--io :deep(.io__byte-row) { padding-top: 0; }
/* Register rows as tall as the CSR rows (18.5 px) */
.cpu-block--regs :deep(.reg-row td) { line-height: 14px; }
/* Data memory as wide as the I/O box; the Load / Store wires end at its right edge */
.cpu-block--ram :deep(.ram) { width: 200px; }

/* Clock controls bar */
.clock-bar {
  position: fixed;
  bottom: 0;
  left: 0;
  right: 0;
  z-index: 10;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  background: #1a1a2e;
  border-top: 2px solid #333;
  font-family: 'JetBrains Mono', monospace;
  /* one row, fixed height: nothing wraps when the phase text changes */
  white-space: nowrap;
  /* never scrolls: wraps to a second row in a narrow window (the canvas
     follows the measured bar height); menus open above it */
  flex-wrap: wrap;
  row-gap: 6px;
}
.clock-bar > * { flex-shrink: 0; }

.clock-btn {
  font-family: 'JetBrains Mono', monospace;
  font-size: 12px;
  font-weight: 600;
  padding: 6px 10px;
  border: 1px solid #555;
  border-radius: 3px;
  background: #2a2a4a;
  color: #eee;
  cursor: pointer;
  letter-spacing: 1px;
  text-transform: uppercase;
  transition: background 0.1s;
}
.clock-btn:hover {
  background: #3a3a6a;
}
.clock-btn:active {
  background: #4a4a8a;
}
.clock-btn--reset {
  border-color: #cc2222;
  color: #ff6666;
}
.clock-btn--reset:hover {
  background: #3a1a1a;
}

.clock-speed {
  width: 90px;
  accent-color: #0099cc;
}

.clock-label {
  font-size: 11px;
  color: #88aacc;
  letter-spacing: 0.5px;
}

.clock-btn--edit {
  border-color: #88f;
  color: #aaf;
}
.clock-btn--edit:hover {
  background: #1a1a4a;
}

.clock-btn--file {
  border-color: #4a8;
  color: #8fc;
}
.clock-btn--file:hover {
  background: #1a3a2a;
}

.clock-divider {
  color: #444;
  font-size: 16px;
  user-select: none;
}

.clock-menu {
  position: relative;
}
.clock-menu__list {
  position: absolute;
  bottom: 100%;
  left: 0;
  margin-bottom: 6px;
  background: #1a1a2e;
  border: 1px solid #555;
  border-radius: 3px;
  overflow: hidden;
  min-width: 170px;
  z-index: 20;
}
.clock-menu__list--right { left: auto; right: 0; }
.clock-menu__head {
  padding: 5px 12px 3px;
  border-top: 1px solid #444;
  color: #88aacc;
  font-size: 10px;
  letter-spacing: 1px;
  text-transform: uppercase;
}
.clock-menu__item {
  display: block;
  width: 100%;
  padding: 6px 12px;
  border: none;
  background: none;
  color: #ccc;
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  text-align: left;
  cursor: pointer;
  white-space: nowrap;
}
.clock-menu__item:hover {
  background: #2a2a4a;
  color: #fff;
}
.clock-menu__item--current,
.clock-menu__item--current:hover { color: #7fd4e4; background: none; cursor: default; }
.clock-menu__item--current::before { content: '● '; }
.clock-tag { padding: 2px 6px; border-radius: 3px; background: #222; }
.clock-tag--trap { background: #a02020; color: #fff; }
.clock-tag--masked { background: #6a5a20; color: #fff; cursor: help; }
.modal__toggle--off { opacity: 0.5; }

/* Config Modal */
.modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  z-index: 100;
  display: flex;
  align-items: center;
  justify-content: center;
}
.modal {
  background: #1a1a2e;
  border: 1px solid #555;
  border-radius: 6px;
  min-width: 320px;
  font-family: 'JetBrains Mono', monospace;
  color: #eee;
}
.modal__header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px 16px;
  border-bottom: 1px solid #333;
}
.modal__title {
  font-size: 13px;
  font-weight: 700;
  letter-spacing: 1px;
  text-transform: uppercase;
}
.modal__close {
  background: none;
  border: none;
  color: #888;
  font-size: 14px;
  cursor: pointer;
  font-family: 'JetBrains Mono', monospace;
}
.modal__close:hover { color: #fff; }

.modal__body {
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.modal__row {
  display: flex;
  align-items: center;
  gap: 12px;
}
.modal__label {
  font-size: 11px;
  color: #aaa;
  width: 140px;
  flex-shrink: 0;
}
.modal__input {
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  padding: 4px 8px;
  background: #2a2a4a;
  border: 1px solid #555;
  border-radius: 3px;
  color: #eee;
  width: 140px;
}
.modal__input:focus {
  outline: none;
  border-color: #0099cc;
}

.modal__footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 10px 16px;
  border-top: 1px solid #333;
}
.modal__btn {
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  padding: 5px 14px;
  border: 1px solid #555;
  border-radius: 3px;
  background: #2a2a4a;
  color: #ccc;
  cursor: pointer;
}
.modal__btn:hover { background: #3a3a6a; }
.modal__btn--apply {
  border-color: #4a8;
  color: #8fc;
}
.modal__btn--apply:hover { background: #1a3a2a; }

.modal__section {
  font-size: 11px;
  font-weight: 700;
  color: #888;
  letter-spacing: 1px;
  text-transform: uppercase;
  padding-top: 8px;
  border-top: 1px solid #333;
}
.modal__section:first-child {
  border-top: none;
  padding-top: 0;
}

.modal__toggle {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11px;
  color: #ccc;
  cursor: pointer;
}
.modal__toggle input[type="checkbox"] {
  accent-color: #0099cc;
  width: 16px;
  height: 16px;
}

.clock-phase {
  font-size: 12px;
  color: #ffffff;
  font-weight: 600;
  letter-spacing: 0.5px;
  /* fixed width so the controls after it don't shift as the text changes */
  width: 200px;
  overflow: hidden;
  text-overflow: ellipsis;
}
</style>
