use crate::index::{AlpineIndex, Dependency};
use crate::version::AlpineVersion;
use core::fmt::Display;
use pubgrub::{Dependencies, DependencyConstraints, DependencyProvider, Map, Range};
use std::convert::Infallible;
use std::str::FromStr;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum AlpinePackage {
    Root(Vec<(AlpinePackage, Range<AlpineVersion>)>),
    Base(String),
}

impl FromStr for AlpinePackage {
    type Err = String;
    fn from_str(pkg: &str) -> Result<Self, Self::Err> {
        let mut pkg_parts = pkg.split('/');
        match (pkg_parts.next(), pkg_parts.next()) {
            (Some(base), None) => Ok(AlpinePackage::Base(base.to_string())),
            _ => Err(format!("{} is not a valid package name", pkg)),
        }
    }
}

impl Display for AlpinePackage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlpinePackage::Root(_) => write!(f, "Root"),
            AlpinePackage::Base(pkg) => write!(f, "{}", pkg),
        }
    }
}

impl AlpineIndex {
    pub fn list_versions(
        &self,
        package: &AlpinePackage,
    ) -> impl Iterator<Item = AlpineVersion> + '_ {
        let versions = match package {
            AlpinePackage::Root(_) => vec![AlpineVersion("".to_string())],
            AlpinePackage::Base(pkg) => self.available_versions(pkg),
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
        };
        versions.into_iter()
    }
}

impl DependencyProvider for AlpineIndex {
    type P = AlpinePackage;

    type V = AlpineVersion;

    type VS = Range<AlpineVersion>;

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
        package: &AlpinePackage,
        version: &AlpineVersion,
    ) -> Result<Dependencies<Self::P, Self::VS, Self::M>, Self::Err> {
        match package {
            AlpinePackage::Root(deps) => {
                Ok(Dependencies::Available(deps.into_iter().cloned().collect()))
            }
            AlpinePackage::Base(pkg) => {
                let all_versions = match self.packages.get(pkg) {
                    None => return Ok(Dependencies::Unavailable("".to_string())),
                    Some(all_versions) => all_versions,
                };
                let dependencies = match all_versions.get(version) {
                    None => return Ok(Dependencies::Unavailable("".to_string())),
                    Some(d) => d,
                };
                let deps = from_dependencies(dependencies);
                if self.debug.get() {
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
                Ok(Dependencies::Available(deps))
            }
        }
    }
}

pub fn from_dependencies(
    dependencies: &Vec<Dependency>,
) -> DependencyConstraints<AlpinePackage, Range<AlpineVersion>> {
    let mut map = Map::default();
    for dep in dependencies.clone() {
        map.insert(AlpinePackage::Base(dep.name.clone()), dep.range.0.clone());
    }
    map
}
