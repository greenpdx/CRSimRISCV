//! ActivityTrace: which components fired in which phase during one `step()`.

use crate::sim::phase::Phase;
use crate::data::netlist::ComponentId;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ActivityTrace {
    pub by_phase: BTreeMap<Phase, BTreeSet<ComponentId>>,
}

impl ActivityTrace {
    pub fn new() -> Self { Self::default() }

    pub fn mark(&mut self, phase: Phase, component: ComponentId) {
        self.by_phase.entry(phase).or_default().insert(component);
    }

    pub fn components_in(&self, phase: Phase) -> Option<&BTreeSet<ComponentId>> {
        self.by_phase.get(&phase)
    }
}
