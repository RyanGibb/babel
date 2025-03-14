use std::path::Path;

use super::*;

fn case_from_file_name(file_name: &str) -> (&str, semver::Version) {
    let (name, rest) = file_name.split_once("@").unwrap();
    let ver = rest.strip_suffix(".ron").unwrap();
    (name, ver.parse().unwrap())
}

fn crates_data_from_file<P: AsRef<Path>>(path: P) -> IndexMapLookup {
    let data = std::fs::read_to_string(path).unwrap();
    let data: Vec<index_data::Version> = ron::de::from_str(&data).unwrap();
    read_test_file(data)
}

#[must_use]
fn check<'c>(dp: &mut Index<'c>, root: Names<'c>, ver: &semver::Version) -> bool {
    dp.reset();
    let res = resolve(dp, root.clone(), ver.clone());
    let pub_cyclic_package_dependency = if let Ok(map) = res.as_ref() {
        dp.check_cycles(root.clone(), map)
    } else {
        false
    };

    match res.as_ref() {
        Ok(map) => {
            if !dp.check(root.clone(), &map) {
                return false;
            }
        }

        Err(PubGrubError::NoSolution(_derivation)) => {
            // eprintln!("{}", DefaultStringReporter::report(&derivation));
        }
        Err(_e) => {
            return false;
        }
    }
    dp.reset_time();
    let cargo_out = cargo_resolver::resolve(root.crate_().into(), &ver, dp);

    let cyclic_package_dependency = &cargo_out
        .as_ref()
        .map_err(|e| e.to_string().starts_with("cyclic package dependency"))
        == &Err(true);

    if cyclic_package_dependency != pub_cyclic_package_dependency {
        return false;
    }

    if !cyclic_package_dependency && res.is_ok() != cargo_out.is_ok() {
        return false;
    }

    if res.is_ok() {
        dp.past_result = res
            .as_ref()
            .map(|map| {
                let mut results: HashMap<
                    InternedString,
                    BTreeSet<semver::Version>,
                    rustc_hash::FxBuildHasher,
                > = HashMap::default();
                for (k, v) in map.iter() {
                    if k.is_real() {
                        results.entry(k.crate_()).or_default().insert(v.clone());
                    }
                }
                results
            })
            .ok();
        dp.reset_time();
        let cargo_check_pub_lock_out = cargo_resolver::resolve(root.crate_().into(), &ver, dp);

        let cyclic_package_dependency_pub_lock = &cargo_check_pub_lock_out
            .as_ref()
            .map_err(|e| e.to_string().starts_with("cyclic package dependency"))
            == &Err(true);

        if !cyclic_package_dependency_pub_lock && !cargo_check_pub_lock_out.is_ok() {
            return false;
        }
    }
    if cargo_out.is_ok() {
        dp.past_result = cargo_out
            .as_ref()
            .map(|map| {
                let mut results: HashMap<
                    InternedString,
                    BTreeSet<semver::Version>,
                    rustc_hash::FxBuildHasher,
                > = HashMap::default();
                for v in map.iter() {
                    results
                        .entry(v.name())
                        .or_default()
                        .insert(v.version().clone());
                }
                results
            })
            .ok();
        dp.reset_time();
        let pub_check_cargo_lock_out = resolve(dp, root.clone(), ver.clone());

        if !pub_check_cargo_lock_out.is_ok() {
            return false;
        }
    }

    true
}

#[test]
fn serde_round_trip() {
    // Switch to https://docs.rs/snapbox/latest/snapbox/harness/index.html
    let mut faild: Vec<_> = vec![];
    for case in std::fs::read_dir("out/index_ron").unwrap() {
        let case = case.unwrap().path();
        let file_name = case.file_name().unwrap().to_string_lossy().to_string();
        eprintln!("Running: {file_name}");
        let raw_data = std::fs::read_to_string(&case).unwrap();
        let data: Vec<index_data::Version> = ron::de::from_str(&raw_data).unwrap();
        let raw_data_2 = ron::ser::to_string_pretty(&data, PrettyConfig::new()).unwrap();
        let crates_1 = read_test_file(data);
        let data_2: Vec<index_data::Version> = ron::de::from_str(&raw_data_2).unwrap();
        let crates_2 = read_test_file(data_2);
        if crates_1 != crates_2 {
            faild.push(file_name);
        } else if raw_data != raw_data_2 {
            let mut file = File::create(&case).unwrap();
            file.write_all(raw_data_2.as_bytes()).unwrap();
            file.flush().unwrap();
        }
    }
    assert_eq!(faild.as_slice(), &Vec::<String>::new());
}

#[test]
fn named_from_files_pass_tests() {
    // Switch to https://docs.rs/snapbox/latest/snapbox/harness/index.html
    let mut faild: Vec<_> = vec![];
    for case in std::fs::read_dir("out/index_ron").unwrap() {
        let case = case.unwrap().path();
        let file_name = case.file_name().unwrap().to_string_lossy();
        let (name, ver) = case_from_file_name(&file_name);
        let name = InternedString::new(name);
        eprintln!("Running: {name} @ {ver}");
        let start_time = std::time::Instant::now();
        let crates = crates_data_from_file(&case);
        let mut dp = Index::new(&crates);
        let root = new_bucket(name, (&ver).into(), true);
        if !check(&mut dp, root, &ver) {
            dp.make_index_ron_file();
            faild.push(file_name.to_string());
        };
        dp.make_pubgrub_ron_file();
        eprintln!(" in {}s", start_time.elapsed().as_secs());
    }
    assert_eq!(faild.as_slice(), &Vec::<String>::new());
}

#[test]
fn named_from_files_pass_without_vers() {
    // Switch to https://docs.rs/snapbox/latest/snapbox/harness/index.html
    for case in std::fs::read_dir("out/index_ron").unwrap() {
        let case = case.unwrap().path();
        let file_name = case.file_name().unwrap().to_string_lossy();
        let (name, ver) = case_from_file_name(&file_name);
        let name = InternedString::new(name);
        eprintln!("Running: {name} @ {ver}");
        let root = new_bucket(name, (&ver).into(), true);
        let start_time = std::time::Instant::now();
        let data = std::fs::read_to_string(&case).unwrap();
        let mut data: Vec<index_data::Version> = ron::de::from_str(&data).unwrap();
        let mut offset = 0;
        'data: loop {
            for i in 0..data.len() {
                let len = data.len();
                let i = (i + offset) % len;
                let removed = data.swap_remove(i);
                let crates = read_test_file(data.iter().cloned());
                let mut dp = Index::new(&crates);
                if !check(&mut dp, root.clone(), &ver) {
                    data = dp.make_index_ron_data();
                    offset = i;
                    println!("Failed on removing {i} of {len}");
                    continue 'data;
                };
                if let Some(without_features) = removed.clone().without_features() {
                    data.push(without_features);
                    let crates = read_test_file(data.iter().cloned());
                    let mut dp = Index::new(&crates);
                    if !check(&mut dp, root.clone(), &ver) {
                        data = dp.make_index_ron_data();
                        offset = i;
                        println!("Failed on without_features {i} of {len}");
                        continue 'data;
                    };
                    data.pop();
                }
                for f in 0..removed.features.len() {
                    if let Some(without_features) = removed.clone().without_a_feature(f) {
                        if TryInto::<Summary>::try_into(&without_features).is_ok() {
                            data.push(without_features);
                            let crates = read_test_file(data.iter().cloned());
                            let mut dp = Index::new(&crates);
                            if !check(&mut dp, root.clone(), &ver) {
                                data = dp.make_index_ron_data();
                                offset = i;
                                println!("Failed on without_a_feature({f}) {i} of {len}");
                                continue 'data;
                            };
                            data.pop();
                        }
                    }
                }
                if let Some(without_deps) = removed.clone().without_deps() {
                    if TryInto::<Summary>::try_into(&without_deps).is_ok() {
                        data.push(without_deps);
                        let crates = read_test_file(data.iter().cloned());
                        let mut dp = Index::new(&crates);
                        if !check(&mut dp, root.clone(), &ver) {
                            data = dp.make_index_ron_data();
                            offset = i;
                            println!("Failed on without_deps {i} of {len}");
                            continue 'data;
                        };
                        data.pop();
                    }
                }
                for d in 0..removed.deps.len() {
                    if let Some(without_deps) = removed.clone().without_a_dep(d) {
                        if TryInto::<Summary>::try_into(&without_deps).is_ok() {
                            data.push(without_deps);
                            let crates = read_test_file(data.iter().cloned());
                            let mut dp = Index::new(&crates);
                            if !check(&mut dp, root.clone(), &ver) {
                                data = dp.make_index_ron_data();
                                offset = i;
                                println!("Failed on without_a_dep({d}) {i} of {len}");
                                continue 'data;
                            };
                            data.pop();
                        }
                    }
                }
            }
            break;
        }
        let crates = read_test_file(data);
        let mut dp = Index::new(&crates);
        if !check(&mut dp, root, &ver) {
            dp.make_index_ron_file();
        };

        eprintln!(" in {}s", start_time.elapsed().as_secs());
    }
}

#[test]
fn all_vers_in_files_pass_tests() {
    // Switch to https://docs.rs/snapbox/latest/snapbox/harness/index.html
    let mut faild: Vec<_> = vec![];
    for case in std::fs::read_dir("out/index_ron").unwrap() {
        let case = case.unwrap().path();
        let file_name = case.file_name().unwrap().to_string_lossy();
        eprintln!("Running: {file_name}");
        let start_time = std::time::Instant::now();
        let crates = crates_data_from_file(&case);
        let mut dp = Index::new(&crates);
        for (name, vers) in &crates {
            for ver in vers.keys() {
                let root = new_bucket(*name, ver.into(), true);
                if !check(&mut dp, root, ver) {
                    dp.make_index_ron_file();
                    faild.push(format!("{file_name}:{name}@{ver}"));
                };
            }
        }
        eprintln!(" in {}s", start_time.elapsed().as_secs());
    }
    assert_eq!(faild.as_slice(), &Vec::<String>::new());
}
