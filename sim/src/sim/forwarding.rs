//! Shared forwarding source enum used by both the 5-stage pipeline
//! and the barrel processor's EX stage.

/// Where an ALU input operand comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ForwardSrc {
    #[default]
    Regfile,
    ExMa,
    MaWb,
}
