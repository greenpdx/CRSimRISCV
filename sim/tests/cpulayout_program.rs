//! Sanity-check that the CpuLayout default program disassembles back to the
//! intended mnemonic for every instruction. Catches hand-encoding mistakes.

use cpu_sim::disasm::disassemble;

const PROG: &[(u32, &str, &str)] = &[
    (0x00010137, "LUI",   "x2, 0x10"),
    (0x10010113, "ADDI",  "x2, x2, 256"),
    (0x00700293, "ADDI",  "x5, x0, 7"),
    (0x00500313, "ADDI",  "x6, x0, 5"),
    (0x006283b3, "ADD",   "x7, x5, x6"),
    (0x40628433, "SUB",   "x8, x5, x6"),
    (0x0062f4b3, "AND",   "x9, x5, x6"),
    (0x0062e533, "OR",    "x10, x5, x6"),
    (0x0062c5b3, "XOR",   "x11, x5, x6"),
    (0x00629633, "SLL",   "x12, x5, x6"),
    (0x0063d6b3, "SRL",   "x13, x7, x6"),
    (0x4062d733, "SRA",   "x14, x5, x6"),
    (0x0062a7b3, "SLT",   "x15, x5, x6"),
    (0x00533833, "SLTU",  "x16, x6, x5"),
    (0xfff00893, "ADDI",  "x17, x0, -1"),
    (0x0ff8f913, "ANDI",  "x18, x17, 255"),
    (0x00a06993, "ORI",   "x19, x0, 10"),
    (0x00429a13, "SLLI",  "x20, x5, 4"),
    (0x0088da93, "SRLI",  "x21, x17, 8"),
    (0x4088db13, "SRAI",  "x22, x17, 8"),
    (0x00010bb7, "LUI",   "x23, 0x10"),
    (0x05500c13, "ADDI",  "x24, x0, 85"),
    (0x018b8023, "SB",    "x24, 0(x23)"),
    (0x06600c93, "ADDI",  "x25, x0, 102"),
    (0x019b80a3, "SB",    "x25, 1(x23)"),
    (0x000b9d03, "LH",    "x26, 0(x23)"),
    (0x000bad83, "LW",    "x27, 0(x23)"),
    (0x01bba223, "SW",    "x27, 4(x23)"),
    (0x00528463, "BEQ",   "x5, x5, 8"),
    (0x00629463, "BNE",   "x5, x6, 8"),
    (0x00534463, "BLT",   "x6, x5, 8"),
    (0x0062d463, "BGE",   "x5, x6, 8"),
    (0x00536463, "BLTU",  "x6, x5, 8"),
    (0x0062f463, "BGEU",  "x5, x6, 8"),
    (0xff010113, "ADDI",  "x2, x2, -16"),
    (0x00112023, "SW",    "x1, 0(x2)"),
    (0x00400513, "ADDI",  "x10, x0, 4"),
    (0x00600593, "ADDI",  "x11, x0, 6"),
    (0x02c000ef, "JAL",   "x1, 44"),
    (0x00012083, "LW",    "x1, 0(x2)"),
    (0x01010113, "ADDI",  "x2, x2, 16"),
    (0x00050e93, "ADDI",  "x29, x10, 0"),
    (0x04200f13, "ADDI",  "x30, x0, 66"),
    (0x340f1073, "CSRRW", "x0, mscratch(0x340), x30"),
    (0x34002ff3, "CSRRS", "x31, mscratch(0x340), x0"),
    (0x40020237, "LUI",   "x4, 0x40020"),
    (0x0aa00193, "ADDI",  "x3, x0, 170"),
    (0x00322023, "SW",    "x3, 0(x4)"),
    (0x0000006f, "JAL",   "x0, 0"),
    (0x00159593, "SLLI",  "x11, x11, 1"),
    (0x00b50533, "ADD",   "x10, x10, x11"),
    (0x00008067, "JALR",  "x0, 0(x1)"),
];

#[test]
fn cpulayout_default_program_disassembles_correctly() {
    let mut failures = Vec::new();
    for (i, &(word, want_mnem, want_ops)) in PROG.iter().enumerate() {
        let d = disassemble(word);
        if d.mnemonic != want_mnem || d.operands != want_ops {
            failures.push(format!(
                "[{i:02}] 0x{word:08x}: expected `{want_mnem} {want_ops}`, got `{} {}`",
                d.mnemonic, d.operands
            ));
        }
    }
    if !failures.is_empty() {
        panic!("\n{}\n", failures.join("\n"));
    }
}
