//! Load / store unit: lane select, extension, byte enables.

use crate::data::load_width::sign_extend_load;

/// Load: pick the lane of `rdata` by `addr[1:0]`, sign- / zero-extend (funct3).
pub fn load_data(rdata: u32, addr: u32, funct3: u8) -> u32 {
    sign_extend_load(rdata, addr, funct3)
}

/// Store: (wdata_o, bw_o) — rs2 replicated into its lane(s), byte enables
/// from the width (funct3) and `addr[1:0]`.
pub fn store_data(rs2: u32, addr: u32, funct3: u8) -> (u32, u8) {
    let off = addr & 3;
    match funct3 & 3 {
        0b00 => (u32::from_ne_bytes([rs2 as u8; 4]), 1u8 << off),
        0b01 => {
            let h = rs2 & 0xffff;
            (h | (h << 16), if off & 2 != 0 { 0b1100 } else { 0b0011 })
        }
        _ => (rs2, 0b1111),
    }
}

/// Word `old` after a store of `rs2` at `addr` with width `funct3`
/// (SB / SH replace only their byte lanes).
pub fn store_merge(old: u32, rs2: u32, addr: u32, funct3: u8) -> u32 {
    let (wdata, bw) = store_data(rs2, addr, funct3);
    let mask = (0..4).filter(|i| bw >> i & 1 != 0).fold(0u32, |m, i| m | 0xff << (8 * i));
    (old & !mask) | (wdata & mask)
}
