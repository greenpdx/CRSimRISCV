//! Mirrors `src/types/cpu.ts` — shared types for the simulator.
//! TODO: port Register, MemoryWord, BusState, ControlSignalState, Port, Net, ComponentDescriptor, Netlist.

pub type ComponentId = &'static str;
pub type NetId = &'static str;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalState { Idle, Active }
