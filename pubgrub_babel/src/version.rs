use pubgrub_alpine::version::AlpineVersion;
use pubgrub_debian::version::DebianVersion;
use pubgrub_opam::version::OpamVersion;
use std::fmt;

#[derive(Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub enum BabelVersion {
    Singular,
    Platform(String),
    Opam(OpamVersion),
    Debian(DebianVersion),
    Alpine(AlpineVersion),
}

impl fmt::Display for BabelVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BabelVersion::Singular => write!(f, ""),
            BabelVersion::Platform(ver) => write!(f, "{}", ver),
            BabelVersion::Opam(ver) => write!(f, "{}", ver),
            BabelVersion::Debian(ver) => write!(f, "{}", ver),
            BabelVersion::Alpine(ver) => write!(f, "{}", ver),
        }
    }
}
