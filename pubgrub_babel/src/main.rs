use pubgrub::{
    DefaultStringReporter, Dependencies, DependencyProvider, PubGrubError, Reporter,
    SelectedDependencies,
};
use pubgrub_alpine::deps::AlpinePackage;
use pubgrub_babel::deps::{BabelPackage, PlatformPackage};
use pubgrub_babel::index::BabelIndex;
use pubgrub_babel::version::BabelVersion;
use pubgrub_debian::deps::DebianPackage;
use pubgrub_opam::deps::OpamPackage;
use pubgrub_opam::index::OpamIndex;
use pubgrub_opam::version::OpamVersion;
use std::collections::{BTreeMap, HashSet};
use std::error::Error;
use std::str::FromStr;

fn solve_repo(
    pkg: BabelPackage,
    version: BabelVersion,
    opam_repo: &str,
    debian_repo: &str,
    alpine_repo: &str,
) -> Result<SelectedDependencies<BabelIndex>, Box<dyn Error>> {
    let opam_index = OpamIndex::new(opam_repo.to_string());
    let debian_index = pubgrub_debian::parse::create_index(debian_repo.to_string())?;
    let alpine_index = pubgrub_alpine::parse::create_index(alpine_repo.to_string())?;
    let index = BabelIndex::new(opam_index, debian_index, alpine_index);
    index.set_debug(true);
    index.set_version_debug(true);
    let sol: SelectedDependencies<BabelIndex> = match pubgrub::resolve(&index, pkg, version) {
        Ok(sol) => Ok(sol),
        Err(PubGrubError::NoSolution(mut derivation_tree)) => {
            derivation_tree.collapse_no_versions();
            eprintln!("\n\n\n{}", DefaultStringReporter::report(&derivation_tree));
            Err(PubGrubError::<BabelIndex>::NoSolution(derivation_tree))
        }
        Err(err) => panic!("{:?}", err),
    }?;

    index.set_debug(false);
    index.set_version_debug(false);

    fn get_resolved_deps<'a>(
        index: &'a BabelIndex,
        sol: &'a SelectedDependencies<BabelIndex>,
        package: &BabelPackage,
        version: &'a BabelVersion,
    ) -> HashSet<(String, &'a BabelVersion)> {
        let dependencies = index.get_dependencies(&package, &version);
        match dependencies {
            Ok(Dependencies::Available(constraints)) => {
                let mut dependents = HashSet::new();
                for (dep_package, _dep_versions) in constraints {
                    let solved_version = sol.get(&dep_package).unwrap();
                    match dep_package.clone() {
                        BabelPackage::Root(_deps) => {
                            dependents.extend(get_resolved_deps(
                                &index,
                                sol,
                                &dep_package,
                                solved_version,
                            ));
                        }
                        BabelPackage::Platform(_deps) => {
                            dependents.extend(get_resolved_deps(
                                &index,
                                sol,
                                &dep_package,
                                solved_version,
                            ));
                        }
                        BabelPackage::Opam(pkg) => {
                            match pkg {
                                OpamPackage::Base(name) => {
                                    dependents.insert((format!("Opam {}", name), solved_version));
                                }
                                OpamPackage::Lor { lhs: _, rhs: _ } => {
                                    dependents.extend(get_resolved_deps(
                                        &index,
                                        sol,
                                        &dep_package,
                                        solved_version,
                                    ));
                                }
                                OpamPackage::Proxy { .. } => {
                                    dependents.extend(get_resolved_deps(
                                        &index,
                                        sol,
                                        &dep_package,
                                        solved_version,
                                    ));
                                }
                                OpamPackage::Formula { .. } => {
                                    dependents.extend(get_resolved_deps(
                                        &index,
                                        sol,
                                        &dep_package,
                                        solved_version,
                                    ));
                                }
                                OpamPackage::Var(_) => {
                                    dependents
                                        .insert((format!("Opam {}", pkg), solved_version));
                                }
                                OpamPackage::Root(_deps) => {
                                    dependents.extend(get_resolved_deps(
                                        &index,
                                        sol,
                                        &dep_package,
                                        solved_version,
                                    ));
                                }
                                OpamPackage::Depext { .. } => {
                                    dependents.extend(get_resolved_deps(
                                        &index,
                                        sol,
                                        &dep_package,
                                        solved_version,
                                    ));
                                }
                                OpamPackage::ConflictClass(_) => {
                                    dependents.extend(get_resolved_deps(
                                        &index,
                                        sol,
                                        &dep_package,
                                        solved_version,
                                    ));
                                }
                            };
                        }
                        BabelPackage::Debian(pkg) => {
                            match pkg {
                                DebianPackage::Base(name) => {
                                    dependents.insert((format!("Debian {}", name), solved_version));
                                }
                                DebianPackage::Proxy(_) => {
                                    dependents.extend(get_resolved_deps(
                                        &index,
                                        sol,
                                        &dep_package,
                                        solved_version,
                                    ));
                                }
                                DebianPackage::Root(_deps) => {
                                    dependents.extend(get_resolved_deps(
                                        &index,
                                        sol,
                                        &dep_package,
                                        solved_version,
                                    ));
                                }
                            };
                        }
                        BabelPackage::Alpine(pkg) => {
                            match pkg {
                                AlpinePackage::Base(name) => {
                                    if ! (name.starts_with("so:") && index.list_versions(&dep_package).count() == 1) {
                                        dependents
                                            .insert((format!("Alpine {}", name), solved_version));
                                    } else {
                                        dependents.extend(get_resolved_deps(
                                            &index,
                                            sol,
                                            &dep_package,
                                            solved_version,
                                        ));
                                    }
                                }
                                AlpinePackage::Root(_deps) => {
                                    dependents.extend(get_resolved_deps(
                                        &index,
                                        sol,
                                        &dep_package,
                                        solved_version,
                                    ));
                                }
                            };
                        }
                    }
                }
                dependents
            }
            _ => {
                println!("No available dependencies for package {}", package);
                HashSet::new()
            }
        }
    }

    println!("\nSolution Set:");
    for (package, version) in &sol {
        match package {
            BabelPackage::Platform(PlatformPackage::OS) => {
                println!("\t(OS, {})", version);
            },
            BabelPackage::Opam(pkg) => match pkg {
                OpamPackage::Base(name) => {
                    println!("\tOpam\t({}, {})", name, version);
                }
                OpamPackage::Var(name) => {
                    println!("\tOpam\t{} = {}", name, version);
                }
                _ => (),
            },
            BabelPackage::Debian(pkg) => match pkg {
                DebianPackage::Base(name) => {
                    println!("\tDebian\t({}, {})", name, version);
                }
                _ => (),
            },
            BabelPackage::Alpine(pkg) => match pkg {
                AlpinePackage::Base(name) => {
                    if ! (name.starts_with("so:") && index.list_versions(&package).count() == 1) {
                        println!("\tAlpine\t({}, {})", name, version);
                    }
                }
                _ => (),
            },
            _ => (),
        }
    }

    let mut resolved_graph: BTreeMap<(String, &BabelVersion), Vec<(String, &BabelVersion)>> =
        BTreeMap::new();
    for (package, version) in &sol {
        let mut deps = get_resolved_deps(&index, &sol, &package, version)
            .into_iter()
            .collect::<Vec<_>>();
        deps.sort_by(|(p1, _v1), (p2, _v2)| p1.cmp(p2));
        match package {
            BabelPackage::Opam(OpamPackage::Base(name)) => {
                resolved_graph.insert((format!("Opam {}", name), version), deps);
            }
            BabelPackage::Debian(DebianPackage::Base(name)) => {
                resolved_graph.insert((format!("Debian {}", name).clone(), version), deps);
            }
            BabelPackage::Alpine(AlpinePackage::Base(name)) => {
                if ! (name.starts_with("so:") && index.list_versions(&package).count() == 1) {
                    resolved_graph.insert((format!("Alpine {}", name).clone(), version), deps);
                }
            }
            _ => {}
        }
    }

    println!("\nResolved Dependency Graph:");
    for ((name, version), dependents) in resolved_graph {
        print!("\t({}, {})", name, version);
        if dependents.len() > 0 {
            print!(" -> ")
        }
        let mut first = true;
        for (dep_name, dep_version) in &dependents {
            if !first {
                print!(", ");
            }
            print!("({}, {})", dep_name, dep_version);
            first = false;
        }
        println!()
    }

    Ok(sol)
}

fn main() -> Result<(), Box<dyn Error>> {
    solve_repo(
        BabelPackage::Opam(OpamPackage::from_str("A").unwrap()),
        BabelVersion::Opam("1.0.0".parse::<OpamVersion>().unwrap()),
        "pubgrub_opam/example-repo/packages",
        "pubgrub_debian/repositories/buster/Packages",
        "pubgrub_alpine/repositories/3.20/APKINDEX",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use pubgrub::Range;
    use pubgrub_debian::{deps::DebianPackage, version::DebianVersion};
    use pubgrub_opam::deps::TRUE_VERSION;

    use super::*;

    #[test]
    fn test_opam_dune_simple() -> Result<(), Box<dyn Error>> {
        solve_repo(
            BabelPackage::Opam(OpamPackage::Base("dune".to_string())),
            BabelVersion::Opam(OpamVersion("3.17.2".to_string())),
            "../pubgrub_opam/opam-repository/packages",
            "../pubgrub_debian/repositories/buster/Packages",
            "../pubgrub_alpine/repositories/3.20/APKINDEX",
        )?;
        Ok(())
    }

    #[test]
    fn test_opam_dune_with_variables() -> Result<(), Box<dyn Error>> {
        let root = OpamPackage::Root(vec![
            (
                OpamPackage::Base("dune".to_string()),
                Range::singleton(OpamVersion("3.17.2".to_string())),
            ),
            (
                OpamPackage::Var("arch".to_string()),
                Range::singleton(OpamVersion("x86_64".to_string())),
            ),
            (
                OpamPackage::Var("os".to_string()),
                Range::singleton(OpamVersion("linux".to_string())),
            ),
            (
                OpamPackage::Var("post".to_string()),
                Range::singleton(TRUE_VERSION.clone()),
            ),
        ]);
        solve_repo(
            BabelPackage::Opam(root),
            BabelVersion::Opam(OpamVersion("".to_string())),
            "../pubgrub_opam/opam-repository/packages",
            "../pubgrub_debian/repositories/buster/Packages",
            "../pubgrub_alpine/repositories/3.20/APKINDEX",
        )?;
        Ok(())
    }

    #[test]
    fn test_debian_openssh_server() -> Result<(), Box<dyn Error>> {
        solve_repo(
            BabelPackage::Debian(DebianPackage::Base("openssh-server".to_string())),
            BabelVersion::Debian(DebianVersion("1:7.9p1-10+deb10u2".to_string())),
            "../pubgrub_opam/opam-repository/packages",
            "../pubgrub_debian/repositories/buster/Packages",
            "../pubgrub_alpine/repositories/3.20/APKINDEX",
        )?;
        Ok(())
    }

    #[test]
    fn test_debian_ssh_server() -> Result<(), Box<dyn Error>> {
        let root = DebianPackage::Root(vec![(
            DebianPackage::Base("ssh-server".to_string()),
            Range::full(),
        )]);
        solve_repo(
            BabelPackage::Debian(root),
            BabelVersion::Debian(DebianVersion("".to_string())),
            "../pubgrub_opam/opam-repository/packages",
            "../pubgrub_debian/repositories/buster/Packages",
            "../pubgrub_alpine/repositories/3.20/APKINDEX",
        )?;
        Ok(())
    }

    #[test]
    fn test_conf_gmp_debian() -> Result<(), Box<dyn Error>> {
        let root = OpamPackage::Root(vec![
            (
                OpamPackage::Base("conf-gmp".to_string()),
                Range::singleton(OpamVersion("4".to_string())),
            ),
            (
                OpamPackage::Var("os-family".to_string()),
                Range::singleton(OpamVersion("debian".to_string())),
            ),
            (
                OpamPackage::Var("os-distribution".to_string()),
                Range::singleton(OpamVersion("debian".to_string())),
            ),
        ]);
        solve_repo(
            BabelPackage::Opam(root),
            BabelVersion::Opam(OpamVersion("".to_string())),
            "../pubgrub_opam/opam-repository/packages",
            "../pubgrub_debian/repositories/buster/Packages",
            "../pubgrub_alpine/repositories/3.20/APKINDEX",
        )?;
        Ok(())
    }

    #[test]
    fn test_conf_gmp_alpine() -> Result<(), Box<dyn Error>> {
        let root = OpamPackage::Root(vec![
            (
                OpamPackage::Base("conf-gmp".to_string()),
                Range::singleton(OpamVersion("4".to_string())),
            ),
            (
                OpamPackage::Var("os-family".to_string()),
                Range::singleton(OpamVersion("alpine".to_string())),
            ),
            (
                OpamPackage::Var("os-distribution".to_string()),
                Range::singleton(OpamVersion("alpine".to_string())),
            ),
        ]);
        solve_repo(
            BabelPackage::Opam(root),
            BabelVersion::Opam(OpamVersion("".to_string())),
            "../pubgrub_opam/opam-repository/packages",
            "../pubgrub_debian/repositories/buster/Packages",
            "../pubgrub_alpine/repositories/3.20/APKINDEX",
        )?;
        Ok(())
    }

    #[test]
    fn test_ocluster_debian() -> Result<(), Box<dyn Error>> {
        let root = BabelPackage::Root(vec![
            (
                BabelPackage::Opam(OpamPackage::Base("ocluster".to_string())),
                Range::singleton(BabelVersion::Opam(OpamVersion("0.3.0".to_string()))),
            ),
            (
                BabelPackage::Opam(OpamPackage::Base("opam-devel".to_string())),
                Range::full(),
            ),
            (
                BabelPackage::Opam(OpamPackage::Var("os-family".to_string())),
                Range::singleton(BabelVersion::Opam(OpamVersion("debian".to_string()))),
            ),
            (
                BabelPackage::Opam(OpamPackage::Var("os-distribution".to_string())),
                Range::singleton(BabelVersion::Opam(OpamVersion("debian".to_string()))),
            ),
        ]);
        solve_repo(
            root,
            BabelVersion::Singular,
            "../pubgrub_opam/opam-repository/packages",
            "../pubgrub_debian/repositories/buster/Packages",
            "../pubgrub_alpine/repositories/3.20/APKINDEX",
        )?;
        Ok(())
    }

    #[test]
    fn test_ocluster_alpine() -> Result<(), Box<dyn Error>> {
        let root = BabelPackage::Root(vec![
            (
                BabelPackage::Opam(OpamPackage::Base("ocluster".to_string())),
                Range::singleton(BabelVersion::Opam(OpamVersion("0.3.0".to_string()))),
            ),
            (
                BabelPackage::Opam(OpamPackage::Base("opam-devel".to_string())),
                Range::full(),
            ),
            (
                BabelPackage::Opam(OpamPackage::Var("os-family".to_string())),
                Range::singleton(BabelVersion::Opam(OpamVersion("alpine".to_string()))),
            ),
            (
                BabelPackage::Opam(OpamPackage::Var("os-distribution".to_string())),
                Range::singleton(BabelVersion::Opam(OpamVersion("alpine".to_string()))),
            ),
        ]);
        solve_repo(
            root,
            BabelVersion::Singular,
            "../pubgrub_opam/opam-repository/packages",
            "../pubgrub_debian/repositories/buster/Packages",
            "../pubgrub_alpine/repositories/3.20/APKINDEX",
        )?;
        Ok(())
    }

    #[test]
    fn test_ocluster_select_os() -> Result<(), Box<dyn Error>> {
        let root = BabelPackage::Root(vec![
            (
                BabelPackage::Opam(OpamPackage::Base("ocluster".to_string())),
                Range::singleton(BabelVersion::Opam(OpamVersion("0.3.0".to_string()))),
            ),
            (
                BabelPackage::Opam(OpamPackage::Base("opam-devel".to_string())),
                Range::full(),
            ),
        ]);
        solve_repo(
            root,
            BabelVersion::Singular,
            "../pubgrub_opam/opam-repository/packages",
            "../pubgrub_debian/repositories/buster/Packages",
            "../pubgrub_alpine/repositories/3.20/APKINDEX",
        )?;
        Ok(())
    }
}
