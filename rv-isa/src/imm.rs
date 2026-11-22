//! Immediates of the five RV32I formats, sign-extended.

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImmSel { I = 0, S = 1, B = 2, U = 3, J = 4, #[default] None = 5 }

impl ImmSel {
    pub const fn from_u8(v: u8) -> ImmSel {
        match v {
            0 => ImmSel::I, 1 => ImmSel::S, 2 => ImmSel::B,
            3 => ImmSel::U, 4 => ImmSel::J, _ => ImmSel::None,
        }
    }
}

const fn sign_extend(value: u32, bits: u32) -> i32 {
    let shift = 32 - bits;
    ((value << shift) as i32) >> shift
}

/// I-type: inst[31:20]
pub const fn imm_i(inst: u32) -> i32 { (inst as i32) >> 20 }

/// S-type: inst[31:25] | inst[11:7]
pub const fn imm_s(inst: u32) -> i32 {
    sign_extend(((inst >> 25) & 0x7f) << 5 | ((inst >> 7) & 0x1f), 12)
}

/// B-type: inst[31] | inst[7] | inst[30:25] | inst[11:8], bit 0 = 0
pub const fn imm_b(inst: u32) -> i32 {
    let raw = ((inst >> 31) & 1) << 12 | ((inst >> 7) & 1) << 11 | ((inst >> 25) & 0x3f) << 5 | ((inst >> 8) & 0xf) << 1;
    sign_extend(raw, 13)
}

/// U-type: inst[31:12] << 12
pub const fn imm_u(inst: u32) -> i32 { (inst & 0xffff_f000) as i32 }

/// J-type: inst[31] | inst[19:12] | inst[20] | inst[30:21], bit 0 = 0
pub const fn imm_j(inst: u32) -> i32 {
    let raw = ((inst >> 31) & 1) << 20 | ((inst >> 12) & 0xff) << 12 | ((inst >> 20) & 1) << 11 | ((inst >> 21) & 0x3ff) << 1;
    sign_extend(raw, 21)
}

/// Immediate of `inst` in format `sel` (0 for `None`).
pub const fn extract_immediate(inst: u32, sel: ImmSel) -> i32 {
    match sel {
        ImmSel::I => imm_i(inst),
        ImmSel::S => imm_s(inst),
        ImmSel::B => imm_b(inst),
        ImmSel::U => imm_u(inst),
        ImmSel::J => imm_j(inst),
        ImmSel::None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i_type() {
        assert_eq!(extract_immediate(0x005_00013, ImmSel::I), 5);      // addi x0, x0, 5
        assert_eq!(extract_immediate(0xfff_00013, ImmSel::I), -1);     // addi x0, x0, -1
    }

    #[test]
    fn s_type() {
        // sw x2, 12(x1): imm[11:5] = 0, imm[4:0] = 12
        let inst = (2 << 20) | (1 << 15) | (2 << 12) | (12 << 7) | 0x23;
        assert_eq!(extract_immediate(inst, ImmSel::S), 12);
        assert_eq!(extract_immediate(0xfe00_0fa3, ImmSel::S), -1);    // sw x0, -1(x0)
    }

    #[test]
    fn b_type() {
        // beq x0, x0, +16: imm[4:1] = 8
        let inst = (8 << 8) | 0x63;
        assert_eq!(extract_immediate(inst, ImmSel::B), 16);
        assert_eq!(extract_immediate(0xfe00_0ee3, ImmSel::B), -4);    // beq x0, x0, -4
    }

    #[test]
    fn u_type() {
        assert_eq!(extract_immediate(0x1234_5037, ImmSel::U) as u32, 0x1234_5000);
    }

    #[test]
    fn j_type() {
        // jal x0, +16: imm[10:1] = 8
        let inst = (8 << 21) | 0x6f;
        assert_eq!(extract_immediate(inst, ImmSel::J), 16);
        assert_eq!(extract_immediate(0xffdf_f06f, ImmSel::J), -4);    // jal x0, -4
    }

    #[test]
    fn none_is_zero() {
        assert_eq!(extract_immediate(0xdead_beef, ImmSel::None), 0);
    }

    #[test]
    fn default_is_none() {
        assert_eq!(ImmSel::default(), ImmSel::None);
    }
}
