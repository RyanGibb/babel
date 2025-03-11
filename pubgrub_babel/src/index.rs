use std::cell::Cell;

use pubgrub_alpine::index::AlpineIndex;
use pubgrub_debian::index::DebianIndex;
use pubgrub_opam::index::OpamIndex;

pub struct BabelIndex {
    pub opam: OpamIndex,
    pub debian: DebianIndex,
    pub alpine: AlpineIndex,
    pub debug: Cell<bool>,
    pub version_debug: Cell<bool>,
}

impl BabelIndex {
    pub fn new(opam: OpamIndex, debian: DebianIndex, alpine: AlpineIndex) -> Self {
        Self {
            opam,
            debian,
            alpine,
            debug: false.into(),
            version_debug: false.into(),
        }
    }
    pub fn set_debug(&self, flag: bool) {
        self.debug.set(flag);
    }

    pub fn set_version_debug(&self, flag: bool) {
        self.version_debug.set(flag);
    }
}
