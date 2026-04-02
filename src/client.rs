//! Shared abstractions for API clients
//!
//! This module provides common patterns used across all API clients:
//! - Client construction with optional caching
//! - Cache-aware fetching with automatic fallback
//! - Batching and concurrency control

use crate::{Cacheable, DiskCacheMiddleware, Error};
use reqwest_middleware::ClientWithMiddleware;
use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;

/// Trait for API clients that support caching and batching
pub trait CachedApiClient: Clone {
    /// Get reference to the underlying HTTP client
    fn http_client(&self) -> &ClientWithMiddleware;

    /// Get reference to the cache middleware if enabled
    fn cache(&self) -> Option<&Arc<DiskCacheMiddleware>>;
}

/// Helper trait for types that can be used as cache keys
pub trait CacheKey: Clone + Cacheable {
    /// Convert to string for use in HashMap keys
    fn to_key(&self) -> String;
}

/// Generic cache-aware fetch implementation
///
/// This function handles the common pattern of:
/// 1. Check cache for existing items
/// 2. Fetch missing items from API
/// 3. Stage fetched items in cache
/// 4. Return combined results
///
/// Note: This is a thin wrapper around DiskCacheMiddleware::fetch.
/// The cache middleware already implements the full caching logic.
pub async fn fetch_with_cache<K, V, F, Fut>(
    cache: Option<&Arc<DiskCacheMiddleware>>,
    ids: Vec<K>,
    fetch_fn: F,
) -> Result<BTreeMap<K, V>, Error>
where
    K: CacheKey + Into<crate::GlobalId> + From<crate::GlobalId> + Ord,
    V: Cacheable,
    F: FnOnce(Vec<K>) -> Fut,
    Fut: Future<Output = Result<BTreeMap<K, V>, Error>>,
{
    if let Some(cache_ref) = cache {
        // Use the cache's fetch method which handles cache check + fetch + staging
        cache_ref.fetch(ids, fetch_fn).await
    } else {
        // No cache - fetch directly
        fetch_fn(ids).await
    }
}

/// Builder for creating API clients with optional caching
#[derive(Clone)]
pub struct ApiClientBuilder {
    http_client: ClientWithMiddleware,
    cache: Option<Arc<DiskCacheMiddleware>>,
}

impl ApiClientBuilder {
    /// Create a new builder with the given HTTP client
    pub fn new(http_client: ClientWithMiddleware) -> Self {
        Self {
            http_client,
            cache: None,
        }
    }

    /// Enable caching with the given cache middleware
    pub fn with_cache(mut self, cache: Arc<DiskCacheMiddleware>) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Build a client that implements the FromBuilder trait
    pub fn build<T: FromBuilder>(self) -> T {
        T::from_builder(self.http_client, self.cache)
    }
}

/// Trait for types that can be constructed from an ApiClientBuilder
pub trait FromBuilder: Sized {
    /// Construct from HTTP client and optional cache
    fn from_builder(
        http_client: ClientWithMiddleware,
        cache: Option<Arc<DiskCacheMiddleware>>,
    ) -> Self;
}
