use super::models::Entity;
use super::models::response::{GetClaimsResponse, GetEntitiesResponse};
use crate::{
    CachedApiClient, DiskCacheMiddleware, Error, FromBuilder, create_http_client,
    wikidata::WikidataId,
};
use derive_builder::Builder;
use futures::stream::{self, StreamExt};
use reqwest_middleware::ClientWithMiddleware;
use std::collections::BTreeMap;
use std::sync::Arc;

const WIKIDATA_API_URL: &str = "https://www.wikidata.org/w/api.php";

/// Maximum number of IDs per Wikidata API request (API limit)
const MAX_BATCH_SIZE: usize = 50;

/// Maximum number of concurrent requests to avoid rate limiting
const MAX_CONCURRENCY: usize = 5;

#[derive(Debug, Clone)]
pub struct WikidataClient {
    client: ClientWithMiddleware,
    cache: Option<Arc<DiskCacheMiddleware>>,
}

impl WikidataClient {
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

    pub async fn get_entities(
        &self,
        query: &GetEntitiesQuery,
    ) -> Result<BTreeMap<WikidataId, Entity>, Error> {
        if let Some(cache) = &self.cache {
            // Use cache middleware - it will call us back with only missing IDs
            cache
                .fetch(query.ids.clone(), |missing_ids| async move {
                    let mut modified_query = query.clone();
                    modified_query.ids = missing_ids;
                    // Fetch missing IDs with batching
                    self.get_entities_uncached(&modified_query).await
                })
                .await
        } else {
            // No cache - fetch with batching
            self.get_entities_uncached(query).await
        }
    }

    /// Fetch entities with automatic batching and concurrency control (uncached)
    async fn get_entities_uncached(
        &self,
        query: &GetEntitiesQuery,
    ) -> Result<BTreeMap<WikidataId, Entity>, Error> {
        // Empty query optimization
        if query.ids.is_empty() {
            return Ok(BTreeMap::new());
        }

        // Single batch optimization - avoid batching overhead for small requests
        if query.ids.len() <= MAX_BATCH_SIZE {
            let response = self.fetch_entities_from_api(query).await?;
            return Vec::<Entity>::try_from(&response)
                .map(|entities| entities.into_iter().map(|e| (e.id.clone(), e)).collect());
        }

        // Multiple batches - process concurrently
        let batches: Vec<Vec<WikidataId>> = query
            .ids
            .chunks(MAX_BATCH_SIZE)
            .map(|chunk| chunk.to_vec())
            .collect();

        tracing::debug!(
            "Batching {} IDs into {} batches of up to {} IDs each",
            query.ids.len(),
            batches.len(),
            MAX_BATCH_SIZE
        );

        let results = stream::iter(batches)
            .map(|batch| {
                let client_clone = self.clone();
                let mut batch_query = query.clone();
                batch_query.ids = batch;

                async move { client_clone.fetch_entities_from_api(&batch_query).await }
            })
            .buffer_unordered(MAX_CONCURRENCY)
            .collect::<Vec<_>>()
            .await;

        // Merge all results
        let mut all_entities = BTreeMap::new();
        for result in results {
            let response = result?;
            let entities = Vec::<Entity>::try_from(&response)?;
            for entity in entities {
                all_entities.insert(entity.id.clone(), entity);
            }
        }

        Ok(all_entities)
    }

    async fn fetch_entities_from_api(
        &self,
        query: &GetEntitiesQuery,
    ) -> Result<GetEntitiesResponse, Error> {
        let mut params = vec![
            ("action", "wbgetentities".to_string()),
            ("format", "json".to_string()),
        ];

        if !query.ids.is_empty() {
            tracing::debug!(
                "Fetching {} entities: {}",
                query.ids.len(),
                query
                    .ids
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );

            let ids = query
                .ids
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join("|");
            params.push(("ids", ids));
        }

        if let Some(ref languages) = query.languages {
            let langs = languages.join("|");
            params.push(("languages", langs));
        }

        if let Some(ref props) = query.props {
            let props_str = props.join("|");
            params.push(("props", props_str));
        }

        let response = self
            .client
            .get(WIKIDATA_API_URL)
            .query(&params)
            .send()
            .await?;

        let response_text = response.text().await?;
        let parsed: GetEntitiesResponse = serde_json::from_str(&response_text)?;

        // Check for API errors
        if let Some(error) = parsed.error.clone() {
            return Err(Error::WikidataApi {
                code: error.code,
                info: error.info,
            });
        }

        Ok(parsed)
    }

    pub async fn get_claims(&self, query: &GetClaimsQuery) -> Result<GetClaimsResponse, Error> {
        let mut params = vec![
            ("action", "wbgetclaims".to_string()),
            ("format", "json".to_string()),
            ("entity", query.entity.to_string()),
        ];

        if let Some(ref property) = query.property {
            params.push(("property", property.to_string()));
        }

        if let Some(ref rank) = query.rank {
            params.push(("rank", rank.clone()));
        }

        let response = self
            .client
            .get(WIKIDATA_API_URL)
            .query(&params)
            .send()
            .await?;

        let response_text = response.text().await?;
        let parsed: GetClaimsResponse = serde_json::from_str(&response_text)?;

        // Check for API errors
        if let Some(error) = parsed.error.clone() {
            return Err(Error::WikidataApi {
                code: error.code,
                info: error.info,
            });
        }

        Ok(parsed)
    }
}

impl Default for WikidataClient {
    fn default() -> Self {
        Self::new()
    }
}

impl CachedApiClient for WikidataClient {
    fn http_client(&self) -> &ClientWithMiddleware {
        &self.client
    }

    fn cache(&self) -> Option<&Arc<DiskCacheMiddleware>> {
        self.cache.as_ref()
    }
}

impl FromBuilder for WikidataClient {
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

#[derive(Debug, Clone, Builder)]
#[builder(setter(into))]
pub struct GetEntitiesQuery {
    #[builder(default)]
    pub ids: Vec<WikidataId>,
    #[builder(default)]
    pub languages: Option<Vec<String>>,
    #[builder(default)]
    pub props: Option<Vec<String>>,
}

impl GetEntitiesQuery {
    pub fn builder() -> GetEntitiesQueryBuilder {
        GetEntitiesQueryBuilder::default()
    }
}

#[derive(Debug, Clone, Builder)]
#[builder(setter(into))]
pub struct GetClaimsQuery {
    pub entity: WikidataId,
    #[builder(default)]
    pub property: Option<WikidataId>,
    #[builder(default)]
    pub rank: Option<String>,
}

impl GetClaimsQuery {
    pub fn builder() -> GetClaimsQueryBuilder {
        GetClaimsQueryBuilder::default()
    }
}
