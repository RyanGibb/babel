use pubgrub_alpine::index::AlpineIndex;
use pubgrub_debian::index::DebianIndex;
use pubgrub_opam::index::OpamIndex;

pub struct BabelIndex {
    pub opam: OpamIndex,
    pub debian: DebianIndex,
    pub alpine: AlpineIndex,
}

impl BabelIndex {
    pub fn new(opam: OpamIndex, debian: DebianIndex, alpine: AlpineIndex) -> Self {
        Self {
            opam,
            debian,
            alpine,
        }
    }
}
