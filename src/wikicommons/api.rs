use super::models::{
    CategoryMember, CategoryMembersResponse, CategoryResult, ImageInfoResponse, MediaWikiNS,
    PageImageInfo,
};
use crate::{CachedApiClient, DiskCacheMiddleware, Error, FromBuilder, create_http_client};
use futures::stream::{self, StreamExt};
use reqwest_middleware::ClientWithMiddleware;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

const WIKICOMMONS_API_URL: &str = "https://commons.wikimedia.org/w/api.php";

/// Maximum number of pageids per API request
const MAX_PAGEID_BATCH_SIZE: usize = 50;

/// Maximum number of concurrent requests to avoid rate limiting
const MAX_CONCURRENCY: usize = 5;

/// Thumbnail width for imageinfo requests
const THUMB_WIDTH: u32 = 800;

/// Thumbnail height for imageinfo requests
const THUMB_HEIGHT: u32 = 600;

#[derive(Debug, Clone)]
pub struct WikicommonsClient {
    client: ClientWithMiddleware,
    cache: Option<Arc<DiskCacheMiddleware>>,
}

impl WikicommonsClient {
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

    /// Fetch all members of multiple categories (with caching and pagination)
    pub async fn get_category_members(
        &self,
        categories: &[String],
    ) -> Result<HashMap<String, CategoryResult>, Error> {
        if let Some(cache) = &self.cache {
            // Check cache for each category
            let mut results = HashMap::new();
            let mut missing = Vec::new();

            for category in categories {
                let global_id = crate::GlobalId::WikicommonsCategory(category.clone());
                if let Some(cached_result) = cache.get::<CategoryResult>(&global_id)? {
                    tracing::debug!("Cache hit for category: {}", category);
                    results.insert(category.clone(), cached_result);
                } else {
                    tracing::debug!("Cache miss for category: {}", category);
                    missing.push(category.clone());
                }
            }

            if !missing.is_empty() {
                // Fetch missing categories
                let fetched = self.fetch_category_members_uncached(&missing).await?;

                // Stage for caching
                for result in fetched.values() {
                    cache.stage(result)?;
                }

                results.extend(fetched);
            }

            Ok(results)
        } else {
            // No cache - fetch all
            self.fetch_category_members_uncached(categories).await
        }
    }

    /// Fetch category members without caching (with pagination and concurrency)
    async fn fetch_category_members_uncached(
        &self,
        categories: &[String],
    ) -> Result<HashMap<String, CategoryResult>, Error> {
        // Process categories concurrently
        let results = stream::iter(categories.iter().cloned())
            .map(|category| {
                let client_clone = self.clone();
                async move {
                    let members = client_clone.fetch_single_category(&category).await?;
                    let result = CategoryResult {
                        category: category.clone(),
                        members,
                        parent_category: None,
                    };
                    Ok::<_, Error>((category, result))
                }
            })
            .buffer_unordered(MAX_CONCURRENCY)
            .collect::<Vec<_>>()
            .await;

        // Collect results or return first error
        let mut map = HashMap::new();
        for result in results {
            let (category, category_result) = result?;
            map.insert(category, category_result);
        }

        Ok(map)
    }

    /// Fetch category members recursively based on subcategory pattern matching
    /// Uses BFS traversal to discover matching subcategories
    pub async fn get_category_members_recursive(
        &self,
        initial_categories: &[String],
        subcategory_pattern: &str,
    ) -> Result<HashMap<String, CategoryResult>, Error> {
        use regex::Regex;
        use std::collections::{HashSet, VecDeque};

        // Compile regex pattern once
        let pattern = Regex::new(subcategory_pattern)
            .map_err(|e| Error::InvalidInput(format!("Invalid regex pattern: {}", e)))?;

        // Queue for BFS: (category_name, parent_category_name)
        let mut queue: VecDeque<(String, Option<String>)> = initial_categories
            .iter()
            .map(|cat| (cat.clone(), None))
            .collect();

        // Track visited categories to prevent cycles
        let mut visited: HashSet<String> = HashSet::new();

        // Results accumulator
        let mut all_results = HashMap::new();

        tracing::info!(
            "Starting recursive category traversal with pattern: {}",
            subcategory_pattern
        );

        while let Some((category, parent)) = queue.pop_front() {
            // Skip if already visited
            if visited.contains(&category) {
                tracing::debug!("Skipping already visited category: {}", category);
                continue;
            }

            visited.insert(category.clone());
            tracing::info!("Processing category: {} (parent: {:?})", category, parent);

            // Fetch this category's members
            let members = self.fetch_single_category(&category).await?;

            // Store the result with parent metadata
            let result = CategoryResult {
                category: category.clone(),
                members: members.clone(),
                parent_category: parent,
            };

            // Cache the result if caching is enabled
            if let Some(ref cache) = self.cache {
                cache.stage(&result)?;
            }

            all_results.insert(category.clone(), result);

            // Look for matching subcategories (ns=14)
            if let Some(subcategories) = members.get(&MediaWikiNS::Category) {
                for (_, subcategory_member) in subcategories.iter() {
                    let subcategory_title = &subcategory_member.title;

                    // Normalize: remove "Category:" prefix for pattern matching
                    let normalized_title = subcategory_title
                        .strip_prefix("Category:")
                        .unwrap_or(subcategory_title);

                    // Check if the normalized title matches the pattern
                    if pattern.is_match(normalized_title) {
                        // Add to queue if not already visited (using normalized name)
                        if !visited.contains(normalized_title) {
                            queue.push_back((normalized_title.to_string(), Some(category.clone())));
                        }
                    }
                }
            }
        }

        tracing::info!(
            "Recursive traversal complete. Processed {} categories.",
            all_results.len()
        );
        Ok(all_results)
    }

    /// Fetch all members of a single category (handles pagination)
    async fn fetch_single_category(
        &self,
        category: &str,
    ) -> Result<BTreeMap<MediaWikiNS, BTreeMap<u64, CategoryMember>>, Error> {
        let mut all_members_raw = Vec::new();
        let mut continue_token: Option<String> = None;

        // Prepend "Category:" to the category name for the API query
        let category_title = if category.starts_with("Category:") {
            category.to_string()
        } else {
            format!("Category:{}", category)
        };

        loop {
            let mut params = vec![
                ("action", "query".to_string()),
                ("format", "json".to_string()),
                ("list", "categorymembers".to_string()),
                ("cmtitle", category_title.clone()),
                ("cmlimit", "500".to_string()), // Max limit per request
            ];

            if let Some(ref token) = continue_token {
                params.push(("cmcontinue", token.clone()));
            }

            tracing::debug!(
                "Fetching category members for '{}' (continue: {:?})",
                category_title,
                continue_token
            );

            let response = self
                .client
                .get(WIKICOMMONS_API_URL)
                .query(&params)
                .send()
                .await?;

            let response_text = response.text().await?;
            let parsed: CategoryMembersResponse =
                serde_json::from_str(&response_text).map_err(|e| {
                    Error::InvalidInput(format!("Failed to parse category response: {}", e))
                })?;

            all_members_raw.extend(parsed.query.categorymembers);

            // Check for continuation
            if let Some(cont) = parsed.r#continue {
                continue_token = Some(cont.cmcontinue);
            } else {
                break;
            }
        }

        tracing::debug!(
            "Fetched {} total members for category '{}'",
            all_members_raw.len(),
            category
        );

        // Transform flat list into nested BTreeMap structure
        let mut members: BTreeMap<MediaWikiNS, BTreeMap<u64, CategoryMember>> = BTreeMap::new();
        for raw_member in all_members_raw {
            let ns = MediaWikiNS::from(raw_member.ns);
            let pageid = raw_member.pageid;
            let member = CategoryMember {
                title: raw_member.title,
            };

            members.entry(ns).or_default().insert(pageid, member);
        }

        Ok(members)
    }

    /// Fetch imageinfo for multiple pageids and/or titles (with caching and batching)
    /// Titles will automatically be prefixed with "File:" if not already present
    pub async fn get_image_info(
        &self,
        pageids: &[u64],
        titles: &[String],
    ) -> Result<HashMap<String, PageImageInfo>, Error> {
        let mut results = HashMap::new();

        // Handle pageids
        if !pageids.is_empty() {
            if let Some(cache) = &self.cache {
                // Check cache for each pageid
                let mut missing = Vec::new();

                for &pageid in pageids {
                    let global_id = crate::GlobalId::WikicommonsPageId(pageid);
                    if let Some(cached_info) = cache.get::<PageImageInfo>(&global_id)? {
                        tracing::debug!("Cache hit for pageid: {}", pageid);
                        results.insert(pageid.to_string(), cached_info);
                    } else {
                        tracing::debug!("Cache miss for pageid: {}", pageid);
                        missing.push(pageid);
                    }
                }

                if !missing.is_empty() {
                    // Fetch missing pageids
                    let fetched = self.fetch_image_info_by_pageids(&missing).await?;

                    // Stage for caching
                    for info in fetched.values() {
                        cache.stage(info)?;
                    }

                    results.extend(fetched);
                }
            } else {
                // No cache - fetch all
                let fetched = self.fetch_image_info_by_pageids(pageids).await?;
                results.extend(fetched);
            }
        }

        // Handle titles
        if !titles.is_empty() {
            if let Some(cache) = &self.cache {
                // Check cache for each title
                let mut missing = Vec::new();

                for title in titles {
                    if let Some(cached_info) = cache.get_by_title(title)? {
                        tracing::debug!("Cache hit for title: {}", title);
                        results.insert(cached_info.pageid.to_string(), cached_info);
                    } else {
                        tracing::debug!("Cache miss for title: {}", title);
                        missing.push(title.clone());
                    }
                }

                if !missing.is_empty() {
                    // Fetch missing titles
                    let fetched = self.fetch_image_info_by_titles(&missing).await?;

                    // Stage for caching and create title symlinks
                    for info in fetched.values() {
                        // Stage by pageid
                        cache.stage(info)?;

                        // Create symlink from title to pageid
                        // Use the canonical title from the API response
                        if let Err(e) = cache.cache_title_symlink(&info.title, info.pageid) {
                            tracing::warn!(
                                "Failed to create title symlink for '{}': {}",
                                info.title,
                                e
                            );
                        }
                    }

                    results.extend(fetched);
                }
            } else {
                // No cache - fetch all
                let fetched = self.fetch_image_info_by_titles(titles).await?;
                results.extend(fetched);
            }
        }

        Ok(results)
    }

    /// Fetch imageinfo by pageids (with batching and concurrency)
    async fn fetch_image_info_by_pageids(
        &self,
        pageids: &[u64],
    ) -> Result<HashMap<String, PageImageInfo>, Error> {
        if pageids.is_empty() {
            return Ok(HashMap::new());
        }

        // Batch pageids into groups of MAX_PAGEID_BATCH_SIZE
        let batches: Vec<Vec<u64>> = pageids
            .chunks(MAX_PAGEID_BATCH_SIZE)
            .map(|chunk| chunk.to_vec())
            .collect();

        tracing::debug!(
            "Batching {} pageids into {} batches of up to {} each",
            pageids.len(),
            batches.len(),
            MAX_PAGEID_BATCH_SIZE
        );

        // Process batches concurrently
        let results = stream::iter(batches)
            .map(|batch| {
                let client_clone = self.clone();
                async move { client_clone.fetch_image_info_batch_by_pageids(&batch).await }
            })
            .buffer_unordered(MAX_CONCURRENCY)
            .collect::<Vec<_>>()
            .await;

        // Merge results
        let mut all_results = HashMap::new();
        for result in results {
            all_results.extend(result?);
        }

        Ok(all_results)
    }

    /// Fetch imageinfo by titles (with batching and concurrency)
    /// Titles will automatically be prefixed with "File:" if not already present
    async fn fetch_image_info_by_titles(
        &self,
        titles: &[String],
    ) -> Result<HashMap<String, PageImageInfo>, Error> {
        if titles.is_empty() {
            return Ok(HashMap::new());
        }

        // Batch titles into groups of MAX_PAGEID_BATCH_SIZE
        let batches: Vec<Vec<String>> = titles
            .chunks(MAX_PAGEID_BATCH_SIZE)
            .map(|chunk| chunk.to_vec())
            .collect();

        tracing::debug!(
            "Batching {} titles into {} batches of up to {} each",
            titles.len(),
            batches.len(),
            MAX_PAGEID_BATCH_SIZE
        );

        // Process batches concurrently
        let results = stream::iter(batches)
            .map(|batch| {
                let client_clone = self.clone();
                async move { client_clone.fetch_image_info_batch_by_titles(&batch).await }
            })
            .buffer_unordered(MAX_CONCURRENCY)
            .collect::<Vec<_>>()
            .await;

        // Merge results
        let mut all_results = HashMap::new();
        for result in results {
            all_results.extend(result?);
        }

        Ok(all_results)
    }

    /// Build common imageinfo query parameters
    fn build_imageinfo_params() -> Vec<(&'static str, String)> {
        vec![
            ("action", "query".to_string()),
            ("format", "json".to_string()),
            ("prop", "imageinfo".to_string()),
            ("iiprop", "url|metadata|extmetadata".to_string()),
            ("iiurlwidth", THUMB_WIDTH.to_string()),
            ("iiurlheight", THUMB_HEIGHT.to_string()),
            ("iilimit", "1".to_string()),
        ]
    }

    /// Shared helper to fetch and parse imageinfo response
    async fn fetch_and_parse_imageinfo(
        &self,
        params: Vec<(&str, String)>,
        debug_msg: String,
    ) -> Result<HashMap<String, PageImageInfo>, Error> {
        tracing::debug!("{}", debug_msg);

        let response = self
            .client
            .get(WIKICOMMONS_API_URL)
            .query(&params)
            .send()
            .await?;

        let response_text = response.text().await?;
        let parsed: ImageInfoResponse = serde_json::from_str(&response_text).map_err(|e| {
            Error::InvalidInput(format!("Failed to parse imageinfo response: {}", e))
        })?;

        // Convert pages map to HashMap<String, PageImageInfo>
        let mut results = HashMap::new();
        for (pageid_str, page_value) in parsed.query.pages {
            let page_info: PageImageInfo = serde_json::from_value(page_value)
                .map_err(|e| Error::InvalidInput(format!("Failed to parse page info: {}", e)))?;
            results.insert(pageid_str, page_info);
        }

        Ok(results)
    }

    /// Fetch imageinfo for a single batch of pageids
    async fn fetch_image_info_batch_by_pageids(
        &self,
        pageids: &[u64],
    ) -> Result<HashMap<String, PageImageInfo>, Error> {
        let pageids_str = pageids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join("|");

        let mut params = Self::build_imageinfo_params();
        params.push(("pageids", pageids_str.clone()));

        self.fetch_and_parse_imageinfo(
            params,
            format!("Fetching imageinfo for pageids: {}", pageids_str),
        )
        .await
    }

    /// Fetch imageinfo for a single batch of titles
    /// Titles will automatically be prefixed with "File:" if not already present
    async fn fetch_image_info_batch_by_titles(
        &self,
        titles: &[String],
    ) -> Result<HashMap<String, PageImageInfo>, Error> {
        // Prefix titles with "File:" if not already present
        let prefixed_titles: Vec<String> = titles
            .iter()
            .map(|title| {
                if title.starts_with("File:") {
                    title.clone()
                } else {
                    format!("File:{}", title)
                }
            })
            .collect();

        let titles_str = prefixed_titles.join("|");

        let mut params = Self::build_imageinfo_params();
        params.push(("titles", titles_str.clone()));

        self.fetch_and_parse_imageinfo(
            params,
            format!("Fetching imageinfo for titles: {}", titles_str),
        )
        .await
    }
}

impl Default for WikicommonsClient {
    fn default() -> Self {
        Self::new()
    }
}

impl CachedApiClient for WikicommonsClient {
    fn http_client(&self) -> &ClientWithMiddleware {
        &self.client
    }

    fn cache(&self) -> Option<&Arc<DiskCacheMiddleware>> {
        self.cache.as_ref()
    }
}

impl FromBuilder for WikicommonsClient {
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

/// Enrich category members with imageinfo
///
/// Takes category results and optionally fetches imageinfo for all NS_FILE members.
/// Returns enriched results with imageinfo merged into EnrichedCategoryMember.
pub async fn enrich_category_members(
    results: BTreeMap<String, CategoryResult>,
    client: &WikicommonsClient,
    traverse_pageid: bool,
) -> Result<BTreeMap<String, super::models::EnrichedCategoryResult>, Error> {
    use super::models::{EnrichedCategoryMember, EnrichedCategoryResult};
    use std::collections::HashSet;

    if traverse_pageid {
        // Collect all unique pageids from NS_FILE (ns=6) across all categories
        let all_pageids: Vec<u64> = results
            .values()
            .filter_map(|result| result.members.get(&MediaWikiNS::File))
            .flat_map(|file_map| file_map.keys().copied())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        tracing::debug!(
            "Fetching imageinfo for {} unique NS_FILE pageids",
            all_pageids.len()
        );

        // Batch fetch imageinfo for all NS_FILE pageids only
        let imageinfo_map = client.get_image_info(&all_pageids, &[]).await?;

        // Transform results to enriched format
        let enriched_results: BTreeMap<String, EnrichedCategoryResult> = results
            .into_iter()
            .map(|(category, result)| {
                // Transform nested structure: ns -> pageid -> member
                let enriched_members: BTreeMap<MediaWikiNS, BTreeMap<u64, EnrichedCategoryMember>> = result
                    .members
                    .into_iter()
                    .map(|(ns, pageid_map)| {
                        let enriched_map: BTreeMap<u64, EnrichedCategoryMember> = pageid_map
                            .into_iter()
                            .map(|(pageid, member)| {
                                let mut enriched = EnrichedCategoryMember::from(member);

                                // Only enrich NS_FILE members
                                if ns == MediaWikiNS::File {
                                    if let Some(page_info) = imageinfo_map.get(&pageid.to_string()) {
                                        enriched.imageinfo = page_info.imageinfo
                                            .as_ref()
                                            .and_then(|arr| arr.first())
                                            .cloned();

                                        if enriched.imageinfo.is_none() {
                                            tracing::warn!("Pageid {} has no imageinfo or empty imageinfo array", pageid);
                                        }
                                    } else {
                                        tracing::warn!("Pageid {} not found in imageinfo results", pageid);
                                    }
                                }

                                (pageid, enriched)
                            })
                            .collect();

                        (ns, enriched_map)
                    })
                    .collect();

                let enriched_result = EnrichedCategoryResult {
                    category: result.category,
                    members: enriched_members,
                };

                (category, enriched_result)
            })
            .collect();

        Ok(enriched_results)
    } else {
        // No enrichment - just convert to enriched format without imageinfo
        let enriched_results: BTreeMap<String, EnrichedCategoryResult> = results
            .into_iter()
            .map(|(category, result)| {
                let enriched_members: BTreeMap<MediaWikiNS, BTreeMap<u64, EnrichedCategoryMember>> =
                    result
                        .members
                        .into_iter()
                        .map(|(ns, pageid_map)| {
                            let enriched_map: BTreeMap<u64, EnrichedCategoryMember> = pageid_map
                                .into_iter()
                                .map(|(pageid, member)| {
                                    (pageid, EnrichedCategoryMember::from(member))
                                })
                                .collect();
                            (ns, enriched_map)
                        })
                        .collect();

                let enriched_result = EnrichedCategoryResult {
                    category: result.category,
                    members: enriched_members,
                };

                (category, enriched_result)
            })
            .collect();

        Ok(enriched_results)
    }
}
