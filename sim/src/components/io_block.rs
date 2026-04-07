//! Mirrors `src/components/IoBlock.vue`.
//! Props: base_address, address, data, op.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IoOp { #[default] Idle, Read, Write }

#[derive(Debug, Clone, Copy, Default)]
pub struct IoBlock {
    pub base_address: u32,
    pub address: u32,
    pub data: u32,
    pub op: IoOp,
}

impl IoBlock {
    pub fn get_bit(&self, bit: u8) -> bool { (self.data >> (bit & 0x1f)) & 1 == 1 }

    pub fn toggle_bit(&mut self, bit: u8) { self.data ^= 1 << (bit & 0x1f); }

    pub fn set_bit(&mut self, bit: u8, val: bool) {
        let mask = 1u32 << (bit & 0x1f);
        if val { self.data |= mask } else { self.data &= !mask }
    }

    pub fn matches(&self, addr: u32) -> bool { addr >= self.base_address }
}
