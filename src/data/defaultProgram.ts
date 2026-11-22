// Default program for the single-cycle panel (ROM at 0)
// Default program — exercises every non-trap RV32I operation visible in
// the single-cycle datapath: R-type ALU, I-ALU, loads (B/H/W), stores
// (B/W), all 6 conditional branches taken, JAL/JALR with a real stack
// frame, CSR ops, and an I/O write.
//
// Stack convention: x2 (sp) starts at 0x10100 and grows down into RAM.
// The function at 0xdc computes a0 = a0 + a1*2; the caller pushes ra and
// pops it after the call.
export const DEFAULT_PROGRAM: { value: number; asm: string }[] = [
  // 0x00 — set up stack pointer at 0x10100 (top of RAM)
  { value: 0x00010137, asm: 'lui  x2, 0x10'      },
  { value: 0x10010113, asm: 'addi x2, x2, 0x100' },
  // 0x08 — R-type ALU sweep with t0=7, t1=5
  { value: 0x00700293, asm: 'addi x5, x0, 7'     },
  { value: 0x00500313, asm: 'addi x6, x0, 5'     },
  { value: 0x006283b3, asm: 'add  x7, x5, x6'    },
  { value: 0x40628433, asm: 'sub  x8, x5, x6'    },
  { value: 0x0062f4b3, asm: 'and  x9, x5, x6'    },
  { value: 0x0062e533, asm: 'or   x10, x5, x6'   },
  { value: 0x0062c5b3, asm: 'xor  x11, x5, x6'   },
  { value: 0x00629633, asm: 'sll  x12, x5, x6'   },
  { value: 0x0063d6b3, asm: 'srl  x13, x7, x6'   },
  { value: 0x4062d733, asm: 'sra  x14, x5, x6'   },
  { value: 0x0062a7b3, asm: 'slt  x15, x5, x6'   },
  { value: 0x00533833, asm: 'sltu x16, x6, x5'   },
  // 0x38 — I-ALU sweep
  { value: 0xfff00893, asm: 'addi x17, x0, -1'   },
  { value: 0x0ff8f913, asm: 'andi x18, x17, 0xff'},
  { value: 0x00a06993, asm: 'ori  x19, x0, 0xa'  },
  { value: 0x00429a13, asm: 'slli x20, x5, 4'    },
  { value: 0x0088da93, asm: 'srli x21, x17, 8'   },
  { value: 0x4088db13, asm: 'srai x22, x17, 8'   },
  // 0x50 — Memory sweep (RAM base 0x10000)
  { value: 0x00010bb7, asm: 'lui  x23, 0x10'     },
  { value: 0x05500c13, asm: 'addi x24, x0, 0x55' },
  { value: 0x018b8023, asm: 'sb   x24, 0(x23)'   },
  { value: 0x06600c93, asm: 'addi x25, x0, 0x66' },
  { value: 0x019b80a3, asm: 'sb   x25, 1(x23)'   },
  { value: 0x000b9d03, asm: 'lh   x26, 0(x23)'   },
  { value: 0x000bad83, asm: 'lw   x27, 0(x23)'   },
  { value: 0x01bba223, asm: 'sw   x27, 4(x23)'   },
  // 0x70 — Branch sweep (each conditional taken, skipping the nop after)
  { value: 0x00528463, asm: 'beq  x5, x5, 8'     },
  { value: 0x00000013, asm: 'nop'                },
  { value: 0x00629463, asm: 'bne  x5, x6, 8'     },
  { value: 0x00000013, asm: 'nop'                },
  { value: 0x00534463, asm: 'blt  x6, x5, 8'     },
  { value: 0x00000013, asm: 'nop'                },
  { value: 0x0062d463, asm: 'bge  x5, x6, 8'     },
  { value: 0x00000013, asm: 'nop'                },
  { value: 0x00536463, asm: 'bltu x6, x5, 8'     },
  { value: 0x00000013, asm: 'nop'                },
  { value: 0x0062f463, asm: 'bgeu x5, x6, 8'     },
  { value: 0x00000013, asm: 'nop'                },
  // 0xa0 — Function call with stack frame
  { value: 0xff010113, asm: 'addi x2, x2, -16'   }, // sp -= 16
  { value: 0x00112023, asm: 'sw   x1, 0(x2)'     }, // save ra
  { value: 0x00400513, asm: 'addi x10, x0, 4'    }, // a0 = 4
  { value: 0x00600593, asm: 'addi x11, x0, 6'    }, // a1 = 6
  { value: 0x02c000ef, asm: 'jal  x1, 44'        }, // call func at 0xdc
  { value: 0x00012083, asm: 'lw   x1, 0(x2)'     }, // restore ra
  { value: 0x01010113, asm: 'addi x2, x2, 16'    }, // sp += 16
  { value: 0x00050e93, asm: 'addi x29, x10, 0'   }, // t4 = result (= 16)
  // 0xc0 — CSR sweep on mscratch
  { value: 0x04200f13, asm: 'addi x30, x0, 0x42' },
  { value: 0x340f1073, asm: 'csrrw x0, mscratch, x30' },
  { value: 0x34002ff3, asm: 'csrrs x31, mscratch, x0' },
  // 0xcc — I/O write (LED block at 0x40020000)
  { value: 0x40020237, asm: 'lui  x4, 0x40020'   },
  { value: 0x0aa00193, asm: 'addi x3, x0, 0xaa'  },
  { value: 0x00322023, asm: 'sw   x3, 0(x4)'     },
  // 0xd8 — Spin
  { value: 0x0000006f, asm: 'jal  x0, 0'         },
  // 0xdc — Function: a0 = a0 + a1*2 ; ret
  { value: 0x00159593, asm: 'slli x11, x11, 1'   },
  { value: 0x00b50533, asm: 'add  x10, x10, x11' },
  { value: 0x00008067, asm: 'jalr x0, x1, 0'     },
]
