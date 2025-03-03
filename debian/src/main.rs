use pubgrub::{
    DefaultStringReporter, Dependencies, DependencyProvider, PubGrubError, Reporter,
    SelectedDependencies,
};
use pubgrub_debian::debian_deps::Package;
use pubgrub_debian::debian_version::DebianVersion;
use pubgrub_debian::index::Index;
use pubgrub_debian::parse::create_index;
use std::collections::{BTreeMap, HashSet};
use std::error::Error;
use std::str::FromStr;

fn solve_repo(
    pkg: Package,
    version: DebianVersion,
    repo: &str,
) -> Result<SelectedDependencies<Index>, Box<dyn Error>> {
    let index = create_index(repo.to_string())?;
    index.set_debug(true);

    let sol: SelectedDependencies<Index> = match pubgrub::resolve(&index, pkg, version) {
        Ok(sol) => Ok(sol),
        Err(PubGrubError::NoSolution(mut derivation_tree)) => {
            derivation_tree.collapse_no_versions();
            eprintln!("\n\n\n{}", DefaultStringReporter::report(&derivation_tree));
            Err(PubGrubError::<Index>::NoSolution(derivation_tree))
        }
        Err(err) => panic!("{:?}", err),
    }?;

    index.set_debug(false);

    fn get_resolved_deps<'a>(
        index: &'a Index,
        sol: &'a SelectedDependencies<Index>,
        package: &Package,
        version: &'a DebianVersion,
    ) -> HashSet<(String, &'a DebianVersion)> {
        let dependencies = index.get_dependencies(&package, &version);
        match dependencies {
            Ok(Dependencies::Available(constraints)) => {
                let mut dependents = HashSet::new();
                for (dep_package, _dep_versions) in constraints {
                    let solved_version = sol.get(&dep_package).unwrap();
                    match dep_package.clone() {
                        Package::Base(name) => {
                            dependents.insert((name, solved_version));
                        }
                        Package::Proxy(_) => {
                            dependents.extend(get_resolved_deps(
                                &index,
                                sol,
                                &dep_package,
                                solved_version,
                            ));
                        }
                        Package::Root(_deps) => {
                            dependents.extend(get_resolved_deps(&index, sol, &dep_package, solved_version));
                        }
                    };
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
            Package::Base(name) => {
                println!("\t({}, {})", name, version);
            }
            _ => (),
        }
    }

    let mut resolved_graph: BTreeMap<(String, &DebianVersion), Vec<(String, &DebianVersion)>> =
        BTreeMap::new();
    for (package, version) in &sol {
        match package {
            Package::Base(name) => {
                let mut deps = get_resolved_deps(&index, &sol, &package, version)
                    .into_iter()
                    .collect::<Vec<_>>();
                deps.sort_by(|(p1, _v1), (p2, _v2)| p1.cmp(p2));
                resolved_graph.insert((name.clone(), version), deps);
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
    let _ = solve_repo(
        Package::from_str("openssh-server").unwrap(),
        "1:7.9p1-10+deb10u2".parse::<DebianVersion>().unwrap(),
        "./repositories/buster/Packages",
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use pubgrub::Range;

    use super::*;

    #[test]
    fn test_simple_solve() -> Result<(), Box<dyn Error>> {
        solve_repo(
            Package::from_str("openssh-server").unwrap(),
            "1:7.9p1-10+deb10u2".parse::<DebianVersion>().unwrap(),
            "./repositories/buster/Packages",
        )?;
        Ok(())
    }

    #[test]
    fn test_filtered_package_formula_variable_set_test_true() -> Result<(), Box<dyn Error>> {
        let root = Package::Root(vec![(
            Package::Base("ssh-server".to_string()),
            Range::full(),
        )]);
        let _ = solve_repo(
            root,
            DebianVersion("".to_string()),
            "./repositories/buster/Packages",
        )?;
        Ok(())
    }
}
