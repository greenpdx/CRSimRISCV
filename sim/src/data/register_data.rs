//! Mirrors `src/data/registerData.ts` — RV32I register ABI names.

#[derive(Debug, Clone, Copy)]
pub struct RegisterDef {
    pub x_num: u8,
    pub name: &'static str,
}

pub const REGISTER_DEFS: &[RegisterDef] = &[
    RegisterDef { x_num:  0, name: "zero" }, RegisterDef { x_num:  1, name: "ra"   },
    RegisterDef { x_num:  2, name: "sp"   }, RegisterDef { x_num:  3, name: "gp"   },
    RegisterDef { x_num:  4, name: "tp"   }, RegisterDef { x_num:  5, name: "t0"   },
    RegisterDef { x_num:  6, name: "t1"   }, RegisterDef { x_num:  7, name: "t2"   },
    RegisterDef { x_num:  8, name: "s0"   }, RegisterDef { x_num:  9, name: "s1"   },
    RegisterDef { x_num: 10, name: "a0"   }, RegisterDef { x_num: 11, name: "a1"   },
    RegisterDef { x_num: 12, name: "a2"   }, RegisterDef { x_num: 13, name: "a3"   },
    RegisterDef { x_num: 14, name: "a4"   }, RegisterDef { x_num: 15, name: "a5"   },
    RegisterDef { x_num: 16, name: "a6"   }, RegisterDef { x_num: 17, name: "a7"   },
    RegisterDef { x_num: 18, name: "s2"   }, RegisterDef { x_num: 19, name: "s3"   },
    RegisterDef { x_num: 20, name: "s4"   }, RegisterDef { x_num: 21, name: "s5"   },
    RegisterDef { x_num: 22, name: "s6"   }, RegisterDef { x_num: 23, name: "s7"   },
    RegisterDef { x_num: 24, name: "s8"   }, RegisterDef { x_num: 25, name: "s9"   },
    RegisterDef { x_num: 26, name: "s10"  }, RegisterDef { x_num: 27, name: "s11"  },
    RegisterDef { x_num: 28, name: "t3"   }, RegisterDef { x_num: 29, name: "t4"   },
    RegisterDef { x_num: 30, name: "t5"   }, RegisterDef { x_num: 31, name: "t6"   },
];
