# ─────────────────────────────────────────────────────────────────────────────
# GCC / Clang flags for 8-register RV32I educational CPU
#
# This constrains the compiler to only use x0–x7, matching the simplified
# register file:
#   x0 = zero   x1 = ra   x2 = sp   x3 = t0
#   x4 = a0     x5 = a1   x6 = s0   x7 = s1
#
# Usage:
#   riscv32-unknown-elf-gcc $(RV8_CFLAGS) -o out.elf main.c
#   riscv32-unknown-elf-objcopy -O binary out.elf out.bin
# ─────────────────────────────────────────────────────────────────────────────

# Base architecture: RV32I only (no M/A/F/D extensions)
RV8_ARCH   := -march=rv32i -mabi=ilp32

# Tell the compiler x8–x31 are off-limits.
# -ffixed-REG means "never allocate this register".
RV8_FIXED  := \
  -ffixed-x8  -ffixed-x9  \
  -ffixed-x10 -ffixed-x11 -ffixed-x12 -ffixed-x13 \
  -ffixed-x14 -ffixed-x15 -ffixed-x16 -ffixed-x17 \
  -ffixed-x18 -ffixed-x19 -ffixed-x20 -ffixed-x21 \
  -ffixed-x22 -ffixed-x23 -ffixed-x24 -ffixed-x25 \
  -ffixed-x26 -ffixed-x27 -ffixed-x28 -ffixed-x29 \
  -ffixed-x30 -ffixed-x31

# Override the ABI register mapping so the compiler uses our convention:
#   a0 = x4 (not x10),  a1 = x5 (not x11)
#   s0 = x6 (not x8),   s1 = x7 (not x9)
#   t0 = x3 (not x5)
#
# GCC's -ffixed only hides registers from allocation. To remap the ABI
# names (a0→x4 etc.) you need a custom register-rename header, see below.
# For simple programs, just use x-register names directly in inline asm.

# Remap callee-saved / caller-saved roles for x3–x7:
#   x3 = caller-saved temp      → -fcall-used-x3
#   x4 = caller-saved arg/ret   → -fcall-used-x4
#   x5 = caller-saved arg       → -fcall-used-x5
#   x6 = callee-saved           → -fcall-saved-x6
#   x7 = callee-saved           → -fcall-saved-x7
RV8_CALLING := \
  -fcall-used-x3  \
  -fcall-used-x4  \
  -fcall-used-x5  \
  -fcall-saved-x6 \
  -fcall-saved-x7

# No GP/TP relaxation (we don't have x3=gp or x4=tp in this ABI)
RV8_RELAX  := -mno-relax

# Minimal code model — no function calls beyond direct jumps
RV8_CODE   := -fno-pic -fno-plt -nostdlib

# Combined flags
RV8_CFLAGS := $(RV8_ARCH) $(RV8_FIXED) $(RV8_CALLING) $(RV8_RELAX) $(RV8_CODE) -Os

# Linker script: place .text at 0x00000000, .data at 0x00010000
RV8_LDFLAGS := -T rv32i-8reg.ld -nostdlib
