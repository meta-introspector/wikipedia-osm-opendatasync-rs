use crate::overpass::models::response::Element;
use crate::overpass::{OSMId, OverpassResponse, Request};
use crate::{CachedApiClient, DiskCacheMiddleware, Error, FromBuilder, create_http_client};
use reqwest_middleware::ClientWithMiddleware;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

const OVERPASS_API_URL: &str = "https://overpass-api.de/api/interpreter";

#[derive(Debug, Clone)]
pub struct OverpassClient {
    client: ClientWithMiddleware,
    cache: Option<Arc<DiskCacheMiddleware>>,
}

impl OverpassClient {
    pub fn new() -> Self {
        Self {
            client: create_http_client(),
            cache: None,
        }
    }

    pub fn with_cache(cache: Arc<DiskCacheMiddleware>) -> Self {
        Self {
            client: create_http_client(),
            cache: Some(cache),
        }
    }

    /// Execute an Overpass query request
    pub async fn execute(&self, request: &Request) -> Result<BTreeMap<OSMId, Element>, Error> {
        if let Some(cache) = &self.cache {
            // Use cache middleware
            cache
                .fetch(request.query_by_ids.clone(), |missing_ids| async move {
                    let mut modified_request = request.clone();
                    modified_request.query_by_ids = missing_ids;
                    let response = self.fetch_from_api_with_retry(&modified_request).await?;
                    Ok(response.elements)
                })
                .await
        } else {
            // No cache, fetch directly
            let response = self.fetch_from_api_with_retry(request).await?;
            Ok(response.elements)
        }
    }

    async fn fetch_from_api(&self, request: &Request) -> Result<OverpassResponse, Error> {
        let query = request.to_query_string();

        tracing::debug!("Executing Overpass query: {}", query);

        let response = self
            .client
            .post(OVERPASS_API_URL)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(format!("data={}", urlencoding::encode(&query)))
            .send()
            .await?;

        let status = response.status();
        let response_text = response.text().await?;

        if !status.is_success() {
            return Err(Error::OverpassApi {
                status: status.as_u16(),
                message: response_text,
            });
        }

        let parsed: OverpassResponse = serde_json::from_str(&response_text)?;

        // Count element types for summary
        let mut node_count = 0;
        let mut way_count = 0;
        let mut relation_count = 0;

        for element in parsed.elements.values() {
            match element {
                Element::Node(_) => node_count += 1,
                Element::Way(_) => way_count += 1,
                Element::Relation(_) => relation_count += 1,
            }
        }

        tracing::debug!(
            "Received Overpass response: {} total elements ({} nodes, {} ways, {} relations)",
            parsed.elements.len(),
            node_count,
            way_count,
            relation_count
        );

        Ok(parsed)
    }

    /// Fetch from API with retry logic for 504 errors
    async fn fetch_from_api_with_retry(
        &self,
        request: &Request,
    ) -> Result<OverpassResponse, Error> {
        const MAX_RETRIES: u32 = 5;
        const RETRY_DELAY_SECS: u64 = 60;

        for attempt in 1..=MAX_RETRIES {
            match self.fetch_from_api(request).await {
                Ok(response) => return Ok(response),
                Err(Error::OverpassApi {
                    status,
                    ref message,
                }) if status == 504 => {
                    if attempt < MAX_RETRIES {
                        // Extract brief error info from the HTML message
                        let error_summary = if message.contains("timeout") {
                            "server timeout"
                        } else if message.contains("too busy") {
                            "server too busy"
                        } else {
                            "gateway timeout"
                        };

                        tracing::warn!(
                            "Retrying [{}/{}] after 504 error: {} (waiting {} seconds)",
                            attempt,
                            MAX_RETRIES,
                            error_summary,
                            RETRY_DELAY_SECS
                        );

                        sleep(Duration::from_secs(RETRY_DELAY_SECS)).await;
                    } else {
                        // Last attempt failed, return the error
                        return Err(Error::OverpassApi {
                            status,
                            message: message.clone(),
                        });
                    }
                }
                Err(e) => {
                    // Non-504 error, fail immediately without retry
                    return Err(e);
                }
            }
        }

        // This shouldn't be reached, but satisfy the compiler
        unreachable!("Retry loop should always return")
    }
}

impl Default for OverpassClient {
    fn default() -> Self {
        Self::new()
    }
}

impl CachedApiClient for OverpassClient {
    fn http_client(&self) -> &ClientWithMiddleware {
        &self.client
    }

    fn cache(&self) -> Option<&Arc<DiskCacheMiddleware>> {
        self.cache.as_ref()
    }
}

impl FromBuilder for OverpassClient {
    fn from_builder(
        http_client: ClientWithMiddleware,
        cache: Option<Arc<DiskCacheMiddleware>>,
    ) -> Self {
        Self {
            client: http_client,
            cache,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overpass::OSMId;

    #[test]
    fn test_client_creation() {
        let client = OverpassClient::new();
        assert!(!std::ptr::addr_of!(client).is_null());
    }

    #[test]
    fn test_request_query_generation() {
        let request = Request::builder()
            .query_by_ids(vec![OSMId::Node(123)])
            .build()
            .unwrap();

        let query = request.to_query_string();
        assert!(query.contains("node(123)"));
    }
}
