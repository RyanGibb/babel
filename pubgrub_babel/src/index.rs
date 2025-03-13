use std::cell::Cell;

use pubgrub_alpine::index::AlpineIndex;
use pubgrub_cargo::Index as CargoIndex;
use pubgrub_debian::index::DebianIndex;
use pubgrub_opam::index::OpamIndex;

pub struct BabelIndex<'a> {
    pub opam: OpamIndex,
    pub debian: DebianIndex,
    pub alpine: AlpineIndex,
    pub cargo: CargoIndex<'a>,
    pub debug: Cell<bool>,
    pub version_debug: Cell<bool>,
}

impl<'a> BabelIndex<'a> {
    pub fn new(
        opam: OpamIndex,
        debian: DebianIndex,
        alpine: AlpineIndex,
        cargo: CargoIndex<'a>,
    ) -> Self {
        Self {
            opam,
            debian,
            alpine,
            cargo,
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
