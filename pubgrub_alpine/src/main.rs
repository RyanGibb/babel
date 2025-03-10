use pubgrub::{
    DefaultStringReporter, Dependencies, DependencyProvider, PubGrubError, Reporter,
    SelectedDependencies,
};
use pubgrub_alpine::deps::AlpinePackage;
use pubgrub_alpine::index::AlpineIndex;
use pubgrub_alpine::parse::create_index;
use pubgrub_alpine::version::AlpineVersion;
use std::collections::{BTreeMap, HashSet};
use std::error::Error;
use std::str::FromStr;

fn solve_repo(
    pkg: AlpinePackage,
    version: AlpineVersion,
    repo: &str,
) -> Result<SelectedDependencies<AlpineIndex>, Box<dyn Error>> {
    let index = create_index(repo.to_string())?;
    index.set_debug(true);

    let sol: SelectedDependencies<AlpineIndex> = match pubgrub::resolve(&index, pkg, version) {
        Ok(sol) => Ok(sol),
        Err(PubGrubError::NoSolution(mut derivation_tree)) => {
            derivation_tree.collapse_no_versions();
            eprintln!("\n\n\n{}", DefaultStringReporter::report(&derivation_tree));
            Err(PubGrubError::<AlpineIndex>::NoSolution(derivation_tree))
        }
        Err(err) => panic!("{:?}", err),
    }?;

    index.set_debug(false);

    fn get_resolved_deps<'a>(
        index: &'a AlpineIndex,
        sol: &'a SelectedDependencies<AlpineIndex>,
        package: &AlpinePackage,
        version: &'a AlpineVersion,
    ) -> HashSet<(String, &'a AlpineVersion)> {
        let dependencies = index.get_dependencies(&package, &version);
        match dependencies {
            Ok(Dependencies::Available(constraints)) => {
                let mut dependents = HashSet::new();
                for (dep_package, _dep_versions) in constraints {
                    let solved_version = sol.get(&dep_package).unwrap();
                    match dep_package.clone() {
                        AlpinePackage::Base(name) => {
                            dependents.insert((name, solved_version));
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
            AlpinePackage::Base(name) => {
                println!("\t({}, {})", name, version);
            }
            _ => (),
        }
    }

    let mut resolved_graph: BTreeMap<(String, &AlpineVersion), Vec<(String, &AlpineVersion)>> =
        BTreeMap::new();
    for (package, version) in &sol {
        match package {
            AlpinePackage::Base(name) => {
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
        AlpinePackage::from_str("openssh-server").unwrap(),
        "9.9_p2-r0".parse::<AlpineVersion>().unwrap(),
        "pubgrub_alpine/repositories/3.21/APKINDEX",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_solve() -> Result<(), Box<dyn Error>> {
        solve_repo(
            AlpinePackage::from_str("openssh-server").unwrap(),
            "9.7_p1-r5".parse::<AlpineVersion>().unwrap(),
            "./repositories/3.20/APKINDEX",
        )?;
        Ok(())
    }
}
