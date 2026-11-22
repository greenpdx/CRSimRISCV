//! Golden tests — lock the behaviour of every core.
//!
//! For each (core, program) the test steps the core by its smallest unit and
//! hashes two streams:
//! - `arch`: PC and registers after every step, then CSRs and data memory at
//!   the end. Must never change.
//! - `view`: everything a panel shows each step (latches, micro state,
//!   traces, events). May only change in a step that changes that shape on
//!   purpose, with `arch` unchanged.
//!
//! Goldens live in `tests/golden/streams.txt`. Regenerate with
//! `GOLDEN_UPDATE=1 cargo test --test golden`.
//!
//! `cross_core_results_agree` runs the same programs on every core and
//! compares the final x1..x29.

use std::collections::BTreeMap;
use std::fmt::{Debug, Write};

use cpu_sim::sim::barrel::{BarrelState, EXT_DMEM_BASE};
use cpu_sim::sim::cpu_state::CpuState;
use cpu_sim::sim::memsys::{self, system::MemPanelSystem};
use cpu_sim::sim::ooo::{self, examples::enc, mem::MemMode, OooCpu};
use cpu_sim::sim::pipeline::PipelineState;
use cpu_sim::sim::system::SysUnit;

// ── Hash (FNV-1a 64: fixed, unlike std's DefaultHasher) ─────────────────────

struct H(u64);

impl H {
    fn new() -> Self { H(0xcbf2_9ce4_8422_2325) }
    fn byte(&mut self, b: u8) { self.0 = (self.0 ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3); }
    fn u32(&mut self, v: u32) { for b in v.to_le_bytes() { self.byte(b); } }
    fn u64(&mut self, v: u64) { for b in v.to_le_bytes() { self.byte(b); } }
    fn words(&mut self, w: &[u32]) { for &v in w { self.u32(v); } }
    fn dbg(&mut self, v: &impl Debug) { write!(self, "{v:?}|").unwrap(); }
    fn sys(&mut self, s: &SysUnit) {
        for (&a, &v) in s.csrs.raw() { self.u32(a as u32); self.u32(v); }
        self.u32(s.priv_mode as u32);
        self.u32(s.pending_irq);
        self.u32(s.trap_depth);
        self.dbg(&s.last_trap);
    }
}

impl Write for H {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        for b in s.bytes() { self.byte(b); }
        Ok(())
    }
}

/// Result of one run: the two hashes and the final registers.
struct Run { arch: u64, view: u64, regs: [u32; 32] }

// ── Programs ────────────────────────────────────────────────────────────────
// x30 = link / PC-dependent scratch, x31 = data base: not compared across cores.

const ILLEGAL: u32 = 0xffff_ffff;

fn all_opcodes() -> Vec<u32> {
    include_str!("../../programs/program.hex").lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| u32::from_str_radix(l.trim(), 16).unwrap()).collect()
}

fn arith() -> Vec<u32> {
    use enc::*;
    let mut p = vec![addi(1, 0, -37), lui(2, 0x12345), addi(2, 2, 0x678)];
    // R-type: funct3 0..7, plus SUB and SRA
    for (k, (f3, f7)) in [(0, 0), (0, 0x20), (1, 0), (2, 0), (3, 0), (4, 0), (5, 0), (5, 0x20), (6, 0), (7, 0)].iter().enumerate() {
        p.push(r(*f7, 2, 1, *f3, 3 + k as u32, 0x33));
    }
    p.push(r(0, 1, 2, 1, 13, 0x33));            // sll x13, x2, x1
    p.push(r(0x20, 1, 2, 5, 14, 0x33));         // sra x14, x2, x1
    // I-type
    p.push(i(-5, 1, 2, 15, 0x13));              // slti
    p.push(i(-5, 1, 3, 16, 0x13));              // sltiu
    p.push(i(0x7ff, 2, 4, 17, 0x13));           // xori
    p.push(i(0x555, 2, 6, 18, 0x13));           // ori
    p.push(i(0x0f0, 2, 7, 19, 0x13));           // andi
    p.push(i(7, 2, 1, 20, 0x13));               // slli
    p.push(i(3, 1, 5, 21, 0x13));               // srli
    p.push(i(0x400 | 3, 1, 5, 22, 0x13));       // srai
    p.push(0x0000_1f17);                        // auipc x30, 1
    p.push(SPIN);
    p
}

fn branch() -> Vec<u32> {
    use enc::*;
    let mut p = vec![addi(1, 0, -3), addi(2, 0, 5)];
    for (k, f3) in [0u32, 1, 4, 5, 6, 7].iter().enumerate() {
        p.push(b(8, 1, 2, *f3));                // taken: skip the count
        p.push(addi(5 + k as u32, 5 + k as u32, 1));
    }
    p.push(addi(1, 1, 1));                      // 56
    p.push(addi(3, 3, 1));                      // 60
    p.push(addi(4, 0, 8));                      // 64
    p.push(bne(3, 4, 8 - 68));                  // 68: back to the first branch
    p.push(SPIN);
    p
}

fn call() -> Vec<u32> {
    use enc::*;
    vec![
        0x0000_0e17,            //  0 auipc x28, 0   (at PC 0 of the image: see known_bug_auipc)
        addi(1, 0, 0),          //  4
        jal(30, 24),            //  8 → 32
        addi(1, 1, 100),        // 12
        jalr(30, 28, 32),       // 16 → 32
        addi(28, 0, 0),         // 20
        addi(30, 0, 0),         // 24
        SPIN,                   // 28
        addi(1, 1, 1),          // 32 f:
        jalr(0, 30, 0),         // 36
    ]
}

fn mem(db: u32) -> Vec<u32> {
    use enc::*;
    vec![
        lui(31, db >> 12),
        lui(1, 0x89abc), addi(1, 1, 0x5ef),
        sw(1, 31, 0),
        s(6, 1, 31, 1),             // sh x1, 6(x31)
        s(9, 1, 31, 0),             // sb x1, 9(x31)
        s(11, 1, 31, 0),            // sb x1, 11(x31)
        lw(2, 31, 0), lw(3, 31, 4), lw(4, 31, 8),
        i(0, 31, 0, 5, 0x03),       // lb
        i(3, 31, 4, 6, 0x03),       // lbu
        i(0, 31, 1, 7, 0x03),       // lh
        i(2, 31, 5, 8, 0x03),       // lhu
        i(11, 31, 0, 9, 0x03),      // lb
        i(6, 31, 5, 10, 0x03),      // lhu
        SPIN,
    ]
}

/// Word stores only (SW), every load width.
fn mem_w(db: u32) -> Vec<u32> {
    use enc::*;
    vec![
        lui(31, db >> 12),
        lui(1, 0x89abc), addi(1, 1, 0x5ef),
        sw(1, 31, 0),
        addi(2, 1, 0x123), sw(2, 31, 8),
        lw(3, 31, 0), lw(4, 31, 8),
        i(0, 31, 0, 5, 0x03),       // lb
        i(3, 31, 4, 6, 0x03),       // lbu
        i(0, 31, 1, 7, 0x03),       // lh
        i(2, 31, 5, 8, 0x03),       // lhu
        i(9, 31, 0, 9, 0x03),       // lb
        i(10, 31, 5, 10, 0x03),     // lhu
        SPIN,
    ]
}

fn mul() -> Vec<u32> {
    use enc::*;
    vec![
        addi(1, 0, -7), lui(2, 0x80000), addi(2, 2, 3),
        r(1, 2, 1, 0, 3, 0x33), r(1, 2, 1, 1, 4, 0x33), r(1, 2, 1, 2, 5, 0x33), r(1, 2, 1, 3, 6, 0x33),
        r(1, 1, 2, 2, 7, 0x33), r(1, 1, 1, 1, 8, 0x33),
        SPIN,
    ]
}

/// `body` runs with a trap handler installed: the handler adds 1 to x1 and
/// returns to mepc + 4. x5 = last mcause.
fn with_handler(body: &[u32]) -> Vec<u32> {
    use enc::*;
    let handler = 4 * (4 + body.len() as i32 + 1);
    let mut p = vec![0x0000_0f17 /* auipc x30, 0 */, addi(30, 30, handler), csrw(0x305, 30), addi(1, 0, 0)];
    p.extend_from_slice(body);
    p.push(jal(0, 24));
    p.extend([addi(1, 1, 1), csrr(29, 0x341), addi(29, 29, 4), csrw(0x341, 29), MRET]);
    p.extend([csrr(5, 0x342), addi(29, 0, 0), addi(30, 0, 0), SPIN]);
    p
}

fn trap() -> Vec<u32> {
    use enc::*;
    with_handler(&[
        ECALL,
        addi(1, 1, 100),
        i(0x340, 5, 6, 2, 0x73),    // csrrsi x2, mscratch, 5
        i(0x340, 1, 7, 3, 0x73),    // csrrci x3, mscratch, 1
        i(0x340, 1, 1, 4, 0x73),    // csrrw  x4, mscratch, x1
    ])
}

fn illegal() -> Vec<u32> {
    use enc::*;
    with_handler(&[ILLEGAL, r(1, 2, 1, 4, 3, 0x33) /* div: not in Zmmul */, addi(1, 1, 16)])
}

// ── Cores ───────────────────────────────────────────────────────────────────

const SINGLE_DB: u32 = 0x0001_0000;
const PIPE_DB: u32 = 0x0001_0000;
const OOO_DB: u32 = 0x0000_1000;

fn single(words: &[u32], zmmul: bool, traps: bool) -> Run {
    let mut c = CpuState::new();
    c.zmmul = zmmul;
    c.traps_enabled = traps;
    c.load_program(words);
    let (mut a, mut v) = (H::new(), H::new());
    for _ in 0..3000 {
        c.micro_step();
        a.u32(c.pc); a.words(&c.registers);
        v.dbg(&c.micro);
        v.dbg(&c.last_active); v.dbg(&c.last_consumed);
        v.dbg(&c.last_reg_access); v.dbg(&c.last_ram_access); v.dbg(&c.last_rom_access);
        v.u64(c.cycle_count);
    }
    a.sys(&c.sys);
    a.words(&c.ram); a.dbg(&c.ram_extra); a.u32(c.io_data);
    Run { arch: a.0, view: v.0, regs: c.registers }
}

fn pipeline_view(v: &mut H, c: &PipelineState) {
    v.dbg(&c.if_id); v.dbg(&c.id_ex); v.dbg(&c.ex_ma); v.dbg(&c.ma_wb); v.dbg(&c.last_wb);
    v.dbg(&(c.last_stall, c.last_flush, c.last_forward_a, c.last_forward_b, c.last_trap_taken, c.last_stall_reg));
    v.dbg(&(c.reg_read_mask, c.reg_write_mask, c.last_ram_addr, c.last_ram_kind, c.last_ram_side));
    v.dbg(&c.last_csr_snapshot);
    v.dbg(&c.itlb); v.dbg(&c.dtlb);
    v.u64(c.cycle_count);
}

fn pipeline(words: &[u32], mode: u8) -> Run {
    let mut c = PipelineState::new();
    c.branch_mode = cpu_sim::sim::pipeline::state::BranchMode::from_u8(mode);
    c.load_program(words);
    let (mut a, mut v) = (H::new(), H::new());
    for _ in 0..600 {
        c.step();
        a.u32(c.pc); a.words(&c.registers);
        pipeline_view(&mut v, &c);
    }
    a.sys(&c.sys);
    a.words(&c.machine_ram); a.words(&c.user_ram); a.u32(c.io_data);
    Run { arch: a.0, view: v.0, regs: c.registers }
}

fn barrel(words: &[u32], all_threads: bool) -> Run {
    let mut c = BarrelState::new();
    c.load_program(words);
    if all_threads { c.activate_all(); } else { c.reset_boot(); }
    let (mut a, mut v) = (H::new(), H::new());
    for _ in 0..4000 {
        c.step();
        for t in &c.threads { a.u32(t.pc); a.words(&t.regs); }
        v.dbg(&c.threads);
        v.dbg(&c.lt_if); v.dbg(&c.if_di); v.dbg(&c.di_ex); v.dbg(&c.ex_ma); v.dbg(&c.ma_wb); v.dbg(&c.last_wb);
        v.dbg(&c.bus); v.dbg(&c.store_buffer); v.dbg(&c.dma); v.dbg(&c.irq);
        v.dbg(&(c.icache.hits, c.icache.misses, c.icache.pending, c.dcache.hits, c.dcache.misses, c.dcache.pending));
        v.dbg(&(c.sched_mode, c.current_thread, c.sched_history, c.sched_history_idx, c.cycle_count, c.halted));
    }
    for t in &c.threads { a.sys(&t.sys); }
    a.words(&c.ext_dmem); a.u32(c.io_data);
    Run { arch: a.0, view: v.0, regs: c.threads[0].regs }
}

fn ooo_run(words: &[u32], data: &[(u32, u32)], mode: MemMode, db: u32) -> Run {
    let mut c = OooCpu::new(mode);
    c.load(0, words);
    for &(addr, w) in data { c.load(addr, &[w]); }
    let (mut a, mut v) = (H::new(), H::new());
    for _ in 0..3500 {
        c.step_stage();
        a.u32(c.pc); a.words(&c.rf); a.u64(c.cycle);
        v.dbg(&c.rat); v.dbg(&c.iq); v.dbg(&c.fetching()); v.dbg(&c.rob);
        v.dbg(&c.alu_rs); v.dbg(&c.mul_rs); v.dbg(&c.br_rs); v.dbg(&c.lsq); v.dbg(&c.exec);
        v.dbg(&c.pred);
        v.dbg(&(c.serializing, c.halted, c.stage));
        v.dbg(&c.events); v.dbg(&c.event_stage); v.dbg(&c.stats); v.dbg(&c.history);
    }
    a.sys(&c.sys);
    for k in 0..64 { a.u32(c.mem.peek(db + 4 * k)); }
    Run { arch: a.0, view: v.0, regs: c.rf }
}

fn mem_panel(setup: fn(&mut MemPanelSystem)) -> Run {
    let mut m = MemPanelSystem::new();
    setup(&mut m);
    let (mut a, mut v) = (H::new(), H::new());
    for _ in 0..600 {
        m.step();
        for c in &m.cores {
            a.u32(c.pc); a.words(&c.registers);
            pipeline_view(&mut v, c);
        }
        let mem = m.mem.borrow();
        v.dbg(&mem.events); v.dbg(&mem.stats); v.dbg(&mem.stall); v.dbg(&(mem.now, mem.bus_free));
    }
    for c in &m.cores { a.sys(&c.sys); }
    for base in [0u32, 0x1000, 0x2000, 0x4000, 0x8000, 0x1_0000, 0x40_0000, 0x80_0000] {
        for k in 0..64 { a.u32(m.peek(base + 4 * k)); }
    }
    Run { arch: a.0, view: v.0, regs: m.cores[0].registers }
}

// ── Golden table ────────────────────────────────────────────────────────────

fn all_runs() -> BTreeMap<String, Run> {
    let mut out = BTreeMap::new();
    let progs: Vec<(&str, fn(u32) -> Vec<u32>)> = vec![
        ("all_opcodes", |_| all_opcodes()),
        ("arith", |_| arith()),
        ("branch", |_| branch()),
        ("call", |_| call()),
        ("mem", mem),
        ("mem_w", mem_w),
        ("mul", |_| mul()),
        ("trap", |_| trap()),
        ("illegal", |_| illegal()),
    ];
    for (name, p) in &progs {
        out.insert(format!("single/{name}"), single(&p(SINGLE_DB), true, true));
        out.insert(format!("single-plain/{name}"), single(&p(SINGLE_DB), false, false));
        for mode in 0..3u8 {
            out.insert(format!("pipeline-b{mode}/{name}"), pipeline(&p(PIPE_DB), mode));
        }
        out.insert(format!("barrel-boot/{name}"), barrel(&p(EXT_DMEM_BASE), false));
        out.insert(format!("barrel-all/{name}"), barrel(&p(EXT_DMEM_BASE), true));
        out.insert(format!("ooo-fixed/{name}"), ooo_run(&p(OOO_DB), &[], MemMode::Fixed, OOO_DB));
        out.insert(format!("ooo-caches/{name}"), ooo_run(&p(OOO_DB), &[], MemMode::Caches, OOO_DB));
    }
    for (k, e) in ooo::examples::examples().iter().enumerate() {
        out.insert(format!("ooo-example-fixed/{k}"), ooo_run(&e.prog, &e.data, MemMode::Fixed, OOO_DB));
        out.insert(format!("ooo-example-caches/{k}"), ooo_run(&e.prog, &e.data, MemMode::Caches, OOO_DB));
    }
    for (k, e) in memsys::examples::examples().iter().enumerate() {
        out.insert(format!("mempanel-example/{k:02}"), mem_panel(e.setup));
    }
    out
}

const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/streams.txt");

#[test]
fn streams_match_golden() {
    let runs = all_runs();
    let text: String = runs.iter().map(|(n, r)| format!("{n} {:016x} {:016x}\n", r.arch, r.view)).collect();
    if std::env::var("GOLDEN_UPDATE").is_ok() {
        std::fs::create_dir_all(std::path::Path::new(GOLDEN).parent().unwrap()).unwrap();
        std::fs::write(GOLDEN, &text).unwrap();
        return;
    }
    let want = std::fs::read_to_string(GOLDEN).expect("tests/golden/streams.txt (GOLDEN_UPDATE=1 to create)");
    let want: BTreeMap<&str, (&str, &str)> = want.lines().filter_map(|l| {
        let mut f = l.split_whitespace();
        Some((f.next()?, (f.next()?, f.next()?)))
    }).collect();
    let mut bad = Vec::new();
    for (n, r) in &runs {
        match want.get(n.as_str()) {
            None => bad.push(format!("{n}: not in the golden file")),
            Some((a, v)) => {
                if *a != format!("{:016x}", r.arch) { bad.push(format!("{n}: ARCH stream changed")); }
                else if *v != format!("{:016x}", r.view) { bad.push(format!("{n}: view stream changed")); }
            }
        }
    }
    for n in want.keys() {
        if !runs.contains_key(*n) { bad.push(format!("{n}: in the golden file, not run")); }
    }
    assert!(bad.is_empty(), "{} of {} runs differ:\n{}", bad.len(), runs.len(), bad.join("\n"));
}

// ── Cross-core agreement ────────────────────────────────────────────────────

fn x1_29(r: &Run) -> Vec<u32> { r.regs[1..30].to_vec() }

#[test]
fn cross_core_results_agree() {
    // (program, uses MUL, uses traps / CSRs)
    let progs: Vec<(&str, fn(u32) -> Vec<u32>, bool, bool)> = vec![
        ("arith", |_| arith(), false, false),
        ("branch", |_| branch(), false, false),
        ("call", |_| call(), false, false),
        ("mem", mem, false, false),
        ("mem_w", mem_w, false, false),
        ("mul", |_| mul(), true, false),
        ("trap", |_| trap(), false, true),
        ("illegal", |_| illegal(), false, true),
    ];
    for (name, p, uses_mul, _uses_sys) in progs {
        // Reference: the out-of-order core (RV32I + Zmmul + Zicsr, traps).
        let want = x1_29(&ooo_run(&p(OOO_DB), &[], MemMode::Fixed, OOO_DB));
        let mut got: Vec<(&str, Vec<u32>)> = vec![
            ("single", x1_29(&single(&p(SINGLE_DB), true, true))),
            ("ooo-caches", x1_29(&ooo_run(&p(OOO_DB), &[], MemMode::Caches, OOO_DB))),
        ];
        // Pipeline: RV32I + Zicsr (no MUL).
        if !uses_mul {
            for mode in 0..3u8 { got.push(("pipeline", x1_29(&pipeline(&p(PIPE_DB), mode)))); }
        }
        // Barrel with one thread: straight-line code only. It does not squash
        // the instructions fetched after a taken branch / jump when a single
        // thread fills the pipeline.
        if matches!(name, "arith" | "mem" | "mem_w") {
            got.push(("barrel-boot", x1_29(&barrel(&p(EXT_DMEM_BASE), false))));
        }
        for (core, regs) in got {
            assert_eq!(regs, want, "{name}: {core} differs from the out-of-order core (x1..x29)");
        }
    }
}

/// AUIPC = pc + imm on every core (was `x[instr[19:15]] + imm` on the cores
/// that decode through the opcode table).
#[test]
fn auipc_adds_the_pc() {
    // nop; nop; auipc x5, 0x12345; j .      (auipc at PC = base + 8)
    let p = [0x13, 0x13, 0x1234_5297, 0x6f];
    assert_eq!(ooo_run(&p, &[], MemMode::Fixed, OOO_DB).regs[5], 0x1234_5008);
    assert_eq!(single(&p, true, true).regs[5], 0x1234_5008, "single-cycle");
    for mode in 0..3u8 { assert_eq!(pipeline(&p, mode).regs[5], 0x1234_5008, "pipeline"); }
    assert_eq!(barrel(&p, false).regs[5], 0x1234_5008, "barrel");
}

/// SB / SH write only their byte lanes on every core.
#[test]
fn subword_stores_merge() {
    // x3 = the word at +4 after `sh x1, 6(x31)` with x1 = 0x89abc5ef
    assert_eq!(single(&mem(SINGLE_DB), true, true).regs[3], 0xc5ef_0000);
    assert_eq!(pipeline(&mem(PIPE_DB), 0).regs[3], 0xc5ef_0000);
    assert_eq!(barrel(&mem(EXT_DMEM_BASE), false).regs[3], 0xc5ef_0000);
}

/// Encodings outside a core's ISA trap as illegal instruction (cause 2).
/// Program: 0xffff_ffff, then DIV x3, x1, x2, with a handler that counts
/// traps in x1 (x1 = 16 + traps); x5 = mcause.
#[test]
fn illegal_instructions_trap() {
    let runs = [
        ("ooo", ooo_run(&illegal(), &[], MemMode::Fixed, OOO_DB)),
        ("single", single(&illegal(), true, true)),
        ("pipeline", pipeline(&illegal(), 0)),
    ];
    for (core, r) in runs {
        assert_eq!((r.regs[1], r.regs[3], r.regs[5]), (18, 0, 2), "{core}");
    }
    // Single-cycle with traps off: both are NOPs (x3 stays 0, no trap).
    let r = single(&illegal(), true, false);
    assert_eq!((r.regs[1], r.regs[3]), (16, 0), "single-cycle, traps off");
    // The pipeline has no M extension: MUL is illegal too.
    let r = pipeline(&with_handler(&[enc::mul(3, 1, 1)]), 0);
    assert_eq!((r.regs[1], r.regs[5]), (1, 2), "pipeline MUL");
}
