//! Mirrors `src/components/DecoderControl.vue`.
//! Props: instruction, opcode, active, asm_text, phase_text.
//! Computed: entry = OPCODE_BY_VALUE.get(opcode); signals = entry.signals.

use crate::data::control_data::{opcode_by_value, ControlSignals, OpcodeEntry};

#[derive(Debug, Clone, Default)]
pub struct DecoderControl {
    pub instruction: u32,
    pub opcode: u8,
    pub active: bool,
}

impl DecoderControl {
    pub fn entry(&self) -> Option<&'static OpcodeEntry> { opcode_by_value(self.opcode) }
    pub fn signals(&self) -> Option<ControlSignals> { self.entry().map(|e| e.signals) }
    pub fn name(&self) -> Option<&'static str> { self.entry().map(|e| e.name) }
}
