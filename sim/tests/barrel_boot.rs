//! Boot sequence test: T0 initializes CSRs, DMAs I-SRAMs for
//! slots 1 and 2, FORKs them, DMAs slot 0's I-SRAM, enables IRQs,
//! FORKs general workers 3..7, then enters workload.

use cpu_sim::sim::barrel::{
    dma::{DMA_SRC, DMA_DST, DMA_COUNT, DMA_TRIGGER, DMA_STATUS},
    BarrelState, ISRAM_BASE, ISRAM_BYTES_PER_SLOT, N_THREADS,
};

const NOP: u32 = 0x00000013;

fn addi(rd: u32, rs1: u32, imm: i32) -> u32 {
    (((imm as u32) & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x13
}
fn lui(rd: u32, imm: u32) -> u32 { (imm << 12) | (rd << 7) | 0x37 }
fn sw(rs1: u32, rs2: u32, off: i32) -> u32 {
    let imm = off as u32 & 0xfff;
    (((imm >> 5) & 0x7f) << 25) | (rs2 << 20) | (rs1 << 15) | (0b010 << 12) | (((imm) & 0x1f) << 7) | 0x23
}
fn lw(rd: u32, rs1: u32, off: i32) -> u32 {
    (((off as u32) & 0xfff) << 20) | (rs1 << 15) | (0b010 << 12) | (rd << 7) | 0x03
}
fn fork(rd: u32, rs1: u32, rs2: u32) -> u32 {
    (0x00 << 25) | (rs2 << 20) | (rs1 << 15) | (0 << 12) | (rd << 7) | 0x0b
}
fn done() -> u32 { (0x02 << 25) | 0x0b }
fn jal_spin() -> u32 { 0x0000006f }

/// Helper: build a DMA "copy N words from ext_imem[src_word] to
/// slot's I-SRAM" instruction sequence. Uses t3 (x28) as scratch
/// for the DMA base address, t4 (x29) as scratch for values.
/// Returns a Vec of instruction words.
fn dma_sequence(src_byte: u32, dst_slot: u32, word_count: u32) -> Vec<u32> {
    // lui t3, DMA_BASE >> 12      ; t3 = 0x40001000
    // addi t4, x0, src_byte       ; (if fits in 12-bit)
    // sw t4, 0(t3)                 ; DMA_SRC = src_byte
    // addi t4, x0, dst_slot
    // sw t4, 4(t3)                 ; DMA_DST_SLOT = dst_slot
    // addi t4, x0, word_count
    // sw t4, 8(t3)                 ; DMA_COUNT = word_count
    // sw x0, 12(t3)               ; DMA_TRIGGER (write anything)
    vec![
        lui(28, 0x40001),                               // t3 = 0x40001000
        addi(29, 0, src_byte as i32),                   // t4 = src_byte
        sw(28, 29, 0x00),                               // DMA_SRC
        addi(29, 0, dst_slot as i32),                   // t4 = dst_slot
        sw(28, 29, 0x04),                               // DMA_DST
        addi(29, 0, word_count as i32),                 // t4 = word_count
        sw(28, 29, 0x08),                               // DMA_COUNT
        sw(28, 0, 0x0C),                                // DMA_TRIGGER (val=0, doesn't matter)
    ]
}

#[test]
fn barrel_boot_sequence_dma_and_fork() {
    // Build the ROM with:
    //   - Boot code for T0 at 0x000
    //   - Handler code for slot 1 at ext ROM offset 0x100 (word 64)
    //   - Handler code for slot 2 at ext ROM offset 0x140 (word 80)
    //   - Worker code for slots 3..7 at ext ROM offset 0x200 (word 128)
    let mut rom = vec![NOP; 256];

    // ── Handler code for slot 1 (will be DMA'd into I-SRAM) ──
    // Located at ext_imem word 64 = byte 0x100
    rom[64] = addi(6, 0, 0x11);   // x6 = 0x11 (marker)
    rom[65] = jal_spin();

    // ── Handler code for slot 2 ──
    // Located at ext_imem word 80 = byte 0x140
    rom[80] = addi(6, 0, 0x22);
    rom[81] = jal_spin();

    // ── Worker code for slots 3..7 ──
    // Located at ext_imem word 128 = byte 0x200
    rom[128] = addi(5, 5, 1);     // increment counter
    rom[129] = jal_spin();

    // ── T0 boot code at 0x000 ──
    let mut boot: Vec<u32> = Vec::new();

    // Step 1: Global CSR init (placeholder — just a NOP for now)
    boot.push(NOP);

    // Step 3: DMA slot 1's I-SRAM from ext ROM @ 0x100
    boot.extend(dma_sequence(0x100, 1, 2));

    // Step 4: DMA slot 2's I-SRAM from ext ROM @ 0x140
    boot.extend(dma_sequence(0x140, 2, 2));

    // Step 5: Wait for DMA (instant in our model, but read status)
    boot.push(NOP);  // placeholder

    // Step 6: FORK pinned slots 1 and 2 with I-SRAM entry points.
    // Slot 1 entry = ISRAM_BASE + 1 * ISRAM_BYTES_PER_SLOT = 0xF0004000
    // Need to build this address. Use lui + addi.
    // 0xF0004000: lui = 0xF0004, but that's > 20 bits. Hmm.
    // 0xF0004000 >> 12 = 0xF0004. lui takes top 20 bits. 0xF0004 = 20 bits. ✓
    boot.push(lui(10, 0xF0004));                // a0 = 0xF0004000 (slot 1 ISRAM)
    boot.push(addi(11, 0, 0));                  // a1 = 0 (arg)
    boot.push(fork(5, 10, 11));                 // fork slot 1

    boot.push(lui(10, 0xF0008));                // a0 = 0xF0008000 (slot 2 ISRAM)
    boot.push(addi(11, 0, 0));
    boot.push(fork(5, 10, 11));                 // fork slot 2

    // Step 2 (reordered): DMA slot 0's I-SRAM (T0's own)
    // We skip this in this test — T0 stays on ext ROM.

    // Step 8: Enable IRQs (placeholder NOP)
    boot.push(NOP);

    // Step 7: FORK general workers 3..7 at ext ROM 0x200
    // 0x200 fits in 12 bits.
    for _ in 3..=7 {
        boot.push(addi(10, 0, 0x200));          // a0 = 0x200
        boot.push(addi(11, 0, 0));
        boot.push(fork(5, 10, 11));
    }

    // Step 9: T0 spins
    boot.push(jal_spin());

    // Place boot code at start of ROM
    for (i, &w) in boot.iter().enumerate() {
        if i < rom.len() { rom[i] = w; }
    }

    // ── Run the barrel ──
    let mut cpu = BarrelState::new();
    cpu.load_program(&rom);
    cpu.prewarm_icache();

    // Boot mode: only T0 active
    cpu.threads[0].active = true;
    cpu.threads[0].pc = 0;
    cpu.threads[0].priority = 3;

    // Step many cycles to let boot + fork + execution happen.
    for _ in 0..500 { cpu.step(); }

    // ── Verify ──

    // Slot 1 should have been forked and run from I-SRAM.
    assert!(cpu.threads[1].active || cpu.read_thread_register(1, 6) == 0x11,
            "slot 1 should have run from I-SRAM (x6=0x{:x})", cpu.read_thread_register(1, 6));
    assert_eq!(cpu.read_thread_register(1, 6), 0x11,
               "slot 1 handler wrote marker 0x11");

    // Slot 2 should have run from I-SRAM.
    assert_eq!(cpu.read_thread_register(2, 6), 0x22,
               "slot 2 handler wrote marker 0x22");

    // Slots 3..7 should have been forked and run from ext ROM.
    for s in 3..=7u8 {
        let v = cpu.read_thread_register(s, 5);
        assert!(v > 0, "slot {} worker should have incremented x5 (got {})", s, v);
    }

    // DMA should have completed at least 2 transfers.
    assert!(cpu.dma.transfers >= 2,
            "expected >=2 DMA transfers, got {}", cpu.dma.transfers);

    // Verify I-SRAM for slot 1 has the handler code.
    assert_eq!(cpu.isram_read(1, 0), addi(6, 0, 0x11),
               "slot 1 I-SRAM[0] should have the DMA'd instruction");
}

#[test]
fn barrel_dma_register_read_write() {
    // Verify DMA registers are accessible via load/store.
    let mut cpu = BarrelState::new();
    cpu.threads[0].active = true;
    cpu.threads[0].pc = 0;
    cpu.threads[0].priority = 1;
    // Spinners on other slots.
    for s in 1..N_THREADS {
        cpu.threads[s].active = true;
        cpu.threads[s].pc = 0x40;
        cpu.threads[s].priority = 1;
    }
    let mut rom = vec![NOP; 32];
    // Write DMA_SRC = 0x42 via store
    rom[0]  = lui(28, 0x40001);        // t3 = 0x40001000
    rom[1]  = addi(29, 0, 0x42);       // t4 = 0x42
    rom[2]  = sw(28, 29, 0);           // sw t4, 0(t3) → DMA_SRC=0x42
    // Read DMA_SRC back into x7
    rom[3]  = lw(7, 28, 0);            // lw x7, 0(t3) → should be 0x42
    rom[4]  = jal_spin();
    rom[16] = jal_spin();              // spinners at 0x40
    cpu.load_program(&rom);
    cpu.prewarm_icache();

    for _ in 0..200 { cpu.step(); }
    assert_eq!(cpu.dma.src, 0x42, "DMA_SRC register should hold 0x42");
    assert_eq!(cpu.read_thread_register(0, 7), 0x42, "load from DMA_SRC should return 0x42");
}

#[test]
fn barrel_dma_copies_to_isram() {
    // Direct DMA test: write DMA regs to copy 4 words from
    // ext_imem[0] into slot 2's I-SRAM.
    let mut cpu = BarrelState::new();
    cpu.ext_imem[0] = 0xAAAA_0001;
    cpu.ext_imem[1] = 0xBBBB_0002;
    cpu.ext_imem[2] = 0xCCCC_0003;
    cpu.ext_imem[3] = 0xDDDD_0004;

    // Direct register poke (bypass the pipeline).
    cpu.dma.write_reg(DMA_SRC, 0, &cpu.ext_imem.clone(), &mut cpu.local_isram);
    cpu.dma.write_reg(DMA_DST, 2, &cpu.ext_imem.clone(), &mut cpu.local_isram);
    cpu.dma.write_reg(DMA_COUNT, 4, &cpu.ext_imem.clone(), &mut cpu.local_isram);
    cpu.dma.write_reg(DMA_TRIGGER, 0, &cpu.ext_imem.clone(), &mut cpu.local_isram);

    assert_eq!(cpu.isram_read(2, 0), 0xAAAA_0001);
    assert_eq!(cpu.isram_read(2, 4), 0xBBBB_0002);
    assert_eq!(cpu.isram_read(2, 8), 0xCCCC_0003);
    assert_eq!(cpu.isram_read(2, 12), 0xDDDD_0004);
    assert_eq!(cpu.dma.transfers, 1);
}
