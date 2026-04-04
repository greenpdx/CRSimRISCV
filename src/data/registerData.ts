// ─────────────────────────────────────────────────────────────────────────────
// RISC-V RV32I register definitions
// Single source of truth — used by RegisterFile, assembler, disassembler, docs
// ─────────────────────────────────────────────────────────────────────────────

export type SavedBy = 'caller' | 'callee' | 'none' | 'special'

export interface RegisterDef {
  xNum:      number      // x0 … x31
  name:      string      // ABI name: zero, ra, sp …
  altName?:  string      // e.g. fp for s0
  group:     string      // zero | return | stack | global | thread | temp | saved | arg | extra
  savedBy:   SavedBy
  resetVal:  number      // conventional reset/startup value (0 for most)
  description: string
}

export const REGISTER_DEFS: RegisterDef[] = [
  // ── Hardwired zero ──────────────────────────────────────────────────────
  { xNum:  0, name: 'zero', group: 'zero',    savedBy: 'special', resetVal: 0,
    description: 'Hardwired to 0, writes ignored' },

  // ── Return address ───────────────────────────────────────────────────────
  { xNum:  1, name: 'ra',   group: 'return',  savedBy: 'caller',  resetVal: 0,
    description: 'Return address (call/ret)' },

  // ── Stack / global / thread ──────────────────────────────────────────────
  { xNum:  2, name: 'sp',   group: 'stack',   savedBy: 'callee',  resetVal: 0,
    description: 'Stack pointer (grows down)' },
  { xNum:  3, name: 'gp',   group: 'global',  savedBy: 'special', resetVal: 0,
    description: 'Global pointer (linker-set)' },
  { xNum:  4, name: 'tp',   group: 'thread',  savedBy: 'special', resetVal: 0,
    description: 'Thread pointer (OS-set)' },

  // ── Temporaries (caller-saved) ───────────────────────────────────────────
  { xNum:  5, name: 't0',   group: 'temp',    savedBy: 'caller',  resetVal: 0,
    description: 'Temporary / alt link register' },
  { xNum:  6, name: 't1',   group: 'temp',    savedBy: 'caller',  resetVal: 0,
    description: 'Temporary' },
  { xNum:  7, name: 't2',   group: 'temp',    savedBy: 'caller',  resetVal: 0,
    description: 'Temporary' },

  // ── Saved registers (callee-saved) ───────────────────────────────────────
  { xNum:  8, name: 's0',   altName: 'fp', group: 'saved', savedBy: 'callee', resetVal: 0,
    description: 'Saved register / frame pointer' },
  { xNum:  9, name: 's1',   group: 'saved',   savedBy: 'callee',  resetVal: 0,
    description: 'Saved register' },

  // ── Function arguments / return values ───────────────────────────────────
  { xNum: 10, name: 'a0',   group: 'arg',     savedBy: 'caller',  resetVal: 0,
    description: 'Argument 0 / return value 0' },
  { xNum: 11, name: 'a1',   group: 'arg',     savedBy: 'caller',  resetVal: 0,
    description: 'Argument 1 / return value 1' },
  { xNum: 12, name: 'a2',   group: 'arg',     savedBy: 'caller',  resetVal: 0,
    description: 'Argument 2' },
  { xNum: 13, name: 'a3',   group: 'arg',     savedBy: 'caller',  resetVal: 0,
    description: 'Argument 3' },
  { xNum: 14, name: 'a4',   group: 'arg',     savedBy: 'caller',  resetVal: 0,
    description: 'Argument 4' },
  { xNum: 15, name: 'a5',   group: 'arg',     savedBy: 'caller',  resetVal: 0,
    description: 'Argument 5' },
  { xNum: 16, name: 'a6',   group: 'arg',     savedBy: 'caller',  resetVal: 0,
    description: 'Argument 6' },
  { xNum: 17, name: 'a7',   group: 'arg',     savedBy: 'caller',  resetVal: 0,
    description: 'Argument 7 / syscall number' },

  // ── More saved registers ─────────────────────────────────────────────────
  { xNum: 18, name: 's2',   group: 'saved',   savedBy: 'callee',  resetVal: 0,
    description: 'Saved register' },
  { xNum: 19, name: 's3',   group: 'saved',   savedBy: 'callee',  resetVal: 0,
    description: 'Saved register' },
  { xNum: 20, name: 's4',   group: 'saved',   savedBy: 'callee',  resetVal: 0,
    description: 'Saved register' },
  { xNum: 21, name: 's5',   group: 'saved',   savedBy: 'callee',  resetVal: 0,
    description: 'Saved register' },
  { xNum: 22, name: 's6',   group: 'saved',   savedBy: 'callee',  resetVal: 0,
    description: 'Saved register' },
  { xNum: 23, name: 's7',   group: 'saved',   savedBy: 'callee',  resetVal: 0,
    description: 'Saved register' },
  { xNum: 24, name: 's8',   group: 'saved',   savedBy: 'callee',  resetVal: 0,
    description: 'Saved register' },
  { xNum: 25, name: 's9',   group: 'saved',   savedBy: 'callee',  resetVal: 0,
    description: 'Saved register' },
  { xNum: 26, name: 's10',  group: 'saved',   savedBy: 'callee',  resetVal: 0,
    description: 'Saved register' },
  { xNum: 27, name: 's11',  group: 'saved',   savedBy: 'callee',  resetVal: 0,
    description: 'Saved register' },

  // ── More temporaries ─────────────────────────────────────────────────────
  { xNum: 28, name: 't3',   group: 'temp',    savedBy: 'caller',  resetVal: 0,
    description: 'Temporary' },
  { xNum: 29, name: 't4',   group: 'temp',    savedBy: 'caller',  resetVal: 0,
    description: 'Temporary' },
  { xNum: 30, name: 't5',   group: 'temp',    savedBy: 'caller',  resetVal: 0,
    description: 'Temporary' },
  { xNum: 31, name: 't6',   group: 'temp',    savedBy: 'caller',  resetVal: 0,
    description: 'Temporary' },
]

// ── Lookup helpers ───────────────────────────────────────────────────────────

/** Fast map: xNum → RegisterDef */
export const REG_BY_NUM = new Map<number, RegisterDef>(
  REGISTER_DEFS.map(r => [r.xNum, r])
)

/** Fast map: ABI name → RegisterDef */
export const REG_BY_NAME = new Map<string, RegisterDef>(
  REGISTER_DEFS.map(r => [r.name, r])
)

// ── Simplified 8-register subset for Level 1 (educational) view ─────────────
// Maps x0–x7 with custom ABI names for an 8-register teaching CPU.
//   x0 = zero   x1 = ra   x2 = sp   x3 = t0
//   x4 = a0     x5 = a1   x6 = s0   x7 = s1
export const REGISTER_DEFS_8: RegisterDef[] = [
  { xNum: 0, name: 'zero', group: 'zero',   savedBy: 'special', resetVal: 0,
    description: 'Hardwired to 0, writes ignored' },
  { xNum: 1, name: 'ra',   group: 'return', savedBy: 'caller',  resetVal: 0,
    description: 'Return address (call/ret)' },
  { xNum: 2, name: 'sp',   group: 'stack',  savedBy: 'callee',  resetVal: 0,
    description: 'Stack pointer (grows down)' },
  { xNum: 3, name: 't0',   group: 'temp',   savedBy: 'caller',  resetVal: 0,
    description: 'Temporary' },
  { xNum: 4, name: 'a0',   group: 'arg',    savedBy: 'caller',  resetVal: 0,
    description: 'Argument 0 / return value' },
  { xNum: 5, name: 'a1',   group: 'arg',    savedBy: 'caller',  resetVal: 0,
    description: 'Argument 1' },
  { xNum: 6, name: 's0',   altName: 'fp', group: 'saved', savedBy: 'callee', resetVal: 0,
    description: 'Saved register / frame pointer' },
  { xNum: 7, name: 's1',   group: 'saved',  savedBy: 'callee',  resetVal: 0,
    description: 'Saved register' },
]

// ── Group colour hints (used by RegisterRow for group-tinted rows) ───────────
export const GROUP_COLORS: Record<string, string> = {
  zero:    '#e0f0ff',
  return:  '#fff8e0',
  stack:   '#f0ffe0',
  global:  '#f5f5f5',
  thread:  '#f5f5f5',
  temp:    '#ffffff',
  saved:   '#f0f8ff',
  arg:     '#fff0f8',
  extra:   '#ffffff',
}
