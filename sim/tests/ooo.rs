//! Out-of-order core: results must match a one-instruction-at-a-time
//! reference exactly (registers and memory), in both memory modes; plus
//! the out-of-order behaviour itself (completion order, commit order,
//! flushes, forwarding, precise traps).

use cpu_sim::sim::ooo::decode::{alu, branch_taken, decode, mul, Op};
use cpu_sim::sim::ooo::mem::{extract, merge, MemMode};
use cpu_sim::sim::ooo::{OooCpu, OooEvent};
use std::collections::HashMap;

// ── encoders (independent of the core's decoder) ───────────────────────
fn r(f7: u32, rs2: u32, rs1: u32, f3: u32, rd: u32, op: u32) -> u32 { f7 << 25 | rs2 << 20 | rs1 << 15 | f3 << 12 | rd << 7 | op }
fn i(imm: i32, rs1: u32, f3: u32, rd: u32, op: u32) -> u32 { ((imm as u32) & 0xfff) << 20 | rs1 << 15 | f3 << 12 | rd << 7 | op }
fn s(imm: i32, rs2: u32, rs1: u32, f3: u32) -> u32 { let m = imm as u32; ((m >> 5) & 0x7f) << 25 | rs2 << 20 | rs1 << 15 | f3 << 12 | (m & 0x1f) << 7 | 0x23 }
fn b(off: i32, rs2: u32, rs1: u32, f3: u32) -> u32 {
    let o = off as u32;
    ((o >> 12) & 1) << 31 | ((o >> 5) & 0x3f) << 25 | rs2 << 20 | rs1 << 15 | f3 << 12 | ((o >> 1) & 0xf) << 8 | ((o >> 11) & 1) << 7 | 0x63
}
fn jal(rd: u32, off: i32) -> u32 { let o = off as u32; ((o >> 20) & 1) << 31 | ((o >> 1) & 0x3ff) << 21 | ((o >> 11) & 1) << 20 | ((o >> 12) & 0xff) << 12 | rd << 7 | 0x6f }
fn addi(rd: u32, rs1: u32, imm: i32) -> u32 { i(imm, rs1, 0, rd, 0x13) }
fn add(rd: u32, a: u32, bb: u32) -> u32 { r(0, bb, a, 0, rd, 0x33) }
fn sub(rd: u32, a: u32, bb: u32) -> u32 { r(0x20, bb, a, 0, rd, 0x33) }
fn mulr(f3: u32, rd: u32, a: u32, bb: u32) -> u32 { r(1, bb, a, f3, rd, 0x33) }
fn lui(rd: u32, imm20: u32) -> u32 { imm20 << 12 | rd << 7 | 0x37 }
fn lw(rd: u32, rs1: u32, off: i32) -> u32 { i(off, rs1, 2, rd, 0x03) }
fn sw(rs2: u32, rs1: u32, off: i32) -> u32 { s(off, rs2, rs1, 2) }
fn csrr(rd: u32, csr: i32) -> u32 { i(csr, 0, 2, rd, 0x73) }
fn csrw(csr: i32, rs1: u32) -> u32 { i(csr, rs1, 1, 0, 0x73) }
const SPIN: u32 = 0x0000_006f;
const ECALL: u32 = 0x0000_0073;
const MRET: u32 = 0x3020_0073;

// ── reference: one instruction at a time (no traps) ────────────────────
struct Ref { x: [u32; 32], mem: HashMap<u32, u32>, pc: u32 }
impl Ref {
    fn new(prog: &[u32], data: &[(u32, u32)]) -> Self {
        let mut mem = HashMap::new();
        for (k, w) in prog.iter().enumerate() { mem.insert(4 * k as u32, *w); }
        for &(a, v) in data { mem.insert(a, v); }
        Self { x: [0; 32], mem, pc: 0 }
    }
    fn word(&self, a: u32) -> u32 { *self.mem.get(&(a & !3)).unwrap_or(&0) }
    fn run(&mut self, max: usize) -> bool {
        for _ in 0..max {
            let w = self.word(self.pc);
            if w == SPIN { return true; }
            let d = decode(w);
            let (a, bb) = (self.x[d.rs1 as usize], self.x[d.rs2 as usize]);
            let mut next = self.pc.wrapping_add(4);
            let v = match d.op {
                Op::Alu { op, imm } => Some(alu(op, a, if imm { d.imm } else { bb }, self.pc)),
                Op::Mul { f3 } => Some(mul(f3, a, bb)),
                Op::Branch { f3 } => { if branch_taken(f3, a, bb) { next = self.pc.wrapping_add(d.imm) } None }
                Op::Jal => { next = self.pc.wrapping_add(d.imm); Some(self.pc + 4) }
                Op::Jalr => { next = a.wrapping_add(d.imm) & !1; Some(self.pc + 4) }
                Op::Load { size, signed } => { let ad = a.wrapping_add(d.imm); Some(extract(self.word(ad), ad, size, signed)) }
                Op::Store { size } => { let ad = a.wrapping_add(d.imm); let m = merge(self.word(ad), ad, bb, size); self.mem.insert(ad & !3, m); None }
                Op::Fence => None,
                o => panic!("reference: {o:?} not supported"),
            };
            if let Some(v) = v { if d.rd != 0 { self.x[d.rd as usize] = v; } }
            self.pc = next;
        }
        false
    }
}

fn run_both(prog: &[u32], data: &[(u32, u32)], max: u64) -> Vec<OooCpu> {
    let mut rf = Ref::new(prog, data);
    assert!(rf.run(100_000), "reference did not reach `j .`");
    [MemMode::Fixed, MemMode::Caches].into_iter().map(|mode| {
        let mut c = OooCpu::new(mode);
        c.load(0, prog);
        for &(a, v) in data { c.load(a, &[v]); }
        assert!(c.run(max), "{mode:?}: no `j .` after {max} cycles (pc {:#x}, committed {})", c.pc, c.stats.committed);
        assert_eq!(c.rf, rf.x, "{mode:?}: registers differ from the reference");
        for (&a, &v) in &rf.mem { assert_eq!(c.mem.peek(a), v, "{mode:?}: memory {a:#x}"); }
        c
    }).collect()
}

// ── decode sanity (immediates against the independent encoders) ────────
#[test]
fn decode_immediates() {
    assert_eq!(decode(addi(1, 2, -5)).imm, (-5i32) as u32);
    assert_eq!(decode(sw(3, 4, -12)).imm, (-12i32) as u32);
    assert_eq!(decode(b(-16, 1, 2, 1)).imm, (-16i32) as u32);
    assert_eq!(decode(b(2048, 1, 2, 0)).imm, 2048);
    assert_eq!(decode(jal(1, -1024)).imm, (-1024i32) as u32);
    assert_eq!(decode(jal(0, 0x7fffe)).imm, 0x7fffe);
    assert_eq!(decode(lui(5, 0xfffff)).imm, 0xffff_f000);
    assert!(matches!(decode(r(1, 2, 3, 4, 5, 0x33)).op, Op::Illegal), "DIV is not in Zmmul");
}

// ── the diagram's example ──────────────────────────────────────────────
#[test]
fn diagram_example_completes_out_of_order_commits_in_order() {
    let prog = [lui(10, 1), addi(2, 0, 6), addi(3, 0, 7), addi(1, 0, 1), sw(2, 10, 0),
        lw(4, 10, 0), mulr(0, 6, 2, 3), add(5, 4, 1), addi(7, 0, 5), sub(8, 6, 7), sw(8, 10, 4), SPIN];
    let cpus = run_both(&prog, &[], 500);
    let c = &cpus[0];
    assert_eq!((c.rf[4], c.rf[5], c.rf[6], c.rf[8]), (6, 7, 42, 37));
    let t = |pc: u32| c.history.iter().find(|h| h.pc == pc && !h.squashed).unwrap().t;
    let (t_add, t_addi) = (t(4 * 7), t(4 * 8));
    assert!(t_addi.complete < t_add.complete, "addi (younger) finishes before add (waits for the load): {t_addi:?} {t_add:?}");
    assert!(t_addi.issue < t_add.issue, "addi issues before the older add");
    let commits: Vec<u64> = c.history.iter().filter(|h| !h.squashed).map(|h| h.t.commit.unwrap()).collect();
    assert!(commits.windows(2).all(|w| w[0] < w[1]), "commit in program order: {commits:?}");
}

// ── branches: the predictor learns a loop; a mispredict flushes ────────
#[test]
fn loop_branch_learned_and_mispredicts_flushed() {
    // sum 1..20 into x5
    let prog = [addi(1, 0, 20), addi(5, 0, 0), add(5, 5, 1), addi(1, 1, -1), b(-8, 0, 1, 1), sw(5, 0, 0x100), SPIN];
    let cpus = run_both(&prog, &[], 2000);
    for c in &cpus {
        assert_eq!(c.rf[5], 210);
        assert!(c.stats.mispredicts >= 2 && c.stats.mispredicts <= 5, "learn the loop, miss the exit: {}", c.stats.mispredicts);
        assert!(c.stats.squashed > 0);
    }
}

#[test]
fn flush_restores_the_rat_and_drops_wrong_path_results() {
    // the taken branch skips writes to x3 / x4 that fetch ran into (BTB cold → predicted not taken)
    let prog = [addi(1, 0, 1), addi(2, 0, 1), b(12, 2, 1, 0), addi(3, 0, 99), addi(4, 0, 99), addi(3, 3, 7), add(4, 3, 1), SPIN];
    let cpus = run_both(&prog, &[], 500);
    for c in &cpus {
        assert_eq!((c.rf[3], c.rf[4]), (7, 8));
        // the branch, and the final `j .` (both cold in the BTB)
        assert_eq!(c.stats.mispredicts, 2);
        assert!(c.history.iter().any(|h| h.squashed && h.pc == 12), "the wrong-path addi x3, 99 was squashed");
    }
}

#[test]
fn function_call_jal_jalr() {
    // main: x10 = 5; call double; x11 = x10 + 1; spin.  double: x10 += x10; ret
    let prog = [addi(10, 0, 5), jal(1, 12), addi(11, 10, 1), SPIN, add(10, 10, 10), i(0, 1, 0, 0, 0x67)];
    let cpus = run_both(&prog, &[], 500);
    for c in &cpus { assert_eq!((c.rf[10], c.rf[11], c.rf[1]), (10, 11, 8)); }
}

// ── memory: forwarding, byte / half, ordering ──────────────────────────
#[test]
fn store_to_load_forwarding() {
    // two dependent MULs hold the ROB head, so the store waits in the LSQ and the load takes its data
    let prog = [lui(10, 1), addi(5, 0, 0x55), mulr(0, 9, 5, 5), mulr(0, 9, 9, 9), sw(5, 10, 8), lw(6, 10, 8), addi(7, 6, 1), SPIN];
    let cpus = run_both(&prog, &[], 500);
    for c in &cpus { assert_eq!((c.rf[6], c.rf[7]), (0x55, 0x56)); }
    // fixed latency: the store is still waiting in the LSQ when the load comes (with caches,
    // the cold L1I slows fetch and the store may already have committed)
    assert!(cpus[0].stats.forwards >= 1, "the load took the store's data");
}

#[test]
fn byte_and_half_loads_stores() {
    let sb = |rs2, rs1, o| s(o, rs2, rs1, 0);
    let sh = |rs2, rs1, o| s(o, rs2, rs1, 1);
    let prog = [lui(10, 1), addi(5, 0, -2), sb(5, 10, 1), sh(5, 10, 6),
        i(1, 10, 0, 11, 0x03), i(1, 10, 4, 12, 0x03), i(6, 10, 1, 13, 0x03), i(6, 10, 5, 14, 0x03), lw(15, 10, 0), lw(16, 10, 4), SPIN];
    let cpus = run_both(&prog, &[(0x1000, 0x1122_3344), (0x1004, 0x5566_7788)], 500);
    let c = &cpus[0];
    assert_eq!((c.rf[11], c.rf[12], c.rf[13], c.rf[14]), (0xffff_fffe, 0xfe, 0xffff_fffe, 0xfffe));
    assert_eq!((c.rf[15], c.rf[16]), (0x1122_fe44, 0xfffe_7788));
}

#[test]
fn mul_variants() {
    let prog = [addi(1, 0, -3), addi(2, 0, 7), mulr(0, 3, 1, 2), mulr(1, 4, 1, 2), mulr(2, 5, 1, 2), mulr(3, 6, 1, 2), SPIN];
    let cpus = run_both(&prog, &[], 500);
    let c = &cpus[0];
    assert_eq!((c.rf[3], c.rf[4], c.rf[5], c.rf[6]), ((-21i32) as u32, 0xffff_ffff, 0xffff_ffff, 6));
}

// ── traps: precise, at commit ──────────────────────────────────────────
#[test]
fn ecall_is_precise() {
    // handler at 0x40: mepc += 4, count in x20, mret
    let mut prog = vec![addi(9, 0, 0x40), csrw(0x305, 9), addi(1, 0, 1), ECALL, addi(2, 0, 2), addi(3, 2, 1), SPIN];
    prog.resize(16, 0x13);
    prog.extend([csrr(21, 0x341), addi(21, 21, 4), csrw(0x341, 21), addi(20, 20, 1), MRET]);
    for mode in [MemMode::Fixed, MemMode::Caches] {
        let mut c = OooCpu::new(mode);
        c.load(0, &prog);
        assert!(c.run(1000), "{mode:?}");
        assert_eq!((c.rf[1], c.rf[2], c.rf[3], c.rf[20]), (1, 2, 3, 1), "{mode:?}: handler ran once, the rest resumed");
        assert_eq!(c.sys.csrs.read(0x342), 11, "mcause = ECALL from M-mode");
        assert_eq!(c.stats.traps, 1);
    }
}

#[test]
fn illegal_instruction_traps_only_when_it_reaches_the_head() {
    // a DIV (illegal in Zmmul) on a mispredicted path must not trap
    let div = r(1, 2, 1, 4, 3, 0x33);
    let mut prog = vec![addi(9, 0, 0x40), csrw(0x305, 9), addi(1, 0, 1), b(8, 0, 1, 1), div, addi(4, 0, 4), SPIN];
    prog.resize(16, 0x13);
    prog.push(SPIN);
    let mut c = OooCpu::new(MemMode::Fixed);
    c.load(0, &prog);
    assert!(c.run(1000));
    assert_eq!((c.stats.traps, c.rf[4]), (0, 4), "the wrong-path DIV was flushed");
    // on the real path it traps, with mcause 2 and mtval = the instruction
    let mut prog2 = vec![addi(9, 0, 0x40), csrw(0x305, 9), div, addi(4, 0, 4), SPIN];
    prog2.resize(16, 0x13);
    prog2.push(SPIN);
    let mut c = OooCpu::new(MemMode::Fixed);
    c.load(0, &prog2);
    assert!(c.run(1000));
    assert_eq!((c.stats.traps, c.rf[4], c.sys.csrs.read(0x342), c.sys.csrs.read(0x343)), (1, 0, 2, div));
}

#[test]
fn csr_instructions_serialize() {
    let prog = [addi(1, 0, 0x7f), csrw(0x340, 1), csrr(2, 0x340), addi(3, 2, 1), SPIN];
    let mut c = OooCpu::new(MemMode::Fixed);
    c.load(0, &prog);
    assert!(c.run(500));
    assert_eq!((c.rf[2], c.rf[3]), (0x7f, 0x80));
    assert!(c.stats.stall_serialize > 0, "dispatch waited for the ROB to drain");
}

// ── random programs against the reference ──────────────────────────────
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; (self.0 >> 16) as u32 }
    fn pick(&mut self, n: u32) -> u32 { self.next() % n }
}

fn random_program(rng: &mut Rng) -> Vec<u32> {
    let mut p = vec![lui(10, 1)];                          // x10 = 0x1000: data base
    for k in 1..8 { p.push(addi(k, 0, rng.pick(64) as i32 - 32)); }
    let n = 40 + rng.pick(40) as usize;
    for _ in 0..n {
        let (rd, a, bb) = (1 + rng.pick(9), 1 + rng.pick(9), 1 + rng.pick(9));
        let off = 4 * rng.pick(16) as i32;
        p.push(match rng.pick(12) {
            0..=2 => add(rd, a, bb),
            3 => sub(rd, a, bb),
            4 => addi(rd, a, rng.pick(200) as i32 - 100),
            5 => r(0, bb, a, [1u32, 4, 5, 6, 7][rng.pick(5) as usize], rd, 0x33),
            6 => mulr(rng.pick(4), rd, a, bb),
            7 | 8 => lw(rd, 10, off),
            9 | 10 => sw(a, 10, off),
            _ => b(4 * (2 + rng.pick(3) as i32), bb, a, [0u32, 1, 4, 5, 6, 7][rng.pick(6) as usize]),   // forward skip
        });
    }
    p.extend([0x13, 0x13, 0x13, SPIN]);   // landing pad for the last skips
    p
}

#[test]
fn random_programs_match_the_reference() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut squashed = 0;
    for _ in 0..300 {
        let p = random_program(&mut rng);
        let data: Vec<(u32, u32)> = (0..16).map(|k| (0x1000 + 4 * k, rng.next())).collect();
        for c in run_both(&p, &data, 5000) { squashed += c.stats.squashed; }
    }
    assert!(squashed > 0, "the random branches exercised the flush path");
}

#[test]
fn events_describe_a_cycle() {
    let prog = [addi(1, 0, 1), addi(2, 1, 1), SPIN];
    let mut c = OooCpu::new(MemMode::Fixed);
    c.load(0, &prog);
    let mut seen = Vec::new();
    while !c.halted && c.cycle < 100 { c.step(); seen.extend(c.events.iter().cloned()); }
    for k in ["Fetch", "Dispatch", "Issue", "Broadcast", "Wakeup", "Commit", "Halt"] {
        assert!(seen.iter().any(|e| format!("{e:?}").starts_with(k)), "no {k} event");
    }
    assert!(seen.iter().any(|e| matches!(e, OooEvent::Wakeup { .. })), "x2 waited for x1 and was woken by the CDB");
}

#[test]
fn panel_examples_run_to_the_end() {
    use cpu_sim::sim::ooo::examples::examples;
    for (k, ex) in examples().iter().enumerate() {
        for mode in [MemMode::Fixed, MemMode::Caches] {
            let mut c = OooCpu::new(mode);
            c.load(0, &ex.prog);
            for &(a, v) in &ex.data { c.load(a, &[v]); }
            assert!(c.run(3000), "example {k} '{}' ({mode:?}) did not finish", ex.name);
            match k {
                0 => assert_eq!((c.rf[4], c.rf[5], c.rf[6], c.rf[8]), (41, 42, 42, 37)),
                1 => assert_eq!(c.rf[5], 78),
                2 => assert_eq!((c.rf[3], c.rf[4]), (7, 8)),
                3 => assert_eq!((c.rf[6], c.rf[7]), (0x55, 0x56)),
                4 => assert_eq!((c.rf[2], c.rf[9]), (243, 32)),
                5 => assert_eq!((c.rf[2], c.rf[3], c.rf[20]), (2, 3, 1)),
                6 => assert_eq!((c.rf[10], c.rf[11]), (10, 11)),
                _ => {}
            }
        }
    }
}
