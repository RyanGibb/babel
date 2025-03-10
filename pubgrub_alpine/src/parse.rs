use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use pubgrub::Range;

use crate::index;
use crate::index::{AlpineIndex, HashedRange};
use crate::version::AlpineVersion;

#[derive(Debug, Clone, PartialEq)]
pub struct Package {
    pub package: String,
    pub version: String,
    pub arch: Option<String>,
    pub depends: Vec<Dependency>,
    pub provides: Vec<Dependency>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Dependency {
    pub package: String,
    pub version_constraint: Option<VersionConstraint>,
    pub arch: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VersionConstraint {
    pub relation: VersionRelation,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VersionRelation {
    Less,
    LessOrEqual,
    Equal,
    GreaterOrEqual,
    Greater,
}

impl FromStr for VersionRelation {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "<" => Ok(VersionRelation::Less),
            "<=" => Ok(VersionRelation::LessOrEqual),
            "=" => Ok(VersionRelation::Equal),
            ">=" => Ok(VersionRelation::GreaterOrEqual),
            ">" => Ok(VersionRelation::Greater),
            _ => Err(format!("Unknown version relation: {}", s)),
        }
    }
}

/// Parse an Alpine dependency item
/// Example formats:
/// - Simple dependency: "libc"
/// - With version constraint: "libc>=1.2.3"
/// - With operators: "so:libc.musl-x86_64.so.1"
fn parse_alpine_dependency_item(s: &str) -> Result<Dependency, Box<dyn Error>> {
    let mut pkg_part = s;
    let mut version_constraint = None;
    let re_patterns = vec![
        (">=", VersionRelation::GreaterOrEqual),
        (">", VersionRelation::Greater),
        ("=", VersionRelation::Equal),
        ("<=", VersionRelation::LessOrEqual),
        ("<", VersionRelation::Less),
    ];
    for (pattern, relation) in re_patterns {
        if let Some(idx) = s.find(pattern) {
            pkg_part = &s[0..idx];
            let version_str = &s[idx + pattern.len()..];
            version_constraint = Some(VersionConstraint {
                relation,
                version: version_str.to_string(),
            });
            break;
        }
    }
    let dep = Dependency {
        package: pkg_part.to_string(),
        version_constraint,
        // TODO
        arch: None,
    };
    Ok(dep)
}

/// Parse a single APKINDEX stanza into an Alpine Package.
/// Each field starts with a letter code followed by a colon.
/// P: package name
/// V: version
/// A: architecture
/// D: dependencies (space separated)
/// p: provides (contains package names and commands)
pub fn parse_alpine_package(stanza: &str) -> Result<Package, Box<dyn Error>> {
    let mut fields: HashMap<String, String> = HashMap::new();

    for line in stanza.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(pos) = line.find(':') {
            let key = line[..pos].trim().to_string();
            let value = line[pos + 1..].trim().to_string();
            fields.insert(key, value);
        } else {
            return Err(format!("Line without colon: {}", line).into());
        }
    }

    let package = fields.remove("P").ok_or("Missing Package field (P)")?;
    let version = fields.remove("V").ok_or("Missing Version field (V)")?;

    let arch = fields.remove("A");

    let depends = match fields.remove("D") {
        Some(s) => {
            let dependencies = s
                .split_whitespace()
                .filter_map(|dep_str| {
                    let trimmed = dep_str.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        match parse_alpine_dependency_item(trimmed) {
                            Ok(dep) => Some(dep),
                            Err(e) => {
                                eprintln!("Error parsing dependency '{}': {}", trimmed, e);
                                None
                            }
                        }
                    }
                })
                .collect();
            dependencies
        }
        None => vec![],
    };

    let provides = match fields.remove("p") {
        Some(s) => {
            let provides = s
                .split_whitespace()
                .filter_map(|prov_str| {
                    let trimmed = prov_str.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        match parse_alpine_dependency_item(trimmed) {
                            Ok(dep) => Some(dep),
                            Err(e) => {
                                eprintln!("Error parsing provides '{}': {}", trimmed, e);
                                None
                            }
                        }
                    }
                })
                .collect();
            provides
        }
        None => vec![],
    };

    Ok(Package {
        package,
        version,
        arch,
        depends,
        provides,
    })
}

pub fn parse_alpine_index<P: AsRef<Path>>(path: P) -> Result<Vec<Package>, Box<dyn Error>> {
    let content = fs::read_to_string(path)?;
    let stanzas: Vec<&str> = content
        .split("\n\n")
        .filter(|s| !s.trim().is_empty())
        .collect();
    let mut packages = Vec::new();
    for stanza in stanzas {
        packages.push(parse_alpine_package(stanza)?);
    }
    Ok(packages)
}

pub fn version_constraint_to_range(
    relop: &VersionRelation,
    version: AlpineVersion,
) -> Range<AlpineVersion> {
    match relop {
        VersionRelation::Equal => Range::<AlpineVersion>::singleton(version),
        VersionRelation::GreaterOrEqual => Range::<AlpineVersion>::higher_than(version),
        VersionRelation::Greater => Range::<AlpineVersion>::strictly_higher_than(version),
        VersionRelation::Less => Range::<AlpineVersion>::strictly_lower_than(version),
        VersionRelation::LessOrEqual => Range::<AlpineVersion>::lower_than(version),
    }
}

fn convert_dependency(dep: &Dependency) -> index::Dependency {
    let range = match &dep.version_constraint {
        Some(vc) => {
            let version = AlpineVersion(vc.version.clone());
            version_constraint_to_range(&vc.relation, version)
        }
        None => Range::full(),
    };
    index::Dependency {
        name: dep.package.clone(),
        range: HashedRange(range),
    }
}

fn convert_dependency_field(parsed: &Vec<crate::parse::Dependency>) -> Vec<index::Dependency> {
    parsed.iter().map(|dep| convert_dependency(dep)).collect()
}

pub fn create_index<P: AsRef<Path>>(path: P) -> Result<AlpineIndex, Box<dyn Error>> {
    let alpine_packages = parse_alpine_index(path)?;
    let mut index = AlpineIndex::new();
    for ap in alpine_packages {
        let ver = AlpineVersion::from_str(&ap.version)
            .map_err(|e| format!("Error parsing version {}: {}", ap.version, e))?;
        let dependencies = convert_dependency_field(&ap.depends);
        index.add_deps(&ap.package, ver, dependencies);
        let provides = convert_dependency_field(&ap.provides);
        for provided in provides {
            index.add_deps(
                provided.name.as_str(),
                AlpineVersion(ap.package.clone()),
                // TODO versioned provides, Range::as_singleton(dep.range.0)?,
                vec![index::Dependency {
                    name: ap.package.clone(),
                    range: HashedRange(Range::singleton(AlpineVersion(ap.version.clone()))),
                }],
            )
        }
    }
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_alpine_dependency() {
        // Test dependency with version constraint
        let dep_str = "libc>=1.2.3";
        let dep = parse_alpine_dependency_item(dep_str).unwrap();
        assert_eq!(dep.package, "libc");
        assert!(dep.version_constraint.is_some());
        let vc = dep.version_constraint.as_ref().unwrap();
        assert_eq!(vc.version, "1.2.3");
        assert_eq!(vc.relation, VersionRelation::GreaterOrEqual);
    }

    #[test]
    fn test_parse_so_dependency() {
        // Test SO dependency
        let dep_str = "so:libc.musl-x86_64.so.1";
        let dep = parse_alpine_dependency_item(dep_str).unwrap();
        assert_eq!(dep.package, "so:libc.musl-x86_64.so.1");
        assert!(dep.version_constraint.is_none());
    }

    #[test]
    fn test_parse_alpine_package() -> Result<(), Box<dyn Error>> {
        let sample = r#"C:Q1ssmf0Td4W/0BrJdhzbhot4cQkCs=
P:git-gitk
V:2.26.3-r1
A:x86_64
S:167766
I:843776
T:Gitk interface for git
U:https://www.git-scm.com/
L:GPL-2.0-or-later
o:git
m:Natanael Copa <ncopa@alpinelinux.org>
t:1647343650
c:234c8fe7737f97ec355b069a1f0c8764af8b7e43
D:git=2.26.3-r1 tcl tk
p:cmd:gitk
"#;
        let pkg = parse_alpine_package(sample)?;
        assert_eq!(pkg.package, "git-gitk");
        assert_eq!(pkg.version, "2.26.3-r1");
        assert_eq!(pkg.arch, Some("x86_64".to_string()));
        assert_eq!(pkg.depends.len(), 3); // git=2.26.3-r1, tcl, tk

        // Check dependencies
        let dep1 = &pkg.depends[0];
        assert_eq!(dep1.package, "git");
        assert!(dep1.version_constraint.is_some());
        let vc = &dep1.version_constraint.as_ref().unwrap();
        assert_eq!(vc.relation, VersionRelation::Equal);
        assert_eq!(vc.version, "2.26.3-r1");

        let dep2 = &pkg.depends[1];
        assert_eq!(dep2.package, "tcl");
        assert!(dep2.version_constraint.is_none());

        let dep3 = &pkg.depends[2];
        assert_eq!(dep3.package, "tk");
        assert!(dep3.version_constraint.is_none());

        assert_eq!(pkg.provides.len(), 1);
        assert_eq!(pkg.provides[0].package, "cmd:gitk");

        println!("{:?}", pkg);
        Ok(())
    }

    // Disable these tests until we have the data files properly set up
    #[test]
    fn test_alpine_3_12() -> Result<(), Box<dyn Error>> {
        // Use an absolute path for testing
        let repo_path = std::env::current_dir()?.join("repositories/3.12/APKINDEX");
        let pkgs = parse_alpine_index(repo_path)?;
        println!("Found {} packages in Alpine 3.12", pkgs.len());
        Ok(())
    }

    #[test]
    fn test_alpine_3_13() -> Result<(), Box<dyn Error>> {
        // Use an absolute path for testing
        let repo_path = std::env::current_dir()?.join("repositories/3.13/APKINDEX");
        let pkgs = parse_alpine_index(repo_path)?;
        println!("Found {} packages in Alpine 3.13", pkgs.len());
        Ok(())
    }

    #[test]
    fn test_alpine_3_14() -> Result<(), Box<dyn Error>> {
        // Use an absolute path for testing
        let repo_path = std::env::current_dir()?.join("repositories/3.14/APKINDEX");
        let pkgs = parse_alpine_index(repo_path)?;
        println!("Found {} packages in Alpine 3.14", pkgs.len());
        Ok(())
    }

    #[test]
    fn test_alpine_3_12_index() -> Result<(), Box<dyn Error>> {
        // Use an absolute path for testing
        let repo_path = std::env::current_dir()?.join("repositories/3.12/APKINDEX");
        let index = create_index(repo_path)?;
        println!("Created index with {} packages", index.package_count());
        Ok(())
    }
}
