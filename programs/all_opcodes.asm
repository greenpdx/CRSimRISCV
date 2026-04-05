# ── RV32I All Opcodes Test Program ──
# Uses 8-register set: x0=zero x1=ra x2=sp x3=t0 x4=a0 x5=a1 x6=s0 x7=s1

# ── R-type ──
          ADDI X3, X0, 7
          ADDI X4, X0, 3
          ADD  X5, X3, X4
          SUB  X5, X3, X4
          AND  X5, X3, X4
          OR   X5, X3, X4
          XOR  X5, X3, X4
          SLL  X5, X3, X4
          SRL  X5, X3, X4
          SLT  X5, X4, X3
# ── I-ALU ──
          ANDI X5, X3, 3
          ORI  X5, X3, 16
          XORI X5, X3, -1
          SLLI X5, X3, 2
          SRLI X5, X3, 1
          SLTI X5, X3, 10
# ── U-type ──
          LUI  X6, 0x10
          AUIPC X7, 0
# ── Store + Load ──
          SW   X3, 0(X6)
          LW   X5, 0(X6)
# ── Branch ──
          ADDI X3, X0, 2
LOOP:     ADDI X3, X3, -1
          BNE  X3, X0, LOOP
# ── JAL + JALR (function call pattern) ──
          JAL  X1, FUNC
          ADDI X5, X0, 42
          JAL  X0, CONT
FUNC:     ADDI X3, X0, 99
          JALR X0, X1, 0
# ── I/O write ──
CONT:     LUI  X6, 0x40020
          SW   X5, 0(X6)
# ── HALT ──
HALT:     JAL  X0, HALT
