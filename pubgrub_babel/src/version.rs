use pubgrub::{Range, VersionSet};
use pubgrub_alpine::version::AlpineVersion;
use pubgrub_cargo::rc_semver_pubgrub::RcSemverPubgrub;
use pubgrub_debian::version::DebianVersion;
use pubgrub_opam::version::OpamVersion;
use semver::Version as CargoVersion;
use std::fmt;

#[derive(Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub enum BabelVersion {
    Singular,
    Platform(String),
    Opam(OpamVersion),
    Debian(DebianVersion),
    Alpine(AlpineVersion),
    Cargo(CargoVersion),
}

impl fmt::Display for BabelVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BabelVersion::Singular => write!(f, ""),
            BabelVersion::Platform(ver) => write!(f, "{}", ver),
            BabelVersion::Opam(ver) => write!(f, "{}", ver),
            BabelVersion::Debian(ver) => write!(f, "{}", ver),
            BabelVersion::Alpine(ver) => write!(f, "{}", ver),
            BabelVersion::Cargo(ver) => write!(f, "{}", ver),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub enum BabelVersionSet {
    Singular,
    Platform(Range<String>),
    Opam(Range<OpamVersion>),
    Debian(Range<DebianVersion>),
    Alpine(Range<AlpineVersion>),
    Cargo(RcSemverPubgrub),
}

impl BabelVersionSet {
    pub fn empty() -> Self {
        // TODO
        BabelVersionSet::Platform(Range::empty())
    }

    pub fn singleton(v: BabelVersion) -> Self {
        match v {
            BabelVersion::Singular => BabelVersionSet::Singular,
            BabelVersion::Platform(ver) => BabelVersionSet::Platform(Range::singleton(ver)),
            BabelVersion::Opam(ver) => BabelVersionSet::Opam(Range::singleton(ver)),
            BabelVersion::Debian(ver) => BabelVersionSet::Debian(Range::singleton(ver)),
            BabelVersion::Alpine(ver) => BabelVersionSet::Alpine(Range::singleton(ver)),
            BabelVersion::Cargo(ver) => BabelVersionSet::Cargo(RcSemverPubgrub::singleton(ver)),
        }
    }

    pub fn complement(&self) -> Self {
        match self {
            BabelVersionSet::Singular => BabelVersionSet::Singular,
            BabelVersionSet::Platform(set) => BabelVersionSet::Platform(Range::complement(set)),
            BabelVersionSet::Opam(set) => BabelVersionSet::Opam(Range::complement(set)),
            BabelVersionSet::Debian(set) => BabelVersionSet::Debian(Range::complement(set)),
            BabelVersionSet::Alpine(set) => BabelVersionSet::Alpine(Range::complement(set)),
            BabelVersionSet::Cargo(set) => BabelVersionSet::Cargo(set.complement()),
        }
    }

    pub fn intersection(&self, other: &Self) -> Self {
        match (self, other) {
            (BabelVersionSet::Singular, BabelVersionSet::Singular) => BabelVersionSet::Singular,
            (BabelVersionSet::Platform(set), BabelVersionSet::Platform(other_set)) => {
                BabelVersionSet::Platform(set.intersection(other_set))
            }
            (BabelVersionSet::Opam(set), BabelVersionSet::Opam(other_set)) => {
                BabelVersionSet::Opam(set.intersection(other_set))
            }
            (BabelVersionSet::Debian(set), BabelVersionSet::Debian(other_set)) => {
                BabelVersionSet::Debian(set.intersection(other_set))
            }
            (BabelVersionSet::Alpine(set), BabelVersionSet::Alpine(other_set)) => {
                BabelVersionSet::Alpine(set.intersection(other_set))
            }
            (BabelVersionSet::Cargo(set), BabelVersionSet::Cargo(other_set)) => {
                BabelVersionSet::Cargo(set.intersection(other_set))
            }
            _ => panic!(),
        }
    }

    pub fn contains(&self, v: &BabelVersion) -> bool {
        match (self, v) {
            (BabelVersionSet::Singular, BabelVersion::Singular) => true,
            (BabelVersionSet::Platform(set), BabelVersion::Platform(ver)) => set.contains(ver),
            (BabelVersionSet::Opam(set), BabelVersion::Opam(ver)) => set.contains(ver),
            (BabelVersionSet::Debian(set), BabelVersion::Debian(ver)) => set.contains(ver),
            (BabelVersionSet::Alpine(set), BabelVersion::Alpine(ver)) => set.contains(ver),
            (BabelVersionSet::Cargo(set), BabelVersion::Cargo(ver)) => set.contains(ver),
            _ => panic!(),
        }
    }

    pub fn full() -> Self {
        todo!()
    }

    pub fn union(&self, other: &Self) -> Self {
        match (self, other) {
            (BabelVersionSet::Singular, BabelVersionSet::Singular) => BabelVersionSet::Singular,
            (BabelVersionSet::Platform(set), BabelVersionSet::Platform(other_set)) => {
                BabelVersionSet::Platform(set.union(other_set))
            }
            (BabelVersionSet::Opam(set), BabelVersionSet::Opam(other_set)) => {
                BabelVersionSet::Opam(set.union(other_set))
            }
            (BabelVersionSet::Debian(set), BabelVersionSet::Debian(other_set)) => {
                BabelVersionSet::Debian(set.union(other_set))
            }
            (BabelVersionSet::Alpine(set), BabelVersionSet::Alpine(other_set)) => {
                BabelVersionSet::Alpine(set.union(other_set))
            }
            (BabelVersionSet::Cargo(set), BabelVersionSet::Cargo(other_set)) => {
                BabelVersionSet::Cargo(set.union(other_set))
            }
            _ => panic!(),
        }
    }

    pub fn is_disjoint(&self, other: &Self) -> bool {
        match (self, other) {
            (BabelVersionSet::Singular, BabelVersionSet::Singular) => false,
            (BabelVersionSet::Platform(set), BabelVersionSet::Platform(other_set)) => {
                set.is_disjoint(other_set)
            }
            (BabelVersionSet::Opam(set), BabelVersionSet::Opam(other_set)) => {
                set.is_disjoint(other_set)
            }
            (BabelVersionSet::Debian(set), BabelVersionSet::Debian(other_set)) => {
                set.is_disjoint(other_set)
            }
            (BabelVersionSet::Alpine(set), BabelVersionSet::Alpine(other_set)) => {
                set.is_disjoint(other_set)
            }
            (BabelVersionSet::Cargo(set), BabelVersionSet::Cargo(other_set)) => {
                set.is_disjoint(other_set)
            }
            _ => panic!(),
        }
    }

    pub fn subset_of(&self, other: &Self) -> bool {
        match (self, other) {
            (BabelVersionSet::Singular, BabelVersionSet::Singular) => true,
            (BabelVersionSet::Platform(set), BabelVersionSet::Platform(other_set)) => {
                set.subset_of(other_set)
            }
            (BabelVersionSet::Opam(set), BabelVersionSet::Opam(other_set)) => {
                set.subset_of(other_set)
            }
            (BabelVersionSet::Debian(set), BabelVersionSet::Debian(other_set)) => {
                set.subset_of(other_set)
            }
            (BabelVersionSet::Alpine(set), BabelVersionSet::Alpine(other_set)) => {
                set.subset_of(other_set)
            }
            (BabelVersionSet::Cargo(set), BabelVersionSet::Cargo(other_set)) => {
                set.subset_of(other_set)
            }
            _ => panic!(),
        }
    }
}

impl VersionSet for BabelVersionSet {
    type V = BabelVersion;

    fn empty() -> Self {
        Self::empty()
    }

    fn full() -> Self {
        Self::full()
    }

    fn singleton(v: Self::V) -> Self {
        Self::singleton(v)
    }

    fn complement(&self) -> Self {
        self.complement()
    }

    fn intersection(&self, other: &Self) -> Self {
        self.intersection(other)
    }

    fn contains(&self, v: &Self::V) -> bool {
        self.contains(v)
    }

    fn union(&self, other: &Self) -> Self {
        self.union(other)
    }

    fn is_disjoint(&self, other: &Self) -> bool {
        self.is_disjoint(other)
    }

    fn subset_of(&self, other: &Self) -> bool {
        self.subset_of(other)
    }
}

impl fmt::Display for BabelVersionSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BabelVersionSet::Singular => write!(f, ""),
            BabelVersionSet::Platform(set) => write!(f, "{}", set),
            BabelVersionSet::Opam(set) => write!(f, "{}", set),
            BabelVersionSet::Debian(set) => write!(f, "{}", set),
            BabelVersionSet::Alpine(set) => write!(f, "{}", set),
            BabelVersionSet::Cargo(set) => write!(f, "{}", set),
        }
    }
}
