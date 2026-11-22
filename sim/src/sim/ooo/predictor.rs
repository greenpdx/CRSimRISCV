//! Branch prediction at fetch: a 2-bit counter table (BHT) for the
//! direction of conditional branches, and a branch target buffer (BTB)
//! for where taken branches and jumps go.

pub const BHT_SIZE: usize = 64;
pub const BTB_SIZE: usize = 16;

#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]

#[cfg_attr(feature = "wasm", tsify(missing_as_null))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BtbEntry {
    pub valid: bool,
    /// Full PC of the branch (the tag)
    pub pc: u32,
    pub target: u32,
    /// JAL / JALR: always taken; else a conditional branch (ask the BHT)
    pub jump: bool,
}

/// What fetch predicted for one instruction.
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(missing_as_null))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Prediction {
    pub taken: bool,
    /// Next PC fetch went to (target if taken, else PC + 4)
    pub next: u32,
    pub btb_hit: bool,
    /// BHT counter read (0..3)
    pub counter: u8,
}

#[derive(Debug, Clone)]
pub struct Predictor {
    /// 2-bit saturating counters: 0, 1 → not taken; 2, 3 → taken
    pub bht: [u8; BHT_SIZE],
    pub btb: [BtbEntry; BTB_SIZE],
}

impl Default for Predictor {
    fn default() -> Self { Self { bht: [1; BHT_SIZE], btb: [BtbEntry::default(); BTB_SIZE] } }
}

pub fn bht_index(pc: u32) -> usize { ((pc >> 2) as usize) % BHT_SIZE }
pub fn btb_index(pc: u32) -> usize { ((pc >> 2) as usize) % BTB_SIZE }

impl Predictor {
    pub fn predict(&self, pc: u32) -> Prediction {
        let e = self.btb[btb_index(pc)];
        let counter = self.bht[bht_index(pc)];
        let btb_hit = e.valid && e.pc == pc;
        let taken = btb_hit && (e.jump || counter >= 2);
        Prediction { taken, next: if taken { e.target } else { pc.wrapping_add(4) }, btb_hit, counter }
    }

    /// A branch resolved: train the counter (conditional only) and remember taken targets.
    pub fn update(&mut self, pc: u32, jump: bool, taken: bool, target: u32) {
        if !jump {
            let c = &mut self.bht[bht_index(pc)];
            *c = if taken { (*c + 1).min(3) } else { c.saturating_sub(1) };
        }
        if taken {
            self.btb[btb_index(pc)] = BtbEntry { valid: true, pc, target, jump };
        }
    }
}
