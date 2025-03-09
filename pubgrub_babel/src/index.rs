use pubgrub_debian::index::DebianIndex;
use pubgrub_opam::index::OpamIndex;

pub struct BabelIndex {
    pub opam: OpamIndex,
    pub debian: DebianIndex,
}

impl BabelIndex {
    pub fn new(opam: OpamIndex, debian: DebianIndex) -> Self {
        Self { opam, debian }
    }
}
