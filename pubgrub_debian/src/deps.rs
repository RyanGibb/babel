use crate::index::{DebianIndex, Dependency};
use crate::version::DebianVersion;
use core::fmt::Display;
use pubgrub::{Dependencies, DependencyConstraints, DependencyProvider, Map, Range};
use std::convert::Infallible;
use std::str::FromStr;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum DebianPackage {
    Root(Vec<(DebianPackage, Range<DebianVersion>)>),
    Base(String),
    Proxy(Dependency),
}

impl FromStr for DebianPackage {
    type Err = String;
    fn from_str(pkg: &str) -> Result<Self, Self::Err> {
        let mut pkg_parts = pkg.split('/');
        match (pkg_parts.next(), pkg_parts.next()) {
            (Some(base), None) => Ok(DebianPackage::Base(base.to_string())),
            _ => Err(format!("{} is not a valid package name", pkg)),
        }
    }
}

impl Display for DebianPackage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DebianPackage::Root(_) => write!(f, "Root"),
            DebianPackage::Base(pkg) => write!(f, "{}", pkg),
            DebianPackage::Proxy(dependency) => write!(f, "{}", dependency),
        }
    }
}

impl DebianIndex {
    pub fn list_versions(
        &self,
        package: &DebianPackage,
    ) -> impl Iterator<Item = DebianVersion> + '_ {
        let versions = match package {
            DebianPackage::Root(_) => vec![DebianVersion("".to_string())],
            DebianPackage::Base(pkg) => self.available_versions(pkg),
            DebianPackage::Proxy(dependencies) => dependencies
                .clone()
                .alternatives
                .into_iter()
                .map(|dep| DebianVersion(dep.name))
                .collect(),
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

impl DependencyProvider for DebianIndex {
    type P = DebianPackage;

    type V = DebianVersion;

    type VS = Range<DebianVersion>;

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
        package: &DebianPackage,
        version: &DebianVersion,
    ) -> Result<Dependencies<Self::P, Self::VS, Self::M>, Self::Err> {
        match package {
            DebianPackage::Root(deps) => {
                Ok(Dependencies::Available(deps.into_iter().cloned().collect()))
            }
            DebianPackage::Base(pkg) => {
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
            DebianPackage::Proxy(dependency) => {
                let deps = from_proxy(dependency, version);
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
) -> DependencyConstraints<DebianPackage, Range<DebianVersion>> {
    let mut map = Map::default();
    for dependency in dependencies.clone() {
        match &dependency.alternatives[..] {
            [dep] => map.insert(DebianPackage::Base(dep.name.clone()), dep.range.0.clone()),
            _ => map.insert(DebianPackage::Proxy(dependency), Range::full()),
        };
    }
    map
}

pub fn from_proxy(
    dependency: &Dependency,
    version: &DebianVersion,
) -> DependencyConstraints<DebianPackage, Range<DebianVersion>> {
    let mut map = Map::default();
    for alt in dependency.alternatives.clone() {
        match &alt.name {
            n if version.to_string().eq(n) => {
                map.insert(DebianPackage::Base(alt.name), alt.range.0.clone());
            }
            _ => {}
        }
    }
    map
}
