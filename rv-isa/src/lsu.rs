//! Load / store lanes: select and extend a loaded value, place a stored
//! value in its byte lanes. `funct3` gives the width (B / H / W) and, for
//! loads, zero-extension (bit 2: LBU / LHU).

/// The value a load of width `funct3` at `addr` reads from `word`
/// (the aligned word holding `addr`), sign- or zero-extended.
pub const fn load(word: u32, addr: u32, funct3: u8) -> u32 {
    let byte_off = (addr & 0x3) * 8;
    match funct3 {
        0b000 => {
            let v = (word >> byte_off) & 0xff;
            if v & 0x80 != 0 { v | 0xffff_ff00 } else { v }
        }
        0b001 => {
            let v = (word >> byte_off) & 0xffff;
            if v & 0x8000 != 0 { v | 0xffff_0000 } else { v }
        }
        0b100 => (word >> byte_off) & 0xff,
        0b101 => (word >> byte_off) & 0xffff,
        _     => word,
    }
}

/// Store: (wdata, byte enables) — `rs2` replicated into its lane(s), byte
/// enables from the width (funct3) and `addr[1:0]`.
pub const fn store(rs2: u32, addr: u32, funct3: u8) -> (u32, u8) {
    let off = addr & 3;
    match funct3 & 3 {
        0b00 => {
            let b = rs2 & 0xff;
            (b | b << 8 | b << 16 | b << 24, 1u8 << off)
        }
        0b01 => {
            let h = rs2 & 0xffff;
            (h | (h << 16), if off & 2 != 0 { 0b1100 } else { 0b0011 })
        }
        _ => (rs2, 0b1111),
    }
}

/// Word mask of the byte enables `bw` (bit i = byte i).
pub const fn byte_mask(bw: u8) -> u32 {
    let mut m = 0u32;
    let mut i = 0;
    while i < 4 {
        if bw >> i & 1 != 0 { m |= 0xff << (8 * i); }
        i += 1;
    }
    m
}

/// Word `old` after a store of `rs2` at `addr` with width `funct3`
/// (SB / SH replace only their byte lanes).
pub const fn store_merge(old: u32, rs2: u32, addr: u32, funct3: u8) -> u32 {
    let (wdata, bw) = store(rs2, addr, funct3);
    let mask = byte_mask(bw);
    (old & !mask) | (wdata & mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads() {
        let w = 0x89ab_cdef;
        assert_eq!(load(w, 0, 0b000), 0xffff_ffef);   // lb
        assert_eq!(load(w, 1, 0b100), 0xcd);          // lbu
        assert_eq!(load(w, 2, 0b001), 0xffff_89ab);   // lh
        assert_eq!(load(w, 0, 0b101), 0xcdef);        // lhu
        assert_eq!(load(w, 0, 0b010), w);             // lw
    }

    #[test]
    fn stores() {
        assert_eq!(store(0x1234_56ab, 3, 0b000), (0xabab_abab, 0b1000));
        assert_eq!(store(0x1234_5678, 2, 0b001), (0x5678_5678, 0b1100));
        assert_eq!(store(0x1234_5678, 0, 0b010), (0x1234_5678, 0b1111));
        assert_eq!(store_merge(0x89ab_c5ef, 0x89ab_c5ef, 6, 0b001), 0xc5ef_c5ef);
        assert_eq!(store_merge(0, 0x89ab_c5ef, 6, 0b001), 0xc5ef_0000);
        assert_eq!(store_merge(0xffff_ffff, 0, 1, 0b000), 0xffff_00ff);
    }
}
