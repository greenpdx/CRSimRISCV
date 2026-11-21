//! Example programs for the out-of-order panel. Code is at address 0;
//! data at 0x1000. Each ends in `j .`.

pub struct OooExample {
    pub name: &'static str,
    /// What to watch for.
    pub about: &'static str,
    pub prog: Vec<u32>,
    /// (address, word) loaded before the run
    pub data: Vec<(u32, u32)>,
}

/// RV32I + Zmmul encoders.
pub mod enc {
    pub fn r(f7: u32, rs2: u32, rs1: u32, f3: u32, rd: u32, op: u32) -> u32 { f7 << 25 | rs2 << 20 | rs1 << 15 | f3 << 12 | rd << 7 | op }
    pub fn i(imm: i32, rs1: u32, f3: u32, rd: u32, op: u32) -> u32 { ((imm as u32) & 0xfff) << 20 | rs1 << 15 | f3 << 12 | rd << 7 | op }
    pub fn s(imm: i32, rs2: u32, rs1: u32, f3: u32) -> u32 { let m = imm as u32; ((m >> 5) & 0x7f) << 25 | rs2 << 20 | rs1 << 15 | f3 << 12 | (m & 0x1f) << 7 | 0x23 }
    pub fn b(off: i32, rs1: u32, rs2: u32, f3: u32) -> u32 {
        let o = off as u32;
        ((o >> 12) & 1) << 31 | ((o >> 5) & 0x3f) << 25 | rs2 << 20 | rs1 << 15 | f3 << 12 | ((o >> 1) & 0xf) << 8 | ((o >> 11) & 1) << 7 | 0x63
    }
    pub fn jal(rd: u32, off: i32) -> u32 { let o = off as u32; ((o >> 20) & 1) << 31 | ((o >> 1) & 0x3ff) << 21 | ((o >> 11) & 1) << 20 | ((o >> 12) & 0xff) << 12 | rd << 7 | 0x6f }
    pub fn jalr(rd: u32, rs1: u32, off: i32) -> u32 { i(off, rs1, 0, rd, 0x67) }
    pub fn addi(rd: u32, rs1: u32, imm: i32) -> u32 { i(imm, rs1, 0, rd, 0x13) }
    pub fn add(rd: u32, a: u32, b: u32) -> u32 { r(0, b, a, 0, rd, 0x33) }
    pub fn sub(rd: u32, a: u32, b: u32) -> u32 { r(0x20, b, a, 0, rd, 0x33) }
    pub fn mul(rd: u32, a: u32, b: u32) -> u32 { r(1, b, a, 0, rd, 0x33) }
    pub fn lui(rd: u32, imm20: u32) -> u32 { imm20 << 12 | rd << 7 | 0x37 }
    pub fn lw(rd: u32, rs1: u32, off: i32) -> u32 { i(off, rs1, 2, rd, 0x03) }
    pub fn sw(rs2: u32, rs1: u32, off: i32) -> u32 { s(off, rs2, rs1, 2) }
    /// bne rs1, rs2, off
    pub fn bne(rs1: u32, rs2: u32, off: i32) -> u32 { b(off, rs1, rs2, 1) }
    pub fn beq(rs1: u32, rs2: u32, off: i32) -> u32 { b(off, rs1, rs2, 0) }
    pub fn csrr(rd: u32, csr: i32) -> u32 { i(csr, 0, 2, rd, 0x73) }
    pub fn csrw(csr: i32, rs1: u32) -> u32 { i(csr, rs1, 1, 0, 0x73) }
    pub const SPIN: u32 = 0x0000_006f;
    pub const NOP: u32 = 0x0000_0013;
    pub const ECALL: u32 = 0x0000_0073;
    pub const MRET: u32 = 0x3020_0073;
}
use enc::*;

pub fn examples() -> Vec<OooExample> {
    vec![
        OooExample {
            name: "Out-of-order completion",
            about: "The load (3 cycles) and the MUL (3 cycles) start first. add x5 needs x4 from the load and waits in the ALU RS; addi x7 is independent and runs past it. Watch completion out of order on the CDBs, and commit strictly in order at the ROB head.",
            prog: vec![lui(10, 1), addi(1, 0, 1), addi(2, 0, 6), addi(3, 0, 7),
                lw(4, 10, 0), mul(6, 2, 3), add(5, 4, 1), addi(7, 0, 5), sub(8, 6, 7), sw(8, 10, 4), SPIN],
            data: vec![(0x1000, 41)],
        },
        OooExample {
            name: "Loop — the predictor learns",
            about: "Sums 1..12. The first time round, the BTB has no entry for the bne, so fetch goes straight on and the branch unit flushes. After that the BHT counter says taken and the loop runs without flushes — until the exit, which is mispredicted once.",
            prog: vec![addi(1, 0, 12), addi(5, 0, 0), add(5, 5, 1), addi(1, 1, -1), bne(1, 0, -8), sw(5, 0, 0x1000), SPIN],
            data: vec![],
        },
        OooExample {
            name: "Mispredict — flush at execute",
            about: "beq x1, x2 is taken, but the BTB is cold, so fetch runs down the wrong path (addi x3 / x4 = 99) and those are dispatched. When the branch executes it flushes them: they leave the ROB and RS, the RAT is rebuilt, and fetch restarts at the target. x3 / x4 never see 99.",
            prog: vec![addi(1, 0, 1), addi(2, 0, 1), beq(1, 2, 12), addi(3, 0, 99), addi(4, 0, 99), addi(3, 3, 7), add(4, 3, 1), SPIN],
            data: vec![],
        },
        OooExample {
            name: "Store → load forwarding",
            about: "Two dependent MULs hold the ROB head, so the store (address and data ready) has to wait in the LSQ. The load to the same address takes the store's data straight from the LSQ instead of memory.",
            prog: vec![lui(10, 1), addi(5, 0, 0x55), mul(9, 5, 5), mul(9, 9, 9), sw(5, 10, 8), lw(6, 10, 8), addi(7, 6, 1), SPIN],
            data: vec![],
        },
        OooExample {
            name: "MUL chain — independent work runs past",
            about: "Four dependent MULs (3 cycles each) form a long chain. The independent addi / add after them dispatch, execute and finish meanwhile; they fill the ROB and wait there to commit behind the chain.",
            prog: vec![addi(1, 0, 3), mul(2, 1, 1), mul(2, 2, 1), mul(2, 2, 1), mul(2, 2, 1),
                addi(3, 0, 1), addi(4, 3, 1), addi(5, 4, 1), addi(6, 5, 1), add(7, 6, 6), add(8, 7, 7), add(9, 8, 8), SPIN],
            data: vec![],
        },
        OooExample {
            name: "Precise trap — ECALL",
            about: "ECALL waits for an empty ROB, then traps at commit: mepc = its address, mcause = 11, everything younger is thrown away, fetch goes to the handler at 0x40. The handler steps mepc past the ECALL and MRETs; the code after it runs exactly once.",
            prog: {
                let mut p = vec![addi(9, 0, 0x40), csrw(0x305, 9), addi(1, 0, 1), ECALL, addi(2, 0, 2), addi(3, 2, 1), SPIN];
                p.resize(16, NOP);
                p.extend([csrr(21, 0x341), addi(21, 21, 4), csrw(0x341, 21), addi(20, 20, 1), MRET]);
                p
            },
            data: vec![],
        },
        OooExample {
            name: "Function call — JAL / JALR",
            about: "JAL writes the return address (x1) through a CDB like any result; JALR returns. The first time, the BTB has neither, so both are mispredicted and flushed at execute.",
            prog: vec![addi(10, 0, 5), jal(1, 12), addi(11, 10, 1), SPIN, add(10, 10, 10), jalr(0, 1, 0)],
            data: vec![],
        },
    ]
}
