// ─────────────────────────────────────────────────────────────────────────────
// RV32I Single-Cycle Datapath — Complete Netlist
//
// Defines every component, every port, and every net (wire/bus).
// This is the static topology — never mutated at runtime.
// Simulation state (BusState[], ControlSignalState[]) lives in Pinia.
//
// PC path:
//   mux_pc_offset selects +4 or immediate (branch/JAL)
//   pc_adder adds PC + selected offset
//   mux_pc selects adder output or JALR target
//
// Port directions:
//   in  — receives signal
//   out — drives signal
//   bi  — bidirectional
//   tri — tristate (multiple possible drivers, one active at a time)
//
// Bit widths:
//   1  — single control line
//   2  — 2-bit field (ALUOp, WBSel)
//   3  — 3-bit field (funct3, ImmSel)
//   4  — 4-bit ALU control
//   5  — register address
//   7  — opcode
//  32  — data / address bus
// ─────────────────────────────────────────────────────────────────────────────

import type { Netlist } from '../types/cpu'

export const RV32I_NETLIST: Netlist = {

  // ── COMPONENTS ─────────────────────────────────────────────────────────────
  components: [

    {
      id: 'pc', label: 'Program Counter',
      ports: [
        { componentId: 'pc', name: 'next_pc', direction: 'in',  bits: 32 },
        { componentId: 'pc', name: 'pc_out',  direction: 'out', bits: 32 },
      ],
    },

    {
      id: 'mux_pc_offset', label: 'MUX PC Offset',
      ports: [
        { componentId: 'mux_pc_offset', name: 'in0', direction: 'in',  bits: 32 },  // +4
        { componentId: 'mux_pc_offset', name: 'in1', direction: 'in',  bits: 32 },  // immediate
        { componentId: 'mux_pc_offset', name: 'sel', direction: 'in',  bits: 1  },  // branch_taken
        { componentId: 'mux_pc_offset', name: 'out', direction: 'out', bits: 32 },
      ],
    },

    {
      id: 'pc_adder', label: 'PC Adder',
      ports: [
        { componentId: 'pc_adder', name: 'pc_in',    direction: 'in',  bits: 32 },
        { componentId: 'pc_adder', name: 'offset',   direction: 'in',  bits: 32 },
        { componentId: 'pc_adder', name: 'result',   direction: 'out', bits: 32 },
      ],
    },

    {
      id: 'mux_pc', label: 'MUX PC Select',
      ports: [
        { componentId: 'mux_pc', name: 'in0', direction: 'in',  bits: 32 },  // pc_adder result
        { componentId: 'mux_pc', name: 'in1', direction: 'in',  bits: 32 },  // JALR target
        { componentId: 'mux_pc', name: 'sel', direction: 'in',  bits: 1  },  // is_jalr
        { componentId: 'mux_pc', name: 'out', direction: 'out', bits: 32 },
      ],
    },

    {
      id: 'rom', label: 'Instruction Memory',
      ports: [
        { componentId: 'rom', name: 'pc_addr',   direction: 'in',  bits: 32 },
        { componentId: 'rom', name: 'instr_out', direction: 'out', bits: 32 },
      ],
    },

    {
      id: 'decoder', label: 'Decoder / Control',
      ports: [
        { componentId: 'decoder', name: 'opcode',    direction: 'in',  bits: 7 },
        { componentId: 'decoder', name: 'reg_write', direction: 'out', bits: 1 },
        { componentId: 'decoder', name: 'alu_src',   direction: 'out', bits: 1 },
        { componentId: 'decoder', name: 'alu_op',    direction: 'out', bits: 2 },
        { componentId: 'decoder', name: 'mem_read',  direction: 'out', bits: 1 },
        { componentId: 'decoder', name: 'mem_write', direction: 'out', bits: 1 },
        { componentId: 'decoder', name: 'wb_sel',    direction: 'out', bits: 2 },
        { componentId: 'decoder', name: 'pc_sel',    direction: 'out', bits: 2 },
        { componentId: 'decoder', name: 'imm_sel',   direction: 'out', bits: 3 },
      ],
    },

    {
      id: 'imm_gen', label: 'Immediate Generator',
      ports: [
        { componentId: 'imm_gen', name: 'instruction', direction: 'in',  bits: 32 },
        { componentId: 'imm_gen', name: 'imm_sel',     direction: 'in',  bits: 3  },
        { componentId: 'imm_gen', name: 'immediate',   direction: 'out', bits: 32 },
      ],
    },

    {
      id: 'alu_ctrl', label: 'ALU Control Unit',
      ports: [
        { componentId: 'alu_ctrl', name: 'alu_op',      direction: 'in',  bits: 2 },
        { componentId: 'alu_ctrl', name: 'funct3',      direction: 'in',  bits: 3 },
        { componentId: 'alu_ctrl', name: 'funct7b30',   direction: 'in',  bits: 1 },
        { componentId: 'alu_ctrl', name: 'alu_control', direction: 'out', bits: 4 },
      ],
    },

    {
      id: 'regfile', label: 'Register File',
      ports: [
        { componentId: 'regfile', name: 'rs1_addr',   direction: 'in',  bits: 5  },
        { componentId: 'regfile', name: 'rs2_addr',   direction: 'in',  bits: 5  },
        { componentId: 'regfile', name: 'rd_addr',    direction: 'in',  bits: 5  },
        { componentId: 'regfile', name: 'reg_write',  direction: 'in',  bits: 1  },
        { componentId: 'regfile', name: 'write_data', direction: 'in',  bits: 32 },
        { componentId: 'regfile', name: 'rs1_data',   direction: 'out', bits: 32 },
        { componentId: 'regfile', name: 'rs2_data',   direction: 'out', bits: 32 },
      ],
    },

    {
      id: 'mux_alusrc', label: 'MUX ALU Src B',
      ports: [
        { componentId: 'mux_alusrc', name: 'in0', direction: 'in',  bits: 32 },  // rs2_data
        { componentId: 'mux_alusrc', name: 'in1', direction: 'in',  bits: 32 },  // immediate
        { componentId: 'mux_alusrc', name: 'sel', direction: 'in',  bits: 1  },  // alu_src
        { componentId: 'mux_alusrc', name: 'out', direction: 'out', bits: 32 },
      ],
    },

    {
      id: 'alu', label: 'ALU',
      ports: [
        { componentId: 'alu', name: 'alu_a',       direction: 'in',  bits: 32 },
        { componentId: 'alu', name: 'alu_b',       direction: 'in',  bits: 32 },
        { componentId: 'alu', name: 'alu_control', direction: 'in',  bits: 4  },
        { componentId: 'alu', name: 'alu_result',  direction: 'out', bits: 32 },
        { componentId: 'alu', name: 'zero_flag',   direction: 'out', bits: 1  },
        { componentId: 'alu', name: 'neg_flag',    direction: 'out', bits: 1  },
        { componentId: 'alu', name: 'carry_flag',  direction: 'out', bits: 1  },
        { componentId: 'alu', name: 'ovf_flag',    direction: 'out', bits: 1  },
      ],
    },

    {
      // Zmmul multiplier (combinational): rs1 × rs2 → WB Sel input 4
      id: 'mul', label: 'MUL',
      ports: [
        { componentId: 'mul', name: 'a',      direction: 'in',  bits: 32 },
        { componentId: 'mul', name: 'b',      direction: 'in',  bits: 32 },
        { componentId: 'mul', name: 'result', direction: 'out', bits: 32 },
      ],
    },

    {
      id: 'loadstore', label: 'Load / Store',
      ports: [
        { componentId: 'loadstore', name: 'address',    direction: 'in',  bits: 32 },
        { componentId: 'loadstore', name: 'write_data', direction: 'in',  bits: 32 },
        { componentId: 'loadstore', name: 'mem_read',   direction: 'in',  bits: 1  },
        { componentId: 'loadstore', name: 'mem_write',  direction: 'in',  bits: 1  },
        { componentId: 'loadstore', name: 'read_data',  direction: 'out', bits: 32 },
      ],
    },

    {
      id: 'ram', label: 'Data Memory',
      ports: [
        { componentId: 'ram', name: 'addr',       direction: 'in',  bits: 32 },
        { componentId: 'ram', name: 'write_data', direction: 'in',  bits: 32 },
        { componentId: 'ram', name: 'mem_read',   direction: 'in',  bits: 1  },
        { componentId: 'ram', name: 'mem_write',  direction: 'in',  bits: 1  },
        { componentId: 'ram', name: 'read_data',  direction: 'out', bits: 32 },
      ],
    },

    {
      id: 'io', label: 'I/O Block',
      ports: [
        { componentId: 'io', name: 'addr',       direction: 'in',  bits: 32 },
        { componentId: 'io', name: 'write_data', direction: 'in',  bits: 32 },
        { componentId: 'io', name: 'mem_write',  direction: 'in',  bits: 1  },
        { componentId: 'io', name: 'read_data',  direction: 'out', bits: 32 },
      ],
    },

    {
      id: 'mux_wb', label: 'MUX Write-Back',
      ports: [
        { componentId: 'mux_wb', name: 'in0', direction: 'in',  bits: 32 },  // alu_result
        { componentId: 'mux_wb', name: 'in1', direction: 'in',  bits: 32 },  // read_data
        { componentId: 'mux_wb', name: 'in2', direction: 'in',  bits: 32 },  // pc_adder result (for JAL/JALR link)
        { componentId: 'mux_wb', name: 'in3', direction: 'in',  bits: 32 },  // csr_read_data
        { componentId: 'mux_wb', name: 'in4', direction: 'in',  bits: 32 },  // mul_result
        { componentId: 'mux_wb', name: 'sel', direction: 'in',  bits: 3  },  // wb_sel
        { componentId: 'mux_wb', name: 'out', direction: 'out', bits: 32 },
      ],
    },

  ],

  // ── NETS ───────────────────────────────────────────────────────────────────
  nets: [

    // ── PC path ────────────────────────────────────────────────────────────
    {
      id: 'pc_out', bits: 32,
      from: { componentId: 'pc',       portName: 'pc_out' },
      to: [
        { componentId: 'rom',      portName: 'pc_addr' },
        { componentId: 'pc_adder', portName: 'pc_in'   },
      ],
    },

    // mux_pc_offset: selects +4 or immediate as the offset
    {
      id: 'pc_offset', bits: 32,
      from: { componentId: 'mux_pc_offset', portName: 'out' },
      to:   [{ componentId: 'pc_adder',     portName: 'offset' }],
    },

    // pc_adder: PC + offset → feeds mux_pc and mux_wb
    {
      id: 'pc_adder_result', bits: 32,
      from: { componentId: 'pc_adder', portName: 'result' },
      to: [
        { componentId: 'mux_pc', portName: 'in0' },
        { componentId: 'mux_wb', portName: 'in2' },
      ],
    },

    // mux_pc: selects adder result or JALR target → next PC
    {
      id: 'next_pc', bits: 32,
      from: { componentId: 'mux_pc', portName: 'out' },
      to:   [{ componentId: 'pc',    portName: 'next_pc' }],
    },

    // ── Instruction word fields ────────────────────────────────────────────
    {
      id: 'instr', bits: 32,
      from: { componentId: 'rom', portName: 'instr_out' },
      to: [
        { componentId: 'decoder',  portName: 'opcode'      },
        { componentId: 'imm_gen',  portName: 'instruction' },
        { componentId: 'alu_ctrl', portName: 'funct3'      },
        { componentId: 'alu_ctrl', portName: 'funct7b30'   },
        { componentId: 'regfile',  portName: 'rs1_addr'    },
        { componentId: 'regfile',  portName: 'rs2_addr'    },
        { componentId: 'regfile',  portName: 'rd_addr'     },
      ],
    },

    // ── Decoder control outputs ────────────────────────────────────────────
    {
      id: 'ctrl_reg_write', bits: 1,
      from: { componentId: 'decoder', portName: 'reg_write' },
      to:   [{ componentId: 'regfile', portName: 'reg_write' }],
    },
    {
      id: 'ctrl_alu_src', bits: 1,
      from: { componentId: 'decoder',    portName: 'alu_src' },
      to:   [{ componentId: 'mux_alusrc', portName: 'sel'    }],
    },
    {
      id: 'ctrl_alu_op', bits: 2,
      from: { componentId: 'decoder',  portName: 'alu_op' },
      to:   [{ componentId: 'alu_ctrl', portName: 'alu_op' }],
    },
    {
      id: 'ctrl_mem_read', bits: 1,
      from: { componentId: 'decoder',   portName: 'mem_read' },
      to: [
        { componentId: 'ram',       portName: 'mem_read' },
        { componentId: 'loadstore', portName: 'mem_read' },
      ],
    },
    {
      id: 'ctrl_mem_write', bits: 1,
      from: { componentId: 'decoder',   portName: 'mem_write' },
      to: [
        { componentId: 'ram',       portName: 'mem_write' },
        { componentId: 'loadstore', portName: 'mem_write' },
        { componentId: 'io',        portName: 'mem_write' },
      ],
    },
    {
      id: 'ctrl_wb_sel', bits: 3,
      from: { componentId: 'decoder', portName: 'wb_sel' },
      to:   [{ componentId: 'mux_wb', portName: 'sel'    }],
    },
    {
      id: 'ctrl_pc_sel', bits: 2,
      from: { componentId: 'decoder', portName: 'pc_sel' },
      to:   [],  // consumed by step() to derive offset_sel and is_jalr
    },
    {
      id: 'ctrl_imm_sel', bits: 3,
      from: { componentId: 'decoder', portName: 'imm_sel' },
      to:   [{ componentId: 'imm_gen', portName: 'imm_sel' }],
    },

    // New: offset mux select (branch taken / JAL) and JALR select
    {
      id: 'ctrl_offset_sel', bits: 1,
      from: { componentId: 'decoder',       portName: 'pc_sel' },  // derived in step()
      to:   [{ componentId: 'mux_pc_offset', portName: 'sel'   }],
    },
    {
      id: 'ctrl_is_jalr', bits: 1,
      from: { componentId: 'decoder', portName: 'pc_sel' },  // derived in step()
      to:   [{ componentId: 'mux_pc', portName: 'sel'    }],
    },

    // ── Immediate generator ────────────────────────────────────────────────
    {
      id: 'immediate', bits: 32,
      from: { componentId: 'imm_gen',      portName: 'immediate' },
      to: [
        { componentId: 'mux_alusrc',       portName: 'in1' },
        { componentId: 'mux_pc_offset',    portName: 'in1' },
      ],
    },

    // ── ALU control ────────────────────────────────────────────────────────
    {
      id: 'alu_control', bits: 4,
      from: { componentId: 'alu_ctrl', portName: 'alu_control' },
      to:   [{ componentId: 'alu',     portName: 'alu_control' }],
    },

    // ── Register file outputs ──────────────────────────────────────────────
    {
      id: 'rs1_data', bits: 32,
      from: { componentId: 'regfile',    portName: 'rs1_data' },
      to:   [{ componentId: 'alu',       portName: 'alu_a'    }],
    },
    {
      id: 'rs2_data', bits: 32,
      from: { componentId: 'regfile',    portName: 'rs2_data' },
      to: [
        { componentId: 'mux_alusrc', portName: 'in0'        },
        { componentId: 'ram',        portName: 'write_data'  },
        { componentId: 'loadstore',  portName: 'write_data'  },
        { componentId: 'io',         portName: 'write_data'  },
      ],
    },

    // ── ALU src mux → ALU ──────────────────────────────────────────────────
    {
      id: 'alu_b', bits: 32,
      from: { componentId: 'mux_alusrc', portName: 'out'   },
      to:   [{ componentId: 'alu',       portName: 'alu_b' }],
    },

    // ── ALU outputs ────────────────────────────────────────────────────────
    {
      id: 'alu_result', bits: 32,
      from: { componentId: 'alu',       portName: 'alu_result' },
      to: [
        { componentId: 'mux_wb',    portName: 'in0'      },
        { componentId: 'ram',       portName: 'addr'     },
        { componentId: 'loadstore', portName: 'address'  },
        { componentId: 'io',        portName: 'addr'     },
      ],
    },
    {
      id: 'zero_flag', bits: 1,
      from: { componentId: 'alu', portName: 'zero_flag' },
      to:   [],  // consumed by step() for branch condition
    },
    {
      // JALR target: ALU result (rs1 + imm) with bit0 cleared → mux_pc in1
      id: 'jalr_target', bits: 32,
      from: { componentId: 'alu',    portName: 'alu_result' },
      to:   [{ componentId: 'mux_pc', portName: 'in1'       }],
    },

    // ── Memory read data ───────────────────────────────────────────────────
    {
      id: 'mem_read_data', bits: 32,
      from: { componentId: 'ram',     portName: 'read_data' },
      to:   [{ componentId: 'mux_wb', portName: 'in1'       }],
    },
    {
      id: 'io_read_data', bits: 32,
      from: { componentId: 'io',      portName: 'read_data' },
      to:   [{ componentId: 'mux_wb', portName: 'in1'       }],
    },

    // ── CSR read data (Zicsr) ─────────────────────────────────────────────
    {
      id: 'csr_read_data', bits: 32,
      from: { componentId: 'decoder', portName: 'pc_sel' }, // placeholder source
      to:   [{ componentId: 'mux_wb', portName: 'in3' }],
    },

    // ── MUL result (Zmmul) ───────────────────────────────────────────────
    {
      id: 'mul_result', bits: 32,
      from: { componentId: 'mul',    portName: 'result' },
      to:   [{ componentId: 'mux_wb', portName: 'in4' }],
    },

    // ── Write-back to register file ────────────────────────────────────────
    {
      id: 'write_data', bits: 32,
      from: { componentId: 'mux_wb',  portName: 'out'        },
      to:   [{ componentId: 'regfile', portName: 'write_data' }],
    },

  ],
}

// ── Quick lookup helpers ────────────────────────────────────────────────────

export const NET_BY_ID = new Map(
  RV32I_NETLIST.nets.map(n => [n.id, n])
)

export const COMPONENT_BY_ID = new Map(
  RV32I_NETLIST.components.map(c => [c.id, c])
)

/** All nets driven by a given component */
export function netsFrom(componentId: string) {
  return RV32I_NETLIST.nets.filter(n => n.from.componentId === componentId)
}

/** All nets received by a given component */
export function netsTo(componentId: string) {
  return RV32I_NETLIST.nets.filter(n =>
    n.to.some(t => t.componentId === componentId)
  )
}

/** Control nets (bits < 32) vs data buses (bits === 32) */
export const CONTROL_NETS = RV32I_NETLIST.nets.filter(n => n.bits < 32)
export const DATA_NETS    = RV32I_NETLIST.nets.filter(n => n.bits === 32)
