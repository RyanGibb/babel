use std::{
    cell::{Cell, RefCell},
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    error::Error,
    fs::File,
    hash::{Hash, Hasher},
    io::{BufWriter, Write},
    ops::Bound,
    time::Instant,
};

use cargo::{core::Summary, util::interning::InternedString};
use crates_index::DependencyKind;
use either::Either;
use hasher::StableHasher;
use itertools::Itertools as _;
use names::{new_bucket, new_links, new_wide, FeatureNamespace, Names};
use pubgrub::{
    resolve, Dependencies, DependencyConstraints, DependencyProvider, PackageResolutionStatistics,
    PubGrubError, SelectedDependencies, VersionSet,
};
use rc_semver_pubgrub::RcSemverPubgrub;
use ron::ser::PrettyConfig;
use semver_pubgrub::{SemverCompatibility, SemverPubgrub};

pub mod cargo_resolver;
pub mod hasher;
pub mod index_data;
pub mod names;
mod rc_semver_pubgrub;
pub mod read_index;
#[cfg(test)]
mod tests;

#[cfg(test)]
use read_index::read_test_file;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(target_env = "msvc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

const TIME_MAKE_FILE: f32 = 40.0;
const TIME_CUT_OFF: f32 = TIME_MAKE_FILE * 4.0;

type IndexMapLookup = HashMap<
    InternedString,
    BTreeMap<semver::Version, (index_data::Version, Summary)>,
    rustc_hash::FxBuildHasher,
>;

#[derive(Clone)]
pub struct Index<'c> {
    crates: &'c IndexMapLookup,
    past_result:
        Option<HashMap<InternedString, BTreeSet<semver::Version>, rustc_hash::FxBuildHasher>>,
    dependencies: RefCell<HashSet<(InternedString, semver::Version), rustc_hash::FxBuildHasher>>,
    pubgrub_dependencies: RefCell<HashSet<(Names<'c>, semver::Version), rustc_hash::FxBuildHasher>>,
    start: Cell<Instant>,
    should_cancel_call_count: Cell<u64>,
}

impl<'c> Index<'c> {
    pub fn new(crates: &'c IndexMapLookup) -> Self {
        Self {
            crates,
            past_result: None,
            pubgrub_dependencies: Default::default(),
            dependencies: Default::default(),
            start: Cell::new(Instant::now()),
            should_cancel_call_count: Cell::new(0),
        }
    }

    fn reset(&mut self) {
        self.past_result = None;
        self.dependencies.get_mut().clear();
        self.pubgrub_dependencies.get_mut().clear();
        self.reset_time();
    }

    fn reset_time(&mut self) {
        *self.should_cancel_call_count.get_mut() = 0;
        *self.start.get_mut() = Instant::now();
    }

    fn duration(&self) -> f32 {
        self.start.get().elapsed().as_secs_f32()
    }

    fn should_cancel_call_count(&self) -> u64 {
        self.should_cancel_call_count.get()
    }

    #[cfg(test)]
    fn make_pubgrub_ron_file(&self) {
        let mut dependency_provider: BTreeMap<_, BTreeMap<_, Result<_, _>>> = BTreeMap::new();
        let deps = self
            .pubgrub_dependencies
            .borrow()
            .iter()
            .cloned()
            .collect_vec();

        let Some(name) = deps
            .iter()
            .find(|(name, _)| matches!(name, Names::Bucket(_, _, all) if *all))
        else {
            panic!("no root")
        };

        for (package, version) in &deps {
            match self.get_dependencies(package, version) {
                Ok(Dependencies::Available(dependencies)) => {
                    dependency_provider
                        .entry(package.clone())
                        .or_default()
                        .insert(version.clone(), Ok(dependencies));
                }
                Ok(Dependencies::Unavailable(s)) => {
                    dependency_provider
                        .entry(package.clone())
                        .or_default()
                        .insert(version.clone(), Err(s));
                }
                Err(_) => {
                    dependency_provider
                        .entry(package.clone())
                        .or_default()
                        .insert(version.clone(), Err("SomeError".to_owned()));
                }
            }
        }

        let file_name = format!("out/pubgrub_ron/{}@{}.ron", name.0.crate_(), name.1);
        let mut file = BufWriter::new(File::create(&file_name).unwrap());
        ron::ser::to_writer_pretty(&mut file, &dependency_provider, PrettyConfig::new()).unwrap();
        file.flush().unwrap();
    }

    fn make_index_ron_data(&self) -> Vec<index_data::Version> {
        let deps = self.dependencies.borrow();

        let name_vers: BTreeSet<_> = deps.iter().map(|(n, v)| (n.as_str(), v)).collect();

        name_vers
            .into_iter()
            .map(|(n, version)| self.crates[n][version].0.clone())
            .collect()
    }

    fn make_index_ron_file(&self) {
        let grub_deps = self.pubgrub_dependencies.borrow();

        let name = grub_deps
            .iter()
            .find(|(name, _)| matches!(name, Names::Bucket(_, _, all) if *all))
            .unwrap();

        let out = self.make_index_ron_data();

        let file_name = format!("out/index_ron/{}@{}.ron", name.0.crate_(), name.1);
        let mut file = BufWriter::new(File::create(&file_name).unwrap());
        ron::ser::to_writer_pretty(&mut file, &out, PrettyConfig::new()).unwrap();
        file.flush().unwrap();
    }

    fn get_versions<Q>(&self, name: &Q) -> impl Iterator<Item = &semver::Version> + '_
    where
        Q: ?Sized + Hash + Eq,
        InternedString: std::borrow::Borrow<Q>,
    {
        if let Some(past) = self.past_result.as_ref() {
            let data = self.crates.get(name);
            Either::Left(
                past.get(name)
                    .into_iter()
                    .flat_map(|m| m.iter())
                    .rev()
                    .filter(move |v| data.map_or(false, |d| d.contains_key(v))),
            )
        } else {
            Either::Right(
                self.crates
                    .get(name)
                    .into_iter()
                    .flat_map(|m| m.keys())
                    .rev(),
            )
        }
    }

    fn get_version<Q>(&self, name: &Q, ver: &semver::Version) -> Option<&'c index_data::Version>
    where
        Q: ?Sized + Hash + Eq,
        InternedString: std::borrow::Borrow<Q>,
        &'c str: std::borrow::Borrow<Q>,
    {
        if let Some(past) = &self.past_result {
            past.get(name)?.get(ver)?;
        }
        self.crates.get(name)?.get(ver).map(|v| &v.0)
    }

    fn only_one_compatibility_range_in_data(
        &self,
        dep: &'c index_data::Dependency,
    ) -> Option<SemverCompatibility> {
        let mut iter = self
            .get_versions(dep.package_name.as_str())
            .filter(|v| dep.req.matches(v))
            .map(|v| SemverCompatibility::from(v));
        let first = iter.next().unwrap_or(SemverCompatibility::Patch(0));
        let mut iter = iter.filter(|v| v != &first);
        if iter.next().is_some() {
            None
        } else {
            Some(first)
        }
    }

    fn count_wide_matches<Q>(
        &self,
        range: &RcSemverPubgrub,
        package: &Q,
        req: &&semver::VersionReq,
    ) -> u32
    where
        Q: ?Sized + Hash + Eq,
        InternedString: std::borrow::Borrow<Q>,
    {
        // one version for each bucket that match req
        self.get_versions(package)
            .filter(|v| req.matches(v))
            .map(|v| SemverCompatibility::from(v))
            .dedup()
            .map(|v| v.canonical())
            .filter(|v| range.contains(v))
            .count() as u32
    }

    fn count_matches<Q>(&self, range: &RcSemverPubgrub, package: &Q) -> u32
    where
        Q: ?Sized + Hash + Eq,
        InternedString: std::borrow::Borrow<Q>,
    {
        self.get_versions(package)
            .filter(|v| range.contains(v))
            .count() as u32
    }

    fn from_dep(
        &self,
        dep: &'c index_data::Dependency,
        from: InternedString,
        compat: impl Into<SemverCompatibility>,
    ) -> (Names<'c>, RcSemverPubgrub) {
        if let Some(compat) = dep
            .pubgrub_req
            .only_one_compatibility_range()
            .or_else(|| self.only_one_compatibility_range_in_data(dep))
        {
            (
                new_bucket(dep.package_name, compat, false),
                RcSemverPubgrub::new((*dep.pubgrub_req).clone()),
            )
        } else {
            (
                new_wide(dep.package_name, &dep.req, from, compat.into()),
                RcSemverPubgrub::full(),
            )
        }
    }

    #[must_use]
    fn check_cycles(&self, root: Names<'c>, pubmap: &SelectedDependencies<Self>) -> bool {
        let mut vertions: HashMap<
            (InternedString, SemverCompatibility, bool),
            (semver::Version, BTreeSet<_>, BTreeSet<_>),
        > = HashMap::new();
        // Identify the selected packages
        for (names, ver) in pubmap {
            if let Names::Bucket(name, cap, is_root) = names {
                if cap != &SemverCompatibility::from(ver) {
                    panic!("cap not meet");
                }
                let old_val = vertions.insert(
                    (*name, *cap, *is_root),
                    (ver.clone(), BTreeSet::new(), BTreeSet::new()),
                );

                if old_val.is_some() {
                    panic!("duplicate package");
                }
            }
        }
        // Identify the selected package features and deps
        for (name, ver) in pubmap {
            if let Names::BucketFeatures(name, cap, feat) = name {
                if cap != &SemverCompatibility::from(ver) {
                    panic!("cap not meet for feature");
                }
                let old_val = vertions.get_mut(&(*name, *cap, false)).unwrap();
                if &old_val.0 != ver {
                    panic!("ver not match for feature");
                }
                let old_feat = match *feat {
                    FeatureNamespace::Feat(f) => old_val.1.insert(f),
                    FeatureNamespace::Dep(f) => old_val.2.insert(f),
                };
                if !old_feat {
                    panic!("duplicate feature");
                }
            }
        }

        let mut checked = HashSet::with_capacity(vertions.len());
        let mut visited = HashSet::with_capacity(4);
        let Names::Bucket(name, cap, is_root) = root else {
            panic!("root not bucket");
        };
        self.visit(
            (name, cap, is_root),
            pubmap,
            &vertions,
            &mut visited,
            &mut checked,
        )
        .is_err()
    }

    fn visit(
        &self,
        id: (InternedString, SemverCompatibility, bool),
        pubmap: &SelectedDependencies<Self>,
        vertions: &HashMap<
            (InternedString, SemverCompatibility, bool),
            (semver::Version, BTreeSet<&str>, BTreeSet<&str>),
        >,
        visited: &mut HashSet<(InternedString, SemverCompatibility, bool)>,
        checked: &mut HashSet<(InternedString, SemverCompatibility, bool)>,
    ) -> Result<(), ()> {
        if !visited.insert(id) {
            // We found a cycle and need to construct an error. Performance is no longer top priority.
            return Err(());
        }

        if checked.insert(id) {
            let (version, _feats, deps) = &vertions[&id];

            let index_ver = self.get_version(id.0.as_str(), version).unwrap();
            for dep in index_ver.deps.iter() {
                if dep.kind == DependencyKind::Dev {
                    continue;
                }
                if dep.optional && !id.2 && !deps.contains(dep.name.as_str()) {
                    continue;
                }
                let (cray, _) = self.from_dep(&dep, id.0, version);

                let dep_ver = &pubmap[&cray];
                self.visit(
                    (dep.package_name, dep_ver.into(), false),
                    pubmap,
                    vertions,
                    visited,
                    checked,
                )?;
            }
        }

        visited.remove(&id);
        Ok(())
    }

    #[must_use]
    fn check(&self, root: Names, pubmap: &SelectedDependencies<Self>) -> bool {
        // Basic dependency resolution properties
        if !pubmap.contains_key(&root) {
            return false;
        }
        for (name, ver) in pubmap {
            let Dependencies::Available(deps) = self.get_dependencies(name, ver).unwrap() else {
                return false;
            };
            for (dep, req) in deps {
                let Some(dep_ver) = pubmap.get(&dep) else {
                    return false;
                };
                if !req.contains(dep_ver) {
                    return false;
                }
            }
        }

        let mut vertions: HashMap<
            (InternedString, SemverCompatibility),
            (semver::Version, BTreeSet<_>, BTreeSet<_>, bool),
        > = HashMap::new();
        // Identify the selected packages
        for (names, ver) in pubmap {
            if let Names::Bucket(name, cap, is_root) = names {
                if cap != &SemverCompatibility::from(ver) {
                    return false;
                }
                if *is_root {
                    continue;
                }
                let old_val = vertions.insert(
                    (*name, *cap),
                    (ver.clone(), BTreeSet::new(), BTreeSet::new(), false),
                );

                if old_val.is_some() {
                    return false;
                }
            }
        }
        // Identify the selected package features and deps
        for (name, ver) in pubmap {
            if let Names::BucketFeatures(name, cap, feat) = name {
                if cap != &SemverCompatibility::from(ver) {
                    return false;
                }
                let old_val = vertions.get_mut(&(*name, *cap)).unwrap();
                if &old_val.0 != ver {
                    return false;
                }
                let old_feat = match *feat {
                    FeatureNamespace::Feat(f) => old_val.1.insert(f),
                    FeatureNamespace::Dep(f) => old_val.2.insert(f),
                };
                if !old_feat {
                    return false;
                }
            }
        }
        for (name, ver) in pubmap {
            if let Names::BucketDefaultFeatures(name, cap) = name {
                if cap != &SemverCompatibility::from(ver) {
                    return false;
                }
                let old_val = vertions.get_mut(&(*name, *cap)).unwrap();
                if &old_val.0 != ver {
                    return false;
                }
                if old_val.3 {
                    return false;
                }
                old_val.3 = true;
            }
        }

        let mut links: BTreeSet<_> = BTreeSet::new();
        for ((name, _), (ver, feats, deps, default_feature)) in vertions.iter() {
            let index_ver = self.get_version(name.as_str(), ver).unwrap();
            if index_ver.yanked {
                return false;
            }
            if let Some(link) = &index_ver.links {
                let old_link = links.insert(link.clone());
                if !old_link {
                    return false;
                }
            }

            if *default_feature {
                if index_ver.features.contains_key("default") != feats.contains("default") {
                    return false;
                }
            }

            for dep in index_ver.deps.iter() {
                if dep.optional && !deps.contains(&*dep.name) {
                    continue;
                }
                if index_ver.features.contains_key(&*dep.name) {
                    continue;
                }
                if dep.kind == DependencyKind::Dev {
                    continue;
                }

                // Check for something that meets that dep
                let fulfilled = vertions.iter().find(
                    |(
                        (other_name, _),
                        (other_ver, other_feats, _other_deps, other_default_feature),
                    )| {
                        **other_name == *dep.package_name
                            && dep.req.matches(other_ver)
                            && dep
                                .features
                                .iter()
                                .all(|f| f.is_empty() || other_feats.contains(&**f))
                            && (!dep.default_features || *other_default_feature)
                    },
                );
                if fulfilled.is_none() {
                    return false;
                }
            }

            // todo: check index_ver.features
        }
        true
    }
}

#[derive(Debug)]
pub struct SomeError;

impl std::fmt::Display for SomeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SomeError").finish()
    }
}

impl Error for SomeError {}

fn deps_insert<'c>(
    deps: &mut DependencyConstraints<Names<'c>, RcSemverPubgrub>,
    n: Names<'c>,
    r: RcSemverPubgrub,
) {
    deps.entry(n)
        .and_modify(|old_r| *old_r = old_r.intersection(&r))
        .or_insert(r);
}

impl<'c> DependencyProvider for Index<'c> {
    type P = Names<'c>;

    type V = semver::Version;

    type VS = RcSemverPubgrub;

    type M = String;
    type Err = SomeError;
    fn choose_version(
        &self,
        package: &Names,
        range: &RcSemverPubgrub,
    ) -> Result<Option<semver::Version>, Self::Err> {
        Ok(match package {
            Names::Links(_name) => {
                let Some((_, Bound::Included(v))) = range.inner.bounding_range() else {
                    return Err(SomeError);
                };
                Some(v.clone())
            }

            Names::Wide(_, req, _, _)
            | Names::WideFeatures(_, req, _, _, _)
            | Names::WideDefaultFeatures(_, req, _, _) => {
                // one version for each bucket that match req
                self.get_versions(&*package.crate_())
                    .filter(|v| req.matches(v))
                    .map(|v| SemverCompatibility::from(v))
                    .map(|v| v.canonical())
                    .find(|v| range.contains(v))
            }
            Names::Bucket(_, _, _)
            | Names::BucketFeatures(_, _, _)
            | Names::BucketDefaultFeatures(_, _) => self
                .get_versions(&*package.crate_())
                .find(|v| range.contains(v))
                .cloned(),
        })
    }

    type Priority = (u32, Reverse<u32>);

    fn prioritize(
        &self,
        package: &Names<'c>,
        range: &RcSemverPubgrub,
        stats: &PackageResolutionStatistics,
    ) -> Self::Priority {
        match package {
            Names::Links(_name) => {
                // PubGrub automatically handles when any requirement has no overlap. So this is only deciding a importance of picking the version:
                //
                // - If it only matches one thing, then adding the decision with no additional dependencies makes no difference.
                // - If it can match more than one thing, and it is entirely equivalent to picking the packages directly which would make more sense to the users.
                //
                // So only rubberstamp links attributes when all other decisions are made, by setting the priority as low as it will go.
                (stats.conflict_count(), Reverse(u32::MAX))
            }

            Names::Wide(_, req, _, _) => {
                match (
                    stats.conflict_count(),
                    range.inner.only_one_compatibility_range().is_some(),
                ) {
                    (conflict_count, true) => (conflict_count, Reverse(1)),
                    (0, false) => (0, Reverse(u32::MAX)),
                    (conflict_count, false) => (
                        conflict_count,
                        Reverse(self.count_wide_matches(range, &package.crate_(), req)),
                    ),
                }
            }
            Names::WideFeatures(_, req, _, _, _) | Names::WideDefaultFeatures(_, req, _, _) => {
                match (
                    stats.conflict_count(),
                    range.inner.only_one_compatibility_range().is_some(),
                ) {
                    (conflict_count, true) => (conflict_count, Reverse(0)),
                    (0, false) => (0, Reverse(u32::MAX - 1)),
                    (conflict_count, false) => (
                        conflict_count,
                        Reverse(
                            self.count_wide_matches(range, &package.crate_(), req)
                                .saturating_add(1),
                        ),
                    ),
                }
            }

            Names::Bucket(_, _, _) => {
                match (stats.conflict_count(), range.inner.as_singleton().is_some()) {
                    (conflict_count, true) => (conflict_count, Reverse(1)),
                    (0, false) => (0, Reverse(u32::MAX)),
                    (conflict_count, false) => (
                        conflict_count,
                        Reverse(self.count_matches(range, &package.crate_())),
                    ),
                }
            }
            Names::BucketFeatures(_, _, _) | Names::BucketDefaultFeatures(_, _) => {
                match (stats.conflict_count(), range.inner.as_singleton().is_some()) {
                    (conflict_count, true) => (conflict_count, Reverse(0)),
                    (0, false) => (0, Reverse(u32::MAX - 1)),
                    (conflict_count, false) => (
                        conflict_count,
                        Reverse(
                            self.count_matches(range, &package.crate_())
                                .saturating_add(1),
                        ),
                    ),
                }
            }
        }
    }

    fn get_dependencies(
        &self,
        package: &Names<'c>,
        version: &semver::Version,
    ) -> Result<Dependencies<Self::P, Self::VS, Self::M>, Self::Err> {
        self.pubgrub_dependencies
            .borrow_mut()
            .insert((package.clone(), version.clone()));
        Ok(match package {
            &Names::Bucket(name, _major, all_features) => {
                let Some(index_ver) = self.get_version(name.as_str(), version) else {
                    return Err(SomeError);
                };
                self.dependencies
                    .borrow_mut()
                    .insert((index_ver.name, version.clone()));
                if index_ver.yanked {
                    return Ok(Dependencies::Unavailable("yanked: Bucket".into()));
                }
                let mut deps = DependencyConstraints::default();
                if let Some(link) = &index_ver.links {
                    let index_unique_to_each_crate_version = {
                        let mut state = StableHasher::new();
                        package.hash(&mut state);
                        version.hash(&mut state);
                        state.finish()
                    };
                    let ver = semver::Version::new(index_unique_to_each_crate_version, 0, 0);
                    deps.insert(new_links(*link), RcSemverPubgrub::singleton(ver));
                }
                for dep in index_ver.deps.iter() {
                    if dep.kind == DependencyKind::Dev && !all_features {
                        continue;
                    }
                    if dep.optional && !all_features {
                        continue; // handled in Names::Features
                    }

                    let (cray, req_range) = self.from_dep(&dep, name, version);

                    deps_insert(&mut deps, cray.clone(), req_range.clone());

                    if dep.default_features {
                        deps_insert(&mut deps, cray.with_default_features(), req_range.clone());
                    }
                    for f in &*dep.features {
                        deps_insert(
                            &mut deps,
                            cray.with_features(FeatureNamespace::new(f)),
                            req_range.clone(),
                        );
                    }
                }
                if all_features {
                    for vals in index_ver.features.values() {
                        for val in &**vals {
                            if let Some((dep, dep_feat)) = val.split_once('/') {
                                let dep_name = dep.strip_suffix('?').unwrap_or(dep);
                                for com in index_ver.deps.get(dep_name) {
                                    let (cray, req_range) = self.from_dep(com, name, version);
                                    deps_insert(
                                        &mut deps,
                                        cray.with_features(FeatureNamespace::new(dep_feat)),
                                        req_range,
                                    );
                                }
                            }
                        }
                    }
                }
                Dependencies::Available(deps)
            }
            Names::BucketFeatures(name, _major, FeatureNamespace::Feat(feat)) => {
                let Some(index_ver) = self.get_version(name.as_str(), version) else {
                    return Err(SomeError);
                };
                self.dependencies
                    .borrow_mut()
                    .insert((index_ver.name, version.clone()));
                if index_ver.yanked {
                    return Ok(Dependencies::Unavailable(
                        "yanked: BucketFeatures Feat".into(),
                    ));
                }
                let mut deps = DependencyConstraints::default();
                deps.insert(
                    new_bucket(*name, version.into(), false),
                    RcSemverPubgrub::singleton(version.clone()),
                );

                if let Some(vals) = index_ver.features.get(*feat) {
                    for val in &**vals {
                        if let Some((dep, dep_feat)) = val.split_once('/') {
                            let dep_name = dep.strip_suffix('?');
                            let week = dep_name.is_some();
                            let dep_name = dep_name.unwrap_or(dep);

                            for dep in index_ver.deps.get(dep_name) {
                                if dep.kind == DependencyKind::Dev {
                                    continue;
                                }
                                let (cray, req_range) = self.from_dep(dep, *name, version);

                                if dep.optional {
                                    deps_insert(
                                        &mut deps,
                                        package.with_features(FeatureNamespace::Dep(dep_name)),
                                        RcSemverPubgrub::singleton(version.clone()),
                                    );

                                    if !week
                                        && dep_name != *feat
                                        && index_ver.features.contains_key(dep_name)
                                    {
                                        deps_insert(
                                            &mut deps,
                                            package.with_features(FeatureNamespace::Feat(dep_name)),
                                            RcSemverPubgrub::singleton(version.clone()),
                                        );
                                    }
                                }
                                deps_insert(
                                    &mut deps,
                                    cray.with_features(FeatureNamespace::Feat(dep_feat)),
                                    req_range,
                                );
                            }
                        } else {
                            deps_insert(
                                &mut deps,
                                package.with_features(FeatureNamespace::new(val)),
                                RcSemverPubgrub::singleton(version.clone()),
                            );
                        }
                    }
                    Dependencies::Available(deps)
                } else {
                    Dependencies::Unavailable("no matching feat nor dep".into())
                }
            }
            Names::BucketDefaultFeatures(name, _major) => {
                let Some(index_ver) = self.get_version(name.as_str(), version) else {
                    return Err(SomeError);
                };
                self.dependencies
                    .borrow_mut()
                    .insert((index_ver.name, version.clone()));
                if index_ver.yanked {
                    return Ok(Dependencies::Unavailable(
                        "yanked: BucketFeatures DefaultFeatures".into(),
                    ));
                }
                let mut deps = DependencyConstraints::default();
                deps.insert(
                    new_bucket(*name, version.into(), false),
                    RcSemverPubgrub::singleton(version.clone()),
                );

                if index_ver.features.contains_key("default") {
                    deps_insert(
                        &mut deps,
                        package.with_features(FeatureNamespace::Feat("default")),
                        RcSemverPubgrub::singleton(version.clone()),
                    );
                }

                Dependencies::Available(deps)
            }
            Names::BucketFeatures(name, _major, FeatureNamespace::Dep(feat)) => {
                let Some(index_ver) = self.get_version(name.as_str(), version) else {
                    return Err(SomeError);
                };
                if index_ver.yanked {
                    return Ok(Dependencies::Unavailable(
                        "yanked: BucketFeatures Dep".into(),
                    ));
                }
                let mut deps = DependencyConstraints::default();
                deps.insert(
                    new_bucket(*name, version.into(), false),
                    RcSemverPubgrub::singleton(version.clone()),
                );

                let mut found_name = false;
                for dep in index_ver.deps.get(*feat) {
                    if !dep.optional {
                        continue;
                    }
                    if dep.kind == DependencyKind::Dev {
                        continue;
                    }
                    found_name = true;
                    let (cray, req_range) = self.from_dep(&dep, *name, version);

                    deps_insert(&mut deps, cray.clone(), req_range.clone());

                    if dep.default_features {
                        deps_insert(&mut deps, cray.with_default_features(), req_range.clone());
                    }
                    for f in &*dep.features {
                        deps_insert(
                            &mut deps,
                            cray.with_features(FeatureNamespace::new(f)),
                            req_range.clone(),
                        );
                    }
                }

                if found_name {
                    Dependencies::Available(deps)
                } else {
                    Dependencies::Unavailable("no matching feat".into())
                }
            }
            Names::Wide(name, req, _, _) => {
                let compatibility = SemverCompatibility::from(version);
                let compat_range = SemverPubgrub::from(&compatibility);
                let req_range = SemverPubgrub::from(*req);
                let range = req_range.intersection(&compat_range);
                let range = RcSemverPubgrub::new(range.clone());

                Dependencies::Available(DependencyConstraints::from_iter([(
                    new_bucket(*name, compatibility, false),
                    range,
                )]))
            }
            Names::WideFeatures(name, req, parent, parent_com, feat) => {
                let compatibility = SemverCompatibility::from(version);
                let compat_range = SemverPubgrub::from(&compatibility);
                let req_range = SemverPubgrub::from(*req);
                let range = req_range.intersection(&compat_range);
                let range = RcSemverPubgrub::new(range.clone());
                Dependencies::Available(DependencyConstraints::from_iter([
                    (
                        new_wide(*name, req, *parent, parent_com.clone()),
                        RcSemverPubgrub::singleton(version.clone()),
                    ),
                    (
                        new_bucket(*name, compatibility, false).with_features(*feat),
                        range,
                    ),
                ]))
            }
            Names::WideDefaultFeatures(name, req, parent, parent_com) => {
                let compatibility = SemverCompatibility::from(version);
                let compat_range = SemverPubgrub::from(&compatibility);
                let req_range = SemverPubgrub::from(*req);
                let range = req_range.intersection(&compat_range);
                let range = RcSemverPubgrub::new(range.clone());

                Dependencies::Available(DependencyConstraints::from_iter([
                    (
                        new_wide(*name, req, *parent, parent_com.clone()),
                        RcSemverPubgrub::singleton(version.clone()),
                    ),
                    (
                        new_bucket(*name, compatibility, false).with_default_features(),
                        range,
                    ),
                ]))
            }
            Names::Links(_) => Dependencies::Available(DependencyConstraints::default()),
        })
    }

    fn should_cancel(&self) -> Result<(), Self::Err> {
        let calls = self.should_cancel_call_count.get();
        self.should_cancel_call_count.set(calls + 1);
        if calls % 64 == 0 && TIME_CUT_OFF < self.start.get().elapsed().as_secs_f32() {
            return Err(SomeError);
        }
        Ok(())
    }
}

#[derive(clap::ValueEnum, Clone, Debug, Copy)]
pub enum Mode {
    All,
    Pub,
    Cargo,
    PubLock,
    CargoLock,
}

impl Mode {
    fn build_pub(&self) -> bool {
        match self {
            Mode::All => true,
            Mode::Pub => true,
            Mode::Cargo => false,
            Mode::PubLock => false,
            Mode::CargoLock => true,
        }
    }

    fn build_cargo(&self) -> bool {
        match self {
            Mode::All => true,
            Mode::Pub => false,
            Mode::Cargo => true,
            Mode::PubLock => true,
            Mode::CargoLock => false,
        }
    }

    fn build_pub_lock(&self) -> bool {
        match self {
            Mode::All => true,
            Mode::Pub => false,
            Mode::Cargo => false,
            Mode::PubLock => true,
            Mode::CargoLock => false,
        }
    }

    fn build_cargo_lock(&self) -> bool {
        match self {
            Mode::All => true,
            Mode::Pub => false,
            Mode::Cargo => false,
            Mode::PubLock => false,
            Mode::CargoLock => true,
        }
    }
}

pub fn process_crate_version(
    dp: &mut Index,
    crt: InternedString,
    ver: semver::Version,
    mode: Mode,
) -> OutputSummary {
    let root = new_bucket(crt, (&ver).into(), true);
    dp.reset();
    let mut pub_cyclic_package_dependency = None;
    let mut cyclic_package_dependency = false;
    let mut res = None;
    let mut pub_time = 0.0;
    let mut should_cancel_call_count = 0;
    let mut get_dependencies_call_count = 0;
    if mode.build_pub() {
        res = Some(resolve(dp, root.clone(), (&ver).clone()));
        cyclic_package_dependency = if let Some(Ok(map)) = res.as_ref() {
            dp.check_cycles(root.clone(), map)
        } else {
            false
        };
        pub_cyclic_package_dependency = Some(cyclic_package_dependency);
        pub_time = dp.duration();
        should_cancel_call_count = dp.should_cancel_call_count();
        get_dependencies_call_count = dp.pubgrub_dependencies.borrow().len();
        match res.as_ref().unwrap().as_ref() {
            Ok(map) => {
                if !dp.check(root.clone(), &map) {
                    dp.make_index_ron_file();
                    panic!("failed check");
                }
            }
            Err(PubGrubError::NoSolution(_derivation)) => {}
            Err(e) => {
                dp.make_index_ron_file();
                dbg!(e);
            }
        }
        if pub_time > TIME_MAKE_FILE {
            dp.make_index_ron_file();
        }
    }
    let mut cargo_out = None;
    let mut cargo_time = 0.0;
    if mode.build_cargo() {
        dp.reset_time();
        cargo_out = Some(cargo_resolver::resolve(crt, &ver, dp));
        cargo_time = dp.duration();
        cyclic_package_dependency = &cargo_out
            .as_ref()
            .unwrap()
            .as_ref()
            .map_err(|e| e.to_string().starts_with("cyclic package dependency"))
            == &Err(true);
        if let Some(pub_cyclic_package_dependency) = pub_cyclic_package_dependency {
            if cyclic_package_dependency != pub_cyclic_package_dependency {
                dp.make_index_ron_file();
                println!("failed to cyclic_package_dependency {root:?}");
            }

            if !cyclic_package_dependency
                && res.as_ref().unwrap().is_ok() != cargo_out.as_ref().unwrap().is_ok()
            {
                dp.make_index_ron_file();
                println!("failed to match cargo {root:?}");
            }
        }
    }
    let mut cargo_check_pub_lock_time = 0.0;
    if mode.build_cargo_lock() && res.as_ref().unwrap().is_ok() {
        dp.past_result = res
            .as_ref()
            .unwrap()
            .as_ref()
            .map(|map| {
                let mut results: HashMap<
                    InternedString,
                    BTreeSet<semver::Version>,
                    rustc_hash::FxBuildHasher,
                > = HashMap::default();
                for (k, v) in map.iter() {
                    if k.is_real() {
                        results.entry(k.crate_()).or_default().insert(v.clone());
                    }
                }
                results
            })
            .ok();
        dp.reset_time();
        let cargo_check_pub_lock_out = cargo_resolver::resolve(crt, &ver, dp);
        cargo_check_pub_lock_time = dp.duration();

        let cyclic_package_dependency_pub_lock = &cargo_check_pub_lock_out
            .as_ref()
            .map_err(|e| e.to_string().starts_with("cyclic package dependency"))
            == &Err(true);

        if !cyclic_package_dependency_pub_lock && !cargo_check_pub_lock_out.is_ok() {
            dp.make_index_ron_file();
            println!("failed to match pub lock cargo {root:?}");
        }
    }

    let mut pub_check_cargo_lock_time = 0.0;
    if mode.build_pub_lock() && cargo_out.as_ref().unwrap().is_ok() {
        dp.past_result = cargo_out
            .as_ref()
            .unwrap()
            .as_ref()
            .map(|map| {
                let mut results: HashMap<
                    InternedString,
                    BTreeSet<semver::Version>,
                    rustc_hash::FxBuildHasher,
                > = HashMap::default();
                for v in map.iter() {
                    results
                        .entry(v.name())
                        .or_default()
                        .insert(v.version().clone());
                }
                results
            })
            .ok();
        dp.reset_time();
        let pub_check_cargo_lock_out = resolve(dp, root.clone(), ver.clone());
        pub_check_cargo_lock_time = dp.duration();

        if !pub_check_cargo_lock_out.is_ok() {
            dp.make_index_ron_file();
            println!("failed to match cargo lock pub {root:?}");
        }
    }

    let pubgrub_deps = if let Some(Ok(map)) = &res {
        map.len()
    } else {
        0
    };

    let deps = if let Some(Ok(map)) = &res {
        map.iter().filter(|(v, _)| v.is_real()).count()
    } else {
        0
    };

    let cargo_deps = if let Some(Ok(map)) = &cargo_out {
        map.len()
    } else {
        0
    };

    OutputSummary {
        name: crt,
        ver,
        time: pub_time,
        succeeded: matches!(&res, Some(Ok(_))),
        should_cancel_call_count,
        get_dependencies_call_count,
        pubgrub_deps,
        deps,
        cargo_time,
        cyclic_package_dependency,
        cargo_deps,
        cargo_check_pub_lock_time,
        pub_check_cargo_lock_time,
    }
}

#[derive(serde::Serialize)]
pub struct OutputSummary {
    pub name: InternedString,
    pub ver: semver::Version,
    pub time: f32,
    pub succeeded: bool,
    pub should_cancel_call_count: u64,
    pub get_dependencies_call_count: usize,
    pub pubgrub_deps: usize,
    pub deps: usize,
    pub cargo_time: f32,
    pub cyclic_package_dependency: bool,
    pub cargo_deps: usize,
    pub cargo_check_pub_lock_time: f32,
    pub pub_check_cargo_lock_time: f32,
}
