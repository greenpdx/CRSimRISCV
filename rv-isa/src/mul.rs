//! Zmmul: MUL / MULH / MULHSU / MULHU (combinational reference).

/// `op` = funct3[1:0]: 0 MUL (low 32) · 1 MULH (s×s) · 2 MULHSU (s×u) · 3 MULHU (u×u).
pub const fn mul(op: u8, a: u32, b: u32) -> u32 {
    let sa = if op & 3 == 1 || op & 3 == 2 { a as i32 as i64 } else { a as i64 };
    let sb = if op & 3 == 1 { b as i32 as i64 } else { b as i64 };
    let p = (sa as i128 * sb as i128) as u64;
    if op & 3 == 0 { p as u32 } else { (p >> 32) as u32 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_family() {
        assert_eq!(mul(0, 7, 6), 42);
        assert_eq!(mul(0, 0xffff_ffff, 0xffff_ffff), 1);
        assert_eq!(mul(1, 0xffff_ffff, 0xffff_ffff), 0);            // -1 × -1
        assert_eq!(mul(2, 0xffff_ffff, 0xffff_ffff), 0xffff_ffff);  // -1 × (2^32 - 1)
        assert_eq!(mul(3, 0xffff_ffff, 0xffff_ffff), 0xffff_fffe);
        assert_eq!(mul(1, 0x8000_0000, 0x8000_0000), 0x4000_0000);
    }
}
