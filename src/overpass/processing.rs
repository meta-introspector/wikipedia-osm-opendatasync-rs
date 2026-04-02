//! Core Overpass processing logic shared by CLI and batch modes

use crate::{
    DiskCacheMiddleware, Error,
    overpass::{Element, OSMId, OverpassClient, Request},
    request::OverpassQueryRequest,
};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Core Overpass processing function that handles OSM element fetching
///
/// This function is the single source of truth for Overpass processing logic.
/// Both CLI and batch modes call this function to ensure identical behavior.
///
/// # Arguments
/// * `request` - The Overpass query request configuration
/// * `cache` - Cache middleware for API responses
///
/// # Returns
/// A BTreeMap of OSM elements keyed by OSMId
pub async fn fetch_and_process_overpass(
    request: &OverpassQueryRequest,
    cache: Arc<DiskCacheMiddleware>,
) -> Result<BTreeMap<OSMId, Element>, Error> {
    // Resolve OSM IDs from either inline lists or file
    let (nodes, ways, relations) = request.resolve_ids()?;

    let total_count = nodes.len() + ways.len() + relations.len();
    if total_count == 0 {
        return Ok(BTreeMap::new());
    }

    // Create client with cache
    let client = OverpassClient::with_cache(cache);

    // Combine all IDs into a single vector
    let mut ids = Vec::new();
    ids.extend(nodes.into_iter().map(OSMId::Node));
    ids.extend(ways.into_iter().map(OSMId::Way));
    ids.extend(relations.into_iter().map(OSMId::Relation));

    // Build and execute request
    let overpass_request = Request::builder()
        .bounding_box(request.bbox)
        .query_by_ids(ids)
        .timeout(request.timeout)
        .build()?;

    let elements = client.execute(&overpass_request).await?;

    Ok(elements)
}
