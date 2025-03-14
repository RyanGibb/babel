use std::sync::OnceLock;
use std::task::Poll;

use anyhow::bail;
use cargo::core::dependency::DepKind;
use cargo::core::resolver::{self, ResolveOpts, VersionPreferences};
use cargo::core::Resolve;
use cargo::core::ResolveVersion;
use cargo::core::SourceId;
use cargo::core::{Dependency, PackageId, Registry, Summary};
use cargo::sources::source::QueryKind;
use cargo::sources::IndexSummary;
use cargo::util::interning::InternedString;
use cargo::util::{CargoResult, IntoUrl};
use itertools::Itertools;

impl<'a> Registry for crate::Index<'a> {
    fn query(
        &mut self,
        dep: &Dependency,
        kind: QueryKind,
        f: &mut dyn FnMut(IndexSummary),
    ) -> Poll<CargoResult<()>> {
        if let Some(by_name) = self.crates.get(&dep.package_name()) {
            if let Some(past_result) = &self.past_result {
                for past_ver in past_result
                    .get(dep.package_name().as_str())
                    .into_iter()
                    .flatten()
                {
                    if let Some((_, summary)) = by_name.get(past_ver) {
                        if dep.matches(&summary) {
                            self.dependencies
                                .borrow_mut()
                                .insert((dep.package_name(), summary.version().clone()));
                            f(IndexSummary::Candidate(summary.clone()));
                        }
                    }
                }
            } else {
                for (_, summary) in by_name.values() {
                    let matched = match kind {
                        QueryKind::Exact => dep.matches(&summary),
                        QueryKind::AlternativeNames => true,
                        QueryKind::RejectedVersions => true,
                        QueryKind::Normalized => true,
                    };
                    if matched {
                        self.dependencies
                            .borrow_mut()
                            .insert((dep.package_name(), summary.version().clone()));
                        f(IndexSummary::Candidate(summary.clone()));
                    }
                }
            }
        }
        Poll::Ready(Ok(()))
    }

    fn describe_source(&self, _src: SourceId) -> String {
        String::new()
    }

    fn is_replaced(&self, _src: SourceId) -> bool {
        false
    }

    fn block_until_ready(&mut self) -> CargoResult<()> {
        Ok(())
    }
}

pub fn resolve<'c>(
    name: InternedString,
    ver: &semver::Version,
    dp: &mut crate::Index<'c>,
) -> CargoResult<Resolve> {
    let Some(pack) = dp.crates.get(&name) else {
        bail!("No package found named '{name}'");
    };
    let Some((_, summary)) = pack.get(ver).cloned() else {
        bail!("No version found for package '{name}@{ver}'");
    };
    let new_id = summary.package_id().with_source_id(other_registry_loc());
    let summary = summary.override_id(new_id);
    resolver::resolve(
        &[(summary, ResolveOpts::everything())],
        &[],
        dp,
        &VersionPreferences::default(),
        ResolveVersion::with_rust_version(None),
        None,
    )
}

impl From<&crate::index_data::Dependency> for Dependency {
    fn from(value: &crate::index_data::Dependency) -> Self {
        let mut out = Dependency::parse(value.package_name, None, registry_loc()).unwrap();
        if value.name != value.package_name {
            out.set_explicit_name_in_toml(&*value.name);
        }
        out.set_version_req((&*value.req).clone().into());
        out.set_features(value.features.iter().copied());
        out.set_default_features(value.default_features);
        out.set_kind(match &value.kind {
            crates_index::DependencyKind::Normal => DepKind::Normal,
            crates_index::DependencyKind::Dev => DepKind::Development,
            crates_index::DependencyKind::Build => DepKind::Build,
        });
        out.set_optional(value.optional);
        out
    }
}

impl TryFrom<&crate::index_data::Version> for Summary {
    type Error = anyhow::Error;
    fn try_from(value: &crate::index_data::Version) -> Result<Self, Self::Error> {
        let pid = PackageId::new(value.name, (*value.vers).clone(), registry_loc());
        let dep = value.deps.iter().map(|d| d.into()).collect_vec();
        let features = value
            .features
            .iter()
            .map(|(&f, v)| (f, v.iter().copied().collect()))
            .collect();
        Summary::new(pid, dep, &features, value.links, None)
    }
}

fn registry_loc() -> SourceId {
    static EXAMPLE_DOT_COM: OnceLock<SourceId> = OnceLock::new();
    *EXAMPLE_DOT_COM
        .get_or_init(|| SourceId::for_registry(&"https://example.com".into_url().unwrap()).unwrap())
}

fn other_registry_loc() -> SourceId {
    static OTHER_EXAMPLE_DOT_COM: OnceLock<SourceId> = OnceLock::new();
    *OTHER_EXAMPLE_DOT_COM.get_or_init(|| {
        SourceId::for_registry(&"https://other.example.com".into_url().unwrap()).unwrap()
    })
}
