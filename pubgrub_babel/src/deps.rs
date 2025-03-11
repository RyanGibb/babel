use crate::index::BabelIndex;
use crate::version::BabelVersion;
use core::fmt::Display;
use pubgrub::{Dependencies, DependencyProvider, Map, Range};
use std::convert::Infallible;

use pubgrub_alpine::deps::AlpinePackage;
use pubgrub_debian::deps::DebianPackage;
use pubgrub_opam::{deps::OpamPackage, version::OpamVersion};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum BabelPackage {
    Root(Vec<(BabelPackage, Range<BabelVersion>)>),
    Opam(OpamPackage),
    Debian(DebianPackage),
    Alpine(AlpinePackage),
    Platform(PlatformPackage),
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum PlatformPackage {
    OS,
    // Architecture,
}

impl Display for BabelPackage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BabelPackage::Root(_) => write!(f, "Root"),
            BabelPackage::Platform(PlatformPackage::OS) => write!(f, "Platform OS"),
            BabelPackage::Opam(pkg) => write!(f, "Opam {}", pkg),
            BabelPackage::Debian(pkg) => write!(f, "Debian {}", pkg),
            BabelPackage::Alpine(pkg) => write!(f, "Alpine {}", pkg),
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

impl BabelIndex {
    pub fn list_versions(&self, package: &BabelPackage) -> impl Iterator<Item = BabelVersion> + '_ {
        let versions: Vec<_> = match package {
            BabelPackage::Root(_) => vec![BabelVersion::Singular],
            BabelPackage::Opam(OpamPackage::Depext { .. }) => {
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
            BabelPackage::Platform(PlatformPackage::OS) => vec![
                BabelVersion::Platform("debian".to_string()),
                BabelVersion::Platform("alpine".to_string()),
            ],
        };
        if self.version_debug.get() {
            print!("versions of {}", package);
            if versions.len() > 0 {
                print!(": ")
            }
            let mut first = true;
            for version in versions.clone() {
                if !first {
                    print!(", ");
                }
                print!("{}", version);
                first = false;
            }
            println!();
        }
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
                                Range::singleton(BabelVersion::Opam(OpamVersion(
                                    "debian".to_string(),
                                ))),
                            );
                            map.insert(
                                BabelPackage::Opam(OpamPackage::Var("os-family".to_string())),
                                Range::singleton(BabelVersion::Opam(OpamVersion(
                                    "debian".to_string(),
                                ))),
                            );
                            map.insert(
                                BabelPackage::Opam(OpamPackage::Var("os".to_string())),
                                Range::singleton(BabelVersion::Opam(OpamVersion(
                                    "linux".to_string(),
                                ))),
                            );
                        }
                        "alpine" => {
                            map.insert(
                                BabelPackage::Opam(OpamPackage::Var("os-distribution".to_string())),
                                Range::singleton(BabelVersion::Opam(OpamVersion(
                                    "alpine".to_string(),
                                ))),
                            );
                            map.insert(
                                BabelPackage::Opam(OpamPackage::Var("os-family".to_string())),
                                Range::singleton(BabelVersion::Opam(OpamVersion(
                                    "alpine".to_string(),
                                ))),
                            );
                            map.insert(
                                BabelPackage::Opam(OpamPackage::Var("os".to_string())),
                                Range::singleton(BabelVersion::Opam(OpamVersion(
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
                                match v.as_str() {
                                    "debian" => {
                                        if contains_os_condition(formula, "debian") {
                                            map.insert(
                                                BabelPackage::Debian(DebianPackage::Base(
                                                    depext.to_string(),
                                                )),
                                                Range::<BabelVersion>::full(),
                                            );
                                        }
                                    }
                                    "alpine" => {
                                        if contains_os_condition(formula, "alpine") {
                                            map.insert(
                                                BabelPackage::Alpine(AlpinePackage::Base(
                                                    depext.to_string(),
                                                )),
                                                Range::<BabelVersion>::full(),
                                            );
                                        }
                                    }
                                    _ => {}
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
