use core::ops::{Bound, RangeBounds};
use pubgrub::type_aliases::Map;
use pubgrub::range::Range;
use std::collections::BTreeMap;

use crate::opam_version::OpamVersion;

pub type PackageName = String;

pub struct Index {
    pub packages:
        Map<PackageName, BTreeMap<OpamVersion, Deps>>,
}

pub type Deps = Map<PackageName, Range<OpamVersion>>;

impl Index {
    /// Empty new index.
    pub fn new() -> Self {
        Self {
            packages: Map::default(),
        }
    }

    /// List existing versions for a given package with newest versions first.
    pub fn available_versions(&self, package: &PackageName) -> impl Iterator<Item = &OpamVersion> {
        self.packages
            .get(package)
            .into_iter()
            .flat_map(|k| k.keys())
            .rev()
    }

    /// Register a package and its mandatory dependencies in the index.
    pub fn add_deps<R: RangeBounds<u32>>(
        &mut self,
        package: &str,
        version: u32,
        new_deps: &[(&str, R)],
    ) {
        let deps = self
            .packages
            .entry(package.to_string())
            .or_default()
            .entry(version.into())
            .or_default();
        for (p, r) in new_deps {
            deps.insert(String::from(*p), range_from_bounds(r));
        }
    }
}

/// Convert a range bounds into pubgrub Range type.
fn range_from_bounds<R: RangeBounds<u32>>(bounds: &R) -> Range<OpamVersion> {
    match (bounds.start_bound(), bounds.end_bound()) {
        (Bound::Unbounded, Bound::Unbounded) => Range::any(),
        (Bound::Unbounded, Bound::Excluded(end)) => Range::strictly_lower_than(*end),
        (Bound::Unbounded, Bound::Included(end)) => Range::strictly_lower_than(end + 1),
        (Bound::Included(start), Bound::Unbounded) => Range::higher_than(*start),
        (Bound::Included(start), Bound::Included(end)) => Range::between(*start, end + 1),
        (Bound::Included(start), Bound::Excluded(end)) => Range::between(*start, *end),
        (Bound::Excluded(start), Bound::Unbounded) => Range::higher_than(start + 1),
        (Bound::Excluded(start), Bound::Included(end)) => Range::between(start + 1, end + 1),
        (Bound::Excluded(start), Bound::Excluded(end)) => Range::between(start + 1, *end),
    }
}
