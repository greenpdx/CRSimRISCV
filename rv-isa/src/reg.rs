//! Integer register names.

/// ABI names of x0..x31.
pub const ABI_NAMES: [&str; 32] = [
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2",
    "s0", "s1", "a0", "a1", "a2", "a3", "a4", "a5",
    "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7",
    "s8", "s9", "s10", "s11", "t3", "t4", "t5", "t6",
];

/// ABI name of register `i` (0..31).
pub fn abi_name(i: u8) -> &'static str { ABI_NAMES[(i & 31) as usize] }

/// Register number of `name`: `x0`..`x31`, an ABI name, or `fp` (= s0).
/// Case-insensitive, surrounding spaces ignored.
pub fn reg_number(name: &str) -> Option<u32> {
    let name = name.trim().to_lowercase();
    if let Some(n) = name.strip_prefix('x').and_then(|d| d.parse::<u32>().ok()) {
        return (n <= 31).then_some(n);
    }
    if name == "fp" { return Some(8); }
    ABI_NAMES.iter().position(|&n| n == name).map(|i| i as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(reg_number("x31"), Some(31));
        assert_eq!(reg_number(" A0 "), Some(10));
        assert_eq!(reg_number("fp"), Some(8));
        assert_eq!(reg_number("x32"), None);
        assert_eq!(reg_number("t7"), None);
        assert_eq!(abi_name(8), "s0");
        for i in 0..32u8 { assert_eq!(reg_number(abi_name(i)), Some(i as u32)); }
    }
}
