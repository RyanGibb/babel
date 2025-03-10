use crate::index::BabelIndex;
use crate::version::BabelVersion;
use core::fmt::Display;
use pubgrub::{Dependencies, DependencyProvider, Map, Range};
use std::{collections::HashSet, convert::Infallible};

use pubgrub_alpine::deps::AlpinePackage;
use pubgrub_debian::deps::DebianPackage;
use pubgrub_opam::{
    deps::{OpamPackage, VARIABLE_CACHE},
    version::OpamVersion,
};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum BabelPackage {
    Root(Vec<(BabelPackage, Range<BabelVersion>)>),
    Opam(OpamPackage),
    Debian(DebianPackage),
    Alpine(AlpinePackage),
}

impl Display for BabelPackage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BabelPackage::Root(_) => write!(f, "Root"),
            BabelPackage::Opam(pkg) => write!(f, "Opam {}", pkg),
            BabelPackage::Debian(pkg) => write!(f, "Debian {}", pkg),
            BabelPackage::Alpine(pkg) => write!(f, "Alpine {}", pkg),
        }
    }
}

impl BabelIndex {
    pub fn list_versions(&self, package: &BabelPackage) -> impl Iterator<Item = BabelVersion> + '_ {
        let versions: Vec<_> = match package {
            BabelPackage::Root(_) => vec![BabelVersion::Singular],
            BabelPackage::Opam(OpamPackage::Depext(_)) => {
                vec![
                    BabelVersion::Opam(OpamVersion("alpine".to_string())),
                    BabelVersion::Opam(OpamVersion("debian".to_string())),
                ]
            }
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
            BabelPackage::Alpine(pkg) => self
                .alpine
                .list_versions(pkg)
                .map(|x| BabelVersion::Alpine(x))
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
            BabelPackage::Root(deps) => {
                for (package, range) in deps {
                    match package {
                        BabelPackage::Opam(OpamPackage::Var(var)) => {
                            // we reach in to populate Opam's variable cache
                            if let Some(BabelVersion::Opam(ver)) = range.as_singleton() {
                                VARIABLE_CACHE
                                    .lock()
                                    .unwrap()
                                    .entry(var.to_string())
                                    .or_insert_with(HashSet::new)
                                    .insert(ver.clone());
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Dependencies::Available(deps.into_iter().cloned().collect()))
            }
            BabelPackage::Opam(pkg) => {
                if let BabelVersion::Opam(ver) = version {
                    let deps = match pkg {
                        OpamPackage::Depext(depexts) => {
                            let mut map = Map::default();
                            for depext in depexts {
                                // TODO handle virtual packages
                                let OpamVersion(v) = ver;
                                match v.as_str() {
                                    "debian" => {
                                        map.insert(
                                            BabelPackage::Debian(DebianPackage::Base(
                                                depext.to_string(),
                                            )),
                                            Range::<BabelVersion>::full(),
                                        );
                                        map.insert(
                                            BabelPackage::Opam(OpamPackage::Var(
                                                "os-distribution".to_string(),
                                            )),
                                            Range::singleton(BabelVersion::Opam(OpamVersion(
                                                "debian".to_string(),
                                            ))),
                                        );
                                        map.insert(
                                            BabelPackage::Opam(OpamPackage::Var(
                                                "os-family".to_string(),
                                            )),
                                            Range::singleton(BabelVersion::Opam(OpamVersion(
                                                "debian".to_string(),
                                            ))),
                                        );
                                    }
                                    "alpine" => {
                                        map.insert(
                                            BabelPackage::Alpine(AlpinePackage::Base(
                                                depext.to_string(),
                                            )),
                                            Range::<BabelVersion>::full(),
                                        );
                                        map.insert(
                                            BabelPackage::Opam(OpamPackage::Var(
                                                "os-distribution".to_string(),
                                            )),
                                            Range::singleton(BabelVersion::Opam(OpamVersion(
                                                "alpine".to_string(),
                                            ))),
                                        );
                                    }
                                    _ => panic!(),
                                }
                            }
                            Dependencies::Available(map)
                        }
                        _ => {
                            let deps = match self.opam.get_dependencies(pkg, ver)? {
                                Dependencies::Unavailable(m) => Dependencies::Unavailable(m),
                                Dependencies::Available(dc) => Dependencies::Available(
                                    dc.into_iter()
                                        .map(|(p, vs)| {
                                            (
                                                BabelPackage::Opam(p),
                                                vs.into_iter()
                                                    .map(|(s, e)| {
                                                        (
                                                            s.map(|v| BabelVersion::Opam(v)),
                                                            e.map(|v| BabelVersion::Opam(v)),
                                                        )
                                                    })
                                                    .collect(),
                                            )
                                        })
                                        .collect(),
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
                            dc.into_iter()
                                .map(|(p, vs)| {
                                    (
                                        BabelPackage::Debian(p),
                                        vs.into_iter()
                                            .map(|(s, e)| {
                                                (
                                                    s.map(|v| BabelVersion::Debian(v)),
                                                    e.map(|v| BabelVersion::Debian(v)),
                                                )
                                            })
                                            .collect(),
                                    )
                                })
                                .collect(),
                        ),
                    };
                    Ok(deps)
                } else {
                    panic!();
                }
            }
            BabelPackage::Alpine(pkg) => {
                if let BabelVersion::Alpine(ver) = version {
                    let deps = match self.alpine.get_dependencies(pkg, ver)? {
                        Dependencies::Unavailable(m) => Dependencies::Unavailable(m),
                        Dependencies::Available(dc) => Dependencies::Available(
                            dc.into_iter()
                                .map(|(p, vs)| {
                                    (
                                        BabelPackage::Alpine(p),
                                        vs.into_iter()
                                            .map(|(s, e)| {
                                                (
                                                    s.map(|v| BabelVersion::Alpine(v)),
                                                    e.map(|v| BabelVersion::Alpine(v)),
                                                )
                                            })
                                            .collect(),
                                    )
                                })
                                .collect(),
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
