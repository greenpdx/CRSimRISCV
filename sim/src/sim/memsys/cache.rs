//! The memory panel's caches: the shared cache array with MESI line states.

pub use crate::sim::mem::cache::{LINE_BYTES, LINE_WORDS};
use crate::sim::mem::cache::LineState;

/// Line state. L1D uses full MESI. L1I lines are only I / S. L2 uses
/// I (invalid), E (clean) and M (dirty, newer than memory).
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(missing_as_null))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mesi { #[default] I, S, E, M }

impl LineState for Mesi {
    fn valid(&self) -> bool { *self != Mesi::I }
}

/// One set-associative cache of the hierarchy.
pub type Cache = crate::sim::mem::cache::Cache<Mesi>;
/// One line: MESI state, tag, LRU age, 4 words.
pub type Line = crate::sim::mem::cache::Line<Mesi>;
