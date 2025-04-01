use anyhow::Result;
use rmcp::transport::sse_server::{SseServer, SseServerConfig};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing;

use crate::babel_handler::BabelHandler;

pub async fn run_http_server(bind_address: &str, sse_path: &str, post_path: &str) -> Result<()> {
    tracing::info!("Starting Babel HTTP server on {}", bind_address);

    // Parse the bind address
    let addr = bind_address.parse()?;

    // Set up the SSE server configuration
    let config = SseServerConfig {
        bind: addr,
        sse_path: sse_path.to_string(),
        post_path: post_path.to_string(),
        ct: CancellationToken::new(),
        sse_keep_alive: None,
    };

    // Create a new SseServer with our custom configuration
    let (sse_server, router) = SseServer::new(config);

    // We could customize the router further here if needed
    // e.g., router = router.route("/api/status", get(status_handler));

    // Create and bind a TCP listener to our address
    let listener = TcpListener::bind(sse_server.config.bind).await?;
    tracing::info!("HTTP server listening on {}", sse_server.config.bind);

    // Create a cancellation token for graceful shutdown
    let ct = sse_server.config.ct.child_token();

    // Start the axum HTTP server
    let server = axum::serve(listener, router).with_graceful_shutdown(async move {
        ct.cancelled().await;
        tracing::info!("HTTP server shutdown requested");
    });

    // Spawn the server task
    tokio::spawn(async move {
        if let Err(e) = server.await {
            tracing::error!(error = %e, "HTTP server shutdown with error");
        }
    });

    // Register our BabelHandler with the SSE server
    let _ct = sse_server.with_service(BabelHandler::new);

    // Return the cancellation token so the main process can cancel when needed
    Ok(())
}