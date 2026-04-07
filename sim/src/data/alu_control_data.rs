//! Mirrors `src/data/aluControlData.ts` — compute_alu_control() truth table.

pub fn compute_alu_control(alu_op: u8, funct3: u8, funct7b30: u8, is_lui: bool) -> u8 {
    if alu_op == 0 { return 0b0010; } // force ADD
    if alu_op == 1 { return 0b0110; } // force SUB
    if is_lui      { return 0b1010; } // LUI pass-through

    // alu_op == 2: decode funct3 + funct7[30]
    let f3 = funct3 & 0x7;
    let f7 = funct7b30 & 0x1;
    match (f3, f7) {
        (0b000, 0) => 0b0010, // ADD / ADDI
        (0b000, 1) => 0b0110, // SUB
        (0b001, _) => 0b0100, // SLL / SLLI
        (0b010, _) => 0b0111, // SLT / SLTI
        (0b011, _) => 0b1000, // SLTU / SLTIU
        (0b100, _) => 0b0011, // XOR / XORI
        (0b101, 0) => 0b0101, // SRL / SRLI
        (0b101, 1) => 0b1001, // SRA / SRAI
        (0b110, _) => 0b0001, // OR / ORI
        (0b111, _) => 0b0000, // AND / ANDI
        _ => 0b0010,
    }
}
