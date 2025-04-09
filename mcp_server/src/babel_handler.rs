use std::sync::Arc;

use cargo::util::interning::InternedString;
use rmcp::{
    self, ServerHandler, tool, Error as McpError,
    model::{self, CallToolResult, Content, ServerCapabilities, ServerInfo, ProtocolVersion, Implementation},
};
use pubgrub::{DefaultStringReporter, Map, PubGrubError, Reporter, VersionSet};
use enki_solver::deps::{BabelPackage, PlatformPackage};
use enki_solver::index::BabelIndex;
use enki_solver::version::BabelVersion;
use pubgrub_cargo::names::Names as CargoPackage;
use pubgrub_debian::deps::DebianPackage;
use pubgrub_debian::version::DebianVersion;
use pubgrub_opam::deps::OpamPackage;
use pubgrub_opam::version::OpamVersion;
use semver::Version as CargoVersion;
use semver_pubgrub::SemverPubgrub;
use pubgrub_cargo::rc_semver_pubgrub::RcSemverPubgrub;
use serde_json::json;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct BabelHandler {
    counter: Arc<Mutex<i32>>,
}

#[tool(tool_box)]
impl BabelHandler {
    pub fn new() -> Self {
        Self {
            counter: Arc::new(Mutex::new(0)),
        }
    }

    #[tool(description = "Search for a package by name")]
    async fn search_package(
        &self,
        #[tool(param)]
        #[schemars(description = "The package name to search for")]
        query: String,
    ) -> Result<CallToolResult, McpError> {
        // TODO: Implement actual package search using babel repositories
        let result = json!({
            "status": "not_implemented",
            "message": "Package search not yet implemented",
            "query": query
        })
        .to_string();

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Resolve dependencies for a package")]
    async fn resolve_dependencies(
        &self,
        #[tool(param)]
        #[schemars(description = "The package ecosystem (opam, debian, alpine, cargo)")]
        ecosystem: String,
        #[tool(param)]
        #[schemars(description = "The package name to resolve dependencies for")]
        package: String,
        #[tool(param)]
        #[schemars(description = "The package version")]
        version: String,
        #[tool(param)]
        #[schemars(description = "The platform to use (alpine, debian)")]
        platform: Option<String>,
    ) -> Result<CallToolResult, McpError> {
        match resolve_package_dependencies(&ecosystem, &package, &version, platform.as_deref()) {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => {
                tracing::warn!("Failed to resolve dependencies: {}", e);
                Err(McpError::new(
                    model::ErrorCode::INTERNAL_ERROR,
                    "dep_resolution_error",
                    Some(json!({ "error": e })),
                ))
            }
        }
    }
}

#[tool(tool_box)]
impl ServerHandler for BabelHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some("This server provides tools to search for packages and resolve dependencies in the Babel package manager.".to_string()),
        }
    }
}

// Attempt to resolve dependencies using the actual Babel code
fn resolve_package_dependencies(
    ecosystem: &str,
    package: &str,
    version: &str,
    platform: Option<&str>,
) -> Result<String, String> {
    use pubgrub::Range;
    use enki_solver::version::BabelVersionSet;
    
    let babel_package: BabelPackage<'static>;
    let babel_version: BabelVersion;

    // If a platform is specified, we'll create a root package with both the requested package
    // and a platform dependency
    if let Some(platform_name) = platform {
        // Validate platform - only alpine and debian are supported
        if platform_name != "alpine" && platform_name != "debian" {
            return Err(format!("Unsupported platform: {}. Only 'alpine' and 'debian' are supported.", platform_name));
        }
        
        // First, create the package dependency based on ecosystem
        let (dep_package, dep_version_set) = match ecosystem {
            "opam" => {
                (
                    BabelPackage::Opam(OpamPackage::Base(package.to_string())),
                    BabelVersionSet::Opam(Range::singleton(OpamVersion(version.to_string()))),
                )
            }
            "debian" => {
                (
                    BabelPackage::Debian(DebianPackage::Base(package.to_string())),
                    BabelVersionSet::Debian(Range::singleton(DebianVersion(version.to_string()))),
                )
            }
            "alpine" => {
                (
                    BabelPackage::Alpine(pubgrub_alpine::deps::AlpinePackage::Base(package.to_string())),
                    BabelVersionSet::Alpine(Range::singleton(pubgrub_alpine::version::AlpineVersion(version.to_string()))),
                )
            }
            "cargo" => {
                // Convert to cargo package format
                let ver = match version.parse::<CargoVersion>() {
                    Ok(v) => v,
                    Err(e) => return Err(format!("Invalid Cargo version: {}", e)),
                };

                let pkg = CargoPackage::Bucket(
                    InternedString::from(package.to_string()),
                    SemverPubgrub::<semver::Version>::singleton(ver.clone())
                        .only_one_compatibility_range()
                        .ok_or("Could not get compatibility range")?,
                    false,
                );
                
                {
                    let semver_pubgrub = SemverPubgrub::singleton(ver);
                    (
                        BabelPackage::Cargo(pkg),
                        BabelVersionSet::Cargo(RcSemverPubgrub::new(semver_pubgrub)),
                    )
                }
            }
            _ => return Err(format!("Unsupported ecosystem: {}", ecosystem)),
        };
        
        // Create the root package with both dependencies
        babel_package = BabelPackage::Root(vec![
            (dep_package, dep_version_set),
            (
                BabelPackage::Platform(enki_solver::deps::PlatformPackage::OS),
                BabelVersionSet::Babel(Range::singleton(platform_name.to_string())),
            ),
        ]);
        babel_version = BabelVersion::Babel("root".to_string());
    } else {
        // No platform specified, use the ecosystem-specific package directly
        match ecosystem {
            "opam" => {
                babel_package = BabelPackage::Opam(OpamPackage::Base(package.to_string()));
                babel_version = BabelVersion::Opam(OpamVersion(version.to_string()));
            }
            "debian" => {
                babel_package = BabelPackage::Debian(DebianPackage::Base(package.to_string()));
                babel_version = BabelVersion::Debian(DebianVersion(version.to_string()));
            }
            "alpine" => {
                babel_package = BabelPackage::Alpine(pubgrub_alpine::deps::AlpinePackage::Base(
                    package.to_string(),
                ));
                babel_version =
                    BabelVersion::Alpine(pubgrub_alpine::version::AlpineVersion(version.to_string()));
            }
            "cargo" => {
                // Convert to cargo package format
                let ver = match version.parse::<CargoVersion>() {
                    Ok(v) => v,
                    Err(e) => return Err(format!("Invalid Cargo version: {}", e)),
                };

                let pkg = CargoPackage::Bucket(
                    InternedString::from(package.to_string()),
                    SemverPubgrub::<semver::Version>::singleton(ver.clone())
                        .only_one_compatibility_range()
                        .ok_or("Could not get compatibility range")?,
                    false,
                );
                babel_package = BabelPackage::Cargo(pkg);
                // TODO error handling
                babel_version = BabelVersion::Cargo(ver);
            }
            _ => return Err(format!("Unsupported ecosystem: {}", ecosystem)),
        }
    }

    // Set up the repositories
    let opam_index =
        pubgrub_opam::index::OpamIndex::new("pubgrub_opam/opam-repository/packages".to_string());
    let debian_index = match pubgrub_debian::parse::create_index(
        "pubgrub_debian/repositories/buster/Packages".to_string(),
    ) {
        Ok(idx) => idx,
        Err(e) => return Err(format!("Failed to create Debian index: {}", e)),
    };
    let alpine_index = match pubgrub_alpine::parse::create_index(
        "pubgrub_alpine/repositories/3.20/APKINDEX".to_string(),
    ) {
        Ok(idx) => idx,
        Err(e) => return Err(format!("Failed to create Alpine index: {}", e)),
    };

    // Cargo index setup
    let _crates_index = match crates_index::GitIndex::with_path(
        "pubgrub_cargo/index",
        "https://github.com/rust-lang/crates.io-index",
    ) {
        Ok(idx) => idx,
        Err(e) => return Err(format!("Failed to create Cargo index: {}", e)),
    };

    // let create_filter = |_name: &str| true;
    // let version_filter = |version: &pubgrub_cargo::index_data::Version| !version.yanked;
    // let data = pubgrub_cargo::read_index::read_index(&crates_index, create_filter, version_filter);
    let data = Map::default();
    let cargo_index = pubgrub_cargo::Index::new(&data);

    // Create the Babel index
    let index = BabelIndex::new(opam_index, debian_index, alpine_index, cargo_index);

    // Resolve dependencies
    let sol = match pubgrub::resolve(&index, babel_package.clone(), babel_version.clone()) {
        Ok(sol) => sol,
        Err(PubGrubError::NoSolution(mut derivation_tree)) => {
            derivation_tree.collapse_no_versions();
            let error = DefaultStringReporter::report(&derivation_tree);
            return Err(format!("No solution found: {}", error));
        }
        Err(err) => return Err(format!("Error resolving dependencies: {:?}", err)),
    };

    // Format the solution as JSON
    let mut deps = Vec::new();
    for (pkg, ver) in &sol {
        match pkg {
            BabelPackage::Opam(OpamPackage::Base(name)) => {
                deps.push(json!({
                    "ecosystem": "opam",
                    "name": name,
                    "version": ver.to_string()
                }));
            }
            // BabelPackage::Opam(OpamPackage::Var(name)) => {
            //     deps.push(json!({
            //         "ecosystem": "opam variable",
            //         "name": name,
            //         "version": ver.to_string()
            //     }));
            // }
            BabelPackage::Debian(DebianPackage::Base(name)) => {
                deps.push(json!({
                    "ecosystem": "debian",
                    "name": name,
                    "version": ver.to_string()
                }));
            }
            BabelPackage::Alpine(pubgrub_alpine::deps::AlpinePackage::Base(name)) => {
                // Skip shared object dependencies for cleaner output
                if !name.starts_with("so:") {
                    deps.push(json!({
                        "ecosystem": "alpine",
                        "name": name,
                        "version": ver.to_string()
                    }));
                }
            }
            BabelPackage::Cargo(CargoPackage::Bucket(name, _, _)) => {
                deps.push(json!({
                    "ecosystem": "cargo",
                    "name": name.to_string(),
                    "version": ver.to_string()
                }));
            }
            // BabelPackage::Platform(PlatformPackage::OS) => {
            //     deps.push(json!({
            //         "ecosystem": "babel",
            //         "name": "PLATFORM",
            //         "version": ver.to_string()
            //     }));
            // }
            _ => {} // Skip other package types
        }
    }

    // Create the final result
    let mut result = json!({
        "ecosystem": ecosystem,
        "package": package,
        "version": version,
        "resolved": true,
        "dependencies": deps
    });

    // Add platform information if specified
    if let Some(platform_name) = platform {
        result["platform"] = json!(platform_name);
    }

    Ok(result.to_string())
}
