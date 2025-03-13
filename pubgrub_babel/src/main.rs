use cargo::util::interning::InternedString;
use clap::Parser;
use pubgrub::Range;
use pubgrub::{DefaultStringReporter, Dependencies, DependencyProvider, PubGrubError, Reporter};
use pubgrub_alpine::deps::AlpinePackage;
use pubgrub_alpine::version::AlpineVersion;
use pubgrub_babel::deps::{BabelPackage, PlatformPackage};
use pubgrub_babel::index::BabelIndex;
use pubgrub_babel::version::{BabelVersion, BabelVersionSet};
use pubgrub_cargo::index_data;
use pubgrub_cargo::names::Names as CargoPackage;
use pubgrub_cargo::rc_semver_pubgrub::RcSemverPubgrub;
use pubgrub_cargo::{read_index::read_index, Index as CargoIndex};
use pubgrub_debian::deps::DebianPackage;
use pubgrub_debian::version::DebianVersion;
use pubgrub_opam::deps::OpamPackage;
use pubgrub_opam::index::OpamIndex;
use pubgrub_opam::version::OpamVersion;
use semver::Version as CargoVersion;
use semver_pubgrub::SemverPubgrub;
use std::collections::BTreeMap;
use std::error::Error;

fn solve_repo(
    pkg: BabelPackage<'static>,
    version: BabelVersion,
    opam_repo: &str,
    debian_repo: &str,
    alpine_repo: &str,
    cargo_repo: &str,
) -> Result<(), Box<dyn Error>> {
    let opam_index = OpamIndex::new(opam_repo.to_string());
    let debian_index = pubgrub_debian::parse::create_index(debian_repo.to_string())?;
    let alpine_index = pubgrub_alpine::parse::create_index(alpine_repo.to_string())?;

    let crates_index = crates_index::GitIndex::with_path(
        cargo_repo,
        "https://github.com/rust-lang/crates.io-index",
    )
    .unwrap();
    let create_filter = |_name: &str| true;
    let version_filter = |version: &index_data::Version| !version.yanked;
    let data = read_index(&crates_index, create_filter, version_filter);
    // let data = Map::default();
    let cargo_index = CargoIndex::new(&data);

    let index = BabelIndex::new(opam_index, debian_index, alpine_index, cargo_index);
    index.set_debug(true);
    index.set_version_debug(true);

    let sol = match pubgrub::resolve(&index, pkg, version) {
        Ok(sol) => sol,
        Err(PubGrubError::NoSolution(mut derivation_tree)) => {
            derivation_tree.collapse_no_versions();
            eprintln!("\n\n\n{}", DefaultStringReporter::report(&derivation_tree));
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "No solution found",
            )));
        }
        Err(err) => panic!("{:?}", err),
    };

    index.set_debug(false);
    index.set_version_debug(false);

    println!("\nSolution Set:");
    for (package, version) in &sol {
        match package {
            BabelPackage::Platform(PlatformPackage::OS) => {
                println!("\t(OS, {})", version);
            }
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
                    if !(name.starts_with("so:")
                        && index
                            .alpine
                            .list_versions(&AlpinePackage::Base(name.clone()))
                            .count()
                            == 1)
                    {
                        println!("\tAlpine\t({}, {})", name, version);
                    }
                }
                _ => (),
            },
            _ => (),
        }
    }

    let mut resolved_graph = BTreeMap::new();
    for (root_package, root_version) in &sol {
        let mut transative_deps = vec![(root_package.clone(), root_version.clone())];
        let mut deps = Vec::new();
        while let Some((package, version)) = transative_deps.pop() {
            let dependencies = index.get_dependencies(&package, &version);
            match dependencies {
                Ok(Dependencies::Available(constraints)) => {
                    for (dep_package, _dep_versions) in constraints {
                        let solved_version = sol.get(&dep_package).unwrap();
                        if let Some(dep_version) = sol.get(&dep_package) {
                            match &dep_package {
                                BabelPackage::Opam(OpamPackage::Base(name)) => {
                                    deps.push((format!("Opam {}", name), dep_version.clone()));
                                }
                                BabelPackage::Opam(OpamPackage::Var(name)) => {
                                    deps.push((format!("Opam {}", name), dep_version.clone()));
                                }
                                BabelPackage::Debian(DebianPackage::Base(name)) => {
                                    deps.push((format!("Debian {}", name), dep_version.clone()));
                                }
                                BabelPackage::Alpine(AlpinePackage::Base(name)) => {
                                    if name.starts_with("so:")
                                        && index
                                            .alpine
                                            .list_versions(&AlpinePackage::Base(name.clone()))
                                            .count()
                                            == 1
                                    {
                                        transative_deps.push((dep_package, solved_version.clone()));
                                    } else {
                                        deps.push((
                                            format!("Alpine {}", name),
                                            dep_version.clone(),
                                        ));
                                    }
                                }
                                BabelPackage::Cargo(CargoPackage::Bucket(name, _, _)) => {
                                    deps.push((format!("Cargo {}", name), dep_version.clone()));
                                }
                                _ => {
                                    transative_deps.push((dep_package, solved_version.clone()));
                                }
                            }
                        }
                    }
                    deps.sort_by(|(p1, _), (p2, _)| p1.cmp(p2));
                }
                _ => {}
            };
        }
        match root_package {
            BabelPackage::Opam(OpamPackage::Base(name)) => {
                resolved_graph.insert((format!("Opam {}", name), root_version.clone()), deps);
            }
            BabelPackage::Debian(DebianPackage::Base(name)) => {
                resolved_graph.insert((format!("Debian {}", name), root_version.clone()), deps);
            }
            BabelPackage::Alpine(AlpinePackage::Base(name)) => {
                if !(name.starts_with("so:")
                    && index
                        .alpine
                        .list_versions(&AlpinePackage::Base(name.clone()))
                        .count()
                        == 1)
                {
                    resolved_graph.insert((format!("Alpine {}", name), root_version.clone()), deps);
                }
            }
            _ => {}
        }
    }

    println!("\nResolved Dependency Graph:");
    for ((name, version), dependents) in resolved_graph {
        print!("\t({}, {})", name, version);
        if !dependents.is_empty() {
            print!(" -> ");
            let mut first = true;
            for (dep_name, dep_version) in dependents {
                if !first {
                    print!(", ");
                }
                print!("({}, {})", dep_name, dep_version);
                first = false;
            }
        }
        println!();
    }

    Ok(())
}

#[derive(Parser)]
#[command(name = "solver", about = "Solve repository dependencies")]
struct Cli {
    /// List of packages with their ecosystems and versions in the form `ecosystem:package_name:version`
    packages: Vec<String>,
    /// List of variable assignments in the form `variable_name=value`
    #[clap(short, long, value_name = "VAR=value")]
    variables: Vec<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Cli::parse();
    let mut packages = args
        .packages
        .into_iter()
        .map(|pkg_ver| {
            let parts: Vec<&str> = pkg_ver.split(':').collect();
            if parts.len() != 3 {
                eprintln!("Invalid ecosystem-package-version format: {}", pkg_ver);
                std::process::exit(1);
            }
            let ecosystem = parts[0];
            let name = parts[1];
            let version = parts[2];
            match ecosystem {
                "opam" => (
                    BabelPackage::Opam(OpamPackage::Base(name.to_string())),
                    BabelVersionSet::Opam(Range::singleton(OpamVersion(version.to_string()))),
                ),
                "debian" => (
                    BabelPackage::Debian(DebianPackage::Base(name.to_string())),
                    BabelVersionSet::Debian(Range::singleton(DebianVersion(version.to_string()))),
                ),
                "alpine" => (
                    BabelPackage::Alpine(AlpinePackage::Base(name.to_string())),
                    BabelVersionSet::Alpine(Range::singleton(AlpineVersion(version.to_string()))),
                ),
                "cargo" => {
                    let ver = SemverPubgrub::<semver::Version>::singleton(
                        version.parse::<CargoVersion>().unwrap(),
                    );
                    let pkg = CargoPackage::Bucket(
                        InternedString::from(name.to_string()),
                        ver.only_one_compatibility_range().unwrap(),
                        false,
                    );
                    (
                        BabelPackage::Cargo(pkg),
                        BabelVersionSet::Cargo(RcSemverPubgrub::new(ver)),
                    )
                }
                _ => {
                    eprintln!("Invalid ecosystem: {}", ecosystem);
                    std::process::exit(1);
                }
            }
        })
        .collect::<Vec<_>>();
    let variables = args
        .variables
        .into_iter()
        .map(|var_val| {
            let parts: Vec<&str> = var_val.split('=').collect();
            if parts.len() != 2 {
                eprintln!("Invalid variable format: {}", var_val);
                std::process::exit(1);
            }
            let var = parts[0];
            let val = parts[1];
            (
                BabelPackage::Opam(OpamPackage::Var(var.to_string())),
                BabelVersionSet::Opam(Range::singleton(OpamVersion(val.to_string()))),
            )
        })
        .collect::<Vec<_>>();
    packages.extend(variables);
    let root = BabelPackage::Root(packages);
    solve_repo(
        root,
        BabelVersion::Singular,
        "pubgrub_opam/opam-repository/packages",
        "pubgrub_debian/repositories/buster/Packages",
        "pubgrub_alpine/repositories/3.20/APKINDEX",
        "pubgrub_cargo/index",
    )
}

#[cfg(test)]
mod tests {
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
            "../pubgrub_cargo/index",
        )
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
            "../pubgrub_cargo/index",
        )
    }

    #[test]
    fn test_debian_openssh_server() -> Result<(), Box<dyn Error>> {
        solve_repo(
            BabelPackage::Debian(DebianPackage::Base("openssh-server".to_string())),
            BabelVersion::Debian(DebianVersion("1:7.9p1-10+deb10u2".to_string())),
            "../pubgrub_opam/opam-repository/packages",
            "../pubgrub_debian/repositories/buster/Packages",
            "../pubgrub_alpine/repositories/3.20/APKINDEX",
            "../pubgrub_cargo/index",
        )
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
            "../pubgrub_cargo/index",
        )
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
            "../pubgrub_cargo/index",
        )
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
            "../pubgrub_cargo/index",
        )
    }

    #[test]
    fn test_ocluster_debian() -> Result<(), Box<dyn Error>> {
        let root = BabelPackage::Root(vec![
            (
                BabelPackage::Opam(OpamPackage::Base("ocluster".to_string())),
                BabelVersionSet::Opam(Range::singleton(OpamVersion("0.3.0".to_string()))),
            ),
            (
                BabelPackage::Opam(OpamPackage::Base("opam-devel".to_string())),
                BabelVersionSet::Opam(Range::full()),
            ),
            (
                BabelPackage::Opam(OpamPackage::Var("os-family".to_string())),
                BabelVersionSet::Opam(Range::singleton(OpamVersion("debian".to_string()))),
            ),
            (
                BabelPackage::Opam(OpamPackage::Var("os-distribution".to_string())),
                BabelVersionSet::Opam(Range::singleton(OpamVersion("debian".to_string()))),
            ),
        ]);
        solve_repo(
            root,
            BabelVersion::Singular,
            "../pubgrub_opam/opam-repository/packages",
            "../pubgrub_debian/repositories/buster/Packages",
            "../pubgrub_alpine/repositories/3.20/APKINDEX",
            "../pubgrub_cargo/index",
        )
    }

    #[test]
    fn test_ocluster_alpine() -> Result<(), Box<dyn Error>> {
        let root = BabelPackage::Root(vec![
            (
                BabelPackage::Opam(OpamPackage::Base("ocluster".to_string())),
                BabelVersionSet::Opam(Range::singleton(OpamVersion("0.3.0".to_string()))),
            ),
            (
                BabelPackage::Opam(OpamPackage::Base("opam-devel".to_string())),
                BabelVersionSet::Opam(Range::full()),
            ),
            (
                BabelPackage::Opam(OpamPackage::Var("os-family".to_string())),
                BabelVersionSet::Opam(Range::singleton(OpamVersion("alpine".to_string()))),
            ),
            (
                BabelPackage::Opam(OpamPackage::Var("os-distribution".to_string())),
                BabelVersionSet::Opam(Range::singleton(OpamVersion("alpine".to_string()))),
            ),
        ]);
        solve_repo(
            root,
            BabelVersion::Singular,
            "../pubgrub_opam/opam-repository/packages",
            "../pubgrub_debian/repositories/buster/Packages",
            "../pubgrub_alpine/repositories/3.20/APKINDEX",
            "../pubgrub_cargo/index",
        )
    }

    #[test]
    fn test_ocluster_select_os() -> Result<(), Box<dyn Error>> {
        let root = BabelPackage::Root(vec![
            (
                BabelPackage::Opam(OpamPackage::Base("ocluster".to_string())),
                BabelVersionSet::Opam(Range::singleton(OpamVersion("0.3.0".to_string()))),
            ),
            (
                BabelPackage::Opam(OpamPackage::Base("opam-devel".to_string())),
                BabelVersionSet::Opam(Range::full()),
            ),
        ]);
        solve_repo(
            root,
            BabelVersion::Singular,
            "../pubgrub_opam/opam-repository/packages",
            "../pubgrub_debian/repositories/buster/Packages",
            "../pubgrub_alpine/repositories/3.20/APKINDEX",
            "../pubgrub_cargo/index",
        )
    }

    #[test]
    fn test_cargo_serde() -> Result<(), Box<dyn Error>> {
        let ver =
            SemverPubgrub::<semver::Version>::singleton("1.0.219".parse::<CargoVersion>().unwrap());
        let pkg = CargoPackage::Bucket(
            InternedString::from("serde".to_string()),
            ver.only_one_compatibility_range().unwrap(),
            false,
        );
        let root = BabelPackage::Root(vec![(
            BabelPackage::Cargo(pkg),
            BabelVersionSet::Cargo(RcSemverPubgrub::new(ver)),
        )]);
        solve_repo(
            root,
            BabelVersion::Singular,
            "../pubgrub_opam/opam-repository/packages",
            "../pubgrub_debian/repositories/buster/Packages",
            "../pubgrub_alpine/repositories/3.20/APKINDEX",
            "../pubgrub_cargo/index",
        )
    }
}
