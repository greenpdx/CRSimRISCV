//! The pipeline's memory: either the flat teaching model (ROM, machine RAM,
//! user RAM selected by privilege, one I/O register) or one core's port into
//! the memory panel's cache hierarchy. One enum, so the stages ask the same
//! questions in both modes.

use std::cell::RefCell;
use std::rc::Rc;

use crate::sim::mem::{Bus, Port};
use crate::sim::memsys::{Event, MemSystem};

pub(crate) const ROM_BASE: u32 = 0x0000_0000;
pub(crate) const RAM_BASE: u32 = 0x0001_0000;
pub(crate) const IO_BASE:  u32 = 0x4002_0000;
pub(crate) const MEM_WORDS: usize = 256;

/// Flat model: ROM at 0, RAM at `RAM_BASE` — machine RAM for M-mode
/// accesses and page tables, user RAM for U / S — and the I/O register at
/// every address from `IO_BASE` up.
#[derive(Debug, Clone)]
pub struct FlatMem {
    pub rom: Vec<u32>,
    pub machine_ram: Vec<u32>,
    pub user_ram: Vec<u32>,
    pub io_data: u32,
}

impl Default for FlatMem {
    fn default() -> Self {
        Self { rom: vec![0x0000_0013; MEM_WORDS], machine_ram: vec![0; MEM_WORDS], user_ram: vec![0; MEM_WORDS], io_data: 0 }
    }
}

fn ram_index(addr: u32) -> usize { ((addr & !3).wrapping_sub(RAM_BASE) >> 2) as usize }

impl FlatMem {
    fn side(&self, user: bool) -> &Vec<u32> { if user { &self.user_ram } else { &self.machine_ram } }
    fn side_mut(&mut self, user: bool) -> &mut Vec<u32> { if user { &mut self.user_ram } else { &mut self.machine_ram } }
    pub fn ram_read(&self, addr: u32, user: bool) -> u32 { self.side(user).get(ram_index(addr)).copied().unwrap_or(0) }
    pub fn ram_write(&mut self, addr: u32, val: u32, user: bool) {
        let i = ram_index(addr);
        if let Some(w) = self.side_mut(user).get_mut(i) { *w = val; }
    }
    fn rom_index(&self, addr: u32) -> Option<usize> {
        let i = (addr.wrapping_sub(ROM_BASE) >> 2) as usize;
        (i < self.rom.len()).then_some(i)
    }
}

/// Memory-panel mode: this pipeline is core `core` of a shared memory
/// system (caches, coherence, one physical memory). Its fetch, loads /
/// stores and page-table walks go through `sys`; its MMU reports events.
#[derive(Debug, Clone)]
pub struct Hier {
    pub core: u8,
    pub sys: Rc<RefCell<MemSystem>>,
}

#[derive(Debug, Clone)]
pub enum PipeMem {
    Flat(FlatMem),
    Hier(Hier),
}

impl Default for PipeMem {
    fn default() -> Self { PipeMem::Flat(FlatMem::default()) }
}

impl PipeMem {
    pub fn is_hier(&self) -> bool { matches!(self, PipeMem::Hier(_)) }
    pub fn flat(&self) -> Option<&FlatMem> { match self { PipeMem::Flat(f) => Some(f), _ => None } }
    pub fn flat_mut(&mut self) -> Option<&mut FlatMem> { match self { PipeMem::Flat(f) => Some(f), _ => None } }

    /// Instruction fetch.
    pub fn fetch(&self, addr: u32) -> u32 {
        match self {
            PipeMem::Flat(f) => f.rom_index(addr).map_or(0, |i| f.rom[i]),
            PipeMem::Hier(h) => h.sys.borrow_mut().read(h.core as usize, addr, Port::Fetch),
        }
    }
    /// Data load (`user`: U / S mode — the flat model's user RAM).
    pub fn load(&self, addr: u32, user: bool) -> u32 {
        match self {
            PipeMem::Flat(f) => f.ram_read(addr, user),
            PipeMem::Hier(h) => h.sys.borrow_mut().read(h.core as usize, addr, Port::Load),
        }
    }
    /// Data store of a whole word.
    pub fn store(&mut self, addr: u32, val: u32, user: bool) {
        match self {
            PipeMem::Flat(f) => f.ram_write(addr, val, user),
            PipeMem::Hier(h) => h.sys.borrow_mut().write(h.core as usize, addr, val, Port::Store),
        }
    }
    /// The data word a store would find (SB / SH merge): no access recorded.
    pub fn peek_data(&self, addr: u32, user: bool) -> u32 {
        match self {
            PipeMem::Flat(f) => f.ram_read(addr, user),
            PipeMem::Hier(h) => h.sys.borrow().peek(addr & !3),
        }
    }
    /// Page-table read at `level` (1 = root table, 0 = leaf table).
    pub fn pte_read(&self, addr: u32, level: u8) -> u32 {
        match self {
            PipeMem::Flat(f) => f.ram_read(addr, false),
            PipeMem::Hier(h) => {
                let mut m = h.sys.borrow_mut();
                let pte = m.read(h.core as usize, addr, Port::Walk);
                let port = m.xlat_port[h.core as usize];
                m.event(Event::Walk { core: h.core, port, level, pte_addr: addr, pte });
                pte
            }
        }
    }
    /// Page-table write (A / D bits).
    pub fn pte_write(&mut self, addr: u32, val: u32) {
        match self {
            PipeMem::Flat(f) => f.ram_write(addr, val, false),
            PipeMem::Hier(h) => h.sys.borrow_mut().write(h.core as usize, addr, val, Port::Walk),
        }
    }
    /// I/O at `addr`? The memory-panel machine has no I/O: its whole physical
    /// space is RAM plus the boot ROM.
    pub fn io_at(&self, addr: u32) -> bool { !self.is_hier() && addr >= IO_BASE }
    pub fn io_data(&self) -> u32 { self.flat().map_or(0, |f| f.io_data) }
    pub fn set_io_data(&mut self, v: u32) { if let Some(f) = self.flat_mut() { f.io_data = v; } }

    /// Report an MMU event (memory-panel mode only): `e(core, port)` with the
    /// access being translated.
    pub fn event(&self, e: impl FnOnce(u8, Port) -> Event) {
        if let PipeMem::Hier(h) = self {
            let mut m = h.sys.borrow_mut();
            let port = m.xlat_port[h.core as usize];
            m.event(e(h.core, port));
        }
    }
    /// The access about to be translated (Fetch / Load / Store).
    pub fn set_xlat_port(&self, port: Port) {
        if let PipeMem::Hier(h) = self { h.sys.borrow_mut().set_xlat_port(h.core as usize, port); }
    }
    /// Start of a clock (memory-panel mode): true = the core is frozen this
    /// cycle (an earlier access is still in flight).
    pub fn core_begin(&self) -> bool {
        match self { PipeMem::Hier(h) => h.sys.borrow_mut().core_begin(h.core as usize), _ => false }
    }
    pub fn core_end(&self) {
        if let PipeMem::Hier(h) = self { h.sys.borrow_mut().core_end(h.core as usize); }
    }
}

/// Outside view: ROM below its end, the I/O register, then machine RAM
/// (flat); physical memory as the program sees it (hierarchy).
impl Bus for PipeMem {
    fn read(&mut self, addr: u32, port: Port) -> u32 {
        match port { Port::Fetch => self.fetch(addr), Port::Walk => self.pte_read(addr, 0), _ => self.load(addr, false) }
    }
    fn write(&mut self, addr: u32, word: u32, port: Port) {
        match port { Port::Walk => self.pte_write(addr, word), _ => self.store(addr, word, false) }
    }
    fn peek(&self, addr: u32) -> u32 {
        match self {
            PipeMem::Flat(f) => match f.rom_index(addr) {
                Some(i) => f.rom[i],
                None if addr >= IO_BASE => f.io_data,
                None => f.ram_read(addr, false),
            },
            PipeMem::Hier(h) => h.sys.borrow().peek(addr & !3),
        }
    }
    fn poke(&mut self, addr: u32, word: u32) {
        match self {
            PipeMem::Flat(f) => match f.rom_index(addr) {
                Some(i) => f.rom[i] = word,
                None if addr >= IO_BASE => f.io_data = word,
                None => f.ram_write(addr, word, false),
            },
            PipeMem::Hier(h) => h.sys.borrow_mut().load(addr & !3, &[word]),
        }
    }
}
