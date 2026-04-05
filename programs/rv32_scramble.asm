# ══════════════════════════════════════════════════════════════
# RV32-SCRAMBLE: Crypto hash using every RV32I opcode ONCE
# Branches may repeat. All other instructions used exactly once.
# Input: immediate 0x47 ('G')  Output: hash in x3 + RAM
# ══════════════════════════════════════════════════════════════

# ── Setup: build keys and input ──
          LUI   X6, 0x10         # x6 = 0x10000 (RAM base)
          AUIPC X7, 0            # x7 = PC salt (position-dependent)
          ADDI  X3, X0, 0x47     # x3 = input plaintext 'G'
          ORI   X4, X0, 0x5A     # x4 = round key 0x5A

# ── Round 1: Shift-XOR avalanche ──
          SLLI  X5, X3, 5        # x5 = input << 5
          SRLI  X1, X3, 3        # x1 = input >>> 3
          XOR   X3, X5, X1       # x3 = (in<<5) ^ (in>>>3)
          ADD   X3, X3, X4       # x3 += round key

# ── Round 2: Nonlinear substitution ──
          SRAI  X5, X3, 4        # x5 = arith shift (sign extension)
          XORI  X5, X5, -1       # x5 = NOT(x5) — bit inversion
          AND   X1, X3, X5       # x1 = state & ~(state>>4) — nonlinear
          OR    X3, X1, X7       # x3 |= PC salt — position binding
          SUB   X5, X3, X4       # x5 = state - key — differential

# ── Round 3: Data-dependent rotation ──
          ANDI  X2, X5, 0xF      # x2 = shift amount (0-15)
          SLL   X1, X3, X2       # x1 = state << variable
          SRL   X5, X3, X2       # x5 = state >>> variable
          SRA   X3, X1, X2       # x3 = (state<<var) >>a var

# ── Round 4: Comparison-based bit folding ──
          SLT   X5, X3, X4       # x5 = (hash <s key) ? 1 : 0
          SLTU  X1, X4, X3       # x1 = (key <u hash) ? 1 : 0
          SLTI  X2, X3, 100      # x2 = (hash <s 100) ? 1 : 0
          SLTIU X7, X3, 200      # x7 = (hash <u 200) ? 1 : 0

# ── Subroutine call: store results to RAM ──
          JAL   X1, STORE        # call STORE, x1 = return addr
HALT:     BEQ   X0, X0, HALT     # halt loop (branch reusable)

# ── STORE subroutine: all memory ops ──
STORE:    SW    X3, 0(X6)        # store full 32-bit hash
          SH    X5, 4(X6)        # store comparison half
          SB    X2, 8(X6)        # store flag byte
          LW    X3, 0(X6)        # verify: reload word
          LH    X4, 4(X6)        # verify: reload half (sign-ext)
          LB    X5, 8(X6)        # verify: reload byte (sign-ext)
          LHU   X2, 4(X6)        # verify: reload half (zero-ext)
          LBU   X7, 8(X6)        # verify: reload byte (zero-ext)
          JALR  X0, X1, 0        # return to caller
