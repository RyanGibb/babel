use pubgrub::{Range, VersionSet};
use pubgrub_alpine::version::AlpineVersion;
use pubgrub_cargo::rc_semver_pubgrub::RcSemverPubgrub;
use pubgrub_debian::version::DebianVersion;
use pubgrub_opam::version::OpamVersion;
use semver::Version as CargoVersion;
use std::fmt;

#[derive(Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub enum BabelVersion {
    Babel(String),
    Opam(OpamVersion),
    Debian(DebianVersion),
    Alpine(AlpineVersion),
    Cargo(CargoVersion),
}

impl fmt::Display for BabelVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BabelVersion::Babel(ver) => write!(f, "{}", ver),
            BabelVersion::Opam(ver) => write!(f, "{}", ver),
            BabelVersion::Debian(ver) => write!(f, "{}", ver),
            BabelVersion::Alpine(ver) => write!(f, "{}", ver),
            BabelVersion::Cargo(ver) => write!(f, "{}", ver),
        }
    }
}

#[derive(Clone, Hash, Debug)]
pub enum BabelVersionSet {
    Empty,
    Full,
    Babel(Range<String>),
    Opam(Range<OpamVersion>),
    Debian(Range<DebianVersion>),
    Alpine(Range<AlpineVersion>),
    Cargo(RcSemverPubgrub),
}

impl BabelVersionSet {
    pub fn empty() -> Self {
        BabelVersionSet::Empty
    }

    pub fn singleton(v: BabelVersion) -> Self {
        match v {
            BabelVersion::Babel(ver) => BabelVersionSet::Babel(Range::singleton(ver)),
            BabelVersion::Opam(ver) => BabelVersionSet::Opam(Range::singleton(ver)),
            BabelVersion::Debian(ver) => BabelVersionSet::Debian(Range::singleton(ver)),
            BabelVersion::Alpine(ver) => BabelVersionSet::Alpine(Range::singleton(ver)),
            BabelVersion::Cargo(ver) => BabelVersionSet::Cargo(RcSemverPubgrub::singleton(ver)),
        }
    }

    pub fn complement(&self) -> Self {
        match self {
            BabelVersionSet::Empty => BabelVersionSet::Full,
            BabelVersionSet::Full => BabelVersionSet::Empty,
            BabelVersionSet::Babel(set) => BabelVersionSet::Babel(Range::complement(set)),
            BabelVersionSet::Opam(set) => BabelVersionSet::Opam(Range::complement(set)),
            BabelVersionSet::Debian(set) => BabelVersionSet::Debian(Range::complement(set)),
            BabelVersionSet::Alpine(set) => BabelVersionSet::Alpine(Range::complement(set)),
            BabelVersionSet::Cargo(set) => BabelVersionSet::Cargo(set.complement()),
        }
    }

    pub fn intersection(&self, other: &Self) -> Self {
        match (self, other) {
            (BabelVersionSet::Empty, _) | (_, BabelVersionSet::Empty) => BabelVersionSet::Empty,
            (BabelVersionSet::Full, s) | (s, BabelVersionSet::Full) => s.clone(),
            (BabelVersionSet::Babel(set), BabelVersionSet::Babel(other_set)) => {
                BabelVersionSet::Babel(set.intersection(other_set))
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
            _ => BabelVersionSet::Empty,
        }
    }

    pub fn contains(&self, v: &BabelVersion) -> bool {
        match (self, v) {
            (BabelVersionSet::Full, _) => true,
            (BabelVersionSet::Babel(set), BabelVersion::Babel(ver)) => set.contains(ver),
            (BabelVersionSet::Opam(set), BabelVersion::Opam(ver)) => set.contains(ver),
            (BabelVersionSet::Debian(set), BabelVersion::Debian(ver)) => set.contains(ver),
            (BabelVersionSet::Alpine(set), BabelVersion::Alpine(ver)) => set.contains(ver),
            (BabelVersionSet::Cargo(set), BabelVersion::Cargo(ver)) => set.contains(ver),
            _ => false,
        }
    }

    pub fn full() -> Self {
        BabelVersionSet::Full
    }

    pub fn union(&self, other: &Self) -> Self {
        match (self, other) {
            (BabelVersionSet::Full, s) | (s, BabelVersionSet::Full) => s.clone(),
            (BabelVersionSet::Empty, _) | (_, BabelVersionSet::Empty) => BabelVersionSet::Empty,
            (BabelVersionSet::Babel(set), BabelVersionSet::Babel(other_set)) => {
                BabelVersionSet::Babel(set.union(other_set))
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
        self.intersection(other) == Self::empty()
    }

    pub fn subset_of(&self, other: &Self) -> bool {
        self == &self.intersection(other)
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

impl PartialEq for BabelVersionSet {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (BabelVersionSet::Full, BabelVersionSet::Full)
            | (BabelVersionSet::Empty, BabelVersionSet::Empty) => true,
            (BabelVersionSet::Full, o) | (o, BabelVersionSet::Full) => match o {
                BabelVersionSet::Babel(set) => set == &Range::<String>::full(),
                BabelVersionSet::Opam(set) => set == &Range::<OpamVersion>::full(),
                BabelVersionSet::Debian(set) => set == &Range::<DebianVersion>::full(),
                BabelVersionSet::Alpine(set) => set == &Range::<AlpineVersion>::full(),
                BabelVersionSet::Cargo(set) => set == &RcSemverPubgrub::full(),
                _ => false,
            },
            (BabelVersionSet::Empty, o) | (o, BabelVersionSet::Empty) => {
                match o {
                    BabelVersionSet::Babel(set) => set == &Range::<String>::empty(),
                    BabelVersionSet::Opam(set) => set == &Range::<OpamVersion>::empty(),
                    BabelVersionSet::Debian(set) => set == &Range::<DebianVersion>::empty(),
                    BabelVersionSet::Alpine(set) => set == &Range::<AlpineVersion>::empty(),
                    BabelVersionSet::Cargo(set) => set == &RcSemverPubgrub::empty(),
                    _ => false,
                }
            }
            (BabelVersionSet::Babel(set), BabelVersionSet::Babel(other_set)) => set == other_set,
            (BabelVersionSet::Opam(set), BabelVersionSet::Opam(other_set)) => set == other_set,
            (BabelVersionSet::Debian(set), BabelVersionSet::Debian(other_set)) => set == other_set,
            (BabelVersionSet::Alpine(set), BabelVersionSet::Alpine(other_set)) => set == other_set,
            (BabelVersionSet::Cargo(set), BabelVersionSet::Cargo(other_set)) => set == other_set,
            _ => self == other,
        }
    }
}

impl Eq for BabelVersionSet {}

impl fmt::Display for BabelVersionSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BabelVersionSet::Empty => write!(f, "Empty"),
            BabelVersionSet::Full => write!(f, "Full"),
            BabelVersionSet::Babel(set) => write!(f, "{}", set),
            BabelVersionSet::Opam(set) => write!(f, "{}", set),
            BabelVersionSet::Debian(set) => write!(f, "{}", set),
            BabelVersionSet::Alpine(set) => write!(f, "{}", set),
            BabelVersionSet::Cargo(set) => write!(f, "{}", set),
        }
    }
}
