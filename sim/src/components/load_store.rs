//! Mirrors `src/components/LoadStore.vue`.
//! Props: op, address, width, load_sign, write_data, read_data.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoadStoreOp { #[default] Idle, Read, Write }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AccessWidth { #[default] Byte, Half, Word }

#[derive(Debug, Clone, Copy, Default)]
pub struct LoadStore {
    pub op: LoadStoreOp,
    pub address: u32,
    pub width: AccessWidth,
    pub load_signed: bool,
    pub write_data: u32,
    pub read_data: u32,
}

impl LoadStore {
    pub fn op_label(&self) -> &'static str {
        match self.op {
            LoadStoreOp::Idle  => "idle",
            LoadStoreOp::Read  => "read",
            LoadStoreOp::Write => "write",
        }
    }

    pub fn width_bytes(&self) -> u32 {
        match self.width { AccessWidth::Byte => 1, AccessWidth::Half => 2, AccessWidth::Word => 4 }
    }

    /// Width-aware load with sign/zero extension. Mirrors LB/LH/LW/LBU/LHU semantics.
    pub fn extend_loaded(&self, raw_word: u32) -> u32 {
        let byte_off = (self.address & 0x3) * 8;
        match self.width {
            AccessWidth::Byte => {
                let v = (raw_word >> byte_off) & 0xff;
                if self.load_signed && v & 0x80 != 0 { v | 0xffff_ff00 } else { v }
            }
            AccessWidth::Half => {
                let v = (raw_word >> byte_off) & 0xffff;
                if self.load_signed && v & 0x8000 != 0 { v | 0xffff_0000 } else { v }
            }
            AccessWidth::Word => raw_word,
        }
    }
}
