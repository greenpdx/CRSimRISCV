//! Verifies micro-step diff tracking, particularly the Control phase.

use cpu_sim::sim::cpu_state::CpuState;
use cpu_sim::sim::phase::Phase;

#[test]
fn control_phase_lights_up_signals() {
    let mut cpu = CpuState::new();
    // addi x3, x0, 7
    cpu.load_program(&[0x00700193]);

    assert_eq!(cpu.micro_step(), Phase::IrqCheck);
    assert_eq!(cpu.micro_step(), Phase::Fetch);
    assert_eq!(cpu.micro_step(), Phase::Decode);
    assert_eq!(cpu.micro_step(), Phase::Control);

    let active = cpu.last_active();
    println!("After Control, active = {:?}", active);

    // All 8 control signals must transition None → Some on first Control phase.
    for name in ["reg_write","alu_src","mem_read","mem_write","alu_op","wb_sel","pc_sel","imm_sel"] {
        assert!(active.contains(&name), "expected {name} in active set, got {active:?}");
    }

    // And the value of each control signal should match the I-ALU opcode entry.
    let m = &cpu.micro;
    assert_eq!(m.ctrl_reg_write, Some(true));   // ADDI writes back
    assert_eq!(m.ctrl_alu_src,   Some(true));   // ADDI uses immediate
    assert_eq!(m.ctrl_mem_read,  Some(false));
    assert_eq!(m.ctrl_mem_write, Some(false));
    assert_eq!(m.ctrl_alu_op,    Some(2));      // use funct
    assert_eq!(m.ctrl_wb_sel,    Some(0));      // ALU result
    assert_eq!(m.ctrl_pc_sel,    Some(0));      // PC+4
    assert_eq!(m.ctrl_imm_sel,   Some(0));      // I-type
}
