//! RV32I disassembler — dispatches to per-extension submodules.
//!
//! Submodules are tried in priority order so an extension can override the
//! base ISA's interpretation of a shared opcode. Each submodule returns
//! `Option<Disasm>` so the dispatcher can fall through.

mod rv32i;
mod m;
mod a;
mod b;
mod zk;
// Extensions no core implements (compressed, F / D / Zfh / Q, hypervisor,
// vector): the browser build leaves them out (feature `disasm-ext`).
#[cfg(feature = "disasm-ext")] mod fp;
#[cfg(feature = "disasm-ext")] mod c;
#[cfg(feature = "disasm-ext")] mod h;
#[cfg(feature = "disasm-ext")] mod v;
mod system;
mod misc_mem;
pub mod descriptions;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disasm {
    pub class:    &'static str,
    pub mnemonic: String,
    pub operands: String,
}

impl Disasm {
    pub fn nop() -> Self {
        Disasm { class: "I-ALU", mnemonic: "NOP".into(), operands: String::new() }
    }
    pub fn unknown(opcode: u8) -> Self {
        Disasm { class: "?", mnemonic: "?".into(), operands: format!("op=0b{:07b}", opcode) }
    }
}

/// Format an integer register index as `xN`.
pub(crate) fn r(i: u8) -> String { format!("x{}", i) }
/// Format a floating-point register index as `fN`.
#[cfg(feature = "disasm-ext")]
pub(crate) fn fr(i: u8) -> String { format!("f{}", i) }

pub fn disassemble(instr: u32) -> Disasm {
    // Compressed: low 2 bits ≠ 11 → 16-bit instruction lives in instr[15:0].
    if instr & 0x3 != 0x3 {
        #[cfg(feature = "disasm-ext")]
        return c::decode16(instr as u16);
        #[cfg(not(feature = "disasm-ext"))]
        return Disasm::unknown((instr & 0x7f) as u8);
    }
    // Order matters: extensions override base shared opcodes.
    // - Zk before B (PACK/PACKH/BREV8/XPERM share Zbb-style opcodes).
    // - H  before system (HLV/HSV/HFENCE share SYSTEM opcode).
    if let Some(d) = m::try_decode(instr)        { return d; }
    if let Some(d) = a::try_decode(instr)        { return d; }
    if let Some(d) = zk::try_decode(instr)       { return d; }
    if let Some(d) = b::try_decode(instr)        { return d; }
    #[cfg(feature = "disasm-ext")]
    {
        if let Some(d) = v::try_decode(instr)    { return d; }
        if let Some(d) = fp::try_decode(instr)   { return d; }
        if let Some(d) = h::try_decode(instr)    { return d; }
    }
    if let Some(d) = system::decode(instr)       { return d; }
    if let Some(d) = misc_mem::decode(instr)     { return d; }
    if let Some(d) = rv32i::decode(instr)        { return d; }
    Disasm::unknown((instr & 0x7f) as u8)
}

/// Disassemble a 16-bit compressed instruction directly.
#[cfg(feature = "disasm-ext")]
pub fn disassemble16(half: u16) -> Disasm { c::decode16(half) }

#[cfg(test)]
mod tests {
    use super::*;

    fn d(instr: u32) -> Disasm { disassemble(instr) }
    fn mnem(instr: u32) -> String { d(instr).mnemonic }
    fn ops(instr: u32) -> String  { d(instr).operands }

    fn r_type(funct7: u32, rs2: u32, rs1: u32, f3: u32, rd: u32) -> u32 {
        (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | (f3 << 12) | (rd << 7) | 0b0110011
    }
    fn i_type(imm: u32, rs1: u32, f3: u32, rd: u32, opcode: u32) -> u32 {
        ((imm & 0xfff) << 20) | (rs1 << 15) | (f3 << 12) | (rd << 7) | opcode
    }

    // ── RV32I base regression ──────────────────────────────────────────
    #[test] fn addi_x4_x3_5() {
        let r = d(0x00518213);
        assert_eq!((r.class, r.mnemonic.as_str(), r.operands.as_str()),
                   ("I-ALU", "ADDI", "x4, x3, 5"));
    }
    #[test] fn lui_x5_0x10()  { let r = d(0x000102b7); assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("LUI",  "x5, 0x10")); }
    #[test] fn sw_x4_4_x5()   { let r = d(0x0042a223); assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("SW",   "x4, 4(x5)")); }
    #[test] fn nop_canonical(){ assert_eq!(mnem(0x00000013), "NOP"); }

    // ── M extension ────────────────────────────────────────────────────
    #[test] fn mul_x5_x6_x7() {
        let r = d(r_type(0b0000001, 7, 6, 0b000, 5));
        assert_eq!((r.class, r.mnemonic.as_str(), r.operands.as_str()), ("M", "MUL", "x5, x6, x7"));
    }
    #[test] fn zmmul_all_four() {
        // the words the assembler emits for mul / mulh / mulhsu / mulhu x10..x13, x5, x6
        for (w, m, ops) in [(0x0262_8533, "MUL", "x10, x5, x6"), (0x0262_95b3, "MULH", "x11, x5, x6"),
                            (0x0262_a633, "MULHSU", "x12, x5, x6"), (0x0262_b6b3, "MULHU", "x13, x5, x6")] {
            let r = d(w);
            assert_eq!((r.class, r.mnemonic.as_str(), r.operands.as_str()), ("M", m, ops));
        }
    }
    #[test] fn divu_x5_x6_x7() {
        assert_eq!(mnem(r_type(0b0000001, 7, 6, 0b101, 5)), "DIVU");
    }
    #[test] fn add_not_shadowed_by_m() {
        assert_eq!(mnem(r_type(0b0000000, 7, 6, 0b000, 5)), "ADD");
    }

    // ── A extension ────────────────────────────────────────────────────
    fn amo(funct5: u32, aq: u32, rl: u32, rs2: u32, rs1: u32, rd: u32) -> u32 {
        (funct5 << 27) | (aq << 26) | (rl << 25) | (rs2 << 20) | (rs1 << 15)
            | (0b010 << 12) | (rd << 7) | 0b0101111
    }
    #[test] fn lr_w() {
        let r = d(amo(0b00010, 0, 0, 0, 5, 6));
        assert_eq!((r.class, r.mnemonic.as_str(), r.operands.as_str()), ("A", "LR.W", "x6, (x5)"));
    }
    #[test] fn amoadd_w_aqrl() {
        let r = d(amo(0b00000, 1, 1, 7, 5, 6));
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("AMOADD.W", "x6, x7, (x5).aqrl"));
    }
    #[test] fn amoswap_w_rl() {
        assert_eq!(ops(amo(0b00001, 0, 1, 7, 5, 6)), "x6, x7, (x5).rl");
    }

    // ── System / Privileged ────────────────────────────────────────────
    #[test] fn system_basics() {
        assert_eq!(mnem(0x00000073), "ECALL");
        assert_eq!(mnem(0x00100073), "EBREAK");
        assert_eq!(mnem(0x30200073), "MRET");
        assert_eq!(mnem(0x10200073), "SRET");
        assert_eq!(mnem(0x10500073), "WFI");
    }

    // ── Zicsr ──────────────────────────────────────────────────────────
    #[test] fn csrrw_mstatus() {
        let instr = i_type(0x300, 6, 0b001, 5, 0b1110011);
        let r = d(instr);
        assert_eq!((r.class, r.mnemonic.as_str(), r.operands.as_str()),
                   ("Zicsr", "CSRRW", "x5, mstatus(0x300), x6"));
    }
    #[test] fn csrrsi_unknown_csr() {
        let instr = i_type(0x999, 3, 0b110, 5, 0b1110011);
        let r = d(instr);
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("CSRRSI", "x5, 0x999, 3"));
    }
    #[test] fn csrrw_seed_zkr() {
        // CSRRW x5, seed (0x015), x6 — Zkr entropy source
        let instr = i_type(0x015, 6, 0b001, 5, 0b1110011);
        let r = d(instr);
        assert_eq!(r.operands, "x5, seed(0x015), x6");
    }

    // ── MISC-MEM ───────────────────────────────────────────────────────
    #[test] fn fence_i() { assert_eq!(mnem(0x0000_100f), "FENCE.I"); }
    #[test] fn fence_rw_rw() {
        let instr = (0u32 << 28) | (0b0011 << 24) | (0b0011 << 20) | 0b0001111;
        let r = d(instr);
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("FENCE", "rw, rw"));
    }

    // ── B (Zba/Zbb/Zbc/Zbs) ────────────────────────────────────────────
    #[test] fn sh1add() {
        let r = d(r_type(0b0010000, 7, 6, 0b010, 5));
        assert_eq!((r.class, r.mnemonic.as_str()), ("Zba", "SH1ADD"));
    }
    #[test] fn andn() {
        let r = d(r_type(0b0100000, 7, 6, 0b111, 5));
        assert_eq!((r.class, r.mnemonic.as_str()), ("Zbb", "ANDN"));
    }
    #[test] fn min_zbb() {
        assert_eq!(mnem(r_type(0b0000101, 7, 6, 0b100, 5)), "MIN");
    }
    #[test] fn rol_ror() {
        assert_eq!(mnem(r_type(0b0110000, 7, 6, 0b001, 5)), "ROL");
        assert_eq!(mnem(r_type(0b0110000, 7, 6, 0b101, 5)), "ROR");
    }
    #[test] fn clmul_zbc() {
        let r = d(r_type(0b0000101, 7, 6, 0b001, 5));
        assert_eq!((r.class, r.mnemonic.as_str()), ("Zbc", "CLMUL"));
    }
    #[test] fn bset_zbs() {
        let r = d(r_type(0b0010100, 7, 6, 0b001, 5));
        assert_eq!((r.class, r.mnemonic.as_str()), ("Zbs", "BSET"));
    }
    #[test] fn clz_zbb_imm() {
        let instr = (0b0110000u32 << 25) | (0b00000 << 20) | (6 << 15) | (0b001 << 12) | (5 << 7) | 0b0010011;
        let r = d(instr);
        assert_eq!((r.class, r.mnemonic.as_str(), r.operands.as_str()), ("Zbb", "CLZ", "x5, x6"));
    }
    #[test] fn rori_zbb_imm() {
        let instr = (0b0110000u32 << 25) | (3 << 20) | (6 << 15) | (0b101 << 12) | (5 << 7) | 0b0010011;
        let r = d(instr);
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("RORI", "x5, x6, 3"));
    }
    #[test] fn bclri_zbs_imm() {
        let instr = (0b0100100u32 << 25) | (4 << 20) | (6 << 15) | (0b001 << 12) | (5 << 7) | 0b0010011;
        assert_eq!(mnem(instr), "BCLRI");
    }

    #[cfg(feature = "disasm-ext")]
    mod ext_fp_h {
    use super::*;
    // ── F / D / Zfh / Q ────────────────────────────────────────────────
    fn op_fp(op: u32, fmt: u32, rs2: u32, rs1: u32, f3: u32, rd: u32) -> u32 {
        let funct7 = (op << 2) | fmt;
        (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | (f3 << 12) | (rd << 7) | 0b1010011
    }
    #[test] fn flw() {
        let instr = i_type(8, 6, 0b010, 5, 0b0000111);
        let r = d(instr);
        assert_eq!((r.class, r.mnemonic.as_str(), r.operands.as_str()), ("F", "FLW", "f5, 8(x6)"));
    }
    #[test] fn fld() {
        let instr = i_type(0, 6, 0b011, 5, 0b0000111);
        let r = d(instr);
        assert_eq!((r.class, r.mnemonic.as_str()), ("D", "FLD"));
    }
    #[test] fn fsw() {
        // FSW f7, 12(x6) — encode S-type
        let imm = 12u32;
        let hi = (imm >> 5) & 0x7f;
        let lo = imm & 0x1f;
        let instr = (hi << 25) | (7 << 20) | (6 << 15) | (0b010 << 12) | (lo << 7) | 0b0100111;
        let r = d(instr);
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("FSW", "f7, 12(x6)"));
    }
    #[test] fn fadd_s() {
        let r = d(op_fp(0b00000, 0b00, 7, 6, 0, 5));
        assert_eq!((r.class, r.mnemonic.as_str(), r.operands.as_str()), ("F", "FADD.S", "f5, f6, f7"));
    }
    #[test] fn fadd_d() {
        let r = d(op_fp(0b00000, 0b01, 7, 6, 0, 5));
        assert_eq!((r.class, r.mnemonic.as_str()), ("D", "FADD.D"));
    }
    #[test] fn fsqrt_s() {
        let r = d(op_fp(0b01011, 0b00, 0, 6, 0, 5));
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("FSQRT.S", "f5, f6"));
    }
    #[test] fn fsgnj_d() {
        let r = d(op_fp(0b00100, 0b01, 7, 6, 0, 5));
        assert_eq!(r.mnemonic, "FSGNJ.D");
    }
    #[test] fn fmin_s_fmax_d() {
        assert_eq!(mnem(op_fp(0b00101, 0b00, 7, 6, 0, 5)), "FMIN.S");
        assert_eq!(mnem(op_fp(0b00101, 0b01, 7, 6, 1, 5)), "FMAX.D");
    }
    #[test] fn feq_s() {
        let r = d(op_fp(0b10100, 0b00, 7, 6, 0b010, 5));
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("FEQ.S", "x5, f6, f7"));
    }
    #[test] fn fcvt_w_s() {
        let r = d(op_fp(0b11000, 0b00, 0b00000, 6, 0, 5));
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("FCVT.W.S", "x5, f6"));
    }
    #[test] fn fcvt_s_w() {
        let r = d(op_fp(0b11010, 0b00, 0b00000, 6, 0, 5));
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("FCVT.S.W", "f5, x6"));
    }
    #[test] fn fcvt_d_s_between_fp() {
        // op=01000 fmt=01 (target=D) rs2=00 (source=S)
        let r = d(op_fp(0b01000, 0b01, 0b00000, 6, 0, 5));
        assert_eq!(r.mnemonic, "FCVT.D.S");
    }
    #[test] fn fmv_x_w() {
        let r = d(op_fp(0b11100, 0b00, 0, 6, 0b000, 5));
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("FMV.X.W", "x5, f6"));
    }
    #[test] fn fclass_s() {
        let r = d(op_fp(0b11100, 0b00, 0, 6, 0b001, 5));
        assert_eq!(r.mnemonic, "FCLASS.S");
    }
    #[test] fn fmv_w_x() {
        let r = d(op_fp(0b11110, 0b00, 0, 6, 0b000, 5));
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("FMV.W.X", "f5, x6"));
    }
    #[test] fn fmadd_s() {
        // FMADD.S f5, f6, f7, f8: opcode 1000011, fmt=00, rs3=8
        let instr = (8u32 << 27) | (0b00 << 25) | (7 << 20) | (6 << 15)
                  | (0 << 12) | (5 << 7) | 0b1000011;
        let r = d(instr);
        assert_eq!((r.class, r.mnemonic.as_str(), r.operands.as_str()),
                   ("F", "FMADD.S", "f5, f6, f7, f8"));
    }
    #[test] fn fnmsub_d() {
        let instr = (8u32 << 27) | (0b01 << 25) | (7 << 20) | (6 << 15)
                  | (0 << 12) | (5 << 7) | 0b1001011;
        assert_eq!(mnem(instr), "FNMSUB.D");
    }

    // ── H (hypervisor) ─────────────────────────────────────────────────
    fn sys_f4(funct7: u32, rs2: u32, rs1: u32, rd: u32) -> u32 {
        (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | (0b100 << 12) | (rd << 7) | 0b1110011
    }
    #[test] fn hlv_b() {
        let r = d(sys_f4(0b0110000, 0, 6, 5));
        assert_eq!((r.class, r.mnemonic.as_str(), r.operands.as_str()),
                   ("H", "HLV.B", "x5, (x6)"));
    }
    #[test] fn hlv_w() {
        assert_eq!(mnem(sys_f4(0b0110100, 0, 6, 5)), "HLV.W");
    }
    #[test] fn hsv_w() {
        let r = d(sys_f4(0b0110101, 7, 6, 0));
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("HSV.W", "x7, (x6)"));
    }
    #[test] fn hfence_vvma() {
        let instr = (0b0010001u32 << 25) | (7 << 20) | (6 << 15) | 0b1110011;
        let r = d(instr);
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("HFENCE.VVMA", "x6, x7"));
    }
    #[test] fn hfence_gvma() {
        let instr = (0b0110001u32 << 25) | (7 << 20) | (6 << 15) | 0b1110011;
        assert_eq!(mnem(instr), "HFENCE.GVMA");
    }

    }
    // ── Zk (scalar crypto) ─────────────────────────────────────────────
    #[test] fn aes32esi() {
        // bs=2, funct5=10001 (AES32ESI), f3=000
        let f7 = (2u32 << 5) | 0b10001;
        let r = d(r_type(f7, 7, 6, 0b000, 5));
        assert_eq!((r.class, r.mnemonic.as_str(), r.operands.as_str()),
                   ("Zkne", "AES32ESI", "x5, x6, x7, 2"));
    }
    #[test] fn aes32esmi() {
        let f7 = (1u32 << 5) | 0b10011;
        assert_eq!(mnem(r_type(f7, 7, 6, 0b000, 5)), "AES32ESMI");
    }
    #[test] fn aes32dsi() {
        let f7 = (3u32 << 5) | 0b10101;
        assert_eq!(mnem(r_type(f7, 7, 6, 0b000, 5)), "AES32DSI");
    }
    #[test] fn sm4ed() {
        let f7 = (2u32 << 5) | 0b11000;
        let r = d(r_type(f7, 7, 6, 0b000, 5));
        assert_eq!((r.class, r.mnemonic.as_str()), ("Zksed", "SM4ED"));
    }
    #[test] fn sm4ks() {
        let f7 = (0u32 << 5) | 0b11010;
        assert_eq!(mnem(r_type(f7, 7, 6, 0b000, 5)), "SM4KS");
    }
    #[test] fn sha512sum0r() {
        let r = d(r_type(0b0101000, 7, 6, 0b000, 5));
        assert_eq!((r.class, r.mnemonic.as_str()), ("Zknh", "SHA512SUM0R"));
    }
    #[test] fn sha512sig1h() {
        assert_eq!(mnem(r_type(0b0101111, 7, 6, 0b000, 5)), "SHA512SIG1H");
    }
    fn imm_op_f7(f7: u32, rs2: u32, rs1: u32, f3: u32, rd: u32) -> u32 {
        (f7 << 25) | (rs2 << 20) | (rs1 << 15) | (f3 << 12) | (rd << 7) | 0b0010011
    }
    #[test] fn sha256sum0() {
        let r = d(imm_op_f7(0b0001000, 0b00000, 6, 0b001, 5));
        assert_eq!((r.class, r.mnemonic.as_str(), r.operands.as_str()),
                   ("Zknh", "SHA256SUM0", "x5, x6"));
    }
    #[test] fn sha256sig1() {
        assert_eq!(mnem(imm_op_f7(0b0001000, 0b00011, 6, 0b001, 5)), "SHA256SIG1");
    }
    #[test] fn sm3p0() {
        let r = d(imm_op_f7(0b0001000, 0b01000, 6, 0b001, 5));
        assert_eq!((r.class, r.mnemonic.as_str()), ("Zksh", "SM3P0"));
    }
    #[test] fn sm3p1() {
        assert_eq!(mnem(imm_op_f7(0b0001000, 0b01001, 6, 0b001, 5)), "SM3P1");
    }
    #[test] fn pack() {
        // funct7=0000100, f3=100, rs2!=0
        let r = d(r_type(0b0000100, 7, 6, 0b100, 5));
        assert_eq!((r.class, r.mnemonic.as_str()), ("Zbkb", "PACK"));
    }
    #[test] fn packh() {
        let r = d(r_type(0b0000100, 7, 6, 0b111, 5));
        assert_eq!(r.mnemonic, "PACKH");
    }
    #[test] fn brev8() {
        let r = d(imm_op_f7(0b0110100, 0b00111, 6, 0b101, 5));
        assert_eq!((r.class, r.mnemonic.as_str()), ("Zbkb", "BREV8"));
    }
    #[test] fn xperm4() {
        let r = d(r_type(0b0010100, 7, 6, 0b010, 5));
        assert_eq!((r.class, r.mnemonic.as_str()), ("Zbkx", "XPERM4"));
    }
    #[test] fn xperm8() {
        assert_eq!(mnem(r_type(0b0010100, 7, 6, 0b100, 5)), "XPERM8");
    }
    #[test] fn zext_h_still_works() {
        // ZEXT.H is funct7=0000100, f3=100, rs2=0 — must NOT be shadowed by PACK.
        let r = d(r_type(0b0000100, 0, 6, 0b100, 5));
        assert_eq!(r.mnemonic, "ZEXT.H");
    }

    #[cfg(feature = "disasm-ext")]
    mod ext_c_v {
    use super::*;
    // ── C (RV32C compressed) ───────────────────────────────────────────
    fn c16(half: u16) -> Disasm { disassemble(half as u32) }

    #[test] fn c_nop() { assert_eq!(c16(0x0001).mnemonic, "C.NOP"); }
    #[test] fn c_addi_x5_1() {
        // C.ADDI x5, 1   → 000 0 00101 00001 01  = 0x0285 + bit12=0
        // funct3=000, rd=5, imm=1: c[12]=0, c[6:2]=00001 → 0x0285
        let r = c16(0x0285);
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("C.ADDI", "x5, 1"));
    }
    #[test] fn c_li_x5_neg1() {
        // C.LI x5, -1: funct3=010, rd=5, imm=-1 → c[12]=1, c[6:2]=11111
        let bits: u16 = (0b010u16 << 13) | (1u16 << 12) | (5u16 << 7) | (0b11111 << 2) | 0b01;
        let r = c16(bits);
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("C.LI", "x5, -1"));
    }
    #[test] fn c_mv_x5_x6() {
        // C.MV x5, x6: funct3=100, bit12=0, rd=5, rs2=6, q=10
        let bits: u16 = (0b100u16 << 13) | (5u16 << 7) | (6u16 << 2) | 0b10;
        let r = c16(bits);
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("C.MV", "x5, x6"));
    }
    #[test] fn c_add_x5_x6() {
        let bits: u16 = (0b100u16 << 13) | (1u16 << 12) | (5u16 << 7) | (6u16 << 2) | 0b10;
        let r = c16(bits);
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("C.ADD", "x5, x6"));
    }
    #[test] fn c_jr_x1() {
        // C.JR x1 → 100 0 00001 00000 10
        let bits: u16 = (0b100u16 << 13) | (1u16 << 7) | 0b10;
        assert_eq!(c16(bits).mnemonic, "C.JR");
    }
    #[test] fn c_jalr_x1() {
        let bits: u16 = (0b100u16 << 13) | (1u16 << 12) | (1u16 << 7) | 0b10;
        assert_eq!(c16(bits).mnemonic, "C.JALR");
    }
    #[test] fn c_ebreak() {
        let bits: u16 = (0b100u16 << 13) | (1u16 << 12) | 0b10;
        assert_eq!(c16(bits).mnemonic, "C.EBREAK");
    }
    #[test] fn c_lwsp() {
        // C.LWSP x5, 0(x2) → funct3=010, rd=5, imm=0
        let bits: u16 = (0b010u16 << 13) | (5u16 << 7) | 0b10;
        let r = c16(bits);
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("C.LWSP", "x5, 0(x2)"));
    }
    #[test] fn c_swsp() {
        // C.SWSP x5, 0(x2) → funct3=110, rs2=5, imm=0
        let bits: u16 = (0b110u16 << 13) | (5u16 << 2) | 0b10;
        let r = c16(bits);
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("C.SWSP", "x5, 0(x2)"));
    }
    #[test] fn c_lw_x10_0_x11() {
        // C.LW rd'=2 (x10), rs1'=3 (x11), imm=0 → funct3=010
        let bits: u16 = (0b010u16 << 13) | (3u16 << 7) | (2u16 << 2) | 0b00;
        let r = c16(bits);
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("C.LW", "x10, 0(x11)"));
    }
    #[test] fn c_addi4spn() {
        // C.ADDI4SPN rd'=0 (x8), nzuimm=4 → funct3=000, c[5]=1 (bit3 of imm)
        // bit3 of imm comes from c[5]; for nzuimm=4 we need bit2=1 → c[6]=1
        // funct3=000, c[12:5]=00000010, c[4:2]=000 (rd'=0), q=00
        let bits: u16 = (0b000u16 << 13) | (1u16 << 6) | 0b00;
        let r = c16(bits);
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("C.ADDI4SPN", "x8, x2, 4"));
    }
    #[test] fn c_beqz() {
        // C.BEQZ rs1'=0 (x8), offset=0 → funct3=110
        let bits: u16 = (0b110u16 << 13) | 0b01;
        let r = c16(bits);
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("C.BEQZ", "x8, 0"));
    }
    #[test] fn c_j_zero() {
        let bits: u16 = (0b101u16 << 13) | 0b01;
        assert_eq!(c16(bits).mnemonic, "C.J");
    }
    #[test] fn c_and() {
        // C.AND rd'=0 rs2'=1 → funct3=100, c[12]=0, c[11:10]=11, c[6:5]=11
        let bits: u16 = (0b100u16 << 13) | (0b11 << 10) | (0b11 << 5) | (1u16 << 2) | 0b01;
        let r = c16(bits);
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("C.AND", "x8, x9"));
    }
    #[test] fn rv32i_still_works_after_c_dispatch() {
        // Ensure 32-bit ADDI (low 2 bits = 11) still routes through rv32i.
        assert_eq!(d(0x00518213).mnemonic, "ADDI");
    }

    // ── V (RVV 1.0) ────────────────────────────────────────────────────
    /// Build an OP-V (1010111) instruction with funct6/vm/vs2/vs1/vd/funct3.
    fn op_v(funct6: u32, vm: u32, vs2: u32, vs1: u32, f3: u32, vd: u32) -> u32 {
        (funct6 << 26) | (vm << 25) | (vs2 << 20) | (vs1 << 15) | (f3 << 12) | (vd << 7) | 0b1010111
    }

    #[test] fn vadd_vv() {
        // vadd.vv v5, v6, v7 — funct6=000000, f3=000, vm=1
        let r = d(op_v(0, 1, 6, 7, 0b000, 5));
        assert_eq!((r.class, r.mnemonic.as_str(), r.operands.as_str()),
                   ("V", "vadd.vv", "v5, v6, v7"));
    }
    #[test] fn vadd_vv_masked() {
        let r = d(op_v(0, 0, 6, 7, 0b000, 5));
        assert_eq!(r.operands, "v5, v6, v7, v0.t");
    }
    #[test] fn vadd_vx() {
        // vadd.vx v5, v6, x7
        let r = d(op_v(0, 1, 6, 7, 0b100, 5));
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("vadd.vx", "v5, v6, x7"));
    }
    #[test] fn vadd_vi() {
        // vadd.vi v5, v6, -3 — imm5 = 0b11101
        let r = d(op_v(0, 1, 6, 0b11101, 0b011, 5));
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("vadd.vi", "v5, v6, -3"));
    }
    #[test] fn vmv_v_i() {
        // vmv.v.i v5, 7  — funct6=010111, vm=1, vs2 must be 0
        let r = d(op_v(0b010111, 1, 0, 7, 0b011, 5));
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("vmv.v.i", "v5, 7"));
    }
    #[test] fn vmerge_vvm() {
        // vmerge.vvm v5, v6, v7, v0 — funct6=010111, vm=0
        let r = d(op_v(0b010111, 0, 6, 7, 0b000, 5));
        assert_eq!(r.mnemonic, "vmerge.vvm");
    }
    #[test] fn vmul_vv() {
        // vmul.vv: funct6=100101, f3=010 (OPMVV)
        let r = d(op_v(0b100101, 1, 6, 7, 0b010, 5));
        assert_eq!((r.class, r.mnemonic.as_str()), ("V", "vmul.vv"));
    }
    #[test] fn vfmacc_vv() {
        // OPFVV f3=001, funct6=101100
        let r = d(op_v(0b101100, 1, 6, 7, 0b001, 5));
        assert_eq!(r.mnemonic, "vfmacc.vv");
    }
    #[test] fn vfadd_vf() {
        // OPFVF f3=101, funct6=000000
        let r = d(op_v(0b000000, 1, 6, 7, 0b101, 5));
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("vfadd.vf", "v5, v6, f7"));
    }
    #[test] fn vmv_x_s() {
        // VWXUNARY0: f3=010 funct6=010000 vs1=0
        let r = d(op_v(0b010000, 1, 6, 0b00000, 0b010, 5));
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("vmv.x.s", "x5, v6"));
    }
    #[test] fn vmv_s_x() {
        // OPMVX f3=110 funct6=010000
        let r = d(op_v(0b010000, 1, 0, 7, 0b110, 5));
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("vmv.s.x", "v5, x7"));
    }
    #[test] fn vid_v() {
        // VMUNARY0: f3=010 funct6=010100 vs1=10001
        let r = d(op_v(0b010100, 1, 0, 0b10001, 0b010, 5));
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("vid.v", "v5"));
    }
    #[test] fn vsetvli_basic() {
        // vsetvli x5, x6, e32,m1,tu,mu  → zimm = 0b00010 (vsew=2, vlmul=0)
        let zimm: u32 = (2 << 3) | 0; // tu (vta=0), mu (vma=0)
        let instr = (zimm << 20) | (6 << 15) | (0b111 << 12) | (5 << 7) | 0b1010111;
        let r = d(instr);
        assert_eq!((r.class, r.mnemonic.as_str(), r.operands.as_str()),
                   ("V", "vsetvli", "x5, x6, e32,m1,tu,mu"));
    }
    #[test] fn vsetivli_ta_ma() {
        // vsetivli x5, 4, e8,m2,ta,ma  → zimm[9:0]
        let zimm: u32 = (1 << 7) | (1 << 6) | (0 << 3) | 0b001;
        let instr = (0b11u32 << 30) | (zimm << 20) | (4 << 15) | (0b111 << 12) | (5 << 7) | 0b1010111;
        let r = d(instr);
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()),
                   ("vsetivli", "x5, 4, e8,m2,ta,ma"));
    }
    #[test] fn vsetvl_reg_reg() {
        // vsetvl x5, x6, x7
        let instr = (0b10u32 << 30) | (7 << 20) | (6 << 15) | (0b111 << 12) | (5 << 7) | 0b1010111;
        let r = d(instr);
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("vsetvl", "x5, x6, x7"));
    }

    // Vector load / store
    fn vload(nf: u32, mew: u32, mop: u32, vm: u32, lumop: u32, rs1: u32, f3: u32, vd: u32) -> u32 {
        (nf << 29) | (mew << 28) | (mop << 26) | (vm << 25) | (lumop << 20)
            | (rs1 << 15) | (f3 << 12) | (vd << 7) | 0b0000111
    }
    fn vstore(nf: u32, mew: u32, mop: u32, vm: u32, sumop: u32, rs1: u32, f3: u32, vs3: u32) -> u32 {
        (nf << 29) | (mew << 28) | (mop << 26) | (vm << 25) | (sumop << 20)
            | (rs1 << 15) | (f3 << 12) | (vs3 << 7) | 0b0100111
    }

    #[test] fn vle32_v() {
        let r = d(vload(0, 0, 0b00, 1, 0b00000, 6, 0b110, 5));
        assert_eq!((r.class, r.mnemonic.as_str(), r.operands.as_str()),
                   ("V", "vle32.v", "v5, (x6)"));
    }
    #[test] fn vse8_v() {
        let r = d(vstore(0, 0, 0b00, 1, 0b00000, 6, 0b000, 5));
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()), ("vse8.v", "v5, (x6)"));
    }
    #[test] fn vlse64_v() {
        let r = d(vload(0, 0, 0b10, 1, 7, 6, 0b111, 5));
        assert_eq!((r.mnemonic.as_str(), r.operands.as_str()),
                   ("vlse64.v", "v5, (x6), x7"));
    }
    #[test] fn vluxei16_v() {
        let r = d(vload(0, 0, 0b01, 1, 7, 6, 0b101, 5));
        assert_eq!(r.mnemonic, "vluxei16.v");
    }
    #[test] fn vloxei32_v() {
        let r = d(vload(0, 0, 0b11, 1, 7, 6, 0b110, 5));
        assert_eq!(r.mnemonic, "vloxei32.v");
    }
    #[test] fn vlm_v() {
        let r = d(vload(0, 0, 0b00, 1, 0b01011, 6, 0b000, 5));
        assert_eq!(r.mnemonic, "vlm.v");
    }
    #[test] fn vsm_v() {
        let r = d(vstore(0, 0, 0b00, 1, 0b01011, 6, 0b000, 5));
        assert_eq!(r.mnemonic, "vsm.v");
    }
    #[test] fn vl1re32_v() {
        // Whole-register load: lumop=01000, nf=0 (1 register)
        let r = d(vload(0, 0, 0b00, 1, 0b01000, 6, 0b110, 5));
        assert_eq!(r.mnemonic, "vl1re32.v");
    }
    #[test] fn vle8ff_v() {
        let r = d(vload(0, 0, 0b00, 1, 0b10000, 6, 0b000, 5));
        assert_eq!(r.mnemonic, "vle8ff.v");
    }
    #[test] fn vlseg3e32_v() {
        // nf=2 → seg3
        let r = d(vload(2, 0, 0b00, 1, 0b00000, 6, 0b110, 5));
        assert_eq!(r.mnemonic, "vlseg3e32.v");
    }
    }
}
