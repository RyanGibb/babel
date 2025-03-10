use crate::index::{Binary, OpamIndex, PackageFormula, VersionFormula};
use crate::parse::{negate_relop, parse_dependencies_for_package_version, relop_to_range, RelOp};
use crate::version::OpamVersion;
use core::fmt::Display;
use pubgrub::{Dependencies, DependencyConstraints, DependencyProvider, Map, Range};
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::str::FromStr;
use std::sync::{LazyLock, Mutex};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum OpamPackage {
    Root(Vec<(OpamPackage, Range<OpamVersion>)>),
    Base(String),
    Depext(Vec<String>),
    ConflictClass(String),
    Lor {
        lhs: Box<PackageFormula>,
        rhs: Box<PackageFormula>,
    },
    Formula {
        base: Box<OpamPackage>,
        formula: Box<VersionFormula>,
    },
    Proxy {
        base: Box<Option<OpamPackage>>,
        formula: Box<VersionFormula>,
    },
    Var(String),
}

pub static VARIABLE_CACHE: LazyLock<Mutex<HashMap<String, HashSet<OpamVersion>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static CONFLICT_CLASS_CACHE: LazyLock<Mutex<HashMap<String, HashSet<OpamVersion>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

impl FromStr for OpamPackage {
    type Err = String;
    fn from_str(pkg: &str) -> Result<Self, Self::Err> {
        let mut pkg_parts = pkg.split('/');
        match (pkg_parts.next(), pkg_parts.next()) {
            (Some(base), None) => Ok(OpamPackage::Base(base.to_string())),
            _ => Err(format!("{} is not a valid package name", pkg)),
        }
    }
}

impl Display for OpamPackage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpamPackage::Root(_) => write!(f, "Root"),
            OpamPackage::Base(pkg) => write!(f, "{}", pkg),
            OpamPackage::Depext(pkgs) => write!(f, "{:?}", pkgs),
            OpamPackage::ConflictClass(pkg) => write!(f, "Conflict class {}", pkg),
            OpamPackage::Lor { lhs, rhs } => write!(f, "{} | {}", lhs, rhs),
            OpamPackage::Formula { base, formula } => write!(f, "{} {{{}}}", base, formula),
            OpamPackage::Proxy { base, formula } => match *base.clone() {
                Some(base) => write!(f, "{} {{{}}}", base, formula),
                None => write!(f, "{{{}}}", formula),
            },
            OpamPackage::Var(var) => write!(f, "`{}`", var),
        }
    }
}

static LHS_VERSION: LazyLock<OpamVersion> = LazyLock::new(|| OpamVersion("lhs".to_string()));
static RHS_VERSION: LazyLock<OpamVersion> = LazyLock::new(|| OpamVersion("rhs".to_string()));

pub static TRUE_VERSION: LazyLock<OpamVersion> = LazyLock::new(|| OpamVersion("true".to_string()));
pub static FALSE_VERSION: LazyLock<OpamVersion> =
    LazyLock::new(|| OpamVersion("false".to_string()));

impl OpamIndex {
    pub fn list_versions(&self, package: &OpamPackage) -> impl Iterator<Item = OpamVersion> + '_ {
        let versions = match package {
            OpamPackage::Root(_) => vec![OpamVersion("".to_string())],
            OpamPackage::Depext(_) => vec![OpamVersion("".to_string())],
            OpamPackage::Base(pkg) => self.available_versions(pkg),
            OpamPackage::ConflictClass(pkg) => CONFLICT_CLASS_CACHE
                .lock()
                .unwrap()
                .get(pkg)
                .unwrap()
                .iter()
                .cloned()
                .collect(),
            OpamPackage::Lor { lhs: _, rhs: _ } => vec![LHS_VERSION.clone(), RHS_VERSION.clone()],
            OpamPackage::Var(var) => match var.as_str() {
                "os" => vec![
                    OpamVersion("linux".to_string()),
                    OpamVersion("macos".to_string()),
                    OpamVersion("win32".to_string()),
                    OpamVersion("cygwin".to_string()),
                    OpamVersion("freebsd".to_string()),
                    OpamVersion("openbsd".to_string()),
                    OpamVersion("netbsd".to_string()),
                    OpamVersion("dragonfly".to_string()),
                ],
                "arch" => vec![
                    OpamVersion("arm64".to_string()),
                    OpamVersion("x86_32".to_string()),
                    OpamVersion("x86_64".to_string()),
                    OpamVersion("ppc32".to_string()),
                    OpamVersion("ppc64".to_string()),
                    OpamVersion("arm32".to_string()),
                    OpamVersion("arm64".to_string()),
                ],
                _ => match VARIABLE_CACHE.lock().unwrap().get(var) {
                    Some(m) => m.iter().cloned().collect(),
                    None => vec![FALSE_VERSION.clone(), TRUE_VERSION.clone()],
                },
            },
            OpamPackage::Formula {
                base: _,
                formula: _,
            } => vec![FALSE_VERSION.clone(), TRUE_VERSION.clone()],
            OpamPackage::Proxy {
                base: _,
                formula: _,
            } => vec![LHS_VERSION.clone(), RHS_VERSION.clone()],
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

impl DependencyProvider for OpamIndex {
    type P = OpamPackage;

    type V = OpamVersion;

    type VS = Range<OpamVersion>;

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
        package: &OpamPackage,
        version: &OpamVersion,
    ) -> Result<Dependencies<Self::P, Self::VS, Self::M>, Self::Err> {
        match package {
            OpamPackage::Root(deps) => {
                for (package, range) in deps {
                    match package {
                        OpamPackage::Var(var) => {
                            // TODO support enumerating versions with OR's
                            if let Some(ver) = range.as_singleton() {
                                VARIABLE_CACHE
                                    .lock()
                                    .unwrap()
                                    .entry(var.to_string())
                                    .or_insert_with(HashSet::new)
                                    .insert(ver.clone());
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Dependencies::Available(deps.into_iter().cloned().collect()))
            }
            OpamPackage::Base(pkg) => {
                let formulas = parse_dependencies_for_package_version(
                    self.repo.as_str(),
                    pkg,
                    version.to_string().as_str(),
                )
                .unwrap();
                let deps = from_formulas(&formulas);
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
            OpamPackage::ConflictClass(_) => Ok(Dependencies::Available(Map::default())),
            OpamPackage::Lor { lhs, rhs } => {
                let deps = match version {
                    OpamVersion(ver) => match ver.as_str() {
                        "lhs" => from_formula(*&lhs),
                        "rhs" => from_formula(*&rhs),
                        _ => panic!("Unknown OR version {}", version),
                    },
                };
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
            OpamPackage::Formula { base, formula } => {
                let deps = match version {
                    OpamVersion(ver) => match ver.as_str() {
                        "true" => from_version_formula(Some(&base), formula),
                        "false" => {
                            from_version_formula(None, &Box::new(negate_formula(*formula.clone())))
                        }
                        _ => panic!("Unknown Formula version {}", version),
                    },
                };
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
            OpamPackage::Proxy { base, formula } => {
                let deps = from_proxy_formula(base.as_ref().as_ref(), version, formula);
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
            OpamPackage::Var(_) => {
                if self.debug.get() {
                    println!("({}, {})", package, version);
                }
                Ok(Dependencies::Available(Map::default()))
            }
            OpamPackage::Depext(_) => {
                if self.debug.get() {
                    println!("({}, {})", package, version);
                }
                Ok(Dependencies::Available(Map::default()))
            }
        }
    }
}

pub fn from_formulas(
    formulas: &Vec<PackageFormula>,
) -> DependencyConstraints<OpamPackage, Range<OpamVersion>> {
    formulas
        .iter()
        .map(|formula| from_formula(formula))
        .fold(Map::default(), |acc, cons| merge_constraints(acc, cons))
}

fn from_formula(
    formula: &PackageFormula,
) -> DependencyConstraints<OpamPackage, Range<OpamVersion>> {
    match formula {
        PackageFormula::Base { name, formula } => {
            let mut map = Map::default();
            match formula {
                // in parse.rs we collapse non-filtered formula to a single version dependency
                VersionFormula::Version(range) => {
                    map.insert(OpamPackage::Base(name.to_string()), range.0.clone())
                }
                // otherwise, we need to introduce a formula packge to select variable values
                _ => map.insert(
                    OpamPackage::Formula {
                        base: Box::new(OpamPackage::Base(name.to_string())),
                        formula: Box::new(formula.clone()),
                    },
                    Range::full(),
                ),
            };
            map
        }
        PackageFormula::Depext { names, formula } => {
            let mut map = Map::default();
            match formula {
                // in parse.rs we collapse non-filtered formula to a single version dependency
                VersionFormula::Version(range) => {
                    map.insert(OpamPackage::Depext(names.to_vec()), range.0.clone())
                }
                // otherwise, we need to introduce a formula packge to select variable values
                _ => map.insert(
                    OpamPackage::Formula {
                        base: Box::new(OpamPackage::Depext(names.to_vec())),
                        formula: Box::new(formula.clone()),
                    },
                    Range::full(),
                ),
            };
            map
        }
        PackageFormula::ConflictClass { name, package } => {
            let mut map = Map::default();
            map.insert(
                OpamPackage::ConflictClass(name.to_string()),
                Range::<OpamVersion>::singleton(OpamVersion(package.to_string())),
            );
            CONFLICT_CLASS_CACHE
                .lock()
                .unwrap()
                .entry(name.to_string())
                .or_insert_with(HashSet::new)
                .insert(OpamVersion(package.to_string()));
            map
        }
        PackageFormula::Or(Binary { lhs, rhs }) => {
            let mut map = Map::default();
            map.insert(
                OpamPackage::Lor {
                    lhs: lhs.clone(),
                    rhs: rhs.clone(),
                },
                Range::full(),
            );
            map
        }
        PackageFormula::And(Binary { lhs, rhs }) => {
            let left = from_formula(lhs);
            let right = from_formula(rhs);
            merge_constraints(left, right)
        }
    }
}

fn merge_constraints(
    mut left: DependencyConstraints<OpamPackage, Range<OpamVersion>>,
    right: DependencyConstraints<OpamPackage, Range<OpamVersion>>,
) -> DependencyConstraints<OpamPackage, Range<OpamVersion>> {
    for (pkg, range) in right {
        left.entry(pkg.clone())
            .and_modify(|existing| {
                match pkg {
                    _ => *existing = existing.intersection(&range),
                };
            })
            .or_insert(range);
    }
    left
}

// we depend on this if we don't select a formula
fn negate_formula(expr: VersionFormula) -> VersionFormula {
    match expr {
        // we strip out all versions, and only select variable values
        VersionFormula::Version(_) => panic!("we should never get here"),
        VersionFormula::Variable(variable) => VersionFormula::Not(variable),
        VersionFormula::Not(variable) => VersionFormula::Variable(variable),
        VersionFormula::And(Binary { lhs, rhs }) => match (*lhs.clone(), *rhs.clone()) {
            // strip out versions
            // if there's two versions, we propigate one and it will be stripped above
            (VersionFormula::Version(_), _) => negate_formula(*rhs),
            (_, VersionFormula::Version(_)) => negate_formula(*lhs),
            // De Morgan’s laws
            _ => VersionFormula::Or(Binary {
                lhs: Box::new(negate_formula(*lhs)),
                rhs: Box::new(negate_formula(*rhs)),
            }),
        },
        VersionFormula::Or(Binary { lhs, rhs }) => match (*lhs.clone(), *rhs.clone()) {
            // strip out versions
            // if there's two versions, we propigate one and it will be stripped above
            (VersionFormula::Version(_), _) => negate_formula(*rhs),
            (_, VersionFormula::Version(_)) => negate_formula(*lhs),
            // De Morgan’s laws
            _ => VersionFormula::And(Binary {
                lhs: Box::new(negate_formula(*lhs)),
                rhs: Box::new(negate_formula(*rhs)),
            }),
        },
        VersionFormula::Comparator { relop, binary } => VersionFormula::Comparator {
            relop: negate_relop(relop),
            binary,
        },
        VersionFormula::Lit(lit) => VersionFormula::Lit(lit),
    }
}

fn from_proxy_formula(
    base: Option<&OpamPackage>,
    version: &OpamVersion,
    formula: &VersionFormula,
) -> DependencyConstraints<OpamPackage, Range<OpamVersion>> {
    // let mut map = Map::default();
    match formula {
        VersionFormula::Or(Binary { lhs, rhs }) => match version {
            OpamVersion(ver) => match ver.as_str() {
                "lhs" => from_version_formula(base, lhs),
                "rhs" => from_version_formula(base, rhs),
                _ => panic!("Unknown Formula version {}", version),
            },
        },
        VersionFormula::Comparator { relop, binary } => match relop {
            RelOp::Eq => match version {
                OpamVersion(ver) => match ver.as_str() {
                    "lhs" => {
                        let lhs = from_version_formula(base, &*binary.lhs);
                        let rhs = from_version_formula(base, &*binary.rhs);
                        merge_constraints(lhs, rhs)
                    }
                    "rhs" => {
                        let lhs = from_version_formula(base, &negate_formula(*binary.lhs.clone()));
                        let rhs = from_version_formula(base, &negate_formula(*binary.rhs.clone()));
                        merge_constraints(lhs, rhs)
                    }
                    _ => panic!("Unknown Formula version {}", version),
                },
            },
            RelOp::Neq => match version {
                OpamVersion(ver) => match ver.as_str() {
                    "lhs" => {
                        let lhs = from_version_formula(base, &*binary.lhs);
                        let rhs = from_version_formula(base, &negate_formula(*binary.rhs.clone()));
                        merge_constraints(lhs, rhs)
                    }
                    "rhs" => {
                        let lhs = from_version_formula(base, &negate_formula(*binary.lhs.clone()));
                        let rhs = from_version_formula(base, &*binary.rhs);
                        merge_constraints(lhs, rhs)
                    }
                    _ => panic!("Unknown Formula version {}", version),
                },
            },
            _ => match base {
                Some(base) => panic!("invalid operator for {}: {}", base, formula),
                None => panic!("invalid operator for {}", formula),
            },
        },
        _ => panic!("This formula shouldn't be in a proxy: {}", formula),
    }
}

fn from_version_formula(
    base: Option<&OpamPackage>,
    formula: &VersionFormula,
) -> DependencyConstraints<OpamPackage, Range<OpamVersion>> {
    let mut map = Map::default();
    match formula {
        VersionFormula::Version(range) => {
            if let Some(base) = base {
                map.insert(base.clone(), range.0.clone());
            };
            map
        }
        VersionFormula::Variable(variable) => {
            if let Some(base) = base {
                map.insert(base.clone(), Range::full());
            };
            map.insert(
                OpamPackage::Var(variable.to_string()),
                Range::singleton(TRUE_VERSION.clone()),
            );
            map
        }
        VersionFormula::Not(variable) => {
            if let Some(base) = base {
                map.insert(base.clone(), Range::full());
            };
            map.insert(
                OpamPackage::Var(variable.to_string()),
                Range::singleton(FALSE_VERSION.clone()),
            );
            map
        }
        VersionFormula::Or(_) => {
            map.insert(
                OpamPackage::Proxy {
                    base: Box::new(base.cloned()),
                    formula: Box::new(formula.clone()),
                },
                Range::full(),
            );
            map
        }
        VersionFormula::And(Binary { lhs, rhs }) => {
            let left = from_version_formula(base, lhs);
            let right = from_version_formula(base, rhs);
            merge_constraints(left, right)
        }
        VersionFormula::Comparator { relop, binary } => {
            if let Some(base) = base {
                map.insert(base.clone(), Range::full());
            };
            match (*binary.lhs.clone(), *binary.rhs.clone()) {
                (VersionFormula::Lit(ver), VersionFormula::Variable(var)) => {
                    VARIABLE_CACHE
                        .lock()
                        .unwrap()
                        .entry(var.to_string())
                        .or_insert_with(HashSet::new)
                        .insert(ver.clone());
                    let range = relop_to_range(relop, ver);
                    map.insert(OpamPackage::Var(var.to_string()), range)
                }
                (VersionFormula::Variable(var), VersionFormula::Lit(ver)) => {
                    VARIABLE_CACHE
                        .lock()
                        .unwrap()
                        .entry(var.to_string())
                        .or_insert_with(HashSet::new)
                        .insert(ver.clone());
                    let range = relop_to_range(relop, ver);
                    map.insert(OpamPackage::Var(var.to_string()), range)
                }
                _ => match relop {
                    RelOp::Eq | RelOp::Neq => map.insert(
                        OpamPackage::Proxy {
                            base: Box::new(base.cloned()),
                            formula: Box::new(formula.clone()),
                        },
                        Range::full(),
                    ),
                    _ => match base {
                        Some(base) => panic!("invalid operator for {}: {}", base, formula),
                        None => panic!("invalid operator for {}", formula),
                    },
                },
            };
            map
        }
        VersionFormula::Lit(lit) => match base {
            Some(base) => panic!("invalid literal for {} {{{}}}: {}", base, formula, lit),
            None => panic!("invalid literal for {{{}}}: {}", formula, lit),
        },
    }
}
