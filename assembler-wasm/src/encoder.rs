/// Encode R-type: funct7[6:0] | rs2[4:0] | rs1[4:0] | funct3[2:0] | rd[4:0] | opcode[6:0]
pub fn encode_r(opcode: u32, rd: u32, funct3: u32, rs1: u32, rs2: u32, funct7: u32) -> u32 {
    (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | opcode
}

/// Encode I-type: imm[11:0] | rs1[4:0] | funct3[2:0] | rd[4:0] | opcode[6:0]
pub fn encode_i(opcode: u32, rd: u32, funct3: u32, rs1: u32, imm: i32) -> u32 {
    let imm = (imm as u32) & 0xFFF;
    (imm << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | opcode
}

/// Encode S-type: imm[11:5] | rs2[4:0] | rs1[4:0] | funct3[2:0] | imm[4:0] | opcode[6:0]
pub fn encode_s(opcode: u32, funct3: u32, rs1: u32, rs2: u32, imm: i32) -> u32 {
    let imm = (imm as u32) & 0xFFF;
    let imm_11_5 = (imm >> 5) & 0x7F;
    let imm_4_0 = imm & 0x1F;
    (imm_11_5 << 25) | (rs2 << 20) | (rs1 << 15) | (funct3 << 12) | (imm_4_0 << 7) | opcode
}

/// Encode B-type: imm[12|10:5] | rs2[4:0] | rs1[4:0] | funct3[2:0] | imm[4:1|11] | opcode[6:0]
pub fn encode_b(opcode: u32, funct3: u32, rs1: u32, rs2: u32, imm: i32) -> u32 {
    let imm = (imm as u32) & 0x1FFE; // bits [12:1], bit 0 always 0
    let bit_12 = (imm >> 12) & 1;
    let bit_11 = (imm >> 11) & 1;
    let bits_10_5 = (imm >> 5) & 0x3F;
    let bits_4_1 = (imm >> 1) & 0xF;
    (bit_12 << 31) | (bits_10_5 << 25) | (rs2 << 20) | (rs1 << 15)
        | (funct3 << 12) | (bits_4_1 << 8) | (bit_11 << 7) | opcode
}

/// Encode U-type: imm[31:12] | rd[4:0] | opcode[6:0]
pub fn encode_u(opcode: u32, rd: u32, imm: i32) -> u32 {
    let imm = (imm as u32) & 0xFFFFF000;
    imm | (rd << 7) | opcode
}

/// Encode J-type: imm[20|10:1|11|19:12] | rd[4:0] | opcode[6:0]
pub fn encode_j(opcode: u32, rd: u32, imm: i32) -> u32 {
    let imm = imm as u32;
    let bit_20 = (imm >> 20) & 1;
    let bits_10_1 = (imm >> 1) & 0x3FF;
    let bit_11 = (imm >> 11) & 1;
    let bits_19_12 = (imm >> 12) & 0xFF;
    (bit_20 << 31) | (bits_10_1 << 21) | (bit_11 << 20) | (bits_19_12 << 12) | (rd << 7) | opcode
}

/// Encode shift I-type (SLLI, SRLI, SRAI): shamt in imm[4:0], funct7 in imm[11:5]
pub fn encode_i_shift(opcode: u32, rd: u32, funct3: u32, rs1: u32, shamt: u32, funct7: u32) -> u32 {
    let imm = (funct7 << 5) | (shamt & 0x1F);
    (imm << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | opcode
}
