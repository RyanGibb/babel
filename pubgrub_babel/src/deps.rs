use crate::index::BabelIndex;
use crate::version::{BabelVersion, BabelVersionSet};
use core::fmt::Display;
use pubgrub::{Dependencies, DependencyProvider, Map, Range};

use pubgrub_alpine::deps::AlpinePackage;
use pubgrub_alpine::version::AlpineVersion;
use pubgrub_cargo::names::Names as CargoPackage;
use pubgrub_cargo::SomeError;
use pubgrub_debian::deps::DebianPackage;
use pubgrub_debian::version::DebianVersion;
use pubgrub_opam::{deps::OpamPackage, version::OpamVersion};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum BabelPackage<'a> {
    Root(Vec<(BabelPackage<'a>, BabelVersionSet)>),
    Opam(OpamPackage),
    Debian(DebianPackage),
    Alpine(AlpinePackage),
    Cargo(CargoPackage<'a>),
    Platform(PlatformPackage),
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum PlatformPackage {
    OS,
    // TODO (not now),
    // Architecture
}

impl<'a> Display for BabelPackage<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BabelPackage::Root(_) => write!(f, "Root"),
            BabelPackage::Platform(PlatformPackage::OS) => write!(f, "Platform OS"),
            BabelPackage::Opam(pkg) => write!(f, "Opam {}", pkg),
            BabelPackage::Debian(pkg) => write!(f, "Debian {}", pkg),
            BabelPackage::Alpine(pkg) => write!(f, "Alpine {}", pkg),
            BabelPackage::Cargo(pkg) => write!(f, "Cargo {}", pkg),
        }
    }
}

/// Checks if a version formula contains a condition for a specific OS
/// Either as os-distribution = "os_name" or os-family = "os_name"
fn contains_os_condition(formula: &pubgrub_opam::index::VersionFormula, os_name: &str) -> bool {
    use pubgrub_opam::index::VersionFormula;
    use pubgrub_opam::parse::RelOp;
    match formula {
        VersionFormula::Comparator { relop, binary } if *relop == RelOp::Eq => {
            if let VersionFormula::Variable(var_name) = &*binary.lhs {
                if var_name == "os-distribution" || var_name == "os-family" {
                    if let VersionFormula::Lit(version) = &*binary.rhs {
                        return version.0 == os_name;
                    }
                }
            }
            if let VersionFormula::Variable(var_name) = &*binary.rhs {
                if var_name == "os-distribution" || var_name == "os-family" {
                    if let VersionFormula::Lit(version) = &*binary.lhs {
                        return version.0 == os_name;
                    }
                }
            }
            false
        }
        VersionFormula::And(binary) => {
            contains_os_condition(&binary.lhs, os_name)
                || contains_os_condition(&binary.rhs, os_name)
        }
        VersionFormula::Or(binary) => {
            contains_os_condition(&binary.lhs, os_name)
                || contains_os_condition(&binary.rhs, os_name)
        }
        _ => false,
    }
}

impl<'a> DependencyProvider for BabelIndex<'a> {
    type P = BabelPackage<'a>;

    type V = BabelVersion;

    type VS = BabelVersionSet;

    type M = String;

    type Err = SomeError;

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
        match package {
            BabelPackage::Cargo(pkg) => {
                let set = match range {
                    BabelVersionSet::Cargo(set) => set,
                    _ => panic!(),
                };
                Ok(self
                    .cargo
                    .choose_version(pkg, set)?
                    .map(|v| BabelVersion::Cargo(v)))
            }
            BabelPackage::Root(_) => Ok(Some(BabelVersion::Singular)),
            BabelPackage::Opam(OpamPackage::Depext { .. }) => Ok(vec![
                BabelVersion::Opam(OpamVersion("alpine".to_string())),
                BabelVersion::Opam(OpamVersion("debian".to_string())),
            ]
            .into_iter()
            .filter(|v| range.contains(v))
            .next()),
            BabelPackage::Opam(pkg) => Ok(self
                .opam
                .list_versions(pkg)
                .map(|x| BabelVersion::Opam(x))
                .filter(|v| range.contains(v))
                .next()),
            BabelPackage::Debian(pkg) => Ok(self
                .debian
                .list_versions(pkg)
                .map(|x| BabelVersion::Debian(x))
                .filter(|v| range.contains(v))
                .next()),
            BabelPackage::Alpine(pkg) => Ok(self
                .alpine
                .list_versions(pkg)
                .map(|x| BabelVersion::Alpine(x))
                .filter(|v| range.contains(v))
                .next()),
            BabelPackage::Platform(PlatformPackage::OS) => Ok(vec![
                BabelVersion::Platform("debian".to_string()),
                BabelVersion::Platform("alpine".to_string()),
            ]
            .into_iter()
            .filter(|v| range.contains(v))
            .next()),
        }
    }

    fn get_dependencies(
        &self,
        package: &BabelPackage<'a>,
        version: &BabelVersion,
    ) -> Result<Dependencies<Self::P, Self::VS, Self::M>, Self::Err> {
        let deps = match package {
            BabelPackage::Root(deps) => {
                Ok(Dependencies::Available(deps.into_iter().cloned().collect()))
            }
            BabelPackage::Platform(PlatformPackage::OS) => {
                let mut map = Map::default();
                match version {
                    BabelVersion::Platform(ver) => match ver.as_str() {
                        "debian" => {
                            map.insert(
                                BabelPackage::Opam(OpamPackage::Var("os-distribution".to_string())),
                                BabelVersionSet::singleton(BabelVersion::Opam(OpamVersion(
                                    "debian".to_string(),
                                ))),
                            );
                            map.insert(
                                BabelPackage::Opam(OpamPackage::Var("os-family".to_string())),
                                BabelVersionSet::singleton(BabelVersion::Opam(OpamVersion(
                                    "debian".to_string(),
                                ))),
                            );
                            map.insert(
                                BabelPackage::Opam(OpamPackage::Var("os".to_string())),
                                BabelVersionSet::singleton(BabelVersion::Opam(OpamVersion(
                                    "linux".to_string(),
                                ))),
                            );
                        }
                        "alpine" => {
                            map.insert(
                                BabelPackage::Opam(OpamPackage::Var("os-distribution".to_string())),
                                BabelVersionSet::singleton(BabelVersion::Opam(OpamVersion(
                                    "alpine".to_string(),
                                ))),
                            );
                            map.insert(
                                BabelPackage::Opam(OpamPackage::Var("os-family".to_string())),
                                BabelVersionSet::singleton(BabelVersion::Opam(OpamVersion(
                                    "alpine".to_string(),
                                ))),
                            );
                            map.insert(
                                BabelPackage::Opam(OpamPackage::Var("os".to_string())),
                                BabelVersionSet::singleton(BabelVersion::Opam(OpamVersion(
                                    "linux".to_string(),
                                ))),
                            );
                        }
                        _ => panic![],
                    },
                    _ => panic![],
                }
                Ok(Dependencies::Available(map))
            }
            BabelPackage::Opam(pkg) => {
                if let BabelVersion::Opam(ver) = version {
                    let deps = match pkg {
                        OpamPackage::Depext { names, formula } => {
                            let mut map = Map::default();
                            for depext in names {
                                let OpamVersion(v) = ver;
                                // TODO handle virtual packages
                                match v.as_str() {
                                    "debian" => {
                                        if contains_os_condition(formula, "debian") {
                                            map.insert(
                                                BabelPackage::Debian(DebianPackage::Base(
                                                    depext.to_string(),
                                                )),
                                                BabelVersionSet::Debian(
                                                    Range::<DebianVersion>::full(),
                                                ),
                                            );
                                        }
                                    }
                                    "alpine" => {
                                        if contains_os_condition(formula, "alpine") {
                                            map.insert(
                                                BabelPackage::Alpine(AlpinePackage::Base(
                                                    depext.to_string(),
                                                )),
                                                BabelVersionSet::Alpine(
                                                    Range::<AlpineVersion>::full(),
                                                ),
                                            );
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            Dependencies::Available(map)
                        }
                        _ => {
                            let deps = match self.opam.get_dependencies(pkg, ver) {
                                Ok(Dependencies::Unavailable(m)) => Dependencies::Unavailable(m),
                                Ok(Dependencies::Available(dc)) => Dependencies::Available(
                                    dc.into_iter()
                                        .map(|(p, vs)| {
                                            (BabelPackage::Opam(p), BabelVersionSet::Opam(vs))
                                        })
                                        .collect(),
                                ),
                                _ => panic!(),
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
                    let deps = match self.debian.get_dependencies(pkg, ver) {
                        Ok(Dependencies::Unavailable(m)) => Dependencies::Unavailable(m),
                        Ok(Dependencies::Available(dc)) => Dependencies::Available(
                            dc.into_iter()
                                .map(|(p, vs)| {
                                    (BabelPackage::Debian(p), BabelVersionSet::Debian(vs))
                                })
                                .collect(),
                        ),
                        _ => panic!(),
                    };
                    Ok(deps)
                } else {
                    panic!();
                }
            }
            BabelPackage::Alpine(pkg) => {
                if let BabelVersion::Alpine(ver) = version {
                    let deps = match self.alpine.get_dependencies(pkg, ver) {
                        Ok(Dependencies::Unavailable(m)) => Dependencies::Unavailable(m),
                        Ok(Dependencies::Available(dc)) => Dependencies::Available(
                            dc.into_iter()
                                .map(|(p, vs)| {
                                    (BabelPackage::Alpine(p), BabelVersionSet::Alpine(vs))
                                })
                                .collect(),
                        ),
                        Err(_) => panic!(),
                    };
                    Ok(deps)
                } else {
                    panic!();
                }
            }
            BabelPackage::Cargo(pkg) => {
                if let BabelVersion::Cargo(ver) = version {
                    let deps = match self.cargo.get_dependencies(pkg, ver)? {
                        Dependencies::Unavailable(m) => Dependencies::Unavailable(m),
                        Dependencies::Available(dc) => Dependencies::Available(
                            dc.into_iter()
                                .map(|(p, vs)| (BabelPackage::Cargo(p), BabelVersionSet::Cargo(vs)))
                                .collect(),
                        ),
                    };
                    Ok(deps)
                } else {
                    panic!();
                }
            }
        };
        if self.debug.get() {
            match &deps {
                Ok(Dependencies::Available(deps)) => {
                    print!("({}, {})", package, version);
                    if deps.len() > 0 {
                        print!(" -> ")
                    }
                    let mut first = true;
                    for (package, range) in deps.clone() {
                        if !first {
                            print!(", ");
                        }
                        print!("({}, {})", package, range);
                        first = false;
                    }
                    println!();
                }
                _ => {}
            }
        }
        deps
    }
}
