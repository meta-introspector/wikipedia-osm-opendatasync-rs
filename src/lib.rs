pub mod batch;
mod cache;
pub mod client;
pub mod erdfa;
pub mod error;
pub mod overpass;
pub mod request;
pub mod tracing_middleware;
pub mod traversal;
pub mod types;
pub mod wikicommons;
pub mod wikidata;
pub mod zkperf;

pub use cache::{Cacheable, DiskCacheMiddleware, GlobalId};
pub use client::{ApiClientBuilder, CacheKey, CachedApiClient, FromBuilder, fetch_with_cache};
pub use error::Error;

const USER_AGENT: &str = concat!(
    "opendatasync/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/example/opendatasync)"
);

use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use tracing_middleware::LoggingMiddleware;

/// Creates a configured HTTP client with middleware for API requests
pub fn create_http_client() -> ClientWithMiddleware {
    let reqwest_client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .expect("Failed to create HTTP client");

    ClientBuilder::new(reqwest_client)
        .with(LoggingMiddleware)
        .build()
}
