use core::fmt::Display;
use pubgrub::{Map, Range};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use crate::debian_version::DebianVersion;

pub type PackageName = String;

pub struct Index {
    pub packages: Map<PackageName, BTreeMap<DebianVersion, Vec<Dependency>>>,
    pub debug: Cell<bool>,
    pub version_debug: Cell<bool>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct HashedRange(pub Range<DebianVersion>);

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
    pub alternatives: Vec<Alternative>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Alternative {
    pub name: PackageName,
    pub range: HashedRange,
    // TODO later
    // pub arch: Option<Vec<String>>,
}

impl Display for Dependency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let formatted: Vec<String> = self
            .alternatives
            .iter()
            .map(|alt| format!("{}: {}", alt.name, alt.range))
            .collect();
        write!(f, "{}", formatted.join(" | "))
    }
}

impl Index {
    /// Empty new index.
    pub fn new() -> Self {
        Self {
            packages: Map::default(),
            debug: false.into(),
            version_debug: false.into(),
        }
    }

    /// List existing versions for a given package with newest versions first.
    pub fn available_versions(&self, package: &PackageName) -> Vec<DebianVersion> {
        self.packages
            .get(package)
            .into_iter()
            .flat_map(|k| k.keys())
            .rev()
            .cloned()
            .collect()
    }

    /// Register a package and its mandatory dependencies in the index.
    pub fn add_deps(&mut self, name: &str, version: DebianVersion, dependencies: Vec<Dependency>) {
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
}
