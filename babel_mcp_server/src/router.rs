use std::{future::Future, pin::Pin};

use cargo::util::interning::InternedString;
use mcp_core::{
    handler::{PromptError, ResourceError},
    prompt::Prompt,
    protocol::ServerCapabilities,
    Content, Resource, Tool, ToolError,
};
use mcp_server::router::CapabilitiesBuilder;
use pubgrub::{DefaultStringReporter, Map, PubGrubError, Reporter};
use pubgrub_babel::deps::BabelPackage;
use pubgrub_babel::index::BabelIndex;
use pubgrub_babel::version::BabelVersion;
use pubgrub_cargo::names::Names as CargoPackage;
use pubgrub_debian::deps::DebianPackage;
use pubgrub_debian::version::DebianVersion;
use pubgrub_opam::deps::OpamPackage;
use pubgrub_opam::version::OpamVersion;
use semver::Version as CargoVersion;
use semver_pubgrub::SemverPubgrub;
use serde_json::{json, Value};

#[derive(Clone)]
pub struct BabelRouter;

impl BabelRouter {
    pub fn new() -> Self {
        Self {}
    }
}

impl mcp_server::Router for BabelRouter {
    fn name(&self) -> String {
        "babel".to_string()
    }

    fn instructions(&self) -> String {
        "This server provides tools to search for packages and resolve dependencies in the Babel package manager.".to_string()
    }

    fn capabilities(&self) -> ServerCapabilities {
        CapabilitiesBuilder::new()
            .with_tools(true)
            .with_resources(false, false)
            .with_prompts(false)
            .build()
    }

    fn list_tools(&self) -> Vec<Tool> {
        vec![
            Tool::new(
                "search_package".to_string(),
                "Search for a package by name".to_string(),
                json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The package name to search for",
                        }
                    },
                    "required": ["query"]
                }),
            ),
            Tool::new(
                "resolve_dependencies".to_string(),
                "Resolve dependencies for a package".to_string(),
                json!({
                    "type": "object",
                    "properties": {
                        "ecosystem": {
                            "type": "string",
                            "description": "The package ecosystem (opam, debian, alpine, cargo)",
                        },
                        "package": {
                            "type": "string",
                            "description": "The package name to resolve dependencies for",
                        },
                        "version": {
                            "type": "string",
                            "description": "The package version",
                        }
                    },
                    "required": ["ecosystem", "package", "version"]
                }),
            ),
        ]
    }

    fn call_tool(
        &self,
        tool_name: &str,
        arguments: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Content>, ToolError>> + Send + 'static>> {
        let tool_name = tool_name.to_string();
        let arguments = arguments.clone();

        Box::pin(async move {
            match tool_name.as_str() {
                "search_package" => {
                    let query =
                        arguments
                            .get("query")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| {
                                ToolError::InvalidParameters("Missing query parameter".to_string())
                            })?;

                    // TODO: Implement actual package search using babel repositories
                    let result = json!({
                        "status": "not_implemented",
                        "message": "Package search not yet implemented",
                        "query": query
                    })
                    .to_string();

                    Ok(vec![Content::text(result)])
                }
                "resolve_dependencies" => {
                    let ecosystem = arguments
                        .get("ecosystem")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            ToolError::InvalidParameters("Missing ecosystem parameter".to_string())
                        })?;

                    let package = arguments
                        .get("package")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            ToolError::InvalidParameters("Missing package parameter".to_string())
                        })?;

                    let version = arguments
                        .get("version")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            ToolError::InvalidParameters("Missing version parameter".to_string())
                        })?;

                    // Do a real dependency resolution
                    match resolve_package_dependencies(ecosystem, package, version) {
                        Ok(result) => Ok(vec![Content::text(result)]),
                        Err(e) => {
                            tracing::warn!("Failed to resolve dependencies: {}", e);
                            Err(ToolError::ExecutionError(format!(
                                "Failed to resolve dependencies: {}",
                                e
                            )))
                        }
                    }
                }
                _ => Err(ToolError::NotFound(format!("Tool {} not found", tool_name))),
            }
        })
    }

    fn list_resources(&self) -> Vec<Resource> {
        vec![]
    }

    fn read_resource(
        &self,
        uri: &str,
    ) -> Pin<Box<dyn Future<Output = Result<String, ResourceError>> + Send + 'static>> {
        let uri = uri.to_string();
        Box::pin(async move {
            Err(ResourceError::NotFound(format!(
                "Resource {} not found",
                uri
            )))
        })
    }

    fn list_prompts(&self) -> Vec<Prompt> {
        vec![]
    }

    fn get_prompt(
        &self,
        prompt_name: &str,
    ) -> Pin<Box<dyn Future<Output = Result<String, PromptError>> + Send + 'static>> {
        let prompt_name = prompt_name.to_string();
        Box::pin(async move {
            Err(PromptError::NotFound(format!(
                "Prompt {} not found",
                prompt_name
            )))
        })
    }
}

// Attempt to resolve dependencies using the actual Babel code
fn resolve_package_dependencies(
    ecosystem: &str,
    package: &str,
    version: &str,
) -> Result<String, String> {
    let babel_package: BabelPackage<'static>;
    let babel_version: BabelVersion;

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
    let crates_index = match crates_index::GitIndex::with_path(
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
            _ => {} // Skip other package types
        }
    }

    // Create the final result
    let result = json!({
        "ecosystem": ecosystem,
        "package": package,
        "version": version,
        "resolved": true,
        "dependencies": deps
    });

    Ok(result.to_string())
}
