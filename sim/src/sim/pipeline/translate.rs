//! Sv32 address translation: ITLB / DTLB lookup, then on a miss the
//! two-level page-table walk (CVA6-style PTW), A / D bit updates and the
//! TLB fill. M-mode and satp.MODE = Bare bypass translation.
//!
//! In memory-panel mode every step is reported as an `Event` (lookup with
//! each entry's comparators, PTE address, PTE check, permission check,
//! A / D update, victim choice, fill) so the panel can show the phases of
//! what the hardware does in one access.

use super::state::PipelineState;
use crate::sim::system::addr;
use crate::sim::memsys::{Event, Port, PteVerdict};
use super::tlb::Tlb;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessKind { Fetch, Load, Store }

impl PipelineState {
    /// Fetches use the ITLB, loads and stores the DTLB.
    pub fn tlb_for(&mut self, access: AccessKind) -> &mut Tlb {
        if access == AccessKind::Fetch { &mut self.itlb } else { &mut self.dtlb }
    }
}

const PTE_V: u32 = 1 << 0;
const PTE_R: u32 = 1 << 1;
const PTE_W: u32 = 1 << 2;
const PTE_X: u32 = 1 << 3;
const PTE_U: u32 = 1 << 4;
const PTE_G: u32 = 1 << 5;
const PTE_A: u32 = 1 << 6;
const PTE_D: u32 = 1 << 7;

const MSTATUS_SUM: u32 = 1 << 18;
const MSTATUS_MXR: u32 = 1 << 19;

/// The ASID field of satp (Sv32: bits 30:22).
pub fn satp_asid(satp: u32) -> u16 { ((satp >> 22) & 0x1ff) as u16 }

/// Translate a virtual address. Returns `Ok(phys)` or `Err(cause)`
/// where cause is 12 (instr page fault), 13 (load page fault), or
/// 15 (store page fault).
///
/// On success, hardware-manages A/D bits: sets A on every access,
/// sets D on stores. The updated PTE is written back to RAM.
pub fn translate(state: &mut PipelineState, vaddr: u32, access: AccessKind) -> Result<u32, u32> {
    if state.hier.is_none() { return translate_inner(state, vaddr, access); }
    state.set_xlat_port(match access {
        AccessKind::Fetch => Port::Fetch, AccessKind::Load => Port::Load, AccessKind::Store => Port::Store,
    });
    let r = translate_inner(state, vaddr, access);
    if let Err(cause) = r {
        state.mem_event(|core, port| Event::PageFault { core, port, va: vaddr, cause });
    }
    r
}

fn translate_inner(state: &mut PipelineState, vaddr: u32, access: AccessKind) -> Result<u32, u32> {
    // Bypass translation in M-mode or when satp.MODE == 0 (Bare).
    let satp = state.sys.csrs.read(addr::SATP);
    if state.priv_bits() == 3 || (satp >> 31) == 0 {
        state.mem_event(|core, port| Event::Bare { core, port, va: vaddr });
        return Ok(vaddr);
    }

    let fault_cause = match access {
        AccessKind::Fetch => 12,
        AccessKind::Load  => 13,
        AccessKind::Store => 15,
    };
    let mstatus = state.sys.csrs.read(addr::MSTATUS);
    let priv_mode = state.priv_bits();
    let asid = satp_asid(satp);
    let vpn = vaddr >> 12;
    let off = vaddr & 0xfff;

    // ── TLB lookup: every entry compared at once ─────────────────────
    let tlb = state.tlb_for(access);
    let plru_before = tlb.plru;
    let (hit, cmp) = tlb.lookup(vpn, asid);
    let plru_after = tlb.plru;
    state.mem_event(|core, port| Event::TlbLookup { core, port, va: vaddr, asid, cmp, hit: hit.map(|i| i as u8), plru_before, plru_after });

    if let Some(idx) = hit {
        let entry = state.tlb_for(access).entries[idx];
        if !perm(state, entry.pte, access, priv_mode, mstatus) { return Err(fault_cause); }
        update_ad_for_tlb_entry(state, idx, vpn, access);
        let pa = if entry.is_super {
            ((entry.ppn >> 10) << 22) | (vaddr & 0x003f_ffff)
        } else {
            (entry.ppn << 12) | off
        };
        let idx = idx as u8;
        state.mem_event(|core, port| Event::TlbHit { core, port, idx, va: vaddr, pa });
        return Ok(pa);
    }
    state.mem_event(|core, port| Event::TlbMiss { core, port, va: vaddr });

    // ── Page-table walk ──────────────────────────────────────────────
    let root = (satp & 0x003f_ffff) << 12;
    let vpn1 = (vaddr >> 22) & 0x3ff;
    let vpn0 = (vaddr >> 12) & 0x3ff;

    // Level 1 (root table)
    let pte1_addr = root.wrapping_add(vpn1 * 4);
    state.mem_event(|core, port| Event::PteAddr { core, port, level: 1, base: root, vpn: vpn1, addr: pte1_addr });
    let pte1 = state.pte_read(pte1_addr, 1);
    if let Some(bad) = invalid(pte1) { verdict(state, 1, pte1, bad); return Err(fault_cause); }

    if is_leaf(pte1) {
        let ppn0 = (pte1 >> 10) & 0x3ff;
        if ppn0 != 0 { verdict(state, 1, pte1, PteVerdict::Misaligned); return Err(fault_cause); }
        verdict(state, 1, pte1, PteVerdict::Leaf);
        if !perm(state, pte1, access, priv_mode, mstatus) { return Err(fault_cause); }
        let pte1 = update_ad(state, pte1_addr, pte1, access);
        let ppn1 = (pte1 >> 20) & 0xfff;
        // Superpage entry: ppn stored as full-PPN form (ppn1 in the upper 10 bits).
        let pa = (ppn1 << 22) | (vaddr & 0x003f_ffff);
        fill(state, access, vaddr, pa, vpn, ppn1 << 10, pte1, true, asid, pte1 & PTE_G != 0);
        return Ok(pa);
    }
    verdict(state, 1, pte1, PteVerdict::Pointer);

    // Level 0 (leaf table)
    let base0 = ((pte1 >> 10) & 0x003f_ffff) << 12;
    let pte0_addr = base0.wrapping_add(vpn0 * 4);
    state.mem_event(|core, port| Event::PteAddr { core, port, level: 0, base: base0, vpn: vpn0, addr: pte0_addr });
    let pte0 = state.pte_read(pte0_addr, 0);
    if let Some(bad) = invalid(pte0) { verdict(state, 0, pte0, bad); return Err(fault_cause); }
    if !is_leaf(pte0) { verdict(state, 0, pte0, PteVerdict::NoLeaf); return Err(fault_cause); }
    verdict(state, 0, pte0, PteVerdict::Leaf);
    if !perm(state, pte0, access, priv_mode, mstatus) { return Err(fault_cause); }
    let pte0 = update_ad(state, pte0_addr, pte0, access);

    let ppn = (pte0 >> 10) & 0x003f_ffff;
    let pa = (ppn << 12) | off;
    // G in the pointer PTE makes everything below it global.
    fill(state, access, vaddr, pa, vpn, ppn, pte0, false, asid, (pte0 | pte1) & PTE_G != 0);
    Ok(pa)
}

/// Install the walked translation and report the victim choice and the fill.
#[allow(clippy::too_many_arguments)]
fn fill(state: &mut PipelineState, access: AccessKind, va: u32, pa: u32, vpn: u32, ppn: u32, pte: u32, is_super: bool, asid: u16, global: bool) {
    let v = state.tlb_for(access).install(vpn, ppn, pte, is_super, asid, global);
    state.mem_event(|core, port| Event::TlbVictim { core, port, idx: v.idx, was_invalid: v.was_invalid, plru_before: v.plru_before, plru_after: v.plru_after });
    state.mem_event(|core, port| Event::TlbFill { core, port, idx: v.idx, va, pa });
}

fn verdict(state: &PipelineState, level: u8, pte: u32, verdict: PteVerdict) {
    state.mem_event(|core, port| Event::PteCheck { core, port, level, pte, verdict });
}

/// A / D update on a TLB hit: the cached PTE lacks A (or D on a store).
/// Re-locate the PTE in memory, write the new bits, update the entry.
fn update_ad_for_tlb_entry(state: &mut PipelineState, idx: usize, vpn: u32, access: AccessKind) {
    let entry = state.tlb_for(access).entries[idx];
    let need_a = entry.pte & PTE_A == 0;
    let need_d = access == AccessKind::Store && entry.pte & PTE_D == 0;
    if !need_a && !need_d { return; }
    let root = (state.sys.csrs.read(addr::SATP) & 0x003f_ffff) << 12;
    let vpn1 = vpn >> 10;
    let pte_addr = if entry.is_super {
        root.wrapping_add(vpn1 * 4)
    } else {
        let pte1 = state.pte_read(root.wrapping_add(vpn1 * 4), 1);
        (((pte1 >> 10) & 0x003f_ffff) << 12).wrapping_add((vpn & 0x3ff) * 4)
    };
    let new_pte = update_ad(state, pte_addr, entry.pte, access);
    state.tlb_for(access).entries[idx].pte = new_pte;
}

/// Invalid PTE reasons: V = 0, or the reserved W-without-R encoding.
fn invalid(pte: u32) -> Option<PteVerdict> {
    if pte & PTE_V == 0 { return Some(PteVerdict::Invalid); }
    if (pte & PTE_R) == 0 && (pte & PTE_W) != 0 { return Some(PteVerdict::Reserved); }
    None
}

fn is_leaf(pte: u32) -> bool { pte & (PTE_R | PTE_W | PTE_X) != 0 }

/// Permission check, reported as an event.
fn perm(state: &PipelineState, pte: u32, access: AccessKind, priv_mode: u8, mstatus: u32) -> bool {
    let ok = check_perm(pte, access, priv_mode, mstatus);
    let (sum, mxr) = (mstatus & MSTATUS_SUM != 0, mstatus & MSTATUS_MXR != 0);
    state.mem_event(|core, port| Event::PermCheck { core, port, pte, priv_mode, sum, mxr, ok });
    ok
}

fn check_perm(pte: u32, access: AccessKind, priv_mode: u8, mstatus: u32) -> bool {
    let r = pte & PTE_R != 0;
    let w = pte & PTE_W != 0;
    let x = pte & PTE_X != 0;
    let u = pte & PTE_U != 0;
    let sum = mstatus & MSTATUS_SUM != 0;
    let mxr = mstatus & MSTATUS_MXR != 0;

    // U/S access rules.
    if priv_mode == 0 && !u { return false; }     // U accessing non-U: deny
    if priv_mode == 1 && u {
        // S accessing U: only allowed for loads/stores when SUM=1.
        if access == AccessKind::Fetch || !sum { return false; }
    }

    match access {
        AccessKind::Fetch => x,
        // MXR makes executable pages readable.
        AccessKind::Load  => r || (mxr && x),
        AccessKind::Store => w,
    }
}

/// Hardware A/D bit update. Sets A on every access, D on stores.
/// Writes the PTE back if any bit changed; returns the new PTE.
fn update_ad(state: &mut PipelineState, pte_addr: u32, pte: u32, access: AccessKind) -> u32 {
    let mut new_pte = pte | PTE_A;
    if access == AccessKind::Store { new_pte |= PTE_D; }
    if new_pte != pte {
        state.pte_write(pte_addr, new_pte);
        state.mem_event(|core, port| Event::AdUpdate { core, port, pte_addr, old: pte, new: new_pte });
    }
    new_pte
}
