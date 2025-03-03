use core::fmt::Display;
use pubgrub::Range;
use std::cell::Cell;
use std::hash::{Hash, Hasher};

use crate::opam_version::OpamVersion;
use crate::parse::{available_versions_from_repo, RelOp};

pub type PackageName = String;

pub struct Index {
    pub repo: String,
    pub debug: Cell<bool>,
    pub version_debug: Cell<bool>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Binary<T> {
    pub lhs: Box<T>,
    pub rhs: Box<T>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct HashedRange(pub Range<OpamVersion>);

impl Hash for HashedRange {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let s = format!("{}", self.0);
        s.hash(state);
    }
}

impl Display for HashedRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum VersionFormula {
    Version(HashedRange),
    Lit(OpamVersion),
    Variable(String),
    Not(String),
    And(Binary<VersionFormula>),
    Or(Binary<VersionFormula>),
    Comparator {
        relop: RelOp,
        binary: Binary<VersionFormula>,
    },
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum PackageFormula {
    Or(Binary<PackageFormula>),
    And(Binary<PackageFormula>),
    Base {
        name: PackageName,
        formula: VersionFormula,
    },
    ConflictClass {
        name: PackageName,
        package: PackageName,
    },
}

impl Display for RelOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelOp::Eq => write!(f, "="),
            RelOp::Geq => write!(f, ">="),
            RelOp::Gt => write!(f, ">"),
            RelOp::Leq => write!(f, "<="),
            RelOp::Lt => write!(f, "<"),
            RelOp::Neq => write!(f, "!="),
        }
    }
}

impl Display for VersionFormula {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionFormula::Variable(variable) => {
                write!(f, "{}", variable)
            }
            VersionFormula::Not(variable) => {
                write!(f, "!{}", variable)
            }
            VersionFormula::Lit(literal) => {
                write!(f, "{}", literal)
            }
            VersionFormula::Version(version) => {
                write!(f, "= {}", version)
            }
            VersionFormula::And(binary) => {
                write!(f, "({} & {})", binary.lhs, binary.rhs)
            }
            VersionFormula::Or(binary) => {
                write!(f, "({} | {})", binary.lhs, binary.rhs)
            }
            VersionFormula::Comparator { relop, binary } => {
                // infix notation
                write!(f, "({} {} {})", binary.lhs, relop, binary.rhs)
            }
        }
    }
}

impl Display for PackageFormula {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageFormula::Base { name, formula } => {
                write!(f, "({} {{{}}})", name, formula)
            }
            PackageFormula::ConflictClass { name, package } => {
                write!(f, "(Conflict class ({}, {}) )", name, package)
            }
            PackageFormula::And(binary) => {
                write!(f, "({} & {})", binary.lhs, binary.rhs)
            }
            PackageFormula::Or(binary) => {
                write!(f, "({} | {})", binary.lhs, binary.rhs)
            }
        }
    }
}

impl Index {
    pub fn new(repo: String) -> Self {
        Self {
            repo,
            debug: false.into(),
            version_debug: false.into(),
        }
    }

    pub fn available_versions(&self, package: &PackageName) -> Vec<OpamVersion> {
        available_versions_from_repo(self.repo.as_str(), package).unwrap()
    }

    pub fn set_debug(&self, flag: bool) {
        self.debug.set(flag);
    }

    pub fn set_version_debug(&self, flag: bool) {
        self.version_debug.set(flag);
    }
}
