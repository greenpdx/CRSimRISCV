//! Compatibility path: the trap / CSR code now lives in [`crate::sim::system`].

pub use crate::sim::system::csr::*;
pub use crate::sim::system::irq::*;
pub use crate::sim::system::trap::*;
