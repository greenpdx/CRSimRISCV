<template>
  <div class="cpu-layout">

    <!-- ── SVG wiring layer ── -->
    <svg class="cpu-wires" :width="1600" :height="850">
      <!-- ── Control lines: decoder bottom → components ── -->

      <!-- regWrite → Register File -->
      <ControlLine
        net-id="ctrl_reg_write"
        :points="[{ x: 444, y: 105 }, { x: 444, y: 353 }]"
        :asserted="cpu.controlStates['ctrl_reg_write']?.asserted ?? false"
        :show-label="true"
        :phase="wbPhase"
        />

      <!-- immSel → Imm Generator: only when aluSrc=imm -->
      <ControlLine
        net-id="ctrl_imm_sel"
        :points="[{ x: 569, y: 105 }, { x: 569, y: 255 }]"
        :asserted="busVal('ctrl_imm_sel') !== 5"
        :show-label="true"
        :phase="immChainPhase"
        />

      <!-- ctrl_is_jalr → PC Sel MUX -->
      <ControlLine
        net-id="ctrl_is_jalr"
        :points="[{ x: 640, y: 105 }, { x: 640, y: 131 }]"
        :asserted="cpu.controlStates['ctrl_is_jalr']?.asserted ?? false"
        :show-label="true"
        phase="pcupdate"
        />

      <!-- aluSrc → ALU Src MUX -->
      <ControlLine
        net-id="ctrl_alu_src"
        :points="[{ x: 744, y: 105 }, { x: 744, y: 528 }]"
        :asserted="cpu.controlStates['ctrl_alu_src']?.asserted ?? false"
        :show-label="true"
        phase="regread"
        />

      <!-- ctrl_offset_sel → +4/Imm MUX -->
      <ControlLine
        net-id="ctrl_offset_sel"
        :points="[{ x: 887, y: 105 }, { x: 887, y: 111 }]"
        :asserted="cpu.controlStates['ctrl_offset_sel']?.asserted ?? false"
        :show-label="true"
        phase="pcupdate"
        />

      <!-- aluOp → ALU Control -->
      <ControlLine
        net-id="ctrl_alu_op"
        :points="[{ x: 963, y: 105 }, { x: 963, y: 403 }]"
        :asserted="busVal('ctrl_alu_op') !== 0"
        :show-label="true"
        phase="execute"
        />

      <!-- wbSel → WB Sel MUX -->
      <ControlLine
        net-id="ctrl_wb_sel"
        :points="[{ x: 1065, y: 105 }, { x: 1065, y: 288 }]"
        :asserted="busVal('ctrl_wb_sel') !== 0"
        :show-label="true"
        :phase="wbPhase"
        />

      <!-- ── Data buses ── -->

      <!-- PC addr → ROM: straight left from PC into the side of instruction memory -->
      <BusLine
        net-id="pc_out"
        :points="[{ x: 467, y: 168 }, { x: 288, y: 168 }]"
        :bits="32"
        :value="cpu.pc"
        :state="busActive('pc_out') ? 'active' : 'idle'"
        :show-label="true"
      phase="fetch"
        />

      <!-- PC Sel (next_pc) → PC: straight from mux output to PC right side -->
      <BusLine
        net-id="next_pc"
        :points="[{ x: 584, y: 168 }, { x: 557, y: 168 }]"
        :bits="32"
        :value="cpu.nextPc"
        :state="busActive('next_pc') ? 'active' : 'idle'"
        :show-label="true"
      phase="pcupdate"
        />

      <!-- PC → Adder: down from PC bottom, right along bus, up into adder bottom -->
      <BusLine
        net-id="pc_out_adder"
        :points="[{ x: 427, y: 168 }, { x: 427, y: 244 }, { x: 773, y: 244 }, { x: 773, y: 169 }]"
        :bits="32"
        :value="cpu.pc"
        :state="busActive('pc_out') ? 'active' : 'idle'"
        :show-label="false"
      phase="pcupdate"
        />

       <!-- Adder output → MUX PC Sel input -->
      <BusLine
        net-id="pc_adder_result"
        :points="[{ x: 754, y: 150 }, { x: 697, y: 150 }]"
        :bits="32"
        :value="busVal('pc_adder_result')"
        :state="busActive('pc_adder_result') ? 'active' : 'idle'"
        :show-label="false"
      phase="pcupdate"
        />

      <!-- Instruction bus branch: out of IF/PC box bottom, down to ImmGen (only when aluSrc=imm) -->
      <BusLine
        net-id="instr_immgen"
        :points="[{ x: 380, y: 113 }, { x: 380, y: 287 }, { x: 522, y: 287 }]"
        :bits="32"
        :value="busVal('instr')"
        :state="busActive('instr') ? 'active' : 'idle'"
        :show-label="false"
        :phase="immChainPhase"
        />

      <!-- ImmGen output trunk (only when aluSrc=imm) -->
      <BusLine
        net-id="immediate"
        :points="[
          { x: 632, y: 287 },
          { x: 675, y: 287 },
        ]"
        :bits="32"
        :value="busVal('immediate')"
        :state="busActive('immediate') ? 'active' : 'idle'"
        :show-label="true"
        :phase="immChainPhase"
        />

      <!-- ImmGen → ALU Src mux (imm input): only when aluSrc=imm -->
      <BusLine
        net-id="immediate_alusrc"
        :points="[
          { x: 675, y: 287 },
          { x: 675, y: 574 },
          { x: 688, y: 574 },
        ]"
        :bits="32"
        :value="busVal('immediate')"
        :state="busActive('immediate') ? 'active' : 'idle'"
        :show-label="false"
        :phase="aluSrcSel === 1 ? 'regread' : 'idle'"
        />

      <!-- ImmGen → +4/Imm mux (imm input): right, up, short left into mux -->
      <BusLine
        net-id="immediate_pcoffset"
        :points="[{ x: 675, y: 287 }, { x: 953, y: 287 }, { x: 953, y: 160 }, { x: 943, y: 160 }]"
        :bits="32"
        :value="busVal('immediate')"
        :state="busActive('immediate') ? 'active' : 'idle'"
        :show-label="false"
        phase="pcupdate"
        />

      <!-- ALU Control → ALU -->
      <BusLine
        net-id="alu_control"
        :points="[
          { x: 960, y: 473 },
          { x: 960, y: 495 },
        ]"
        :bits="4"
        :value="busVal('alu_control')"
        :state="busActive('alu_control') ? 'active' : 'idle'"
        :show-label="false"
      phase="execute"
        />

      <!-- ALU Src output → ALU B input -->
      <BusLine
        net-id="alu_b"
        :points="[
          { x: 800, y: 565 },
          { x: 886, y: 565 },
        ]"
        :bits="32"
        :value="busVal('alu_b')"
        :state="busActive('alu_b') ? 'active' : 'idle'"
        :show-label="false"
        phase="regread"
        />

      <!-- WB Sel → RegFile write data -->
      <BusLine
        net-id="write_data"
        :points="[
          { x: 1009, y: 337 },
          { x: 520, y: 337 },
          { x: 520, y: 353 },
        ]"
        :bits="32"
        :value="busVal('write_data')"
        :state="busActive('write_data') ? 'active' : 'idle'"
        :show-label="false"
        :phase="wbPhase"
        />

      <!-- ALU result: horizontal from ALU to corner -->
      <BusLine
        net-id="alu_result_horiz"
        :points="[
          { x: 1046, y: 555 },
          { x: 1170, y: 555 },
        ]"
        :bits="32"
        :value="busVal('alu_result')"
        :state="busActive('alu_result') ? 'active' : 'idle'"
        :show-label="false"
        :phase="aluResultTrunkPhase"
        />

      <!-- ALU result: vertical lower (corner to WB/LS junction) -->
      <BusLine
        net-id="alu_result_vert_lower"
        :points="[
          { x: 1170, y: 545 },
          { x: 1170, y: 310 },
        ]"
        :bits="32"
        :value="busVal('alu_result')"
        :state="busActive('alu_result') ? 'active' : 'idle'"
        :show-label="false"
        :phase="aluResultTrunkPhase"
        />

      <!-- ALU result: junction → WB Sel input 0 -->
      <BusLine
        net-id="alu_result_to_wb"
        :points="[
          { x: 1170, y: 310 },
          { x: 1121, y: 310 },
        ]"
        :bits="32"
        :value="busVal('alu_result')"
        :state="busActive('alu_result') ? 'active' : 'idle'"
        :show-label="false"
        phase="execute"
        />

      <!-- ALU result: vertical upper (junction to JALR tap) -->
      <BusLine
        net-id="alu_result_vert_upper"
        :points="[{ x: 1170, y: 310 }, { x: 1170, y: 198 }]"
        :bits="32"
        :value="busVal('alu_result')"
        :state="busActive('alu_result') ? 'active' : 'idle'"
        :show-label="false"
        :phase="jalrPhase"
        />

      <!-- JALR target → PC Sel mux input 1 -->
      <BusLine
        net-id="jalr_target"
        :points="[{ x: 1170, y: 198 }, { x: 696, y: 198 }]"
        :bits="32"
        :value="busVal('jalr_target')"
        :state="busActive('jalr_target') ? 'active' : 'idle'"
        :show-label="false"
        :phase="jalrPhase"
        />

      <!-- LoadStore load data → WB Sel input 1 -->
      <BusLine
        net-id="mem_read_data"
        :points="[
          { x: 1280, y: 585 },
          { x: 1210, y: 585 },
          { x: 1210, y: 330 },
          { x: 1121, y: 330 },
        ]"
        :bits="32"
        :value="busVal('mem_read_data')"
        :state="busActive('mem_read_data') ? 'active' : 'idle'"
        :show-label="false"
      :phase="memPhase"
        />

      <!-- PC Adder result → WB Sel input 2 (PC+4 link for JAL/JALR only) -->
      <BusLine
        net-id="pc_adder_result_wb"
        :points="[{ x: 734, y: 150 }, { x: 734, y: 260 }, { x: 1145, y: 260 }, { x: 1145, y: 350 }, { x: 1121, y: 350 }]"
        :bits="32"
        :value="busVal('pc_adder_result')"
        :state="busActive('pc_adder_result') ? 'active' : 'idle'"
        :show-label="false"
        :phase="wbSel === 2 ? 'memwb' : 'idle'"
        />

      <!-- CSR read data → WB Sel input 3 (only when Zicsr enabled) -->
      <BusLine
        v-if="config.enableCsr"
        net-id="csr_read_data"
        :points="[
          { x: 1180, y: 166 },
          { x: 1180, y: 388 },
          { x: 1121, y: 388 },
        ]"
        :bits="32"
        :value="busVal('csr_read_data')"
        :state="busActive('csr_read_data') ? 'active' : 'idle'"
        :show-label="false"
        :phase="wbSel === 3 ? 'memwb' : 'idle'"
        />

      <!-- ctrl_mem_read → LoadStore -->
      <ControlLine
        net-id="ctrl_mem_read"
        :points="[{ x: 1080, y: 20 }, { x: 1080, y: 8 }, { x: 1150, y: 8 }, { x: 1210, y: 8 }, { x: 1274, y: 8 }, { x: 1274, y: 545 }, { x: 1280, y: 545 }]"
        :asserted="cpu.controlStates['ctrl_mem_read']?.asserted ?? false"
        :show-label="true"
        :phase="memPhase"
        />

      <!-- ctrl_mem_write → LoadStore -->
      <ControlLine
        net-id="ctrl_mem_write"
        :points="[{ x: 1088, y: 20 }, { x: 1088, y: 13 }, { x: 1110, y: 13 }, { x: 1140, y: 13 }, { x: 1269, y: 13 }, { x: 1269, y: 555 }, { x: 1280, y: 555 }]"
        :asserted="cpu.controlStates['ctrl_mem_write']?.asserted ?? false"
        :show-label="true"
        :phase="memPhase"
        />

      <!-- ALU result → LoadStore addr input -->
      <BusLine
        net-id="alu_result_ls"
        :points="[
          { x: 1170, y: 310 },
          { x: 1170, y: 555 },
          { x: 1280, y: 555 },
        ]"
        :bits="32"
        :value="busVal('alu_result')"
        :state="busActive('alu_result') ? 'active' : 'idle'"
        :show-label="false"
        :phase="memPhase"
        />

      <!-- rs2 store data → LoadStore store data input -->
      <BusLine
        net-id="rs2_data_ls"
        :points="[
          { x: 650, y: 548 },
          { x: 650, y: 620 },
          { x: 1280, y: 620 },
        ]"
        :bits="32"
        :value="busVal('rs2_data')"
        :state="busActive('rs2_data') ? 'active' : 'idle'"
        :show-label="false"
        :phase="memPhase"
        />

      <!-- LoadStore addr → RAM (only for RAM addresses) -->
      <BusLine
        net-id="ls_addr_ram"
        :points="[
          { x: 1420, y: 620 },
          { x: 1526, y: 620 },
          { x: 1526, y: 75 },
          { x: 1450, y: 75 },
        ]"
        :bits="32"
        :value="busVal('alu_result')"
        :state="busActive('alu_result') ? 'active' : 'idle'"
        :show-label="false"
        :phase="ramPhase"
        />

      <!-- LoadStore data → RAM (only for RAM addresses) -->
      <BusLine
        net-id="ls_data_ram"
        :points="[
          { x: 1420, y: 600 },
          { x: 1516, y: 600 },
          { x: 1516, y: 85 },
          { x: 1450, y: 85 },
        ]"
        :bits="32"
        :value="busVal('rs2_data')"
        :state="busActive('rs2_data') ? 'active' : 'idle'"
        :show-label="false"
        :phase="ramPhase"
        />

      <!-- LoadStore addr → I/O (only for I/O addresses) -->
      <BusLine
        net-id="ls_addr_io"
        :points="[
          { x: 1420, y: 570 },
          { x: 1506, y: 570 },
          { x: 1506, y: 450 },
          { x: 1470, y: 450 },
        ]"
        :bits="32"
        :value="busVal('alu_result')"
        :state="busActive('alu_result') ? 'active' : 'idle'"
        :show-label="false"
        :phase="ioPhase"
        />

      <!-- LoadStore data → I/O (only for I/O addresses) -->
      <BusLine
        net-id="ls_data_io"
        :points="[
          { x: 1420, y: 555 },
          { x: 1496, y: 555 },
          { x: 1496, y: 460 },
          { x: 1470, y: 460 },
        ]"
        :bits="32"
        :value="busVal('rs2_data')"
        :state="busActive('rs2_data') ? 'active' : 'idle'"
        :show-label="false"
        :phase="ioPhase"
        />

      <!-- RegFile rs1 → ALU A input -->
      <BusLine
        net-id="rs1_data"
        :points="[
          { x: 630, y: 515 },
          { x: 886, y: 515 },
        ]"
        :bits="32"
        :value="busVal('rs1_data')"
        :state="busActive('rs1_data') ? 'active' : 'idle'"
        :show-label="false"
      phase="regread"
        />

      <!-- RegFile rs2 → ALU Src mux: only when aluSrc=rs2 -->
      <BusLine
        net-id="rs2_data"
        :points="[
          { x: 630, y: 548 },
          { x: 688, y: 548 },
        ]"
        :bits="32"
        :value="busVal('rs2_data')"
        :state="busActive('rs2_data') ? 'active' : 'idle'"
        :show-label="false"
        :phase="aluSrcSel === 0 ? 'regread' : 'idle'"
        />

      <!-- Instruction bus branch: down to ALU Control -->
      <BusLine
        net-id="instr_aluctrl"
        :points="[{ x: 380, y: 287 }, { x: 380, y: 313 }, { x: 930, y: 313 }, { x: 930, y: 403 }]"
        :bits="32"
        :value="busVal('instr')"
        :state="busActive('instr') ? 'active' : 'idle'"
        :show-label="false"
      phase="regread"
        />

      <!-- Instruction bus: ROM → IF/PC box -->
      <BusLine
        net-id="instr_rom_ifpc"
        :points="[{ x: 288, y: 40 }, { x: 296, y: 40 }]"
        :bits="32"
        :value="busVal('instr')"
        :state="busActive('instr') ? 'active' : 'idle'"
        :show-label="false"
      phase="fetch"
        />


      <!-- Instruction bus: IF/PC box → Decoder -->
      <BusLine
        net-id="instr"
        :points="[{ x: 391, y: 40 }, { x: 410, y: 40 }]"
        :bits="32"
        :value="busVal('instr')"
        :state="busActive('instr') ? 'active' : 'idle'"
        :show-label="true"
      phase="fetch"
        />

       <!-- +4/Imm MUX output → Adder right input -->
      <BusLine
        net-id="pc_offset"
        :points="[{ x: 832, y: 150 }, { x: 794, y: 150 }]"
        :bits="32"
        :value="busVal('pc_offset')"
        :state="busActive('pc_offset') ? 'active' : 'idle'"
        :show-label="false"
      phase="pcupdate"
        />

    </svg>

    <!-- ── ROM (far left, tall) ── -->
    <div class="cpu-block" :style="px(0, 20)" @click="onRomClick">
      <RomBlock
        :words="cpu.romWords"
        :pc="cpu.pc"
      />
    </div>

    <!-- ── IF / PC box (between ROM and Decoder) ── -->
    <div class="cpu-block" :style="px(296, 20)">
      <IfPcBox
        :pc="cpu.pc"
        :instr="fetchWord?.value"
        :asm="currentAsm"
        :active="busActive('pc_out') || busActive('instr')"
      />
    </div>

    <!-- ── PC chain (horizontal row: PC → MUX PC Sel → Adder → MUX +4/Imm) ── -->

    <div class="cpu-block" :style="px(467, 148)">
      <PcRegister
        :pc="cpu.pc"
        :next-pc="cpu.nextPc"
        :active="busActive('pc_out')"
      />
    </div>

    <div class="cpu-block" :style="px(584, 131)">
      <MuxTrapezoid
        label="PC Sel"
        output-label="New PC"
        :inputs="muxPcInputs"
        :select="isJalrSel"
        :active="busActive('next_pc')"
      />
    </div>

    <div class="cpu-block" :style="px(753, 129)">
      <PcAdder
        :pc="cpu.pc"
        :offset="busVal('pc_offset')"
        :active="busActive('pc_adder_result')"
      />
    </div>

    <div class="cpu-block" :style="px(831, 111)">
      <MuxTrapezoid
        label="+4/Imm"
        output-label="offset"
        :inputs="muxPcOffsetInputs"
        :select="offsetSel"
        :active="busActive('pc_offset')"
      />
    </div>

    <!-- ── Decoder / Control (top) ── -->
    <div class="cpu-block" :style="px(410, 20)">
      <DecoderControl
        :opcode="opcode"
        :active="busActive('instr')"
        :asm-text="currentAsm"
        :phase-text="phases.phaseLabel"
        @asm-click="(e: MouseEvent) => showInstrPopup(currentAsm, e)"
      />
    </div>

    <!-- ── Immediate Generator ── -->
    <div class="cpu-block" :style="px(524, 255)">
      <ImmGenerator
        :instruction="cpu.lastInstr"
        :imm-sel="immSel"
        :active="busActive('immediate')"
      />
    </div>

    <!-- ── ALU Control ── -->
    <div class="cpu-block" :style="px(913, 403)">
      <AluControlUnit
        :alu-op="aluOp"
        :funct3="funct3"
        :funct7b30="funct7b30"
        :is-lui="isLui"
        :active="busActive('alu_control')"
      />
    </div>

    <!-- ── Register File ── -->
    <div class="cpu-block" :style="px(368, 353)">
      <RegisterFile
        :registers="cpu.registers"
      />
    </div>

    <!-- ── CSR Registers (only when Zicsr enabled) ── -->
    <div v-if="config.enableCsr" class="cpu-block" :style="px(1103, 20)">
      <CsrUnit />
    </div>

    <!-- ── MUX ALU Src B ── -->
    <div class="cpu-block" :style="px(688, 528)">
      <MuxTrapezoid
        label="ALU Src"
        output-label="ALU B"
        direction="right"
        :inputs="muxAluSrcInputs"
        :select="aluSrcSel"
        :active="busActive('alu_b')"
      />
    </div>

    <!-- ── ALU ── -->
    <div class="cpu-block" :style="px(886, 485)">
      <AluUnit
        :alu-control="busVal('alu_control')"
        :operand-a="busVal('rs1_data')"
        :operand-b="busVal('alu_b')"
        :active="busActive('alu_result')"
      />
    </div>

    <!-- ── MUX Write-Back ── -->
    <div class="cpu-block" :style="px(1009, 288)">
      <MuxTrapezoid
        label="WB Sel"
        output-label="write data"
        :inputs="muxWbInputs"
        :select="wbSel"
        :active="busActive('write_data')"
      />
    </div>

    <!-- ── Load / Store ── -->
    <div class="cpu-block" :style="px(1280, 540)">
      <LoadStore
        :op="loadStoreOp"
        :address="busVal('alu_result')"
        :width="'w'"
        :load-sign="'signed'"
        :write-data="busVal('rs2_data')"
        :read-data="busVal('mem_read_data')"
      />
    </div>

    <!-- ── RAM (far right) ── -->
    <div class="cpu-block" :style="px(1280, 20)">
      <RamBlock
        :words="cpu.ramWords"
        :last-op="ramOp"
        :last-address="busVal('alu_result')"
        :last-value="ramLastValue"
      />
    </div>

    <!-- ── I/O Block (below RAM) ── -->
    <div class="cpu-block" :style="px(1280, 434)">
      <IoBlock
        :base-address="cpu.ioBaseAddress"
        :address="busVal('alu_result')"
        :data="cpu.ioData"
        :op="cpu.ioOp"
      />
    </div>

    <!-- ── Clock Controls ── -->
    <div class="clock-bar">
      <!-- Editor (new tab) -->
      <button class="clock-btn clock-btn--edit" @click="openEditor">Edit</button>
      <button class="clock-btn clock-btn--edit" @click="emit('switch-pipeline')">Pipeline</button>
      <button class="clock-btn clock-btn--edit" @click="emit('switch-memory')">Memory</button>

      <span class="clock-divider">|</span>

      <!-- Program menu -->
      <button class="clock-btn clock-btn--file" @click="loadProgram">Load</button>
      <button class="clock-btn clock-btn--file" @click="saveProgram">Save</button>
      <div class="clock-examples">
        <button class="clock-btn clock-btn--file" @click="showExamples = !showExamples">Examples</button>
        <div v-if="showExamples" class="clock-examples__menu">
          <button
            v-for="ex in examplePrograms"
            :key="ex.name"
            class="clock-examples__item"
            @click="loadExample(ex)"
          >{{ ex.name }}</button>
        </div>
      </div>

      <button class="clock-btn clock-btn--file" @click="showConfig = true">Config</button>

      <span class="clock-divider">|</span>

      <!-- Execution controls -->
      <button class="clock-btn" @click="phases.microStep()" :disabled="phases.animating">Micro</button>
      <button class="clock-btn" @click="phases.animatedStep()" :disabled="phases.animating">Step</button>
      <button class="clock-btn" @click="phases.toggleAutoRun()">{{ phases.autoRunning ? 'Stop' : 'Run' }}</button>
      <button class="clock-btn clock-btn--reset" @click="phases.reset()">Reset</button>
      <input
        type="range"
        class="clock-speed"
        min="100"
        max="2000"
        step="50"
        v-model.number="phases.animDuration"
        title="Animation speed (ms)"
      />
      <span class="clock-label">{{ phases.animDuration }}ms</span>
      <span class="clock-phase">{{ phases.phaseLabel }}</span>
      <span class="clock-label">Cycle: {{ cpu.cycleCount }}</span>
      <span class="clock-label">PC: 0x{{ (cpu.pc >>> 0).toString(16).padStart(8, '0') }}</span>

      <span class="clock-divider">|</span>

      <!-- Trap feature toggle -->
      <label class="clock-label" style="cursor:pointer;">
        <input type="checkbox" v-model="cpu.trapsEnabled" />
        Traps
      </label>

      <!-- Visible only when traps enabled -->
      <template v-if="cpu.trapsEnabled">
        <span class="clock-label" :class="'mode-' + cpu.privMode"
              style="padding:.15rem .4rem; border-radius:3px; background:#222;">
          mode: {{ cpu.privMode }}
        </span>
        <span v-if="cpu.trapDepth > 0" class="clock-label"
              style="padding:.15rem .4rem; border-radius:3px; background:#a02020; color:#fff;">
          IN_TRAP
        </span>
        <button class="clock-btn" @click="cpu.raiseIrq('mti')">MTI</button>
        <button class="clock-btn" @click="cpu.raiseIrq('mei')">MEI</button>
        <button class="clock-btn" @click="cpu.raiseIrq('sei')">SEI</button>
        <button class="clock-btn" @click="cpu.clearIrqs()">Clr IRQ</button>
      </template>

      <span class="clock-divider">|</span>

      <!-- Micro-step visualizer launcher (separate Vue view) -->
      <button class="clock-btn clock-btn--edit" @click="emit('switch-microstep')">μStep</button>
      <button class="clock-btn clock-btn--edit" @click="emit('switch-microstep-pipeline')">μStep Pipe</button>
      <button class="clock-btn clock-btn--edit" @click="emit('switch-microstep-barrel')">μStep Barrel</button>
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
            <label class="modal__toggle">
              <input type="checkbox" v-model="config.multicycle" />
              <span>{{ config.multicycle ? 'Enabled' : 'Disabled' }}</span>
            </label>
          </div>

          <div class="modal__section">Extensions</div>
          <div class="modal__row">
            <label class="modal__label">Zicsr (CSR registers)</label>
            <label class="modal__toggle">
              <input type="checkbox" v-model="config.enableCsr" />
              <span>{{ config.enableCsr ? 'Enabled' : 'Disabled' }}</span>
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
import { computed, ref } from 'vue'
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

function showInstrPopup(asm: string, event: MouseEvent) {
  if (!asm || asm === 'nop') return
  instrPopup.value?.show(asm.toUpperCase(), event.clientX + 10, event.clientY - 10)
}

function onRomClick(event: MouseEvent) {
  // Find the clicked row's annotation cell
  const target = event.target as HTMLElement
  const row = target.closest('tr')
  if (!row) return
  const annotation = row.querySelector('.mem-row__annotation')
  if (!annotation) return
  const asm = annotation.textContent?.trim() ?? ''
  if (asm) showInstrPopup(asm, event)
}

// ── Program Load / Save ─────────────────────────────────────────────────────

const fileInput = ref<HTMLInputElement | null>(null)
const showExamples = ref(false)
const showConfig = ref(false)
const configIoBase = ref('0x40020000')
const configRomSize = ref(32)
const configRamSize = ref(32)
const configRegMode = ref('8')

const emit = defineEmits<{
  'switch-tab': []
  'switch-pipeline': []
  'switch-memory': []
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
]

function loadExample(ex: ProgramFile) {
  showExamples.value = false
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

const fetchWord = computed(() => cpu.romWords.find(w => w.address === cpu.pc))
const currentAsm = computed(() => fetchWord.value?.asm ?? '')

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

const muxPcInputs = computed(() => [
  { name: '4BR',  value: busVal('pc_adder_result'), bits: 32 },
  { name: 'JALR', value: busVal('jalr_target'),     bits: 32 },
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
  if (config.enableCsr) {
    inputs.push({ name: 'CSR', value: busVal('csr_read_data'), bits: 32 })
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
  if (ramOp.value === 'write') return busVal('rs2_data')
  return 0
})
</script>

<style scoped>
.cpu-layout {
  position: relative;
  width: 1600px;
  height: 850px;
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

/* Clock controls bar */
.clock-bar {
  position: fixed;
  bottom: 0;
  left: 0;
  right: 0;
  z-index: 10;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 16px;
  background: #1a1a2e;
  border-top: 2px solid #333;
  font-family: 'JetBrains Mono', monospace;
}

.clock-btn {
  font-family: 'JetBrains Mono', monospace;
  font-size: 12px;
  font-weight: 600;
  padding: 6px 16px;
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
  width: 120px;
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

.clock-examples {
  position: relative;
}
.clock-examples__menu {
  position: absolute;
  bottom: 100%;
  left: 0;
  margin-bottom: 4px;
  background: #1a1a2e;
  border: 1px solid #555;
  border-radius: 3px;
  overflow: hidden;
  min-width: 140px;
  z-index: 20;
}
.clock-examples__item {
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
}
.clock-examples__item:hover {
  background: #2a2a4a;
  color: #fff;
}

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
  flex: 1;
}
</style>
