//! CSR address → standard name table.
//! Covers the user/supervisor/machine CSRs the simulator currently models
//! plus the common ones a student is likely to encounter.

pub fn csr_name(addr: u16) -> Option<&'static str> {
    Some(match addr {
        // ── User counters ─────────────────────────────────────────────
        0xC00 => "cycle",       0xC01 => "time",       0xC02 => "instret",
        0xC80 => "cycleh",      0xC81 => "timeh",      0xC82 => "instreth",
        // ── Supervisor ────────────────────────────────────────────────
        0x100 => "sstatus",     0x104 => "sie",        0x105 => "stvec",
        0x140 => "sscratch",    0x141 => "sepc",       0x142 => "scause",
        0x143 => "stval",       0x144 => "sip",        0x180 => "satp",
        // ── Machine information ───────────────────────────────────────
        0xF11 => "mvendorid",   0xF12 => "marchid",    0xF13 => "mimpid",
        0xF14 => "mhartid",     0xF15 => "mconfigptr",
        // ── Machine trap setup ────────────────────────────────────────
        0x300 => "mstatus",     0x301 => "misa",       0x302 => "medeleg",
        0x303 => "mideleg",     0x304 => "mie",        0x305 => "mtvec",
        0x306 => "mcounteren",  0x310 => "mstatush",
        // ── Machine trap handling ─────────────────────────────────────
        0x340 => "mscratch",    0x341 => "mepc",       0x342 => "mcause",
        0x343 => "mtval",       0x344 => "mip",        0x34A => "mtinst",
        0x34B => "mtval2",
        // ── Floating-point ────────────────────────────────────────────
        0x001 => "fflags",      0x002 => "frm",        0x003 => "fcsr",
        // ── Vector ────────────────────────────────────────────────────
        0x008 => "vstart",      0x009 => "vxsat",      0x00A => "vxrm",
        0x00F => "vcsr",        0xC20 => "vl",         0xC21 => "vtype",
        0xC22 => "vlenb",
        // ── Hypervisor — H extension ──────────────────────────────────
        0x600 => "hstatus",     0x602 => "hedeleg",    0x603 => "hideleg",
        0x604 => "hie",         0x606 => "hcounteren", 0x607 => "hgeie",
        0x643 => "htval",       0x644 => "hip",        0x645 => "hvip",
        0x64A => "htinst",      0x680 => "hgatp",      0xE12 => "hgeip",
        // ── Hypervisor — VS (virtual supervisor) ──────────────────────
        0x200 => "vsstatus",    0x204 => "vsie",       0x205 => "vstvec",
        0x240 => "vsscratch",   0x241 => "vsepc",      0x242 => "vscause",
        0x243 => "vstval",      0x244 => "vsip",       0x280 => "vsatp",
        // ── Zkr — entropy source ──────────────────────────────────────
        0x015 => "seed",
        _ => return None,
    })
}

/// Plain-English description for a CSR (used as tooltip in the demo).
pub fn csr_desc(addr: u16) -> Option<&'static str> {
    Some(match addr {
        // User counters
        0xC00 => "User cycle counter (RO). Increments every CPU cycle.",
        0xC01 => "User wall-clock time (RO). Aliased to cycle here.",
        0xC02 => "User instructions-retired counter (RO).",
        0xC80 => "Upper 32 bits of cycle (RV32 only).",
        0xC81 => "Upper 32 bits of time (RV32 only).",
        0xC82 => "Upper 32 bits of instret (RV32 only).",
        // Supervisor
        0x100 => "S-mode status register. Subset view of mstatus: SIE, SPIE, SPP.",
        0x104 => "S-mode interrupt-enable bits (subset of mie: SSI, STI, SEI).",
        0x105 => "S-mode trap-vector base address. Low 2 bits select direct/vectored mode.",
        0x140 => "S-mode scratch register for the trap handler to spill regs to.",
        0x141 => "S-mode exception PC. Set on S-trap to the PC of the trapping instruction.",
        0x142 => "S-mode trap cause. High bit = interrupt, low bits = cause code.",
        0x143 => "S-mode trap value. Faulting address or instruction bits.",
        0x144 => "S-mode interrupt-pending bits (subset of mip).",
        0x180 => "Supervisor address translation and protection. Holds the page-table root.",
        // Machine information
        0xF11 => "Machine vendor ID (RO). Reads 0 for non-commercial.",
        0xF12 => "Machine architecture ID (RO).",
        0xF13 => "Machine implementation ID (RO).",
        0xF14 => "Machine hart ID (RO). Identifies which hardware thread is running.",
        0xF15 => "Machine config pointer (RO). Points to a platform configuration blob.",
        // Machine trap setup
        0x300 => "Machine status register. Holds global MIE/SIE, prior modes (MPP/SPP), prior interrupt-enables (MPIE/SPIE).",
        0x301 => "ISA and extensions register. Reports which extensions are implemented.",
        0x302 => "Machine exception delegation. Each bit set = exception with that cause is delegated to S-mode.",
        0x303 => "Machine interrupt delegation. Each bit set = interrupt is delegated to S-mode.",
        0x304 => "Machine interrupt-enable bits. MIE bit 3, MTI bit 7, MEI bit 11, etc.",
        0x305 => "Machine trap-vector base address. Low 2 bits: 0=direct, 1=vectored.",
        0x306 => "Machine counter-enable. Controls which counters S/U-mode can read.",
        0x310 => "Upper 32 bits of mstatus (RV32 only).",
        // Machine trap handling
        0x340 => "Machine scratch register. Trap handler uses this to bootstrap a register save.",
        0x341 => "Machine exception PC. Set on trap to the PC of the trapping instruction.",
        0x342 => "Machine trap cause. High bit = interrupt, low bits = cause code (see RISC-V Vol II, Table 8.7).",
        0x343 => "Machine trap value. Holds the faulting address (page faults, misaligned) or the bad instruction.",
        0x344 => "Machine interrupt-pending bits. External, timer, and software interrupts at all 3 modes.",
        0x34A => "Machine trap instruction (transformed). H extension only.",
        0x34B => "Machine bad guest physical address. H extension.",
        // FP
        0x001 => "FP accrued exception flags: NV (invalid), DZ (div by 0), OF, UF, NX.",
        0x002 => "FP rounding mode: RNE/RTZ/RDN/RUP/RMM.",
        0x003 => "Combined FP control/status (frm + fflags).",
        // Vector
        0x008 => "Vector start index for partial execution after a trap.",
        0x009 => "Vector fixed-point saturation flag.",
        0x00A => "Vector fixed-point rounding mode.",
        0x00F => "Combined vector control/status.",
        0xC20 => "Current vector length in elements (RO).",
        0xC21 => "Current vector type: SEW, LMUL, ta, ma (RO).",
        0xC22 => "Vector register length in bytes (RO).",
        // Hypervisor
        0x600 => "Hypervisor status register. Tracks the active virtual context.",
        0x602 => "Hypervisor exception delegation to virtual S-mode.",
        0x603 => "Hypervisor interrupt delegation to virtual S-mode.",
        0x604 => "Hypervisor interrupt-enable bits.",
        0x606 => "Hypervisor counter-enable for virtual S-mode.",
        0x607 => "Hypervisor guest external-interrupt enable.",
        0x643 => "Hypervisor trap value.",
        0x644 => "Hypervisor interrupt-pending bits.",
        0x645 => "Hypervisor virtual interrupt-pending.",
        0x64A => "Hypervisor trap instruction.",
        0x680 => "Hypervisor guest address translation and protection.",
        0xE12 => "Hypervisor guest external interrupts pending (RO).",
        // VS (virtual supervisor)
        0x200 => "Virtual S-mode status register.",
        0x204 => "Virtual S-mode interrupt-enable bits.",
        0x205 => "Virtual S-mode trap-vector base.",
        0x240 => "Virtual S-mode scratch.",
        0x241 => "Virtual S-mode exception PC.",
        0x242 => "Virtual S-mode trap cause.",
        0x243 => "Virtual S-mode trap value.",
        0x244 => "Virtual S-mode interrupt-pending.",
        0x280 => "Virtual S-mode address translation and protection.",
        // Zkr
        0x015 => "Entropy source. Reads return random bits with status, writes are ignored.",
        _ => return None,
    })
}
