// ─────────────────────────────────────────────────────────────────────────────
// Godbolt / Compiler Explorer API wrapper
// Compiles Rust or C to RISC-V RV32I assembly via the public API
// ─────────────────────────────────────────────────────────────────────────────

export interface GodboltResult {
  success: boolean
  asm: string
  errors: string[]
}

const COMPILERS: Record<string, { id: string; args: string }> = {
  c: {
    id: 'rv32-cgcctrunk',
    args: '-march=rv32i -mabi=ilp32 -O2 -ffreestanding -nostdlib',
  },
  rust: {
    id: 'nightly',
    args: '--target riscv32i-unknown-none-elf -C opt-level=1',
  },
}

export async function compileWithGodbolt(
  source: string,
  language: 'c' | 'rust',
): Promise<GodboltResult> {
  const compiler = COMPILERS[language]
  if (!compiler) {
    return { success: false, asm: '', errors: [`Unsupported language: ${language}`] }
  }

  try {
    const url = `https://godbolt.org/api/compiler/${compiler.id}/compile`

    const resp = await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Accept': 'application/json',
      },
      body: JSON.stringify({
        source,
        options: {
          userArguments: compiler.args,
          filters: {
            binary: false,
            execute: false,
            labels: true,
            directives: true,
            commentOnly: true,
            trim: true,
          },
        },
      }),
    })

    if (!resp.ok) {
      return { success: false, asm: '', errors: [`Godbolt API error: ${resp.status}`] }
    }

    const data = await resp.json()

    // Extract assembly lines from _start function only
    const rawLines: string[] = (data.asm || []).map((line: { text: string }) => line.text)

    let inTarget = false
    const asmLines: string[] = []

    for (const line of rawLines) {
      const trimmed = line.trim()
      if (trimmed === '') continue

      // Start capturing at _start: (or main:)
      if (trimmed === '_start:' || trimmed === 'main:') {
        inTarget = true
        continue
      }

      // If we hit another top-level symbol (not indented, not .L local label), stop
      if (inTarget && !line.startsWith(' ') && !line.startsWith('\t')
          && trimmed.endsWith(':') && !trimmed.startsWith('.L') && !trimmed.startsWith('.LBB')) {
        break
      }

      if (!inTarget) continue

      // Strip directives but keep .LBB/.L local labels
      if (trimmed.startsWith('.') && !trimmed.startsWith('.LBB') && !trimmed.startsWith('.L')) continue

      asmLines.push(trimmed)
    }

    // If no _start/main found, return everything except directives
    const asm = asmLines.length > 0
      ? asmLines.join('\n')
      : rawLines
          .map(l => l.trim())
          .filter(t => t && !(t.startsWith('.') && !t.startsWith('.L')))
          .join('\n')

    // Extract errors
    const errors: string[] = (data.stderr || [])
      .map((line: { text: string }) => line.text)
      .filter((t: string) => t.trim())

    return {
      success: true,
      asm,
      errors,
    }
  } catch (err: any) {
    return {
      success: false,
      asm: '',
      errors: [err.message || 'Network error — is Godbolt reachable?'],
    }
  }
}
