//! Interrupt sources and priority (mip & mie, global enables, delegation).

use super::csr::{addr, CsrFile, PrivMode, M_MSTATUS_MIE, M_MSTATUS_SIE};

// ── Interrupt-pending / enable bit layout (mip/mie) ────────────────────────
// User-Software   bit 0  (USI — not modeled, bit reserved)
// Supervisor-Soft bit 1  (SSI)
// Machine-Soft    bit 3  (MSI)
// Supervisor-Tmr  bit 5  (STI)
// Machine-Tmr     bit 7  (MTI)
// Supervisor-Ext  bit 9  (SEI)
// Machine-Ext     bit 11 (MEI)
pub const IRQ_SSI: u32 = 1 << 1;
pub const IRQ_MSI: u32 = 1 << 3;
pub const IRQ_STI: u32 = 1 << 5;
pub const IRQ_MTI: u32 = 1 << 7;
pub const IRQ_SEI: u32 = 1 << 9;
pub const IRQ_MEI: u32 = 1 << 11;

// Asynchronous interrupt cause codes (mcause with the high bit set)
pub const INT_SSI: u32 = 1;
pub const INT_MSI: u32 = 3;
pub const INT_STI: u32 = 5;
pub const INT_MTI: u32 = 7;
pub const INT_SEI: u32 = 9;
pub const INT_MEI: u32 = 11;

/// Is interrupt `bit` enabled and pending given the current state?
/// Returns the effective destination mode (M or S) per delegation rules.
pub fn interrupt_destination(
    csrs: &CsrFile,
    cur_mode: PrivMode,
    pending_irq: u32,
) -> Option<(u32, PrivMode)> {
    let mip = csrs.read(addr::MIP) | pending_irq;
    let mie = csrs.read(addr::MIE);
    let mideleg = csrs.read(addr::MIDELEG);
    let mstatus = csrs.read(addr::MSTATUS);

    // Priority order: MEI, MSI, MTI, SEI, SSI, STI (RISC-V spec)
    let priority = [
        (IRQ_MEI, INT_MEI), (IRQ_MSI, INT_MSI), (IRQ_MTI, INT_MTI),
        (IRQ_SEI, INT_SEI), (IRQ_SSI, INT_SSI), (IRQ_STI, INT_STI),
    ];
    for (mask, cause) in priority {
        if (mip & mie & mask) == 0 { continue; }
        // Determine destination mode via delegation
        let delegated = (mideleg & mask) != 0;
        let dest = if delegated { PrivMode::S } else { PrivMode::M };
        // Check global enable for the destination mode
        let enabled = match dest {
            PrivMode::M => match cur_mode {
                PrivMode::M => (mstatus & M_MSTATUS_MIE) != 0,
                _ => true, // higher-priv interrupts always enabled when in lower priv
            }
            PrivMode::S => match cur_mode {
                PrivMode::S => (mstatus & M_MSTATUS_SIE) != 0,
                PrivMode::U => true,
                PrivMode::M => false, // M can't take S-delegated trap
            }
            PrivMode::U => false,
        };
        if enabled { return Some((cause, dest)); }
    }
    None
}

