//! Mirrors `src/data/immData.ts` — extract_immediate() across 5 RV32I formats.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImmSel { I = 0, S = 1, B = 2, U = 3, J = 4, None = 5 }

impl ImmSel {
    pub fn from_u8(v: u8) -> ImmSel {
        match v {
            0 => ImmSel::I, 1 => ImmSel::S, 2 => ImmSel::B,
            3 => ImmSel::U, 4 => ImmSel::J, _ => ImmSel::None,
        }
    }
}

fn sign_extend(value: u32, bits: u32) -> i32 {
    let shift = 32 - bits;
    ((value << shift) as i32) >> shift
}

pub fn extract_immediate(inst: u32, sel: ImmSel) -> i32 {
    match sel {
        ImmSel::I => { // inst[31:20]
            let raw = (inst >> 20) & 0xfff;
            sign_extend(raw, 12)
        }
        ImmSel::S => { // inst[31:25] | inst[11:7]
            let raw = ((inst >> 25) & 0x7f) << 5 | ((inst >> 7) & 0x1f);
            sign_extend(raw, 12)
        }
        ImmSel::B => { // inst[31]|inst[7]|inst[30:25]|inst[11:8], LSB=0
            let bit12   = (inst >> 31) & 0x1;
            let bit11   = (inst >>  7) & 0x1;
            let bit10_5 = (inst >> 25) & 0x3f;
            let bit4_1  = (inst >>  8) & 0xf;
            let raw = (bit12 << 12) | (bit11 << 11) | (bit10_5 << 5) | (bit4_1 << 1);
            sign_extend(raw, 13)
        }
        ImmSel::U => { // inst[31:12] << 12
            (inst & 0xffff_f000) as i32
        }
        ImmSel::J => { // inst[31]|inst[19:12]|inst[20]|inst[30:21], LSB=0
            let bit20    = (inst >> 31) & 0x1;
            let bit19_12 = (inst >> 12) & 0xff;
            let bit11    = (inst >> 20) & 0x1;
            let bit10_1  = (inst >> 21) & 0x3ff;
            let raw = (bit20 << 20) | (bit19_12 << 12) | (bit11 << 11) | (bit10_1 << 1);
            sign_extend(raw, 21)
        }
        ImmSel::None => 0,
    }
}
