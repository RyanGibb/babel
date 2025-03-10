use core::fmt::Display;
use pubgrub::{Map, Range};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use crate::version::AlpineVersion;

pub type PackageName = String;

pub struct AlpineIndex {
    pub packages: Map<PackageName, BTreeMap<AlpineVersion, Vec<Dependency>>>,
    pub debug: Cell<bool>,
    pub version_debug: Cell<bool>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct HashedRange(pub Range<AlpineVersion>);

impl Hash for HashedRange {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let s = format!("{}", self.0);
        s.hash(state);
    }
}

impl Display for HashedRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Delegate to the Display implementation of the inner Range.
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Dependency {
    pub name: PackageName,
    pub range: HashedRange,
    // TODO later
    // pub arch: Option<Vec<String>>,
}

impl Display for Dependency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.name, self.range)
    }
}

impl AlpineIndex {
    pub fn new() -> Self {
        Self {
            packages: Map::default(),
            debug: false.into(),
            version_debug: false.into(),
        }
    }

    pub fn available_versions(&self, package: &PackageName) -> Vec<AlpineVersion> {
        self.packages
            .get(package)
            .into_iter()
            .flat_map(|k| k.keys())
            .rev()
            .cloned()
            .collect()
    }

    pub fn add_deps(&mut self, name: &str, version: AlpineVersion, dependencies: Vec<Dependency>) {
        self.packages
            .entry(name.to_string())
            .or_default()
            .insert(version, dependencies);
    }

    pub fn set_debug(&self, flag: bool) {
        self.debug.set(flag);
    }

    pub fn set_version_debug(&self, flag: bool) {
        self.version_debug.set(flag);
    }

    pub fn package_count(&self) -> usize {
        self.packages.len()
    }
}
