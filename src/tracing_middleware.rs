use http::Extensions;
use reqwest::{Request, Response};
use reqwest_middleware::{Middleware, Next, Result};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct LoggingMiddleware;

#[async_trait::async_trait]
impl Middleware for LoggingMiddleware {
    async fn handle(
        &self,
        req: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> Result<Response> {
        let method = req.method().to_string();
        let url = req.url().to_string();

        let start = Instant::now();
        let result = next.run(req, extensions).await;
        let duration = start.elapsed();
        let duration_ms = duration.as_millis();

        match result {
            Ok(response) => {
                let status = response.status();

                if status.is_success() {
                    tracing::info!(
                        method = %method,
                        url = %url,
                        status = %status,
                        duration_ms = %duration_ms,
                        "HTTP request completed"
                    );
                    Ok(response)
                } else {
                    // Read body for logging, then reconstruct response
                    let version = response.version();
                    let headers = response.headers().clone();
                    let body_bytes = response.bytes().await.unwrap_or_default();
                    let body_text = String::from_utf8_lossy(&body_bytes);

                    tracing::warn!(
                        method = %method,
                        url = %url,
                        status = %status,
                        duration_ms = %duration_ms,
                        body = %body_text,
                        "HTTP request returned error status"
                    );

                    // Reconstruct response so downstream can still read body
                    let mut builder = http::Response::builder().status(status).version(version);
                    for (name, value) in headers.iter() {
                        builder = builder.header(name, value);
                    }
                    let http_response = builder
                        .body(body_bytes)
                        .expect("Failed to rebuild response");
                    Ok(Response::from(http_response))
                }
            }
            Err(error) => {
                tracing::error!(
                    method = %method,
                    url = %url,
                    duration_ms = %duration_ms,
                    error = %error,
                    "HTTP request failed"
                );
                Err(error)
            }
        }
    }
}
