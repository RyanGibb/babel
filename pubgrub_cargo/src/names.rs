use cargo::util::interning::InternedString;
use semver_pubgrub::SemverCompatibility;

#[derive(Clone, Copy, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum FeatureNamespace<'c> {
    Dep(&'c str),
    Feat(&'c str),
}

impl<'c> FeatureNamespace<'c> {
    pub fn new(feat: &'c str) -> Self {
        if let Some(feat) = feat.strip_prefix("dep:") {
            FeatureNamespace::Dep(feat)
        } else {
            FeatureNamespace::Feat(feat)
        }
    }
    pub fn as_str(&self) -> &'c str {
        match self {
            FeatureNamespace::Dep(n) => n,
            FeatureNamespace::Feat(n) => n,
        }
    }
}
impl<'c> std::fmt::Display for FeatureNamespace<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if matches!(self, FeatureNamespace::Dep(_)) {
            f.write_str("dep:")?;
        }
        f.write_str(&self.as_str())
    }
}

impl std::fmt::Debug for FeatureNamespace<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self, f)
    }
}

impl serde::Serialize for FeatureNamespace<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Clone, Eq, PartialEq, Hash)]
pub enum Names<'c> {
    Bucket(InternedString, SemverCompatibility, bool),
    BucketFeatures(InternedString, SemverCompatibility, FeatureNamespace<'c>),
    BucketDefaultFeatures(InternedString, SemverCompatibility),
    Wide(
        InternedString,
        &'c semver::VersionReq,
        InternedString,
        SemverCompatibility,
    ),
    WideFeatures(
        InternedString,
        &'c semver::VersionReq,
        InternedString,
        SemverCompatibility,
        FeatureNamespace<'c>,
    ),
    WideDefaultFeatures(
        InternedString,
        &'c semver::VersionReq,
        InternedString,
        SemverCompatibility,
    ),
    Links(InternedString),
}

pub fn new_bucket<'c>(
    crate_: InternedString,
    compat: SemverCompatibility,
    all_features: bool,
) -> Names<'c> {
    Names::Bucket(crate_, compat, all_features)
}
pub fn new_wide<'c>(
    crate_: InternedString,
    req: &'c semver::VersionReq,
    from: InternedString,
    compat: SemverCompatibility,
) -> Names<'c> {
    Names::Wide(crate_, req, from, compat)
}
pub fn new_links<'c>(crate_: InternedString) -> Names<'c> {
    Names::Links(crate_)
}

impl<'c> Ord for Names<'c> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        {
            match &self {
                Names::Bucket(c, _, _)
                | Names::BucketFeatures(c, _, _)
                | Names::BucketDefaultFeatures(c, _)
                | Names::Wide(c, _, _, _)
                | Names::WideFeatures(c, _, _, _, _)
                | Names::WideDefaultFeatures(c, _, _, _)
                | Names::Links(c) => *c,
            }
        }
        .cmp({
            match &other {
                Names::Bucket(c, _, _)
                | Names::BucketFeatures(c, _, _)
                | Names::BucketDefaultFeatures(c, _)
                | Names::Wide(c, _, _, _)
                | Names::WideFeatures(c, _, _, _, _)
                | Names::WideDefaultFeatures(c, _, _, _)
                | Names::Links(c) => c,
            }
        })
        .then_with(|| self.to_string().cmp(&other.to_string()))
    }
}

impl<'c> PartialOrd for Names<'c> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<'c> Names<'c> {
    pub fn is_real(&self) -> bool {
        matches!(self, &Self::Bucket(..))
    }
    pub fn crate_(&self) -> InternedString {
        match self {
            Names::Bucket(c, _, _)
            | Names::BucketFeatures(c, _, _)
            | Names::BucketDefaultFeatures(c, _)
            | Names::Wide(c, _, _, _)
            | Names::WideFeatures(c, _, _, _, _)
            | Names::WideDefaultFeatures(c, _, _, _) => *c,
            Names::Links(_) => panic!(),
        }
    }
    pub fn with_default_features(&self) -> Self {
        use Names::*;
        match self {
            Bucket(a, b, _) | BucketFeatures(a, b, _) => BucketDefaultFeatures(*a, *b),
            Wide(a, b, c, d) | WideFeatures(a, b, c, d, _) => WideDefaultFeatures(*a, b, *c, *d),
            Links(_) => panic!(),
            s @ BucketDefaultFeatures(_, _) | s @ WideDefaultFeatures(_, _, _, _) => s.clone(),
        }
    }
    pub fn with_features(&self, feat: FeatureNamespace<'c>) -> Self {
        use Names::*;
        match self {
            Bucket(a, b, _) | BucketFeatures(a, b, _) | BucketDefaultFeatures(a, b) => {
                BucketFeatures(*a, *b, feat)
            }
            Wide(a, b, c, d) | WideFeatures(a, b, c, d, _) | WideDefaultFeatures(a, b, c, d) => {
                WideFeatures(*a, b, *c, *d, feat)
            }
            Links(_) => panic!(),
        }
    }
}

impl<'c> std::fmt::Display for Names<'c> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Names::Bucket(n, m, a) => {
                f.write_str("Bucket:")?;
                f.write_str(n)?;
                f.write_str("@")?;
                f.write_str(&match m {
                    SemverCompatibility::Major(i) => format!("{}.x.y", i),
                    SemverCompatibility::Minor(i) => format!("0.{}.x", i),
                    SemverCompatibility::Patch(i) => format!("0.0.{}", i),
                })?;
                if *a {
                    f.write_str("/All-FEATURES")?;
                }
                Ok(())
            }
            Names::BucketFeatures(n, m, feat) => {
                f.write_str("Bucket:")?;
                f.write_str(n)?;
                f.write_str("@")?;
                f.write_str(&match m {
                    SemverCompatibility::Major(i) => format!("{}.x.y", i),
                    SemverCompatibility::Minor(i) => format!("0.{}.x", i),
                    SemverCompatibility::Patch(i) => format!("0.0.{}", i),
                })?;
                f.write_str("/")?;
                feat.fmt(f)
            }
            Names::BucketDefaultFeatures(n, m) => {
                f.write_str("Bucket:")?;
                f.write_str(n)?;
                f.write_str("@")?;
                f.write_str(&match m {
                    SemverCompatibility::Major(i) => format!("{}.x.y", i),
                    SemverCompatibility::Minor(i) => format!("0.{}.x", i),
                    SemverCompatibility::Patch(i) => format!("0.0.{}", i),
                })?;
                f.write_str("/default=true")
            }
            Names::Wide(c, range, parent, parent_com) => {
                f.write_str("Range:")?;
                f.write_str(c)?;
                f.write_str("(From:")?;
                f.write_str(parent)?;
                f.write_str("@")?;
                f.write_str(&match parent_com {
                    SemverCompatibility::Major(i) => format!("{}.x.y", i),
                    SemverCompatibility::Minor(i) => format!("0.{}.x", i),
                    SemverCompatibility::Patch(i) => format!("0.0.{}", i),
                })?;
                f.write_str("):")?;
                f.write_str(&range.to_string())
            }
            Names::WideFeatures(c, range, parent, parent_com, feat) => {
                f.write_str("Range:")?;
                f.write_str(c)?;
                f.write_str("(From:")?;
                f.write_str(parent)?;
                f.write_str("@")?;
                f.write_str(&match parent_com {
                    SemverCompatibility::Major(i) => format!("{}.x.y", i),
                    SemverCompatibility::Minor(i) => format!("0.{}.x", i),
                    SemverCompatibility::Patch(i) => format!("0.0.{}", i),
                })?;
                f.write_str("):")?;
                f.write_str(&range.to_string())?;
                f.write_str("/")?;
                feat.fmt(f)
            }
            Names::WideDefaultFeatures(c, range, parent, parent_com) => {
                f.write_str("Range:")?;
                f.write_str(c)?;
                f.write_str("(From:")?;
                f.write_str(parent)?;
                f.write_str("@")?;
                f.write_str(&match parent_com {
                    SemverCompatibility::Major(i) => format!("{}.x.y", i),
                    SemverCompatibility::Minor(i) => format!("0.{}.x", i),
                    SemverCompatibility::Patch(i) => format!("0.0.{}", i),
                })?;
                f.write_str("):")?;
                f.write_str(&range.to_string())?;
                f.write_str("/default=true")
            }
            Names::Links(name) => {
                f.write_str("Links:")?;
                f.write_str(name)
            }
        }
    }
}

impl std::fmt::Debug for Names<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self, f)
    }
}

impl serde::Serialize for Names<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
