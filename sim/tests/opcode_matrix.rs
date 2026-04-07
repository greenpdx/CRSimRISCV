//! Verifies the (Phase → active component set) matrix per opcode class.

use cpu_sim::sim::cpu_state::CpuState;
use cpu_sim::verify::expected;
use cpu_sim::verify::programs::CANONICAL;

fn expected_for(class: &str) -> cpu_sim::sim::trace::ActivityTrace {
    match class {
        "R-type" => expected::r_type(),
        "I-ALU"  => expected::i_alu(),
        "Load"   => expected::load(),
        "Store"  => expected::store(),
        "Branch" => expected::branch(),
        "LUI"    => expected::lui(),
        "AUIPC"  => expected::auipc(),
        "JAL"    => expected::jal(),
        "JALR"   => expected::jalr(),
        "CSR"    => expected::csr(),
        other    => panic!("no expected matrix for class {other}"),
    }
}

#[test]
fn opcode_phase_matrix() {
    let mut failures = Vec::new();

    for canon in CANONICAL {
        let mut cpu = CpuState::new();
        cpu.load_word(0, canon.instr);
        // Seed rs1 with 0x0001_0000 (RAM base) for Load/Store so the address path
        // hits RAM, not I/O. rs2 = 1.
        cpu.write_register(5, 0);
        cpu.write_register(6, 0x0001_0000);
        cpu.write_register(7, 1);

        let got = cpu.step_traced();
        let want = expected_for(canon.class);

        if got != want {
            failures.push(format!(
                "FAIL [{}] {}\n  expected: {:#?}\n  got:      {:#?}",
                canon.class, canon.asm, want.by_phase, got.by_phase
            ));
        }
    }

    if !failures.is_empty() {
        panic!("\n{}\n", failures.join("\n\n"));
    }
}
