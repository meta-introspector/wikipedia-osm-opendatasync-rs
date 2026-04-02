//! Core Wikicommons processing logic shared by CLI and batch modes

use crate::{
    DiskCacheMiddleware, Error,
    request::{WikicommonsCategorymembersRequest, WikicommonsRequest},
    wikicommons::{CategoryResult, MediaWikiNS, PageImageInfo, WikicommonsClient},
};
use futures::stream::{self, StreamExt};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

/// Result of an image download attempt
#[derive(Debug, Clone, Serialize)]
pub struct DownloadResult {
    pub pageid: u64,
    pub success: bool,
    pub path: Option<String>,
    pub error: Option<String>,
}

/// Download images from imageinfo URLs to the specified directory
///
/// Downloads are performed concurrently (up to 5 at a time) and are idempotent
/// (existing files are skipped).
pub async fn download_images_from_imageinfo(
    imageinfo: &HashMap<String, PageImageInfo>,
    output_dir: &Path,
) -> Result<Vec<DownloadResult>, Error> {
    let images_dir = output_dir.join("images");
    std::fs::create_dir_all(&images_dir)?;

    let client = reqwest::Client::builder()
        .user_agent(crate::USER_AGENT)
        .build()?;

    let download_tasks: Vec<_> = imageinfo
        .values()
        .filter_map(|page_info| {
            let pageid = page_info.pageid;
            let url = page_info
                .imageinfo
                .as_ref()
                .and_then(|ii| ii.first())
                .and_then(|info| info.url.as_ref())?;

            // Extract extension from URL
            let extension = url
                .rsplit('/')
                .next()
                .and_then(|filename| filename.rsplit('.').next())
                .unwrap_or("jpg");

            let file_path = images_dir.join(format!("{}.{}", pageid, extension));

            Some((pageid, url.clone(), file_path))
        })
        .collect();

    let results = stream::iter(download_tasks)
        .map(|(pageid, url, file_path)| {
            let client = client.clone();
            async move {
                // Skip if already downloaded (idempotent)
                if file_path.exists() {
                    tracing::debug!("Skipping already downloaded: {}", file_path.display());
                    return DownloadResult {
                        pageid,
                        success: true,
                        path: Some(file_path.to_string_lossy().to_string()),
                        error: None,
                    };
                }

                match download_single_image(&client, &url, &file_path).await {
                    Ok(()) => {
                        tracing::debug!("Downloaded: {}", file_path.display());
                        DownloadResult {
                            pageid,
                            success: true,
                            path: Some(file_path.to_string_lossy().to_string()),
                            error: None,
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to download pageid {}: {}", pageid, e);
                        DownloadResult {
                            pageid,
                            success: false,
                            path: None,
                            error: Some(e.to_string()),
                        }
                    }
                }
            }
        })
        .buffer_unordered(5) // 5 concurrent downloads
        .collect::<Vec<_>>()
        .await;

    Ok(results)
}

/// Download a single image from URL to file path
async fn download_single_image(
    client: &reqwest::Client,
    url: &str,
    file_path: &Path,
) -> Result<(), Error> {
    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        return Err(Error::InvalidInput(format!(
            "HTTP {} for {}",
            response.status(),
            url
        )));
    }

    let bytes = response.bytes().await?;
    std::fs::write(file_path, &bytes)?;

    Ok(())
}

/// Core Wikicommons categorymembers processing function
///
/// This function is the single source of truth for Wikicommons categorymembers processing logic.
/// Both CLI and batch modes call this function to ensure identical behavior.
///
/// # Arguments
/// * `request` - The Wikicommons categorymembers request configuration
/// * `cache` - Cache middleware for API responses
/// * `output_dir` - Optional output directory for downloading images
///
/// # Returns
/// A tuple containing:
/// * `BTreeMap<String, CategoryResult>` - Category members results
/// * `HashMap<String, PageImageInfo>` - Image info (if traverse_pageid is enabled)
/// * `Option<Vec<DownloadResult>>` - Download results (if download_images is enabled)
pub async fn fetch_and_process_categorymembers(
    request: &WikicommonsCategorymembersRequest,
    cache: Arc<DiskCacheMiddleware>,
    output_dir: Option<&Path>,
) -> Result<
    (
        BTreeMap<String, CategoryResult>,
        HashMap<String, PageImageInfo>,
        Option<Vec<DownloadResult>>,
    ),
    Error,
> {
    // Resolve categories from either inline list or file
    let categories = request.resolve_categories()?;
    let traverse_pageid = request.traverse_pageid;
    let download_images = request.download_images;

    if categories.is_empty() {
        return Ok((BTreeMap::new(), HashMap::new(), None));
    }

    // Create client with cache
    let client = WikicommonsClient::with_cache(cache);

    // Fetch category members (recursive if pattern provided, otherwise standard)
    let results = if let Some(ref pattern) = request.recurse_subcategory_pattern {
        tracing::info!(
            "Using recursive category traversal with pattern: {}",
            pattern
        );
        client
            .get_category_members_recursive(&categories, pattern)
            .await?
    } else {
        client.get_category_members(&categories).await?
    };

    // Convert HashMap to BTreeMap for consistency
    let results_btree: BTreeMap<String, CategoryResult> = results.into_iter().collect();

    // Fetch imageinfo separately if configured
    let imageinfo = if traverse_pageid {
        // Collect all unique NS_FILE (ns=6) pageids
        let all_pageids: Vec<u64> = results_btree
            .values()
            .filter_map(|result| result.members.get(&MediaWikiNS::File))
            .flat_map(|file_map| file_map.keys().copied())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        if !all_pageids.is_empty() {
            tracing::info!(
                "  Fetching imageinfo for {} pageids from categorymembers traversal",
                all_pageids.len()
            );
            client.get_image_info(&all_pageids, &[]).await?
        } else {
            HashMap::new()
        }
    } else {
        HashMap::new()
    };

    // Download images if configured
    let download_results = if download_images && !imageinfo.is_empty() {
        match output_dir {
            Some(dir) => {
                tracing::info!("Downloading {} images to {:?}", imageinfo.len(), dir);
                let results = download_images_from_imageinfo(&imageinfo, dir).await?;
                let success_count = results.iter().filter(|r| r.success).count();
                let fail_count = results.len() - success_count;
                tracing::info!(
                    "Download complete: {} succeeded, {} failed",
                    success_count,
                    fail_count
                );
                Some(results)
            }
            None => {
                tracing::warn!(
                    "--download-images requires --output-dir to be set; skipping downloads"
                );
                None
            }
        }
    } else {
        None
    };

    Ok((results_btree, imageinfo, download_results))
}

/// Core Wikicommons processing function that dispatches to the appropriate handler
///
/// This function handles different types of Wikicommons requests.
/// Currently only Categorymembers is fully supported.
///
/// # Arguments
/// * `request` - The Wikicommons request (enum)
/// * `cache` - Cache middleware for API responses
/// * `output_dir` - Optional output directory for downloading images
///
/// # Returns
/// A tuple containing:
/// * `BTreeMap<String, CategoryResult>` - Category members results (if applicable)
/// * `HashMap<String, PageImageInfo>` - Image info results
/// * `Option<Vec<DownloadResult>>` - Download results (if download_images is enabled)
pub async fn fetch_and_process_wikicommons(
    request: &WikicommonsRequest,
    cache: Arc<DiskCacheMiddleware>,
    output_dir: Option<&Path>,
) -> Result<
    (
        BTreeMap<String, CategoryResult>,
        HashMap<String, PageImageInfo>,
        Option<Vec<DownloadResult>>,
    ),
    Error,
> {
    match request {
        WikicommonsRequest::Categorymembers(categorymembers_req) => {
            fetch_and_process_categorymembers(categorymembers_req, cache, output_dir).await
        }
        WikicommonsRequest::Imageinfo(imageinfo_req) => {
            // Imageinfo requests could be handled here if needed for batch mode
            // For now, return empty results as batch mode doesn't use this variant
            let _ = imageinfo_req; // Suppress unused warning
            Ok((BTreeMap::new(), HashMap::new(), None))
        }
    }
}
