//! External memory bus arbiter.
//!
//! Single bus, fixed-priority arbitration. Each cycle the bus is
//! either Idle or Busy. When Idle, the arbiter scans for work in
//! priority order:
//!
//!   1. D-L1 cache miss (read fill)        — pipeline can't make
//!                                            progress without it
//!   2. I-L1 cache miss (read fill)
//!   3. Store buffer drain (write to ext)
//!
//! A line fill takes `MISS_LATENCY` cycles. A store drain takes
//! 1 cycle. While the bus is busy, no new bus operation can start.

pub const MISS_LATENCY: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusState {
    Idle,
    FillingI {
        line_addr: u32,
        thread: u8,
        cycles_remaining: u32,
    },
    FillingD {
        line_addr: u32,
        thread: u8,
        cycles_remaining: u32,
    },
    Storing {
        // The store entry was already popped from the buffer; the
        // bus is just consuming a cycle to "write" it. We carry
        // the data here so the tick() can apply it on completion.
        addr: u32,
        value: u32,
    },
}

#[derive(Debug, Clone)]
pub struct Bus {
    pub state: BusState,
}

impl Bus {
    pub fn new() -> Self { Self { state: BusState::Idle } }
    pub fn reset(&mut self) { self.state = BusState::Idle; }
    pub fn is_idle(&self) -> bool { matches!(self.state, BusState::Idle) }
}

impl Default for Bus { fn default() -> Self { Self::new() } }
