//! Sv32 page-table walker (Phase 4.2).
//!
//! No TLB. No A/D updates (deferred to 4.3). No SUM/MXR. Page tables
//! are read from RAM via `PipelineState::ram_read`. M-mode and
//! satp.MODE = Bare both bypass translation.

use super::state::PipelineState;
use crate::sim::system::addr;
use super::tlb::TlbEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessKind { Fetch, Load, Store }

const PTE_V: u32 = 1 << 0;
const PTE_R: u32 = 1 << 1;
const PTE_W: u32 = 1 << 2;
const PTE_X: u32 = 1 << 3;
const PTE_U: u32 = 1 << 4;
const PTE_A: u32 = 1 << 6;
const PTE_D: u32 = 1 << 7;

const MSTATUS_SUM: u32 = 1 << 18;
const MSTATUS_MXR: u32 = 1 << 19;

/// Translate a virtual address. Returns `Ok(phys)` or `Err(cause)`
/// where cause is 12 (instr page fault), 13 (load page fault), or
/// 15 (store page fault).
///
/// On success, hardware-manages A/D bits: sets A on every access,
/// sets D on stores. The updated PTE is written back to RAM.
pub fn translate(state: &mut PipelineState, vaddr: u32, access: AccessKind) -> Result<u32, u32> {
    // Bypass translation in M-mode or when satp.MODE == 0 (Bare).
    if state.priv_bits() == 3 { return Ok(vaddr); }
    if (state.sys.csrs.read(addr::SATP) >> 31) == 0 { return Ok(vaddr); }

    let fault_cause = match access {
        AccessKind::Fetch => 12,
        AccessKind::Load  => 13,
        AccessKind::Store => 15,
    };
    let mstatus = state.sys.csrs.read(addr::MSTATUS);
    let priv_mode = state.priv_bits();
    let vpn = vaddr >> 12;
    let off = vaddr & 0xfff;

    // ── TLB lookup (Phase 4.4) ─────────────────────────────────────
    if let Some(entry) = state.tlb.lookup(vpn) {
        if !check_perm(entry.pte, access, priv_mode, mstatus) {
            return Err(fault_cause);
        }
        if access == AccessKind::Store && (entry.pte & PTE_W) == 0 {
            return Err(fault_cause);
        }
        update_ad_for_tlb_entry(state, vpn, entry, access);
        if entry.is_super {
            let ppn1 = entry.ppn >> 10;
            return Ok((ppn1 << 22) | (vaddr & 0x003f_ffff));
        } else {
            return Ok((entry.ppn << 12) | off);
        }
    }

    let root_ppn = state.sys.csrs.read(addr::SATP) & 0x003f_ffff;
    let root = root_ppn << 12;
    let vpn1 = (vaddr >> 22) & 0x3ff;
    let vpn0 = (vaddr >> 12) & 0x3ff;

    // Level 1 PTE
    let pte1_addr = root.wrapping_add(vpn1 * 4);
    let pte1 = state.ram_read(pte1_addr);
    if !pte_valid(pte1) { return Err(fault_cause); }

    let leaf1 = (pte1 & (PTE_R | PTE_W | PTE_X)) != 0;
    if leaf1 {
        if !check_perm(pte1, access, priv_mode, mstatus) { return Err(fault_cause); }
        let ppn0 = (pte1 >> 10) & 0x3ff;
        if ppn0 != 0 { return Err(fault_cause); } // misaligned superpage
        if access == AccessKind::Store && (pte1 & PTE_W) == 0 { return Err(fault_cause); }
        update_ad(state, pte1_addr, pte1, access);
        let ppn1 = (pte1 >> 20) & 0xfff;
        // Install superpage TLB entry. ppn stored as full-PPN form
        // (ppn1 in upper 10 bits, 0 in lower 10).
        state.tlb.install(vpn, ppn1 << 10, pte1, true);
        return Ok((ppn1 << 22) | (vaddr & 0x003f_ffff));
    }

    // Not a leaf — walk to L0.
    let next_ppn = (pte1 >> 10) & 0x003f_ffff;
    let pte0_addr = (next_ppn << 12).wrapping_add(vpn0 * 4);
    let pte0 = state.ram_read(pte0_addr);
    if !pte_valid(pte0) { return Err(fault_cause); }

    let leaf0 = (pte0 & (PTE_R | PTE_W | PTE_X)) != 0;
    if !leaf0 { return Err(fault_cause); } // pointer-to-pointer not allowed at L0
    if !check_perm(pte0, access, priv_mode, mstatus) { return Err(fault_cause); }
    update_ad(state, pte0_addr, pte0, access);

    let ppn = (pte0 >> 10) & 0x003f_ffff;
    state.tlb.install(vpn, ppn, pte0, false);
    Ok((ppn << 12) | off)
}

/// Replay the A/D writeback for a TLB hit. Updates the in-memory PTE
/// (and the cached pte in the TLB entry) so the next walk sees fresh
/// flag bits.
fn update_ad_for_tlb_entry(state: &mut PipelineState, vpn: u32, mut entry: TlbEntry,
                           access: AccessKind) {
    let need_a = entry.pte & PTE_A == 0;
    let need_d = access == AccessKind::Store && entry.pte & PTE_D == 0;
    if !need_a && !need_d { return; }
    // Re-locate the PTE in RAM to write the updated bits. We need
    // satp again because the entry doesn't store its PTE address.
    let root_ppn = state.sys.csrs.read(addr::SATP) & 0x003f_ffff;
    let root = root_ppn << 12;
    let pte_addr = if entry.is_super {
        let vpn1 = vpn >> 10;
        root.wrapping_add(vpn1 * 4)
    } else {
        let vpn1 = vpn >> 10;
        let vpn0 = vpn & 0x3ff;
        let pte1 = state.ram_read(root.wrapping_add(vpn1 * 4));
        let next_ppn = (pte1 >> 10) & 0x003f_ffff;
        (next_ppn << 12).wrapping_add(vpn0 * 4)
    };
    let mut new_pte = entry.pte | PTE_A;
    if access == AccessKind::Store { new_pte |= PTE_D; }
    state.ram_write(pte_addr, new_pte);
    entry.pte = new_pte;
    // Re-install with updated pte. (install slot is deterministic.)
    state.tlb.install(vpn, entry.ppn, new_pte, entry.is_super);
}

fn pte_valid(pte: u32) -> bool {
    if pte & PTE_V == 0 { return false; }
    // Reserved encoding: R=0 W=1
    if (pte & PTE_R) == 0 && (pte & PTE_W) != 0 { return false; }
    true
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
/// Writes the PTE back to RAM if any bit changed.
fn update_ad(state: &mut PipelineState, pte_addr: u32, pte: u32, access: AccessKind) {
    let mut new_pte = pte | PTE_A;
    if access == AccessKind::Store { new_pte |= PTE_D; }
    if new_pte != pte {
        state.ram_write(pte_addr, new_pte);
    }
}
