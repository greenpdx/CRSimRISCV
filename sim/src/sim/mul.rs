//! Zmmul multiplier, combinational (single-cycle core).

/// Combinational multiplier. op = funct3[1:0]:
/// 0 MUL (low 32) · 1 MULH (s×s) · 2 MULHSU (s×u) · 3 MULHU (u×u).
pub fn mul_comb(rs1: u32, rs2: u32, op: u8) -> u32 {
    let a = if op & 3 == 1 || op & 3 == 2 { rs1 as i32 as i64 } else { rs1 as i64 };
    let b = if op & 3 == 1 { rs2 as i32 as i64 } else { rs2 as i64 };
    let p = (a as i128 * b as i128) as u64;
    if op & 3 == 0 { p as u32 } else { (p >> 32) as u32 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_family() {
        assert_eq!(mul_comb(7, 6, 0), 42);
        assert_eq!(mul_comb(0xffff_ffff, 0xffff_ffff, 0), 1);
        assert_eq!(mul_comb(0xffff_ffff, 0xffff_ffff, 1), 0);            // -1 × -1 high
        assert_eq!(mul_comb(0xffff_ffff, 0xffff_ffff, 2), 0xffff_ffff);  // -1 × 2^32-1 high
        assert_eq!(mul_comb(0xffff_ffff, 0xffff_ffff, 3), 0xffff_fffe);  // unsigned high
    }
}
