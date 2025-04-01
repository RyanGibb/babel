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

    // Add a debug logging middleware to see all incoming requests
    use axum::middleware::{self, Next};
    use axum::response::Response;
    use axum::extract::Request;
    use http::HeaderMap;

    async fn log_request_response(request: Request, next: Next) -> Response {
        let method = request.method().clone();
        let uri = request.uri().clone();
        let path = uri.path();
        let query = uri.query();
        let headers = request.headers().clone();
        
        // Log the request details with more explicit output for debugging
        println!("=================================================================");
        println!("👉 INCOMING REQUEST: {} {}", method, uri);
        println!("👉 PATH: {}", path);
        if let Some(q) = query {
            println!("👉 QUERY: {}", q);
        }
        println!("👉 HEADERS: {:#?}", headers);
        
        // Process the request
        let response = next.run(request).await;
        
        // Log the response details
        println!("👉 RESPONSE STATUS: {}", response.status());
        println!("👉 RESPONSE HEADERS: {:#?}", response.headers());
        println!("=================================================================");
        
        // Also log through tracing
        tracing::info!("Request: {} {} -> Response: {}", method, uri, response.status());
        
        response
    }

    // Add a debug endpoint to check server status
    async fn debug_handler() -> axum::response::Json<serde_json::Value> {
        axum::response::Json(serde_json::json!({
            "status": "ok",
            "message": "Babel MCP server is running",
            "timestamp": chrono::Utc::now().to_rfc3339()
        }))
    }

    // Add a root handler that redirects to SSE for testing
    use axum::http::header;
    use axum::http::StatusCode;
    
    async fn root_handler(req: axum::extract::Request) -> Response {
        // Check if this is coming from Claude by examining headers
        let user_agent = req.headers().get(header::USER_AGENT)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
            
        // If Claude is requesting SSE but hitting the root, serve SSE directly
        if user_agent.contains("Claude") {
            println!("🔍 Claude SSE client detected, serving SSE from root");
            
            // Create a response with SSE content type
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/event-stream")
                .header("Cache-Control", "no-cache")
                .body(axum::body::Body::from(
                    "event: endpoint\ndata: /message?sessionId=direct-root-connection\n\n"
                ))
                .unwrap()
        } else {
            // Otherwise serve the normal info response
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "server": "Babel MCP Server",
                        "endpoints": {
                            "sse": "/sse",
                            "message": "/message?sessionId=XXX",
                            "debug": "/debug"
                        },
                        "version": env!("CARGO_PKG_VERSION"),
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    })).unwrap()
                ))
                .unwrap()
        }
    }
    
    // Create a CORS layer to allow all origins
    use axum::http::Method;
    use tower_http::cors::{Any, CorsLayer};
    
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any)
        .expose_headers(Any)
        .allow_credentials(true);
    
    // Instead of wrapping the RMCP router, merge our routes into it
    // This ensures we don't interfere with the SSE content type headers
    let router = router
        .route("/", axum::routing::get(root_handler))
        .route("/debug", axum::routing::get(debug_handler))
        .layer(cors)
        .layer(middleware::from_fn(log_request_response));

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