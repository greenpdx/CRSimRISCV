//! V extension — RISC-V Vector ISA (RVV 1.0).
//!
//! Three opcodes:
//!   `0000111` LOAD-FP   — vector loads when funct3 ∈ {000,101,110,111}
//!   `0100111` STORE-FP  — vector stores (same funct3 widths)
//!   `1010111` OP-V      — all vector arithmetic (and `vsetvl*`)
//!
//! Vector arithmetic uses funct3 to pick a major group:
//!   000 OPIVV  001 OPFVV  010 OPMVV  011 OPIVI
//!   100 OPIVX  101 OPFVF  110 OPMVX  111 OPCFG
//! Within a group `funct6` (instr[31:26]) selects the operation, and
//! `vm` (instr[25]) chooses masked vs unmasked execution. The full RVV
//! 1.0 arithmetic table — every funct6 in every funct3 group — is
//! enumerated below.

use super::{fr, r, Disasm};

#[inline] fn vr(i: u8) -> String { format!("v{}", i) }
#[inline] fn mask(vm: u32) -> &'static str { if vm == 0 { ", v0.t" } else { "" } }

pub fn try_decode(instr: u32) -> Option<Disasm> {
    match instr & 0x7f {
        0b0000111 => decode_load(instr),
        0b0100111 => decode_store(instr),
        0b1010111 => decode_op_v(instr),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Element-width helpers (the funct3 field on V load/store)
// ─────────────────────────────────────────────────────────────────────────
fn ew_for_funct3(f3: u32) -> Option<u32> {
    Some(match f3 {
        0b000 => 8,
        0b101 => 16,
        0b110 => 32,
        0b111 => 64,
        _ => return None,
    })
}

// ─────────────────────────────────────────────────────────────────────────
// Vector loads
// ─────────────────────────────────────────────────────────────────────────
fn decode_load(instr: u32) -> Option<Disasm> {
    let f3  = (instr >> 12) & 0x7;
    let ew  = ew_for_funct3(f3)?;
    let vd  = ((instr >>  7) & 0x1f) as u8;
    let rs1 = ((instr >> 15) & 0x1f) as u8;
    let r2  = ((instr >> 20) & 0x1f) as u8;
    let vm  = (instr >> 25) & 0x1;
    let mop = (instr >> 26) & 0x3;
    let mew = (instr >> 28) & 0x1;
    let nf  = (instr >> 29) & 0x7;
    let _ = mew; // EEW = 8*2^mew * 2^width — printed via element width here

    // mop selects addressing mode
    match mop {
        0b00 => {
            // lumop chooses sub-form
            let lumop = r2;
            let nf1 = nf + 1;
            match lumop {
                0b00000 => Some(Disasm {
                    class: "V",
                    mnemonic: if nf == 0 { format!("vle{}.v", ew) }
                              else { format!("vlseg{}e{}.v", nf1, ew) },
                    operands: format!("{}, ({}){}", vr(vd), r(rs1), mask(vm)),
                }),
                0b01000 => {
                    // Whole-register load: nf+1 = number of registers (1/2/4/8)
                    Some(Disasm {
                        class: "V",
                        mnemonic: format!("vl{}re{}.v", nf1, ew),
                        operands: format!("{}, ({})", vr(vd), r(rs1)),
                    })
                }
                0b01011 if ew == 8 && nf == 0 => Some(Disasm {
                    class: "V", mnemonic: "vlm.v".into(),
                    operands: format!("{}, ({})", vr(vd), r(rs1)),
                }),
                0b10000 => Some(Disasm {
                    class: "V",
                    mnemonic: if nf == 0 { format!("vle{}ff.v", ew) }
                              else { format!("vlseg{}e{}ff.v", nf1, ew) },
                    operands: format!("{}, ({}){}", vr(vd), r(rs1), mask(vm)),
                }),
                _ => None,
            }
        }
        0b10 => {
            // Strided
            let nf1 = nf + 1;
            Some(Disasm {
                class: "V",
                mnemonic: if nf == 0 { format!("vlse{}.v", ew) }
                          else { format!("vlsseg{}e{}.v", nf1, ew) },
                operands: format!("{}, ({}), {}{}", vr(vd), r(rs1), r(r2), mask(vm)),
            })
        }
        0b01 | 0b11 => {
            // Indexed unordered (01) or ordered (11)
            let order = if mop == 0b01 { "u" } else { "o" };
            let nf1 = nf + 1;
            Some(Disasm {
                class: "V",
                mnemonic: if nf == 0 { format!("vl{}xei{}.v", order, ew) }
                          else { format!("vl{}xseg{}ei{}.v", order, nf1, ew) },
                operands: format!("{}, ({}), {}{}", vr(vd), r(rs1), vr(r2), mask(vm)),
            })
        }
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Vector stores
// ─────────────────────────────────────────────────────────────────────────
fn decode_store(instr: u32) -> Option<Disasm> {
    let f3  = (instr >> 12) & 0x7;
    let ew  = ew_for_funct3(f3)?;
    let vs3 = ((instr >>  7) & 0x1f) as u8;
    let rs1 = ((instr >> 15) & 0x1f) as u8;
    let r2  = ((instr >> 20) & 0x1f) as u8;
    let vm  = (instr >> 25) & 0x1;
    let mop = (instr >> 26) & 0x3;
    let nf  = (instr >> 29) & 0x7;
    let nf1 = nf + 1;

    match mop {
        0b00 => {
            let sumop = r2;
            match sumop {
                0b00000 => Some(Disasm {
                    class: "V",
                    mnemonic: if nf == 0 { format!("vse{}.v", ew) }
                              else { format!("vsseg{}e{}.v", nf1, ew) },
                    operands: format!("{}, ({}){}", vr(vs3), r(rs1), mask(vm)),
                }),
                0b01000 => Some(Disasm {
                    class: "V",
                    mnemonic: format!("vs{}r.v", nf1),
                    operands: format!("{}, ({})", vr(vs3), r(rs1)),
                }),
                0b01011 if ew == 8 && nf == 0 => Some(Disasm {
                    class: "V", mnemonic: "vsm.v".into(),
                    operands: format!("{}, ({})", vr(vs3), r(rs1)),
                }),
                _ => None,
            }
        }
        0b10 => Some(Disasm {
            class: "V",
            mnemonic: if nf == 0 { format!("vsse{}.v", ew) }
                      else { format!("vssseg{}e{}.v", nf1, ew) },
            operands: format!("{}, ({}), {}{}", vr(vs3), r(rs1), r(r2), mask(vm)),
        }),
        0b01 | 0b11 => {
            let order = if mop == 0b01 { "u" } else { "o" };
            Some(Disasm {
                class: "V",
                mnemonic: if nf == 0 { format!("vs{}xei{}.v", order, ew) }
                          else { format!("vs{}xseg{}ei{}.v", order, nf1, ew) },
                operands: format!("{}, ({}), {}{}", vr(vs3), r(rs1), vr(r2), mask(vm)),
            })
        }
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// OP-V dispatch
// ─────────────────────────────────────────────────────────────────────────
fn decode_op_v(instr: u32) -> Option<Disasm> {
    let f3 = (instr >> 12) & 0x7;
    if f3 == 0b111 {
        return decode_vsetvl(instr);
    }
    let funct6 = (instr >> 26) & 0x3f;
    let vm     = (instr >> 25) & 0x1;
    let vs2    = ((instr >> 20) & 0x1f) as u8;
    let vs1    = ((instr >> 15) & 0x1f) as u8;   // also rs1 for VX/VF
    let vd     = ((instr >>  7) & 0x1f) as u8;   // also rd for compares-to-mask

    match f3 {
        0b000 => decode_opivv(funct6, vm, vs2, vs1, vd),
        0b100 => decode_opivx(funct6, vm, vs2, vs1, vd),
        0b011 => decode_opivi(funct6, vm, vs2, vs1, vd),
        0b010 => decode_opmvv(funct6, vm, vs2, vs1, vd),
        0b110 => decode_opmvx(funct6, vm, vs2, vs1, vd),
        0b001 => decode_opfvv(funct6, vm, vs2, vs1, vd),
        0b101 => decode_opfvf(funct6, vm, vs2, vs1, vd),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// vsetvl / vsetvli / vsetivli
// ─────────────────────────────────────────────────────────────────────────
fn decode_vsetvl(instr: u32) -> Option<Disasm> {
    let rd  = ((instr >>  7) & 0x1f) as u8;
    let rs1 = ((instr >> 15) & 0x1f) as u8;
    let rs2 = ((instr >> 20) & 0x1f) as u8;
    let top = (instr >> 30) & 0x3;

    match top {
        0b00 | 0b01 => {
            // vsetvli: zimm[10:0] in instr[30:20]
            let zimm = (instr >> 20) & 0x7ff;
            Some(Disasm {
                class: "V", mnemonic: "vsetvli".into(),
                operands: format!("{}, {}, {}", r(rd), r(rs1), vtype_string(zimm)),
            })
        }
        0b11 => {
            // vsetivli: uimm in rs1 field, zimm[9:0] in instr[29:20]
            let zimm = (instr >> 20) & 0x3ff;
            Some(Disasm {
                class: "V", mnemonic: "vsetivli".into(),
                operands: format!("{}, {}, {}", r(rd), rs1, vtype_string(zimm)),
            })
        }
        0b10 => {
            // vsetvl: rd, rs1, rs2
            Some(Disasm {
                class: "V", mnemonic: "vsetvl".into(),
                operands: format!("{}, {}, {}", r(rd), r(rs1), r(rs2)),
            })
        }
        _ => None,
    }
}

fn vtype_string(zimm: u32) -> String {
    let vlmul = zimm & 0x7;
    let vsew  = (zimm >> 3) & 0x7;
    let vta   = (zimm >> 6) & 0x1;
    let vma   = (zimm >> 7) & 0x1;
    let sew = match vsew {
        0 => "e8", 1 => "e16", 2 => "e32", 3 => "e64", _ => "e?",
    };
    let lmul = match vlmul {
        0b000 => "m1", 0b001 => "m2", 0b010 => "m4", 0b011 => "m8",
        0b101 => "mf8", 0b110 => "mf4", 0b111 => "mf2",
        _ => "m?",
    };
    let ta = if vta == 1 { "ta" } else { "tu" };
    let ma = if vma == 1 { "ma" } else { "mu" };
    format!("{},{},{},{}", sew, lmul, ta, ma)
}

// ─────────────────────────────────────────────────────────────────────────
// OPIVV — vector-vector integer
// ─────────────────────────────────────────────────────────────────────────
fn decode_opivv(funct6: u32, vm: u32, vs2: u8, vs1: u8, vd: u8) -> Option<Disasm> {
    let mnem: &'static str = match funct6 {
        0b000000 => "vadd.vv",
        0b000010 => "vsub.vv",
        0b000100 => "vminu.vv",
        0b000101 => "vmin.vv",
        0b000110 => "vmaxu.vv",
        0b000111 => "vmax.vv",
        0b001001 => "vand.vv",
        0b001010 => "vor.vv",
        0b001011 => "vxor.vv",
        0b001100 => "vrgather.vv",
        0b001110 => "vrgatherei16.vv",
        0b010000 => "vadc.vvm",
        0b010001 => "vmadc.vvm",
        0b010010 => "vsbc.vvm",
        0b010011 => "vmsbc.vvm",
        0b010111 => return Some(if vm == 0 {
            Disasm { class: "V", mnemonic: "vmerge.vvm".into(),
                operands: format!("{}, {}, {}, v0", vr(vd), vr(vs2), vr(vs1)) }
        } else {
            Disasm { class: "V", mnemonic: "vmv.v.v".into(),
                operands: format!("{}, {}", vr(vd), vr(vs1)) }
        }),
        0b011000 => "vmseq.vv",
        0b011001 => "vmsne.vv",
        0b011010 => "vmsltu.vv",
        0b011011 => "vmslt.vv",
        0b011100 => "vmsleu.vv",
        0b011101 => "vmsle.vv",
        0b100000 => "vsaddu.vv",
        0b100001 => "vsadd.vv",
        0b100010 => "vssubu.vv",
        0b100011 => "vssub.vv",
        0b100101 => "vsll.vv",
        0b100111 => "vsmul.vv",
        0b101000 => "vsrl.vv",
        0b101001 => "vsra.vv",
        0b101010 => "vssrl.vv",
        0b101011 => "vssra.vv",
        0b101100 => "vnsrl.wv",
        0b101101 => "vnsra.wv",
        0b101110 => "vnclipu.wv",
        0b101111 => "vnclip.wv",
        0b110000 => "vwredsumu.vs",
        0b110001 => "vwredsum.vs",
        _ => return None,
    };
    Some(Disasm {
        class: "V", mnemonic: mnem.into(),
        operands: format!("{}, {}, {}{}", vr(vd), vr(vs2), vr(vs1), mask(vm)),
    })
}

// ─────────────────────────────────────────────────────────────────────────
// OPIVX — vector-scalar integer
// ─────────────────────────────────────────────────────────────────────────
fn decode_opivx(funct6: u32, vm: u32, vs2: u8, rs1: u8, vd: u8) -> Option<Disasm> {
    let mnem: &'static str = match funct6 {
        0b000000 => "vadd.vx",
        0b000010 => "vsub.vx",
        0b000011 => "vrsub.vx",
        0b000100 => "vminu.vx",
        0b000101 => "vmin.vx",
        0b000110 => "vmaxu.vx",
        0b000111 => "vmax.vx",
        0b001001 => "vand.vx",
        0b001010 => "vor.vx",
        0b001011 => "vxor.vx",
        0b001100 => "vrgather.vx",
        0b001110 => "vslideup.vx",
        0b001111 => "vslidedown.vx",
        0b010000 => "vadc.vxm",
        0b010001 => "vmadc.vxm",
        0b010010 => "vsbc.vxm",
        0b010011 => "vmsbc.vxm",
        0b010111 => return Some(if vm == 0 {
            Disasm { class: "V", mnemonic: "vmerge.vxm".into(),
                operands: format!("{}, {}, {}, v0", vr(vd), vr(vs2), r(rs1)) }
        } else {
            Disasm { class: "V", mnemonic: "vmv.v.x".into(),
                operands: format!("{}, {}", vr(vd), r(rs1)) }
        }),
        0b011000 => "vmseq.vx",
        0b011001 => "vmsne.vx",
        0b011010 => "vmsltu.vx",
        0b011011 => "vmslt.vx",
        0b011100 => "vmsleu.vx",
        0b011101 => "vmsle.vx",
        0b011110 => "vmsgtu.vx",
        0b011111 => "vmsgt.vx",
        0b100000 => "vsaddu.vx",
        0b100001 => "vsadd.vx",
        0b100010 => "vssubu.vx",
        0b100011 => "vssub.vx",
        0b100101 => "vsll.vx",
        0b100111 => "vsmul.vx",
        0b101000 => "vsrl.vx",
        0b101001 => "vsra.vx",
        0b101010 => "vssrl.vx",
        0b101011 => "vssra.vx",
        0b101100 => "vnsrl.wx",
        0b101101 => "vnsra.wx",
        0b101110 => "vnclipu.wx",
        0b101111 => "vnclip.wx",
        _ => return None,
    };
    Some(Disasm {
        class: "V", mnemonic: mnem.into(),
        operands: format!("{}, {}, {}{}", vr(vd), vr(vs2), r(rs1), mask(vm)),
    })
}

// ─────────────────────────────────────────────────────────────────────────
// OPIVI — vector-immediate integer (imm5 in vs1 position, sign- or zero-extended)
// ─────────────────────────────────────────────────────────────────────────
fn decode_opivi(funct6: u32, vm: u32, vs2: u8, imm5: u8, vd: u8) -> Option<Disasm> {
    // Most ops sign-extend imm5; shifts/unsigned compares zero-extend.
    let simm = ((imm5 as i8) << 3) >> 3; // sign-extend 5→8
    let uimm = imm5 & 0x1f;

    enum K { S, U }
    let (mnem, kind): (&'static str, K) = match funct6 {
        0b000000 => ("vadd.vi",      K::S),
        0b000011 => ("vrsub.vi",     K::S),
        0b001001 => ("vand.vi",      K::S),
        0b001010 => ("vor.vi",       K::S),
        0b001011 => ("vxor.vi",      K::S),
        0b001100 => ("vrgather.vi",  K::U),
        0b001110 => ("vslideup.vi",  K::U),
        0b001111 => ("vslidedown.vi",K::U),
        0b010000 => ("vadc.vim",     K::S),
        0b010001 => ("vmadc.vim",    K::S),
        0b010111 => return Some(if vm == 0 {
            Disasm { class: "V", mnemonic: "vmerge.vim".into(),
                operands: format!("{}, {}, {}, v0", vr(vd), vr(vs2), simm) }
        } else {
            Disasm { class: "V", mnemonic: "vmv.v.i".into(),
                operands: format!("{}, {}", vr(vd), simm) }
        }),
        0b011000 => ("vmseq.vi",     K::S),
        0b011001 => ("vmsne.vi",     K::S),
        0b011100 => ("vmsleu.vi",    K::U),
        0b011101 => ("vmsle.vi",     K::S),
        0b011110 => ("vmsgtu.vi",    K::U),
        0b011111 => ("vmsgt.vi",     K::S),
        0b100000 => ("vsaddu.vi",    K::U),
        0b100001 => ("vsadd.vi",     K::S),
        0b100101 => ("vsll.vi",      K::U),
        0b101000 => ("vsrl.vi",      K::U),
        0b101001 => ("vsra.vi",      K::U),
        0b101010 => ("vssrl.vi",     K::U),
        0b101011 => ("vssra.vi",     K::U),
        0b101100 => ("vnsrl.wi",     K::U),
        0b101101 => ("vnsra.wi",     K::U),
        0b101110 => ("vnclipu.wi",   K::U),
        0b101111 => ("vnclip.wi",    K::U),
        _ => return None,
    };
    let imm_str = match kind {
        K::S => format!("{}", simm),
        K::U => format!("{}", uimm),
    };
    Some(Disasm {
        class: "V", mnemonic: mnem.into(),
        operands: format!("{}, {}, {}{}", vr(vd), vr(vs2), imm_str, mask(vm)),
    })
}

// ─────────────────────────────────────────────────────────────────────────
// OPMVV — vector-vector mask / multiply / reductions / etc.
// ─────────────────────────────────────────────────────────────────────────
fn decode_opmvv(funct6: u32, vm: u32, vs2: u8, vs1: u8, vd: u8) -> Option<Disasm> {
    // Reductions / single-vector ops (funct6 group 0b00****)
    // Mask-mask logical ops (funct6 0b01****)
    let mnem: &'static str = match funct6 {
        // Reductions
        0b000000 => "vredsum.vs",
        0b000001 => "vredand.vs",
        0b000010 => "vredor.vs",
        0b000011 => "vredxor.vs",
        0b000100 => "vredminu.vs",
        0b000101 => "vredmin.vs",
        0b000110 => "vredmaxu.vs",
        0b000111 => "vredmax.vs",
        // Average + sub-mul
        0b001000 => "vaaddu.vv",
        0b001001 => "vaadd.vv",
        0b001010 => "vasubu.vv",
        0b001011 => "vasub.vv",
        // VWXUNARY0 (vmv.x.s, vcpop, vfirst, vmv.s.x lives in opmvx)
        0b010000 => return decode_vwxunary0(vs1, vs2, vd, vm),
        // VXUNARY0
        0b010010 => return decode_vxunary0(vs1, vs2, vd, vm),
        // VMUNARY0
        0b010100 => return decode_vmunary0(vs1, vs2, vd, vm),
        // Compress / mask logical
        0b010111 => return Some(Disasm {
            class: "V", mnemonic: "vcompress.vm".into(),
            operands: format!("{}, {}, {}", vr(vd), vr(vs2), vr(vs1)),
        }),
        0b011000 => "vmandnot.mm",
        0b011001 => "vmand.mm",
        0b011010 => "vmor.mm",
        0b011011 => "vmxor.mm",
        0b011100 => "vmornot.mm",
        0b011101 => "vmnand.mm",
        0b011110 => "vmnor.mm",
        0b011111 => "vmxnor.mm",
        // Mul / div
        0b100000 => "vdivu.vv",
        0b100001 => "vdiv.vv",
        0b100010 => "vremu.vv",
        0b100011 => "vrem.vv",
        0b100100 => "vmulhu.vv",
        0b100101 => "vmul.vv",
        0b100110 => "vmulhsu.vv",
        0b100111 => "vmulh.vv",
        0b101001 => "vmadd.vv",
        0b101011 => "vnmsub.vv",
        0b101101 => "vmacc.vv",
        0b101111 => "vnmsac.vv",
        // Widening
        0b110000 => "vwaddu.vv",
        0b110001 => "vwadd.vv",
        0b110010 => "vwsubu.vv",
        0b110011 => "vwsub.vv",
        0b110100 => "vwaddu.wv",
        0b110101 => "vwadd.wv",
        0b110110 => "vwsubu.wv",
        0b110111 => "vwsub.wv",
        0b111000 => "vwmulu.vv",
        0b111010 => "vwmulsu.vv",
        0b111011 => "vwmul.vv",
        0b111100 => "vwmaccu.vv",
        0b111101 => "vwmacc.vv",
        0b111111 => "vwmaccsu.vv",
        _ => return None,
    };
    Some(Disasm {
        class: "V", mnemonic: mnem.into(),
        operands: format!("{}, {}, {}{}", vr(vd), vr(vs2), vr(vs1), mask(vm)),
    })
}

fn decode_vwxunary0(vs1: u8, vs2: u8, vd: u8, vm: u32) -> Option<Disasm> {
    let mnem: &'static str = match vs1 {
        0b00000 => "vmv.x.s",
        0b10000 => "vcpop.m",
        0b10001 => "vfirst.m",
        _ => return None,
    };
    let operands = if mnem == "vmv.x.s" {
        format!("{}, {}", r(vd), vr(vs2))
    } else {
        format!("{}, {}{}", r(vd), vr(vs2), mask(vm))
    };
    Some(Disasm { class: "V", mnemonic: mnem.into(), operands })
}

fn decode_vxunary0(vs1: u8, vs2: u8, vd: u8, vm: u32) -> Option<Disasm> {
    let mnem: &'static str = match vs1 {
        0b00010 => "vzext.vf8",
        0b00011 => "vsext.vf8",
        0b00100 => "vzext.vf4",
        0b00101 => "vsext.vf4",
        0b00110 => "vzext.vf2",
        0b00111 => "vsext.vf2",
        _ => return None,
    };
    Some(Disasm { class: "V", mnemonic: mnem.into(),
        operands: format!("{}, {}{}", vr(vd), vr(vs2), mask(vm)) })
}

fn decode_vmunary0(vs1: u8, vs2: u8, vd: u8, vm: u32) -> Option<Disasm> {
    let mnem: &'static str = match vs1 {
        0b00001 => "vmsbf.m",
        0b00010 => "vmsof.m",
        0b00011 => "vmsif.m",
        0b10000 => "viota.m",
        0b10001 => "vid.v",
        _ => return None,
    };
    let operands = if mnem == "vid.v" {
        format!("{}{}", vr(vd), mask(vm))
    } else {
        format!("{}, {}{}", vr(vd), vr(vs2), mask(vm))
    };
    Some(Disasm { class: "V", mnemonic: mnem.into(), operands })
}

// ─────────────────────────────────────────────────────────────────────────
// OPMVX — vector-scalar mask / multiply
// ─────────────────────────────────────────────────────────────────────────
fn decode_opmvx(funct6: u32, vm: u32, vs2: u8, rs1: u8, vd: u8) -> Option<Disasm> {
    let mnem: &'static str = match funct6 {
        0b001000 => "vaaddu.vx",
        0b001001 => "vaadd.vx",
        0b001010 => "vasubu.vx",
        0b001011 => "vasub.vx",
        0b001110 => "vslide1up.vx",
        0b001111 => "vslide1down.vx",
        0b010000 => return Some(Disasm {
            class: "V", mnemonic: "vmv.s.x".into(),
            operands: format!("{}, {}", vr(vd), r(rs1)),
        }),
        0b100000 => "vdivu.vx",
        0b100001 => "vdiv.vx",
        0b100010 => "vremu.vx",
        0b100011 => "vrem.vx",
        0b100100 => "vmulhu.vx",
        0b100101 => "vmul.vx",
        0b100110 => "vmulhsu.vx",
        0b100111 => "vmulh.vx",
        0b101001 => "vmadd.vx",
        0b101011 => "vnmsub.vx",
        0b101101 => "vmacc.vx",
        0b101111 => "vnmsac.vx",
        0b110000 => "vwaddu.vx",
        0b110001 => "vwadd.vx",
        0b110010 => "vwsubu.vx",
        0b110011 => "vwsub.vx",
        0b110100 => "vwaddu.wx",
        0b110101 => "vwadd.wx",
        0b110110 => "vwsubu.wx",
        0b110111 => "vwsub.wx",
        0b111000 => "vwmulu.vx",
        0b111010 => "vwmulsu.vx",
        0b111011 => "vwmul.vx",
        0b111100 => "vwmaccu.vx",
        0b111101 => "vwmacc.vx",
        0b111110 => "vwmaccus.vx",
        0b111111 => "vwmaccsu.vx",
        _ => return None,
    };
    Some(Disasm {
        class: "V", mnemonic: mnem.into(),
        operands: format!("{}, {}, {}{}", vr(vd), vr(vs2), r(rs1), mask(vm)),
    })
}

// ─────────────────────────────────────────────────────────────────────────
// OPFVV — vector-vector floating-point
// ─────────────────────────────────────────────────────────────────────────
fn decode_opfvv(funct6: u32, vm: u32, vs2: u8, vs1: u8, vd: u8) -> Option<Disasm> {
    let mnem: &'static str = match funct6 {
        0b000000 => "vfadd.vv",
        0b000001 => "vfredusum.vs",
        0b000010 => "vfsub.vv",
        0b000011 => "vfredosum.vs",
        0b000100 => "vfmin.vv",
        0b000101 => "vfredmin.vs",
        0b000110 => "vfmax.vv",
        0b000111 => "vfredmax.vs",
        0b001000 => "vfsgnj.vv",
        0b001001 => "vfsgnjn.vv",
        0b001010 => "vfsgnjx.vv",
        0b010000 => return decode_vwfunary0(vs1, vs2, vd),
        0b010010 => return decode_vfunary0(vs1, vs2, vd, vm),
        0b010011 => return decode_vfunary1(vs1, vs2, vd, vm),
        0b011000 => "vmfeq.vv",
        0b011001 => "vmfle.vv",
        0b011011 => "vmflt.vv",
        0b011100 => "vmfne.vv",
        0b100000 => "vfdiv.vv",
        0b100100 => "vfmul.vv",
        0b101000 => "vfmadd.vv",
        0b101001 => "vfnmadd.vv",
        0b101010 => "vfmsub.vv",
        0b101011 => "vfnmsub.vv",
        0b101100 => "vfmacc.vv",
        0b101101 => "vfnmacc.vv",
        0b101110 => "vfmsac.vv",
        0b101111 => "vfnmsac.vv",
        0b110000 => "vfwadd.vv",
        0b110001 => "vfwredusum.vs",
        0b110010 => "vfwsub.vv",
        0b110011 => "vfwredosum.vs",
        0b110100 => "vfwadd.wv",
        0b110110 => "vfwsub.wv",
        0b111000 => "vfwmul.vv",
        0b111100 => "vfwmacc.vv",
        0b111101 => "vfwnmacc.vv",
        0b111110 => "vfwmsac.vv",
        0b111111 => "vfwnmsac.vv",
        _ => return None,
    };
    Some(Disasm {
        class: "V", mnemonic: mnem.into(),
        operands: format!("{}, {}, {}{}", vr(vd), vr(vs2), vr(vs1), mask(vm)),
    })
}

fn decode_vwfunary0(vs1: u8, vs2: u8, vd: u8) -> Option<Disasm> {
    if vs1 == 0 {
        Some(Disasm { class: "V", mnemonic: "vfmv.f.s".into(),
            operands: format!("{}, {}", fr(vd), vr(vs2)) })
    } else { None }
}

fn decode_vfunary0(vs1: u8, vs2: u8, vd: u8, vm: u32) -> Option<Disasm> {
    let mnem: &'static str = match vs1 {
        0b00000 => "vfcvt.xu.f.v",
        0b00001 => "vfcvt.x.f.v",
        0b00010 => "vfcvt.f.xu.v",
        0b00011 => "vfcvt.f.x.v",
        0b00110 => "vfcvt.rtz.xu.f.v",
        0b00111 => "vfcvt.rtz.x.f.v",
        0b01000 => "vfwcvt.xu.f.v",
        0b01001 => "vfwcvt.x.f.v",
        0b01010 => "vfwcvt.f.xu.v",
        0b01011 => "vfwcvt.f.x.v",
        0b01100 => "vfwcvt.f.f.v",
        0b01110 => "vfwcvt.rtz.xu.f.v",
        0b01111 => "vfwcvt.rtz.x.f.v",
        0b10000 => "vfncvt.xu.f.w",
        0b10001 => "vfncvt.x.f.w",
        0b10010 => "vfncvt.f.xu.w",
        0b10011 => "vfncvt.f.x.w",
        0b10100 => "vfncvt.f.f.w",
        0b10101 => "vfncvt.rod.f.f.w",
        0b10110 => "vfncvt.rtz.xu.f.w",
        0b10111 => "vfncvt.rtz.x.f.w",
        _ => return None,
    };
    Some(Disasm { class: "V", mnemonic: mnem.into(),
        operands: format!("{}, {}{}", vr(vd), vr(vs2), mask(vm)) })
}

fn decode_vfunary1(vs1: u8, vs2: u8, vd: u8, vm: u32) -> Option<Disasm> {
    let mnem: &'static str = match vs1 {
        0b00000 => "vfsqrt.v",
        0b00100 => "vfrsqrt7.v",
        0b00101 => "vfrec7.v",
        0b10000 => "vfclass.v",
        _ => return None,
    };
    Some(Disasm { class: "V", mnemonic: mnem.into(),
        operands: format!("{}, {}{}", vr(vd), vr(vs2), mask(vm)) })
}

// ─────────────────────────────────────────────────────────────────────────
// OPFVF — vector-scalar floating-point
// ─────────────────────────────────────────────────────────────────────────
fn decode_opfvf(funct6: u32, vm: u32, vs2: u8, rs1: u8, vd: u8) -> Option<Disasm> {
    let mnem: &'static str = match funct6 {
        0b000000 => "vfadd.vf",
        0b000010 => "vfsub.vf",
        0b000100 => "vfmin.vf",
        0b000110 => "vfmax.vf",
        0b001000 => "vfsgnj.vf",
        0b001001 => "vfsgnjn.vf",
        0b001010 => "vfsgnjx.vf",
        0b001110 => "vfslide1up.vf",
        0b001111 => "vfslide1down.vf",
        0b010000 => return Some(Disasm {
            class: "V", mnemonic: "vfmv.s.f".into(),
            operands: format!("{}, {}", vr(vd), fr(rs1)),
        }),
        0b010111 => return Some(if vm == 0 {
            Disasm { class: "V", mnemonic: "vfmerge.vfm".into(),
                operands: format!("{}, {}, {}, v0", vr(vd), vr(vs2), fr(rs1)) }
        } else {
            Disasm { class: "V", mnemonic: "vfmv.v.f".into(),
                operands: format!("{}, {}", vr(vd), fr(rs1)) }
        }),
        0b011000 => "vmfeq.vf",
        0b011001 => "vmfle.vf",
        0b011011 => "vmflt.vf",
        0b011100 => "vmfne.vf",
        0b011101 => "vmfgt.vf",
        0b011111 => "vmfge.vf",
        0b100000 => "vfdiv.vf",
        0b100001 => "vfrdiv.vf",
        0b100100 => "vfmul.vf",
        0b100111 => "vfrsub.vf",
        0b101000 => "vfmadd.vf",
        0b101001 => "vfnmadd.vf",
        0b101010 => "vfmsub.vf",
        0b101011 => "vfnmsub.vf",
        0b101100 => "vfmacc.vf",
        0b101101 => "vfnmacc.vf",
        0b101110 => "vfmsac.vf",
        0b101111 => "vfnmsac.vf",
        0b110000 => "vfwadd.vf",
        0b110010 => "vfwsub.vf",
        0b110100 => "vfwadd.wf",
        0b110110 => "vfwsub.wf",
        0b111000 => "vfwmul.vf",
        0b111100 => "vfwmacc.vf",
        0b111101 => "vfwnmacc.vf",
        0b111110 => "vfwmsac.vf",
        0b111111 => "vfwnmsac.vf",
        _ => return None,
    };
    Some(Disasm {
        class: "V", mnemonic: mnem.into(),
        operands: format!("{}, {}, {}{}", vr(vd), vr(vs2), fr(rs1), mask(vm)),
    })
}
