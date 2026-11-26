// ─────────────────────────────────────────────────────────────────────────────
// Builds public/data/isa-instructions.json: the instructions the simulator's
// cores implement, sliced from the RISC-V ISA Explorer's data
// (https://github.com/riscv/riscv-isa-explorer, Apache-2.0).
//
//   node scripts/isa-slice.mjs <dir> <commit>
//
// <dir> holds src/instruction-metadata.json, src/instr_dict.json and
// src/riscv_extensions.json of that repository at <commit>. Add a mnemonic to GROUPS
// and run it again.
// ─────────────────────────────────────────────────────────────────────────────
import fs from 'node:fs'
import path from 'node:path'

const [dir, commit] = process.argv.slice(2)
if (!dir || !commit) { console.error('usage: isa-slice.mjs <data dir> <commit>'); process.exit(1) }
const meta = JSON.parse(fs.readFileSync(path.join(dir, 'instruction-metadata.json'), 'utf8'))
const dict = JSON.parse(fs.readFileSync(path.join(dir, 'instr_dict.json'), 'utf8'))
const catalogue = JSON.parse(fs.readFileSync(path.join(dir, 'riscv_extensions.json'), 'utf8'))

/** Every extension of the explorer's catalogue ({ id, instructions: { MNEMONIC: … } }) */
const extensions = (function walk(x) {
  if (Array.isArray(x)) return x.flatMap(walk)
  if (x && typeof x === 'object') return 'id' in x && 'instructions' in x ? [x] : Object.values(x).flatMap(walk)
  return []
})(catalogue)
/** The explorer matches ?insn= on the upper-cased mnemonic (src/permalinks.js) */
const key = m => String(m).trim().toUpperCase().split(/\s+/)[0]
/**
 * Link that opens one instruction: ?ext=<extension>&insn=<MNEMONIC>. The
 * extension must list the instruction, or the explorer ignores the link.
 */
function explorerLink(mnemonic, prefer) {
  const lists = e => Object.keys(e.instructions ?? {}).find(k => key(k) === key(mnemonic))
  const ext = prefer.map(id => extensions.find(e => e.id.toLowerCase() === id.toLowerCase())).find(e => e && lists(e))
    ?? extensions.find(lists)
  if (!ext) throw new Error(`no extension of the explorer lists ${mnemonic}`)
  return { ext: ext.id, url: `${EXPLORER}?ext=${encodeURIComponent(ext.id)}&insn=${encodeURIComponent(lists(ext))}` }
}

const EXPLORER = 'https://tech.riscv.org/isa-explorer/'

/** Mnemonics by the group that implements them (a core lists the groups it has) */
const GROUPS = {
  rv32i: ['lui', 'auipc', 'jal', 'jalr', 'beq', 'bne', 'blt', 'bge', 'bltu', 'bgeu',
    'lb', 'lh', 'lw', 'lbu', 'lhu', 'sb', 'sh', 'sw',
    'addi', 'slti', 'sltiu', 'xori', 'ori', 'andi', 'slli', 'srli', 'srai',
    'add', 'sub', 'sll', 'slt', 'sltu', 'xor', 'srl', 'sra', 'or', 'and',
    'fence', 'ecall', 'ebreak'],
  zmmul: ['mul', 'mulh', 'mulhsu', 'mulhu'],
  zicsr: ['csrrw', 'csrrs', 'csrrc', 'csrrwi', 'csrrsi', 'csrrci'],
  priv: ['mret', 'sret', 'wfi'],
}

/** Explorer extension to link each group to, in order of preference (then the instruction's own) */
const LINK_EXT = { rv32i: ['RV32I'] }

/** RV32 shifts: the explorer's encoding is the RV64 one (6-bit shamt) */
const RV32_SHIFT = { slli: 0x00001013, srli: 0x00005013, srai: 0x40005013 }

function hex(v) { return '0x' + (v >>> 0).toString(16) }
function bits(match, mask) {
  let s = ''
  for (let i = 31; i >= 0; i--) s += (mask >>> i) & 1 ? String((match >>> i) & 1) : '-'
  return s
}

const instructions = {}
for (const [group, list] of Object.entries(GROUPS)) for (const m of list) {
  // The encoding file writes '.' as '_' and keeps RV32-only forms under <name>_rv32
  const k = m.replace(/\./g, '_')
  const a = meta[m], d = dict[`${k}_rv32`] ?? dict[k]
  if (!a || !d) throw new Error(`not in the explorer data: ${m}`)
  // defined_by is one extension, or a combination: take the first one named
  const firstName = x => !x || typeof x !== 'object' ? null : typeof x.name === 'string' ? x.name
    : Object.values(x).flat().map(firstName).find(Boolean) ?? null
  const ext = firstName(a.defined_by?.extension)
  let { encoding, match, mask } = d, fields = d.variable_fields
  if (m in RV32_SHIFT) {
    match = hex(RV32_SHIFT[m]); mask = hex(0xfe00707f); encoding = bits(RV32_SHIFT[m], 0xfe00707f)
    fields = ['rd', 'rs1', 'shamtw']
  }
  const link = explorerLink(m, [...(LINK_EXT[group] ?? []), ...(ext ? [ext] : [])])
  instructions[m] = {
    group, name: a.long_name, description: a.description.trim(), assembly: a.assembly ?? '',
    extension: ext, encoding, fields, match, mask,
    // Opens this instruction in the explorer
    explorer: link.url,
  }
}

const out = {
  source: {
    name: 'RISC-V ISA Explorer',
    repository: 'https://github.com/riscv/riscv-isa-explorer',
    commit,
    license: 'Apache-2.0',
    files: ['src/instruction-metadata.json', 'src/instr_dict.json', 'src/riscv_extensions.json'],
    note: 'name, description, assembly, extension and encoding are taken from these files; RV32 shift encodings are set here (scripts/isa-slice.mjs)',
  },
  explorer: EXPLORER,
  instructions,
}
const file = new URL('../public/data/isa-instructions.json', import.meta.url)
fs.writeFileSync(file, JSON.stringify(out, null, 1) + '\n')
console.log(`${Object.keys(instructions).length} instructions → public/data/isa-instructions.json`)
