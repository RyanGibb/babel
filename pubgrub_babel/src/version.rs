use pubgrub_debian::version::DebianVersion;
use pubgrub_opam::version::OpamVersion;
use std::fmt;

#[derive(Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub enum BabelVersion {
    Opam(OpamVersion),
    Debian(DebianVersion),
}

impl fmt::Display for BabelVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BabelVersion::Opam(ver) => write!(f, "{}", ver),
            BabelVersion::Debian(ver) => write!(f, "{}", ver),
        }
    }
}
