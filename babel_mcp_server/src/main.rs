use anyhow::Result;
use rmcp::transport::IntoTransport;
use rmcp::ServiceExt;
use tokio::io::{stdin, stdout};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{self, EnvFilter};
use std::env;

mod babel_handler;
mod http_server;

const DEFAULT_HTTP_ADDRESS: &str = "127.0.0.1:8000";
const DEFAULT_SSE_PATH: &str = "/sse";
const DEFAULT_POST_PATH: &str = "/message";

#[tokio::main]
async fn main() -> Result<()> {
    // Set up file appender for logging
    let file_appender = RollingFileAppender::new(Rotation::DAILY, "logs", "babel-mcp-server.log");

    // Initialize the tracing subscriber with file and stdout logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .with_writer(file_appender)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    
    // Default to CLI mode unless --http is specified
    let http_mode = args.iter().any(|arg| arg == "--http");

    tracing::info!("Starting Babel MCP server");

    if http_mode {
        // Get the HTTP address from command line or use default
        let http_address = args.iter()
            .position(|arg| arg == "--address")
            .and_then(|pos| args.get(pos + 1))
            .unwrap_or(&DEFAULT_HTTP_ADDRESS.to_string())
            .to_string();

        // Get the SSE path from command line or use default
        let sse_path = args.iter()
            .position(|arg| arg == "--sse-path")
            .and_then(|pos| args.get(pos + 1))
            .unwrap_or(&DEFAULT_SSE_PATH.to_string())
            .to_string();

        // Get the POST path from command line or use default
        let post_path = args.iter()
            .position(|arg| arg == "--post-path")
            .and_then(|pos| args.get(pos + 1))
            .unwrap_or(&DEFAULT_POST_PATH.to_string())
            .to_string();

        tracing::info!("Running in HTTP mode on {}", http_address);
        http_server::run_http_server(&http_address, &sse_path, &post_path).await?;
        
        // Keep the main thread alive until interrupted
        tokio::signal::ctrl_c().await?;
        tracing::info!("Received Ctrl+C, shutting down HTTP server");
        Ok(())
    } else {
        tracing::info!("Running in CLI mode");
        
        // Create our handler
        let handler = babel_handler::BabelHandler::new();
        
        // Create the stdin/stdout transport
        let transport = (stdin(), stdout());
        
        // Serve the handler with the transport
        let server = handler.serve(transport).await?;
        
        tracing::info!("Server initialized and ready to handle requests");
        
        // Wait for the server to finish
        server.waiting().await?;
        
        Ok(())
    }
}
