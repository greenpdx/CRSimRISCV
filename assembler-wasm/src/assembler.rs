use std::collections::HashMap;
use rv_isa::encode::{b, i, j, r, s, shift, u};
use rv_isa::reg::reg_number;
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct Instruction {
    pub address: u32,
    pub value: u32,
    pub hex: String,
    pub asm: String,
}

#[derive(Serialize, Clone)]
pub struct AsmError {
    pub line: usize,
    pub message: String,
}

#[derive(Serialize)]
pub struct AssemblerResult {
    pub success: bool,
    pub instructions: Vec<Instruction>,
    pub errors: Vec<AsmError>,
}

/// Extract label from start of line. Returns position of ':' if valid label found.
/// A valid label is [a-zA-Z_.][a-zA-Z0-9_.]* followed by : (not ::)
fn extract_label(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    // First char must be letter, underscore, or dot
    let first = bytes[0];
    if !first.is_ascii_alphabetic() && first != b'_' && first != b'.' {
        return None;
    }
    // Scan for ':'
    for (i, &b) in bytes.iter().enumerate().skip(1) {
        if b == b':' {
            // Make sure next char is not ':' (reject __rustc::foo)
            if i + 1 < bytes.len() && bytes[i + 1] == b':' {
                return None;
            }
            return Some(i);
        }
        if !b.is_ascii_alphanumeric() && b != b'_' && b != b'.' {
            return None;
        }
    }
    None
}

/// Parse an immediate value: decimal, hex (0x...), or negative
fn parse_imm(s: &str) -> Result<i32, String> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16)
            .map(|v| v as i32)
            .map_err(|_| format!("invalid hex immediate '{}'", s))
    } else {
        s.parse::<i32>()
            .map_err(|_| format!("invalid immediate '{}'", s))
    }
}

/// Parse "offset(reg)" → (offset, reg_num)
fn parse_offset_reg(s: &str) -> Result<(i32, u32), String> {
    // e.g. "4(x5)" or "-8(sp)"
    let s = s.trim();
    if let Some(paren) = s.find('(') {
        let offset_str = &s[..paren];
        let reg_str = s[paren + 1..].trim_end_matches(')');
        let offset = if offset_str.is_empty() { 0 } else { parse_imm(offset_str)? };
        let reg = reg_number(reg_str).ok_or_else(|| format!("unknown register '{}'", reg_str))?;
        Ok((offset, reg))
    } else {
        Err(format!("expected offset(reg), got '{}'", s))
    }
}

/// Two-pass assembler for RV32I
pub fn assemble(source: &str) -> AssemblerResult {
    let mut errors = Vec::new();
    let mut labels: HashMap<String, u32> = HashMap::new();

    // Pre-process: strip comments, collect lines
    let lines: Vec<(usize, String)> = source
        .lines()
        .enumerate()
        .map(|(i, line)| {
            let line = if let Some(pos) = line.find('#') {
                &line[..pos]
            } else {
                line
            };
            (i + 1, line.trim().to_string())
        })
        .collect();

    // Pass 1: collect labels, count instructions
    let mut addr: u32 = 0;
    let mut instr_lines: Vec<(usize, u32, String)> = Vec::new(); // (line_num, address, text)

    for (line_num, text) in &lines {
        if text.is_empty() {
            continue;
        }

        // Check for label: a valid label is [a-zA-Z_.][a-zA-Z0-9_.]*:
        // Must not contain :: (like __rustc::foo) — that's not a label
        if let Some(label_end) = extract_label(text) {
            let label = text[..label_end].trim().to_string();
            if !label.is_empty() {
                labels.insert(label, addr);
            }
            let rest = text[label_end + 1..].trim().to_string();
            if !rest.is_empty() {
                instr_lines.push((*line_num, addr, rest));
                addr += 4;
            }
        } else {
            instr_lines.push((*line_num, addr, text.clone()));
            addr += 4;
        }
    }

    // Pass 2: encode instructions
    let mut instructions = Vec::new();

    for (line_num, address, text) in &instr_lines {
        match encode_instruction(text, *address, &labels) {
            Ok(value) => {
                instructions.push(Instruction {
                    address: *address,
                    value,
                    hex: format!("{:08x}", value),
                    asm: text.clone(),
                });
            }
            Err(msg) => {
                errors.push(AsmError {
                    line: *line_num,
                    message: msg,
                });
            }
        }
    }

    AssemblerResult {
        success: errors.is_empty(),
        instructions,
        errors,
    }
}

/// Parse and encode one instruction line
fn encode_instruction(
    text: &str,
    current_addr: u32,
    labels: &HashMap<String, u32>,
) -> Result<u32, String> {
    let parts: Vec<&str> = text.splitn(2, char::is_whitespace).collect();
    let mnemonic = parts[0].to_lowercase();
    let operands_str = if parts.len() > 1 { parts[1].trim() } else { "" };
    let operands: Vec<&str> = if operands_str.is_empty() {
        vec![]
    } else {
        operands_str.split(',').map(|s| s.trim()).collect()
    };

    // Pseudo-instructions
    match mnemonic.as_str() {
        "nop" => return Ok(0x00000013), // addi x0, x0, 0
        "mv" => {
            // mv rd, rs → addi rd, rs, 0
            let rd = parse_reg(&operands, 0)?;
            let rs = parse_reg(&operands, 1)?;
            return Ok(i(0, rs, 0, rd, 0b0010011));
        }
        "li" => {
            // li rd, imm → addi rd, x0, imm (only for small immediates)
            let rd = parse_reg(&operands, 0)?;
            let imm = resolve_imm(&operands, 1, current_addr, labels)?;
            return Ok(i(imm, 0, 0, rd, 0b0010011));
        }
        "j" => {
            // j label → jal x0, label
            let imm = resolve_imm(&operands, 0, current_addr, labels)?;
            return Ok(j(imm, 0, 0b1101111));
        }
        "ret" => {
            // ret → jalr x0, x1, 0
            return Ok(i(0, 1, 0, 0, 0b1100111));
        }
        "call" => {
            // call label → jal x1, label
            let imm = resolve_imm(&operands, 0, current_addr, labels)?;
            return Ok(j(imm, 1, 0b1101111));
        }
        _ => {}
    }

    // R-type: op rd, rs1, rs2
    match mnemonic.as_str() {
        "add"  => return r_type(&operands, 0b0110011, 0b000, 0b0000000),
        "sub"  => return r_type(&operands, 0b0110011, 0b000, 0b0100000),
        "and"  => return r_type(&operands, 0b0110011, 0b111, 0b0000000),
        "or"   => return r_type(&operands, 0b0110011, 0b110, 0b0000000),
        "xor"  => return r_type(&operands, 0b0110011, 0b100, 0b0000000),
        "sll"  => return r_type(&operands, 0b0110011, 0b001, 0b0000000),
        "srl"  => return r_type(&operands, 0b0110011, 0b101, 0b0000000),
        "sra"  => return r_type(&operands, 0b0110011, 0b101, 0b0100000),
        "slt"  => return r_type(&operands, 0b0110011, 0b010, 0b0000000),
        "sltu" => return r_type(&operands, 0b0110011, 0b011, 0b0000000),
        // Zmmul (M-extension multiplies, no divide)
        "mul"    => return r_type(&operands, 0b0110011, 0b000, 0b0000001),
        "mulh"   => return r_type(&operands, 0b0110011, 0b001, 0b0000001),
        "mulhsu" => return r_type(&operands, 0b0110011, 0b010, 0b0000001),
        "mulhu"  => return r_type(&operands, 0b0110011, 0b011, 0b0000001),
        _ => {}
    }

    // I-type ALU: op rd, rs1, imm
    match mnemonic.as_str() {
        "addi"  => return i_type_alu(&operands, 0b0010011, 0b000, current_addr, labels),
        "andi"  => return i_type_alu(&operands, 0b0010011, 0b111, current_addr, labels),
        "ori"   => return i_type_alu(&operands, 0b0010011, 0b110, current_addr, labels),
        "xori"  => return i_type_alu(&operands, 0b0010011, 0b100, current_addr, labels),
        "slti"  => return i_type_alu(&operands, 0b0010011, 0b010, current_addr, labels),
        "sltiu" => return i_type_alu(&operands, 0b0010011, 0b011, current_addr, labels),
        _ => {}
    }

    // I-type shifts: op rd, rs1, shamt
    match mnemonic.as_str() {
        "slli" => {
            let rd = parse_reg(&operands, 0)?;
            let rs1 = parse_reg(&operands, 1)?;
            let shamt = resolve_imm(&operands, 2, current_addr, labels)? as u32;
            return Ok(shift(0b0000000, shamt, rs1, 0b001, rd, 0b0010011));
        }
        "srli" => {
            let rd = parse_reg(&operands, 0)?;
            let rs1 = parse_reg(&operands, 1)?;
            let shamt = resolve_imm(&operands, 2, current_addr, labels)? as u32;
            return Ok(shift(0b0000000, shamt, rs1, 0b101, rd, 0b0010011));
        }
        "srai" => {
            let rd = parse_reg(&operands, 0)?;
            let rs1 = parse_reg(&operands, 1)?;
            let shamt = resolve_imm(&operands, 2, current_addr, labels)? as u32;
            return Ok(shift(0b0100000, shamt, rs1, 0b101, rd, 0b0010011));
        }
        _ => {}
    }

    // Load: op rd, offset(rs1)
    match mnemonic.as_str() {
        "lb"  => return load_type(&operands, 0b0000011, 0b000),
        "lh"  => return load_type(&operands, 0b0000011, 0b001),
        "lw"  => return load_type(&operands, 0b0000011, 0b010),
        "lbu" => return load_type(&operands, 0b0000011, 0b100),
        "lhu" => return load_type(&operands, 0b0000011, 0b101),
        _ => {}
    }

    // Store: op rs2, offset(rs1)
    match mnemonic.as_str() {
        "sb" => return store_type(&operands, 0b000),
        "sh" => return store_type(&operands, 0b001),
        "sw" => return store_type(&operands, 0b010),
        _ => {}
    }

    // Branch pseudo-instructions: one real branch each (x0 filled in or
    // operands swapped), so they are one word and never shift labels.
    let pseudo_branch = |f3: u32, rs1: &str, rs2: &str, n: usize| -> Result<u32, String> {
        if operands.len() != n {
            return Err(format!("{} expects {} operands, got {}", mnemonic, n, operands.len()));
        }
        let target = operands[n - 1];
        branch_type(&[rs1, rs2, target], f3, current_addr, labels)
    };
    let op = |i: usize| operands.get(i).copied().unwrap_or("");
    match mnemonic.as_str() {
        "beqz" => return pseudo_branch(0b000, op(0), "x0", 2),
        "bnez" => return pseudo_branch(0b001, op(0), "x0", 2),
        "bltz" => return pseudo_branch(0b100, op(0), "x0", 2),
        "bgez" => return pseudo_branch(0b101, op(0), "x0", 2),
        "bgtz" => return pseudo_branch(0b100, "x0", op(0), 2),
        "blez" => return pseudo_branch(0b101, "x0", op(0), 2),
        "bgt"  => return pseudo_branch(0b100, op(1), op(0), 3),
        "ble"  => return pseudo_branch(0b101, op(1), op(0), 3),
        "bgtu" => return pseudo_branch(0b110, op(1), op(0), 3),
        "bleu" => return pseudo_branch(0b111, op(1), op(0), 3),
        _ => {}
    }

    // Branch: op rs1, rs2, label/offset
    match mnemonic.as_str() {
        "beq"  => return branch_type(&operands, 0b000, current_addr, labels),
        "bne"  => return branch_type(&operands, 0b001, current_addr, labels),
        "blt"  => return branch_type(&operands, 0b100, current_addr, labels),
        "bge"  => return branch_type(&operands, 0b101, current_addr, labels),
        "bltu" => return branch_type(&operands, 0b110, current_addr, labels),
        "bgeu" => return branch_type(&operands, 0b111, current_addr, labels),
        _ => {}
    }

    // U-type
    match mnemonic.as_str() {
        "lui" => {
            let rd = parse_reg(&operands, 0)?;
            let imm = resolve_imm(&operands, 1, current_addr, labels)?;
            // LUI takes the upper 20 bits: imm is the page number, shifted left by 12
            return Ok(u(imm as u32, rd, 0b0110111));
        }
        "auipc" => {
            let rd = parse_reg(&operands, 0)?;
            let imm = resolve_imm(&operands, 1, current_addr, labels)?;
            return Ok(u(imm as u32, rd, 0b0010111));
        }
        _ => {}
    }

    // J-type
    match mnemonic.as_str() {
        "jal" => {
            if operands.len() == 1 {
                // jal label → jal x1, label
                let imm = resolve_imm(&operands, 0, current_addr, labels)?;
                return Ok(j(imm, 1, 0b1101111));
            }
            let rd = parse_reg(&operands, 0)?;
            let imm = resolve_imm(&operands, 1, current_addr, labels)?;
            return Ok(j(imm, rd, 0b1101111));
        }
        _ => {}
    }

    // JALR: jalr rd, rs1, imm  OR  jalr rd, offset(rs1)
    if mnemonic == "jalr" {
        if operands.len() == 3 {
            let rd = parse_reg(&operands, 0)?;
            let rs1 = parse_reg(&operands, 1)?;
            let imm = resolve_imm(&operands, 2, current_addr, labels)?;
            return Ok(i(imm, rs1, 0b000, rd, 0b1100111));
        } else if operands.len() == 2 {
            let rd = parse_reg(&operands, 0)?;
            let (offset, rs1) = parse_offset_reg(operands[1])?;
            return Ok(i(offset, rs1, 0b000, rd, 0b1100111));
        }
        return Err("jalr requires 2 or 3 operands".into());
    }

    // CSR instructions: csrXX rd, csr, rs1  OR  csrXXi rd, csr, uimm
    match mnemonic.as_str() {
        "csrrw" | "csrrs" | "csrrc" | "csrrwi" | "csrrsi" | "csrrci" => {
            let rd = parse_reg(&operands, 0)?;
            let csr_addr = resolve_csr(&operands, 1)?;
            let is_imm = mnemonic.ends_with('i');
            let funct3 = match mnemonic.as_str() {
                "csrrw"  => 0b001,
                "csrrs"  => 0b010,
                "csrrc"  => 0b011,
                "csrrwi" => 0b101,
                "csrrsi" => 0b110,
                "csrrci" => 0b111,
                _ => unreachable!(),
            };
            let rs1_or_uimm = if is_imm {
                resolve_imm(&operands, 2, current_addr, labels)? as u32 & 0x1F
            } else {
                parse_reg(&operands, 2)?
            };
            // CSR encoding: I-type with csr addr in imm[11:0], rs1/uimm in rs1 field
            return Ok(i(csr_addr as i32, rs1_or_uimm, funct3, rd, 0b1110011));
        }
        _ => {}
    }

    Err(format!("unknown instruction '{}'", mnemonic))
}

/// Resolve CSR name or numeric address
fn resolve_csr(operands: &[&str], idx: usize) -> Result<u32, String> {
    let s = operands.get(idx).ok_or("missing CSR operand")?.trim();
    // Try known CSR names
    match s.to_lowercase().as_str() {
        "cycle"     => return Ok(0xC00),
        "time"      => return Ok(0xC01),
        "instret"   => return Ok(0xC02),
        "mstatus"   => return Ok(0x300),
        "mtvec"     => return Ok(0x305),
        "mepc"      => return Ok(0x341),
        "mcause"    => return Ok(0x342),
        "mvendorid" => return Ok(0xF11),
        "mimpid"    => return Ok(0xF13),
        _ => {}
    }
    // Try as numeric
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return u32::from_str_radix(hex, 16).map_err(|_| format!("invalid CSR address '{}'", s));
    }
    s.parse::<u32>().map_err(|_| format!("unknown CSR '{}'", s))
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn parse_reg(operands: &[&str], idx: usize) -> Result<u32, String> {
    let name = operands.get(idx).ok_or("missing register operand")?;
    reg_number(name).ok_or_else(|| format!("unknown register '{}'", name))
}

fn resolve_imm(
    operands: &[&str],
    idx: usize,
    current_addr: u32,
    labels: &HashMap<String, u32>,
) -> Result<i32, String> {
    let s = operands.get(idx).ok_or("missing immediate operand")?.trim();
    // Try as label first
    if let Some(&target) = labels.get(s) {
        return Ok(target as i32 - current_addr as i32);
    }
    parse_imm(s)
}

fn r_type(operands: &[&str], opcode: u32, funct3: u32, funct7: u32) -> Result<u32, String> {
    let rd = parse_reg(operands, 0)?;
    let rs1 = parse_reg(operands, 1)?;
    let rs2 = parse_reg(operands, 2)?;
    Ok(r(funct7, rs2, rs1, funct3, rd, opcode))
}

fn i_type_alu(
    operands: &[&str],
    opcode: u32,
    funct3: u32,
    current_addr: u32,
    labels: &HashMap<String, u32>,
) -> Result<u32, String> {
    let rd = parse_reg(operands, 0)?;
    let rs1 = parse_reg(operands, 1)?;
    let imm = resolve_imm(operands, 2, current_addr, labels)?;
    Ok(i(imm, rs1, funct3, rd, opcode))
}

fn load_type(operands: &[&str], opcode: u32, funct3: u32) -> Result<u32, String> {
    let rd = parse_reg(operands, 0)?;
    let (offset, rs1) = parse_offset_reg(operands.get(1).ok_or("missing offset(reg)")?)?;
    Ok(i(offset, rs1, funct3, rd, opcode))
}

fn store_type(operands: &[&str], funct3: u32) -> Result<u32, String> {
    let rs2 = parse_reg(operands, 0)?;
    let (offset, rs1) = parse_offset_reg(operands.get(1).ok_or("missing offset(reg)")?)?;
    Ok(s(offset, rs2, rs1, funct3))
}

fn branch_type(
    operands: &[&str],
    funct3: u32,
    current_addr: u32,
    labels: &HashMap<String, u32>,
) -> Result<u32, String> {
    let rs1 = parse_reg(operands, 0)?;
    let rs2 = parse_reg(operands, 1)?;
    let imm = resolve_imm(operands, 2, current_addr, labels)?;
    Ok(b(imm, rs1, rs2, funct3))
}

#[cfg(test)]
mod zmmul_tests {
    use super::*;

    #[test]
    fn assembles_zmmul() {
        let out = assemble("mul x10, x5, x6\nmulh x11, x5, x6\nmulhsu x12, x5, x6\nmulhu x13, x5, x6\n");
        let words: Vec<u32> = out.instructions.iter().map(|i| i.value).collect();
        assert_eq!(words, vec![0x0262_8533, 0x0262_95b3, 0x0262_a633, 0x0262_b6b3]);
    }
}

#[cfg(test)]
mod branch_pseudo_tests {
    use super::*;

    fn words(src: &str) -> Vec<u32> {
        let out = assemble(src);
        assert!(out.success, "{:?}", out.errors.iter().map(|e| &e.message).collect::<Vec<_>>());
        out.instructions.iter().map(|i| i.value).collect()
    }

    #[test]
    fn each_pseudo_matches_its_real_branch() {
        let pairs = [
            ("beqz x5, 8", "beq x5, x0, 8"),     ("bnez x5, 8", "bne x5, x0, 8"),
            ("bltz x5, 8", "blt x5, x0, 8"),     ("bgez x5, 8", "bge x5, x0, 8"),
            ("bgtz x5, 8", "blt x0, x5, 8"),     ("blez x5, 8", "bge x0, x5, 8"),
            ("bgt x5, x6, 8", "blt x6, x5, 8"),  ("ble x5, x6, 8", "bge x6, x5, 8"),
            ("bgtu x5, x6, 8", "bltu x6, x5, 8"), ("bleu x5, x6, 8", "bgeu x6, x5, 8"),
        ];
        for (p, r) in pairs {
            assert_eq!(words(p), words(r), "{p} vs {r}");
        }
    }

    #[test]
    fn labels_forward_and_backward() {
        let src = "top:\nbnez x5, done\naddi x5, x5, -1\nbgtz x5, top\nble x5, x6, top\ndone:\nnop\n";
        let real = "top:\nbne x5, x0, done\naddi x5, x5, -1\nblt x0, x5, top\nbge x6, x5, top\ndone:\nnop\n";
        assert_eq!(words(src), words(real));
        assert_eq!(words(src).len(), 5);
    }

    #[test]
    fn wrong_operand_count_is_an_error() {
        for src in ["beqz x5, x6, 8", "bgt x5, 8", "bnez"] {
            let out = assemble(src);
            assert!(!out.success, "{src} should fail");
            assert!(out.errors[0].message.contains("expects"), "{src}: {}", out.errors[0].message);
        }
    }
}
