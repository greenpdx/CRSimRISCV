//! Mirrors `src/data/aluData.ts` — ALU op table + alu_compute().

#[derive(Debug, Clone, Copy)]
pub struct AluOpDef {
    pub code: u8,
    pub mnemonic: &'static str,
    pub expression: &'static str,
    pub is_branch_op: bool,
}

pub const ALU_OPS: &[AluOpDef] = &[
    AluOpDef { code: 0b0000, mnemonic: "AND",  expression: "A & B",            is_branch_op: false },
    AluOpDef { code: 0b0001, mnemonic: "OR",   expression: "A | B",            is_branch_op: false },
    AluOpDef { code: 0b0010, mnemonic: "ADD",  expression: "A + B",            is_branch_op: false },
    AluOpDef { code: 0b0011, mnemonic: "XOR",  expression: "A ^ B",            is_branch_op: false },
    AluOpDef { code: 0b0100, mnemonic: "SLL",  expression: "A << (B & 0x1f)",  is_branch_op: false },
    AluOpDef { code: 0b0101, mnemonic: "SRL",  expression: "A >> (B & 0x1f)",  is_branch_op: false },
    AluOpDef { code: 0b0110, mnemonic: "SUB",  expression: "A - B",            is_branch_op: true  },
    AluOpDef { code: 0b0111, mnemonic: "SLT",  expression: "signed A < B",     is_branch_op: true  },
    AluOpDef { code: 0b1000, mnemonic: "SLTU", expression: "unsigned A < B",   is_branch_op: true  },
    AluOpDef { code: 0b1001, mnemonic: "SRA",  expression: "A >>a (B & 0x1f)", is_branch_op: false },
    AluOpDef { code: 0b1010, mnemonic: "LUI",  expression: "B",                is_branch_op: false },
];

pub fn alu_op_by_code(code: u8) -> Option<&'static AluOpDef> {
    ALU_OPS.iter().find(|o| o.code == code)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AluFlags {
    pub zero: bool,
    pub negative: bool,
    pub carry: bool,
    pub overflow: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AluResult {
    pub result: u32,
    pub flags: AluFlags,
}

pub fn alu_compute(code: u8, a: u32, b: u32) -> AluResult {
    let ua = a;
    let ub = b;
    let sa = a as i32;
    let sb = b as i32;

    let result: u32 = match code {
        0b0000 => ua & ub,
        0b0001 => ua | ub,
        0b0010 => ua.wrapping_add(ub),
        0b0011 => ua ^ ub,
        0b0100 => ua.wrapping_shl(ub & 0x1f),
        0b0101 => ua.wrapping_shr(ub & 0x1f),
        0b0110 => ua.wrapping_sub(ub),
        0b0111 => if sa < sb { 1 } else { 0 },
        0b1000 => if ua < ub { 1 } else { 0 },
        0b1001 => (sa.wrapping_shr(ub & 0x1f)) as u32,
        0b1010 => ub,
        _ => 0,
    };

    let carry = match code {
        0b0010 => (ua as u64 + ub as u64) > 0xffff_ffff,
        0b0110 => ua < ub,
        _ => false,
    };

    let sr = result as i32;
    let overflow = match code {
        0b0010 => ((!(sa ^ sb) & (sa ^ sr)) & i32::MIN) != 0,
        0b0110 => (( (sa ^ sb) & (sa ^ sr)) & i32::MIN) != 0,
        _ => false,
    };

    AluResult {
        result,
        flags: AluFlags {
            zero: result == 0,
            negative: (result & 0x8000_0000) != 0,
            carry,
            overflow,
        },
    }
}
