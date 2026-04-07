//! MISC-MEM opcode `0001111` — FENCE, FENCE.I (Zifencei), PAUSE.

use super::Disasm;

const FLAG_LETTERS: [(u32, &str); 4] = [
    (0b1000, "i"), (0b0100, "o"), (0b0010, "r"), (0b0001, "w"),
];

fn decode_set(bits: u32) -> String {
    let mut s = String::new();
    for (mask, letter) in FLAG_LETTERS {
        if bits & mask != 0 { s.push_str(letter); }
    }
    if s.is_empty() { s.push_str("none"); }
    s
}

pub fn decode(instr: u32) -> Option<Disasm> {
    if instr & 0x7f != 0b0001111 { return None; }
    let funct3 = (instr >> 12) & 0x07;

    if funct3 == 0b001 {
        return Some(Disasm {
            class: "Zifencei", mnemonic: "FENCE.I".into(), operands: String::new(),
        });
    }
    if funct3 != 0 { return None; }

    let fm   = (instr >> 28) & 0xf;
    let pred = (instr >> 24) & 0xf;
    let succ = (instr >> 20) & 0xf;
    if fm == 0b0001 && pred == 0 && succ == 0 {
        return Some(Disasm {
            class: "Zihintpause", mnemonic: "PAUSE".into(), operands: String::new(),
        });
    }

    Some(Disasm {
        class: "I", mnemonic: "FENCE".into(),
        operands: format!("{}, {}", decode_set(pred), decode_set(succ)),
    })
}
