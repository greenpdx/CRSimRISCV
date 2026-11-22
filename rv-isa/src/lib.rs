//! RISC-V instruction set — the pure, stateless parts shared by every core,
//! the assembler and the wasm API: instruction fields, immediates, the ALU
//! and branch conditions, load / store lanes, the multiply family, CSR addresses and Zicsr arithmetic,
//! register names, the decoder, the encoders, the disassembler.
#![forbid(unsafe_code)]

pub mod alu;
pub mod branch;
pub mod csr;
pub mod disasm;
pub mod encode;
pub mod fields;
pub mod imm;
pub mod instr;
pub mod lsu;
pub mod mul;
pub mod reg;
