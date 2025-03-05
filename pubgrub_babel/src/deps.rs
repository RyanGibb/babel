use crate::index::BabelIndex;
use crate::version::BabelVersion;
use core::fmt::Display;
use pubgrub::{Dependencies, DependencyProvider, Map, Range};
use std::convert::Infallible;

use pubgrub_debian::deps::DebianPackage;
use pubgrub_opam::deps::OpamPackage;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum BabelPackage {
    Opam(OpamPackage),
    Debian(DebianPackage),
}

impl Display for BabelPackage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BabelPackage::Opam(pkg) => write!(f, "Opam {}", pkg),
            BabelPackage::Debian(pkg) => write!(f, "Debian {}", pkg),
        }
    }
}

impl BabelIndex {
    pub fn list_versions(&self, package: &BabelPackage) -> impl Iterator<Item = BabelVersion> + '_ {
        let versions: Vec<_> = match package {
            BabelPackage::Opam(pkg) => self
                .opam
                .list_versions(pkg)
                .map(|x| BabelVersion::Opam(x))
                .collect(),
            BabelPackage::Debian(pkg) => self
                .debian
                .list_versions(pkg)
                .map(|x| BabelVersion::Debian(x))
                .collect(),
        };
        versions.into_iter()
    }
}

impl DependencyProvider for BabelIndex {
    type P = BabelPackage;

    type V = BabelVersion;

    type VS = Range<BabelVersion>;

    type M = String;

    type Err = Infallible;

    type Priority = u8;

    fn prioritize(
        &self,
        _package: &Self::P,
        _range: &Self::VS,
        _package_conflicts_counts: &pubgrub::PackageResolutionStatistics,
    ) -> Self::Priority {
        1
    }

    fn choose_version(
        &self,
        package: &Self::P,
        range: &Self::VS,
    ) -> Result<Option<Self::V>, Self::Err> {
        Ok(self
            .list_versions(package)
            .filter(|v| range.contains(v))
            .next())
    }

    fn get_dependencies(
        &self,
        package: &BabelPackage,
        version: &BabelVersion,
    ) -> Result<Dependencies<Self::P, Self::VS, Self::M>, Self::Err> {
        match package {
            BabelPackage::Opam(pkg) => {
                if let BabelVersion::Opam(ver) = version {
                    let deps = match pkg {
                        OpamPackage::Depext(depexts) => {
                            let mut map = Map::default();
                            for depext in depexts {
                                // TODO handle virtual packages
                                map.insert(BabelPackage::Debian(DebianPackage::Base(depext.to_string())), Range::<BabelVersion>::full());
                            };
                            Dependencies::Available(map)
                        }
                        _ => {
                            let deps = match self.opam.get_dependencies(pkg, ver)? {
                                Dependencies::Unavailable(m) => Dependencies::Unavailable(m),
                                Dependencies::Available(dc) => Dependencies::Available(
                                    dc.into_iter().map(|(p, vs)| (BabelPackage::Opam(p), vs.into_iter().map(|(s, e)| (s.map(|v| BabelVersion::Opam(v)), e.map(|v| BabelVersion::Opam(v)))).collect())).collect(),
                                ),
                            };
                            deps
                        }
                    };
                    Ok(deps)
                } else {
                    panic!();
                }
            }
            BabelPackage::Debian(pkg) => {
                if let BabelVersion::Debian(ver) = version {
                    let deps = match self.debian.get_dependencies(pkg, ver)? {
                        Dependencies::Unavailable(m) => Dependencies::Unavailable(m),
                        Dependencies::Available(dc) => Dependencies::Available(
                            dc.into_iter().map(|(p, vs)| (BabelPackage::Debian(p), vs.into_iter().map(|(s, e)| (s.map(|v| BabelVersion::Debian(v)), e.map(|v| BabelVersion::Debian(v)))).collect())).collect(),
                        ),
                    };
                    Ok(deps)
                } else {
                    panic!();
                }
            }
        }
    }
}
