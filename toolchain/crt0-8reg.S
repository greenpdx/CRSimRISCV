# ─────────────────────────────────────────────────────────────────────────────
# Minimal startup for 8-register RV32I educational CPU
#
# Register convention (x0–x7 only):
#   x0 = zero   x1 = ra   x2 = sp   x3 = t0
#   x4 = a0     x5 = a1   x6 = s0   x7 = s1
# ─────────────────────────────────────────────────────────────────────────────

.section .text.init, "ax"
.globl _start

_start:
    # Set up stack pointer at top of RAM
    lui   x2, 0x10        # x2 (sp) = 0x00010000
    addi  x2, x2, 128     # sp = 0x00010080 (top of 32-word RAM)

    # Clear registers x3–x7
    addi  x3, x0, 0
    addi  x4, x0, 0
    addi  x5, x0, 0
    addi  x6, x0, 0
    addi  x7, x0, 0

    # Jump to main (return address in x1)
    jal   x1, main

    # Halt: infinite loop
_halt:
    jal   x0, _halt
