//! The forwarding unit shared by the 5-stage pipeline and the barrel
//! processor's EX stage.

/// Where an ALU input operand comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ForwardSrc {
    #[default]
    Regfile,
    ExMa,
    MaWb,
}

/// A stage register that may write a register: EX/MA and MA/WB of both
/// cores.
pub trait RegWriter {
    /// The register it will write (valid, `reg_write`, rd ≠ x0).
    fn writes(&self) -> Option<u8>;
    /// Hardware thread (barrel); 0 for the single-thread pipeline.
    fn thread(&self) -> u8 { 0 }
    /// The value it forwards: EX/MA its ALU result, MA/WB its WB value.
    fn fwd_value(&self) -> u32;
}

/// Pick the source of register `rs` read by an instruction of `thread`:
/// the youngest in-flight write of the same thread, else the regfile.
/// (Different barrel threads have separate register files.)
pub fn forward(rs: u8, thread: u8, ex_ma: &impl RegWriter, ma_wb: &impl RegWriter) -> ForwardSrc {
    if rs == 0 { return ForwardSrc::Regfile; }
    if ex_ma.thread() == thread && ex_ma.writes() == Some(rs) { return ForwardSrc::ExMa; }
    if ma_wb.thread() == thread && ma_wb.writes() == Some(rs) { return ForwardSrc::MaWb; }
    ForwardSrc::Regfile
}

/// The operand value for a forwarding decision.
pub fn forwarded(src: ForwardSrc, regfile: u32, ex_ma: &impl RegWriter, ma_wb: &impl RegWriter) -> u32 {
    match src {
        ForwardSrc::Regfile => regfile,
        ForwardSrc::ExMa => ex_ma.fwd_value(),
        ForwardSrc::MaWb => ma_wb.fwd_value(),
    }
}
