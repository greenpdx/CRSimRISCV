//! Mirrors `src/components/MuxUnit.vue` (generic N-input mux).
//! Props: label, inputs, select, active.

#[derive(Debug, Clone, Default)]
pub struct MuxUnit {
    pub label: &'static str,
    pub inputs: Vec<u32>,
    pub select: u8,
    pub active: bool,
}

impl MuxUnit {
    pub fn output(&self) -> u32 {
        self.inputs.get(self.select as usize).copied().unwrap_or(0)
    }
}
