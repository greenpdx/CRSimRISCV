//! Mirrors `src/data/controlData.ts` — 10-entry decoder table + branch conditions.

#[derive(Debug, Clone, Copy)]
pub struct ControlSignals {
    pub reg_write: bool,
    pub alu_src:   bool,   // false=rs2, true=immediate
    pub alu_op:    u8,     // 0|1|2
    pub mem_read:  bool,
    pub mem_write: bool,
    pub wb_sel:    u8,     // 0=ALU, 1=Mem, 2=PC+4, 3=CSR
    pub pc_sel:    u8,     // 0=PC+4, 1=PC+imm, 2=(rs1+imm)&~1
    pub imm_sel:   u8,     // matches ImmSel encoding
    pub csr_op:    u8,
}

#[derive(Debug, Clone, Copy)]
pub struct OpcodeEntry {
    pub opcode:  u8,
    pub name:    &'static str,
    pub signals: ControlSignals,
}

pub const OPCODE_TABLE: &[OpcodeEntry] = &[
    OpcodeEntry { opcode: 0b0110011, name: "R-type",
        signals: ControlSignals { reg_write: true,  alu_src: false, alu_op: 2, mem_read: false, mem_write: false, wb_sel: 0, pc_sel: 0, imm_sel: 5, csr_op: 0 } },
    OpcodeEntry { opcode: 0b0010011, name: "I-ALU",
        signals: ControlSignals { reg_write: true,  alu_src: true,  alu_op: 2, mem_read: false, mem_write: false, wb_sel: 0, pc_sel: 0, imm_sel: 0, csr_op: 0 } },
    OpcodeEntry { opcode: 0b0000011, name: "Load",
        signals: ControlSignals { reg_write: true,  alu_src: true,  alu_op: 0, mem_read: true,  mem_write: false, wb_sel: 1, pc_sel: 0, imm_sel: 0, csr_op: 0 } },
    OpcodeEntry { opcode: 0b0100011, name: "Store",
        signals: ControlSignals { reg_write: false, alu_src: true,  alu_op: 0, mem_read: false, mem_write: true,  wb_sel: 0, pc_sel: 0, imm_sel: 1, csr_op: 0 } },
    OpcodeEntry { opcode: 0b1100011, name: "Branch",
        signals: ControlSignals { reg_write: false, alu_src: false, alu_op: 1, mem_read: false, mem_write: false, wb_sel: 0, pc_sel: 1, imm_sel: 2, csr_op: 0 } },
    OpcodeEntry { opcode: 0b0110111, name: "LUI",
        signals: ControlSignals { reg_write: true,  alu_src: true,  alu_op: 2, mem_read: false, mem_write: false, wb_sel: 0, pc_sel: 0, imm_sel: 3, csr_op: 0 } },
    OpcodeEntry { opcode: 0b0010111, name: "AUIPC",
        signals: ControlSignals { reg_write: true,  alu_src: true,  alu_op: 0, mem_read: false, mem_write: false, wb_sel: 0, pc_sel: 0, imm_sel: 3, csr_op: 0 } },
    OpcodeEntry { opcode: 0b1101111, name: "JAL",
        signals: ControlSignals { reg_write: true,  alu_src: true,  alu_op: 0, mem_read: false, mem_write: false, wb_sel: 2, pc_sel: 1, imm_sel: 4, csr_op: 0 } },
    OpcodeEntry { opcode: 0b1100111, name: "JALR",
        signals: ControlSignals { reg_write: true,  alu_src: true,  alu_op: 0, mem_read: false, mem_write: false, wb_sel: 2, pc_sel: 2, imm_sel: 0, csr_op: 0 } },
    OpcodeEntry { opcode: 0b1110011, name: "CSR",
        signals: ControlSignals { reg_write: true,  alu_src: false, alu_op: 0, mem_read: false, mem_write: false, wb_sel: 3, pc_sel: 0, imm_sel: 5, csr_op: 0 } },
];

pub fn opcode_by_value(opcode: u8) -> Option<&'static OpcodeEntry> {
    OPCODE_TABLE.iter().find(|e| e.opcode == opcode)
}

/// Branch condition per funct3 — which ALU flag to test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchCondition { Zero, NotZero, Neg, NotNeg, Carry, NotCarry }

pub fn branch_condition(funct3: u8) -> Option<BranchCondition> {
    match funct3 {
        0b000 => Some(BranchCondition::Zero),     // BEQ
        0b001 => Some(BranchCondition::NotZero),  // BNE
        0b100 => Some(BranchCondition::Neg),      // BLT
        0b101 => Some(BranchCondition::NotNeg),   // BGE
        0b110 => Some(BranchCondition::Carry),    // BLTU
        0b111 => Some(BranchCondition::NotCarry), // BGEU
        _ => None,
    }
}
