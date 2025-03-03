use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use pubgrub::Range;

use crate::debian_version::DebianVersion;
use crate::index;
use crate::index::{HashedRange, Index};

#[derive(Debug, Clone, PartialEq)]
pub struct DebianPackage {
    pub package: String,
    pub version: String,
    pub depends: Vec<Dependency>,
    pub provides: Vec<Dependency>,
}

/// A dependency item is a list of alternatives (separated by the '|' symbol).
#[derive(Debug, Clone, PartialEq)]
pub struct Dependency {
    pub alternatives: Vec<Alternative>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Alternative {
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
    StrictlyEarlier, // <<
    EarlierOrEqual,  // <=
    ExactlyEqual,    // =
    LaterOrEqual,    // >=
    StrictlyLater,   // >>
}

impl FromStr for VersionRelation {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "<<" => Ok(VersionRelation::StrictlyEarlier),
            "<=" => Ok(VersionRelation::EarlierOrEqual),
            "=" => Ok(VersionRelation::ExactlyEqual),
            ">=" => Ok(VersionRelation::LaterOrEqual),
            ">>" => Ok(VersionRelation::StrictlyLater),
            _ => Err(format!("Unknown version relation: {}", s)),
        }
    }
}

/// Parse a version constraint string (e.g. ">= 2.2.1") into a VersionConstraint.
fn parse_version_constraint(s: &str) -> Result<VersionConstraint, Box<dyn Error>> {
    // Split on whitespace; expect two parts: the relation and the version.
    let parts: Vec<&str> = s.trim().split_whitespace().collect();
    if parts.len() < 2 {
        return Err(format!("Invalid version constraint: {}", s).into());
    }
    let relation = parts[0].parse::<VersionRelation>()?;
    let version = parts[1..].join(" ");
    Ok(VersionConstraint { relation, version })
}

/// Parse a single dependency alternative.
/// Example alternative strings:
///   "libc6 (>= 2.2.1)"
///   "libqt5core5a (>= 5.7.0) [amd64 i386]"
///   "default-mta"
fn parse_alternative(s: &str) -> Result<Alternative, Box<dyn Error>> {
    let s = s.trim();
    // Look for a version constraint: find the first '('.
    let (pkg_part, remainder) = if let Some(start) = s.find('(') {
        let pkg = s[..start].trim();
        // Find the matching closing ')'
        let end = s
            .find(')')
            .ok_or("Missing closing parenthesis in version constraint")?;
        let constraint_str = s[start + 1..end].trim();
        let after = s[end + 1..].trim();
        (pkg, Some((constraint_str, after)))
    } else {
        (s, None)
    };

    let version_constraint = if let Some((constraint_str, _)) = remainder {
        Some(parse_version_constraint(constraint_str)?)
    } else {
        None
    };

    // Look for architecture restrictions in square brackets, if present.
    let arch = if let Some((_, after)) = remainder {
        if let Some(start) = after.find('[') {
            let end = after
                .find(']')
                .ok_or("Missing closing bracket for architecture restrictions")?;
            let arch_str = &after[start + 1..end];
            let archs = arch_str
                .split_whitespace()
                .map(|s| s.to_string())
                .collect::<Vec<_>>();
            if !archs.is_empty() {
                Some(archs)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        // If no version constraint, check entire string.
        if let Some(start) = s.find('[') {
            let end = s
                .find(']')
                .ok_or("Missing closing bracket for architecture restrictions")?;
            let arch_str = &s[start + 1..end];
            let archs = arch_str
                .split_whitespace()
                .map(|s| s.to_string())
                .collect::<Vec<_>>();
            if !archs.is_empty() {
                Some(archs)
            } else {
                None
            }
        } else {
            None
        }
    };

    Ok(Alternative {
        package: pkg_part.to_string(),
        version_constraint,
        arch,
    })
}

/// Parse a dependency item (which may contain alternatives separated by '|')
fn parse_dependency_item(s: &str) -> Result<Dependency, Box<dyn Error>> {
    let alternatives: Result<Vec<Alternative>, Box<dyn Error>> =
        s.split('|').map(|alt| parse_alternative(alt)).collect();
    Ok(Dependency {
        alternatives: alternatives?,
    })
}

/// Parse the entire Depends field (a comma-separated list of dependency items)
fn parse_dependency_field(s: &str) -> Vec<Dependency> {
    let dependencies: Vec<Dependency> = s
        .split(',')
        .filter_map(|dep_str| {
            let trimmed = dep_str.trim();
            if trimmed.is_empty() {
                None
            } else {
                match parse_dependency_item(trimmed) {
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

/// Parse a single control file stanza into a DebianPackage.
/// This simplified parser assumes that each field is "Field: value" on a single line
/// (with simple support for continuation lines).
pub fn parse_debian_package(stanza: &str) -> Result<DebianPackage, Box<dyn Error>> {
    let mut fields: HashMap<String, String> = HashMap::new();
    let mut current_key: Option<String> = None;
    let mut current_value = String::new();

    for line in stanza.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            // Continuation line.
            current_value.push(' ');
            current_value.push_str(line.trim());
        } else {
            if let Some(key) = current_key.take() {
                fields.insert(key, current_value.trim().to_string());
                current_value.clear();
            }
            if let Some(pos) = line.find(':') {
                let key = line[..pos].trim().to_lowercase();
                let value = line[pos + 1..].trim().to_string();
                current_key = Some(key);
                current_value = value;
            } else {
                return Err(format!("Line without colon: {}", line).into());
            }
        }
    }
    if let Some(key) = current_key {
        fields.insert(key, current_value.trim().to_string());
    }
    let package = fields.remove("package").ok_or("Missing Package field")?;
    let version = fields.remove("version").ok_or("Missing Version field")?;
    let depends = match fields.remove("depends") {
        Some(s) => parse_dependency_field(&s),
        None => parse_dependency_field(""),
    };
    let provides = match fields.remove("provides") {
        Some(s) => parse_dependency_field(&s),
        None => parse_dependency_field(""),
    };

    Ok(DebianPackage {
        package,
        version,
        depends,
        provides,
    })
}

/// Parse an entire control file (which may contain multiple stanzas)
/// into a vector of DebianPackage entries.
pub fn parse_debian_control<P: AsRef<Path>>(path: P) -> Result<Vec<DebianPackage>, Box<dyn Error>> {
    let content = fs::read_to_string(path)?;
    let stanzas: Vec<&str> = content
        .split("\n\n")
        .filter(|s| !s.trim().is_empty())
        .collect();
    let mut packages = Vec::new();
    for stanza in stanzas {
        packages.push(parse_debian_package(stanza)?);
    }
    Ok(packages)
}

pub fn version_constraint_to_range(
    relop: &VersionRelation,
    version: DebianVersion,
) -> Range<DebianVersion> {
    match relop {
        VersionRelation::ExactlyEqual => Range::<DebianVersion>::singleton(version),
        VersionRelation::LaterOrEqual => Range::<DebianVersion>::higher_than(version),
        VersionRelation::StrictlyLater => Range::<DebianVersion>::strictly_higher_than(version),
        VersionRelation::StrictlyEarlier => Range::<DebianVersion>::strictly_lower_than(version),
        VersionRelation::EarlierOrEqual => Range::<DebianVersion>::lower_than(version),
    }
}

fn convert_alternative(alt: &Alternative) -> index::Alternative {
    let range = match &alt.version_constraint {
        Some(vc) => {
            let version = DebianVersion(vc.version.clone());
            version_constraint_to_range(&vc.relation, version)
        }
        None => Range::full(),
    };
    index::Alternative {
        name: alt.package.clone(),
        range: HashedRange(range),
    }
}

fn convert_dependency(dep: &Dependency) -> index::Dependency {
    let alternatives = dep
        .alternatives
        .iter()
        .map(|alt| convert_alternative(alt))
        .collect();
    index::Dependency { alternatives }
}

fn convert_dependency_field(parsed: &Vec<crate::parse::Dependency>) -> Vec<index::Dependency> {
    parsed.iter().map(|dep| convert_dependency(dep)).collect()
}

pub fn create_index<P: AsRef<Path>>(path: P) -> Result<Index, Box<dyn Error>> {
    let debian_packages = parse_debian_control(path)?;
    let mut index = Index::new();
    for dp in debian_packages {
        let ver = DebianVersion::from_str(&dp.version)
            .map_err(|e| format!("Error parsing version {}: {}", dp.version, e))?;
        let dependencies = convert_dependency_field(&dp.depends);
        index.add_deps(&dp.package, ver, dependencies);
        let provides = convert_dependency_field(&dp.provides);
        for provided in provides {
            match &provided.alternatives[..] {
                [dep] => index.add_deps(
                    dep.name.as_str(),
                    DebianVersion(dp.package.clone()),
                    // TODO versioned provides, Range::as_singleton(dep.range.0)?,
                    vec![index::Dependency {
                        alternatives: vec![index::Alternative {
                            name: dp.package.clone(),
                            range: HashedRange(Range::singleton(DebianVersion(dp.version.clone()))),
                        }],
                    }],
                ),
                _ => panic!(""),
            };
        }
    }
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dependency_alternative() {
        // Test alternatives with version constraint and architecture.
        let alt_str = "libqt5core5a (>= 5.7.0) [amd64 i386]";
        let alt = parse_alternative(alt_str).unwrap();
        assert_eq!(alt.package, "libqt5core5a");
        assert!(alt.version_constraint.is_some());
        let vc = alt.version_constraint.unwrap();
        assert_eq!(vc.version, "5.7.0");
        assert_eq!(vc.relation, VersionRelation::LaterOrEqual);
        assert!(alt.arch.is_some());
        let archs = alt.arch.unwrap();
        assert_eq!(archs, vec!["amd64".to_string(), "i386".to_string()]);
    }

    #[test]
    fn test_parse_dependency_field() {
        let s = "libc6 (>= 2.2.1), default-mta | mail-transport-agent";
        let dependencies = parse_dependency_field(s);
        assert_eq!(dependencies.len(), 2);

        let dep1 = &dependencies[0];
        assert_eq!(dep1.alternatives.len(), 1);
        let alt1 = &dep1.alternatives[0];
        assert_eq!(alt1.package, "libc6");
        assert!(alt1.version_constraint.is_some());

        let dep2 = &dependencies[1];
        assert_eq!(dep2.alternatives.len(), 2);
        let alt2a = &dep2.alternatives[0];
        assert_eq!(alt2a.package, "default-mta");
        let alt2b = &dep2.alternatives[1];
        assert_eq!(alt2b.package, "mail-transport-agent");
    }

    #[test]
    fn test_parse_debian_package() -> Result<(), Box<dyn Error>> {
        let sample = r#"Package: mutt
Version: 1.3.17-1
Depends: libc6 (>= 2.2.1), default-mta | mail-transport-agent
Maintainer: John Doe <john@example.com>
Description: Email client
"#;
        let pkg = parse_debian_package(sample)?;
        assert_eq!(pkg.package, "mutt");
        assert_eq!(pkg.version, "1.3.17-1");
        assert_eq!(pkg.depends.len(), 2);
        Ok(())
    }

    #[test]
    fn test_openssh() -> Result<(), Box<dyn Error>> {
        let sample = r#"Package: openssh-server
Source: openssh
Version: 1:7.9p1-10+deb10u2
Installed-Size: 1449
Maintainer: Debian OpenSSH Maintainers <debian-ssh@lists.debian.org>
Architecture: amd64
Replaces: openssh-client (<< 1:7.9p1-8), ssh, ssh-krb5
Provides: ssh-server
Depends: adduser (>= 3.9), dpkg (>= 1.9.0), libpam-modules (>= 0.72-9), libpam-runtime (>= 0.76-14), lsb-base (>= 4.1+Debian3), openssh-client (= 1:7.9p1-10+deb10u2), openssh-sftp-server, procps, ucf (>= 0.28), debconf (>= 0.5) | debconf-2.0, libaudit1 (>= 1:2.2.1), libc6 (>= 2.26), libcom-err2 (>= 1.43.9), libgssapi-krb5-2 (>= 1.17), libkrb5-3 (>= 1.13~alpha1+dfsg), libpam0g (>= 0.99.7.1), libselinux1 (>= 1.32), libssl1.1 (>= 1.1.1), libsystemd0, libwrap0 (>= 7.6-4~), zlib1g (>= 1:1.1.4)
Recommends: default-logind | logind | libpam-systemd, ncurses-term, xauth
Suggests: molly-guard, monkeysphere, rssh, ssh-askpass, ufw
Conflicts: sftp, ssh-socks, ssh2
Description: secure shell (SSH) server, for secure access from remote machines
Multi-Arch: foreign
Homepage: http://www.openssh.com/
Description-md5: 842cc998cae371b9d8106c1696373919
Tag: admin::login, implemented-in::c, interface::daemon, network::server,
 protocol::ssh, role::program, security::authentication,
 security::cryptography, use::login, use::transmission
Section: net
Priority: optional
Filename: pool/main/o/openssh/openssh-server_7.9p1-10+deb10u2_amd64.deb
Size: 352108
MD5sum: 8e8a6fd269cef560d319f6b32c814d63
SHA256: 65bb2ee2cfce60b83523754c3768578417bbb23af760ddd26d53999f4da0f4e6
"#;
        let pkg = parse_debian_package(sample)?;
        assert_eq!(
            pkg,
            DebianPackage {
                package: "openssh-server".to_owned(),
                version: "1:7.9p1-10+deb10u2".to_owned(),
                depends: [
                    Dependency {
                        alternatives: [Alternative {
                            package: "adduser".to_owned(),
                            version_constraint: Some(VersionConstraint {
                                relation: VersionRelation::LaterOrEqual,
                                version: "3.9".to_owned()
                            }),
                            arch: None
                        }]
                        .to_vec()
                    },
                    Dependency {
                        alternatives: [Alternative {
                            package: "dpkg".to_owned(),
                            version_constraint: Some(VersionConstraint {
                                relation: VersionRelation::LaterOrEqual,
                                version: "1.9.0".to_owned()
                            }),
                            arch: None
                        }]
                        .to_vec()
                    },
                    Dependency {
                        alternatives: [Alternative {
                            package: "libpam-modules".to_owned(),
                            version_constraint: Some(VersionConstraint {
                                relation: VersionRelation::LaterOrEqual,
                                version: "0.72-9".to_owned()
                            }),
                            arch: None
                        }]
                        .to_vec()
                    },
                    Dependency {
                        alternatives: [Alternative {
                            package: "libpam-runtime".to_owned(),
                            version_constraint: Some(VersionConstraint {
                                relation: VersionRelation::LaterOrEqual,
                                version: "0.76-14".to_owned()
                            }),
                            arch: None
                        }]
                        .to_vec()
                    },
                    Dependency {
                        alternatives: [Alternative {
                            package: "lsb-base".to_owned(),
                            version_constraint: Some(VersionConstraint {
                                relation: VersionRelation::LaterOrEqual,
                                version: "4.1+Debian3".to_owned()
                            }),
                            arch: None
                        }]
                        .to_vec()
                    },
                    Dependency {
                        alternatives: [Alternative {
                            package: "openssh-client".to_owned(),
                            version_constraint: Some(VersionConstraint {
                                relation: VersionRelation::ExactlyEqual,
                                version: "1:7.9p1-10+deb10u2".to_owned()
                            }),
                            arch: None
                        }]
                        .to_vec()
                    },
                    Dependency {
                        alternatives: [Alternative {
                            package: "openssh-sftp-server".to_owned(),
                            version_constraint: None,
                            arch: None
                        }]
                        .to_vec()
                    },
                    Dependency {
                        alternatives: [Alternative {
                            package: "procps".to_owned(),
                            version_constraint: None,
                            arch: None
                        }]
                        .to_vec()
                    },
                    Dependency {
                        alternatives: [Alternative {
                            package: "ucf".to_owned(),
                            version_constraint: Some(VersionConstraint {
                                relation: VersionRelation::LaterOrEqual,
                                version: "0.28".to_owned()
                            }),
                            arch: None
                        }]
                        .to_vec()
                    },
                    Dependency {
                        alternatives: [
                            Alternative {
                                package: "debconf".to_owned(),
                                version_constraint: Some(VersionConstraint {
                                    relation: VersionRelation::LaterOrEqual,
                                    version: "0.5".to_owned()
                                }),
                                arch: None
                            },
                            Alternative {
                                package: "debconf-2.0".to_owned(),
                                version_constraint: None,
                                arch: None
                            }
                        ]
                        .to_vec()
                    },
                    Dependency {
                        alternatives: [Alternative {
                            package: "libaudit1".to_owned(),
                            version_constraint: Some(VersionConstraint {
                                relation: VersionRelation::LaterOrEqual,
                                version: "1:2.2.1".to_owned()
                            }),
                            arch: None
                        }]
                        .to_vec()
                    },
                    Dependency {
                        alternatives: [Alternative {
                            package: "libc6".to_owned(),
                            version_constraint: Some(VersionConstraint {
                                relation: VersionRelation::LaterOrEqual,
                                version: "2.26".to_owned()
                            }),
                            arch: None
                        }]
                        .to_vec()
                    },
                    Dependency {
                        alternatives: [Alternative {
                            package: "libcom-err2".to_owned(),
                            version_constraint: Some(VersionConstraint {
                                relation: VersionRelation::LaterOrEqual,
                                version: "1.43.9".to_owned()
                            }),
                            arch: None
                        }]
                        .to_vec()
                    },
                    Dependency {
                        alternatives: [Alternative {
                            package: "libgssapi-krb5-2".to_owned(),
                            version_constraint: Some(VersionConstraint {
                                relation: VersionRelation::LaterOrEqual,
                                version: "1.17".to_owned()
                            }),
                            arch: None
                        }]
                        .to_vec()
                    },
                    Dependency {
                        alternatives: [Alternative {
                            package: "libkrb5-3".to_owned(),
                            version_constraint: Some(VersionConstraint {
                                relation: VersionRelation::LaterOrEqual,
                                version: "1.13~alpha1+dfsg".to_owned()
                            }),
                            arch: None
                        }]
                        .to_vec()
                    },
                    Dependency {
                        alternatives: [Alternative {
                            package: "libpam0g".to_owned(),
                            version_constraint: Some(VersionConstraint {
                                relation: VersionRelation::LaterOrEqual,
                                version: "0.99.7.1".to_owned()
                            }),
                            arch: None
                        }]
                        .to_vec()
                    },
                    Dependency {
                        alternatives: [Alternative {
                            package: "libselinux1".to_owned(),
                            version_constraint: Some(VersionConstraint {
                                relation: VersionRelation::LaterOrEqual,
                                version: "1.32".to_owned()
                            }),
                            arch: None
                        }]
                        .to_vec()
                    },
                    Dependency {
                        alternatives: [Alternative {
                            package: "libssl1.1".to_owned(),
                            version_constraint: Some(VersionConstraint {
                                relation: VersionRelation::LaterOrEqual,
                                version: "1.1.1".to_owned()
                            }),
                            arch: None
                        }]
                        .to_vec()
                    },
                    Dependency {
                        alternatives: [Alternative {
                            package: "libsystemd0".to_owned(),
                            version_constraint: None,
                            arch: None
                        }]
                        .to_vec()
                    },
                    Dependency {
                        alternatives: [Alternative {
                            package: "libwrap0".to_owned(),
                            version_constraint: Some(VersionConstraint {
                                relation: VersionRelation::LaterOrEqual,
                                version: "7.6-4~".to_owned()
                            }),
                            arch: None
                        }]
                        .to_vec()
                    },
                    Dependency {
                        alternatives: [Alternative {
                            package: "zlib1g".to_owned(),
                            version_constraint: Some(VersionConstraint {
                                relation: VersionRelation::LaterOrEqual,
                                version: "1:1.1.4".to_owned()
                            }),
                            arch: None
                        }]
                        .to_vec()
                    }
                ]
                .to_vec(),
                provides: [Dependency {
                    alternatives: [Alternative {
                        package: "ssh-server".to_owned(),
                        version_constraint: None,
                        arch: None
                    }]
                    .to_vec()
                }]
                .to_vec()
            }
        );
        Ok(())
    }

    #[test]
    fn test_buster() -> Result<(), Box<dyn Error>> {
        let pkgs = parse_debian_control("repositories/buster/Packages")?;
        println!("{:?}", pkgs);
        Ok(())
    }

    #[test]
    fn test_bullseye() -> Result<(), Box<dyn Error>> {
        let pkgs = parse_debian_control("repositories/bullseye/Packages")?;
        println!("{:?}", pkgs);
        Ok(())
    }

    #[test]
    fn test_bookworm() -> Result<(), Box<dyn Error>> {
        let pkgs = parse_debian_control("repositories/bookworm/Packages")?;
        println!("{:?}", pkgs);
        Ok(())
    }

    #[test]
    fn test_buster_index() -> Result<(), Box<dyn Error>> {
        let _ = create_index("repositories/buster/Packages")?;
        Ok(())
    }

    #[test]
    fn test_bullseye_index() -> Result<(), Box<dyn Error>> {
        let _ = create_index("repositories/bullseye/Packages")?;
        Ok(())
    }

    #[test]
    fn test_bookworm_index() -> Result<(), Box<dyn Error>> {
        let _ = create_index("repositories/bookworm/Packages")?;
        Ok(())
    }
}
