//! Fixed instruction fields (same bit positions in every 32-bit format).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstrFields {
    pub opcode:    u8,
    pub rd:        u8,
    pub funct3:    u8,
    pub rs1:       u8,
    pub rs2:       u8,
    pub funct7:    u8,
    /// instr[30]: SUB / SRA / SRAI select
    pub funct7b30: u8,
}

pub const fn extract_fields(instr: u32) -> InstrFields {
    InstrFields {
        opcode:    (instr & 0x7f) as u8,
        rd:        ((instr >>  7) & 0x1f) as u8,
        funct3:    ((instr >> 12) & 0x07) as u8,
        rs1:       ((instr >> 15) & 0x1f) as u8,
        rs2:       ((instr >> 20) & 0x1f) as u8,
        funct7:    ((instr >> 25) & 0x7f) as u8,
        funct7b30: ((instr >> 30) & 0x1) as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_of_sub() {
        // sub x5, x6, x7
        let f = extract_fields(0x4073_02b3);
        assert_eq!((f.opcode, f.rd, f.funct3, f.rs1, f.rs2, f.funct7, f.funct7b30), (0x33, 5, 0, 6, 7, 0x20, 1));
    }
}
