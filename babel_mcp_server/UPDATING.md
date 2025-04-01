# Babel MCP Server Update Plan

## Overview
We're updating the Babel MCP server to:
1. Migrate from the old MCP SDK to the new RMCP SDK
2. Add an HTTP server implementation alongside the existing CLI-based server

## Required Changes

### Dependencies
- Remove `mcp-core` and `mcp-server` dependencies
- Add `rmcp` with appropriate features
- Add `tokio-util`, `axum`, and `schemars` dependencies

### Router Implementation
- Update `BabelRouter` to implement the RMCP `ServerHandler` trait
- Use the `#[tool]` macros from RMCP for defining tools
- Convert error handling to use RMCP's `Error` type
- Update the capability builder to use RMCP's version

### HTTP Server
- Implement an HTTP server using the SSE transport
- Support command-line arguments for HTTP mode
- Add support for configuring bind address, SSE path, and message POST path

### CLI Server
- Update the existing CLI server to use RMCP's transport
- Maintain backward compatibility with existing usages

## Implementation Steps
1. Update Cargo.toml with the new dependencies
2. Create a new BabelHandler implementing the RMCP ServerHandler trait
3. Implement the HTTP server using SSE transport
4. Update the main.rs to support both CLI and HTTP modes
5. Test both modes of operation

## Usage
- CLI mode: `cargo run -p babel_mcp_server`
- HTTP mode: `cargo run -p babel_mcp_server -- --http [--address IP:PORT] [--sse-path PATH] [--post-path PATH]`