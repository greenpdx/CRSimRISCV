//! Shared sign/zero extension for load instructions (LB/LH/LW/LBU/LHU).

pub fn sign_extend_load(raw: u32, addr: u32, funct3: u8) -> u32 {
    let byte_off = (addr & 0x3) * 8;
    match funct3 {
        0b000 => {
            let v = (raw >> byte_off) & 0xff;
            if v & 0x80 != 0 { v | 0xffff_ff00 } else { v }
        }
        0b001 => {
            let v = (raw >> byte_off) & 0xffff;
            if v & 0x8000 != 0 { v | 0xffff_0000 } else { v }
        }
        0b010 => raw,
        0b100 => (raw >> byte_off) & 0xff,
        0b101 => (raw >> byte_off) & 0xffff,
        _     => raw,
    }
}
