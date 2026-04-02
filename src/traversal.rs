//! Traversal utilities for following references between data sources

use crate::{
    DiskCacheMiddleware, Error,
    overpass::{Element, OSMId, OverpassClient, Request},
    types::CommonsTraverseDepth,
    wikicommons::WikicommonsClient,
    wikidata::{EntityCollection, EntityValue, StatementRank, WikidataId},
};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

/// Extract OSM IDs from Wikidata entities and fetch them via Overpass API
///
/// This function looks for OSM properties in Wikidata entities:
/// - P402: OSM relation ID
/// - P10689: OSM way ID
/// - P11693: OSM node ID
///
/// Returns a map of OSM elements keyed by their OSM ID.
pub async fn fetch_osm_from_wikidata_entities(
    entities: &EntityCollection,
    cache: Arc<DiskCacheMiddleware>,
) -> Result<BTreeMap<OSMId, Element>, Error> {
    // Define OSM property IDs
    let osm_relation_prop = WikidataId::try_from("P402")?;
    let osm_way_prop = WikidataId::try_from("P10689")?;
    let osm_node_prop = WikidataId::try_from("P11693")?;

    let mut osm_ids = Vec::new();

    // Extract OSM IDs from all entities
    for entity in entities.values() {
        for (prop, osm_id_constructor) in [
            (&osm_relation_prop, OSMId::Relation as fn(u64) -> OSMId),
            (&osm_way_prop, OSMId::Way as fn(u64) -> OSMId),
            (&osm_node_prop, OSMId::Node as fn(u64) -> OSMId),
        ] {
            if let Some(property) = entity.properties.get(prop) {
                for stmt in &property.statements {
                    if stmt.rank != StatementRank::Deprecated {
                        match &stmt.value {
                            EntityValue::String(s) | EntityValue::ExternalId(s) => {
                                if let Ok(id) = s.parse::<u64>() {
                                    osm_ids.push(osm_id_constructor(id));
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    // If no OSM IDs found, return empty map
    if osm_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    tracing::info!(
        "Fetching {} OSM elements from wikidata traversal",
        osm_ids.len()
    );

    // Fetch OSM elements via Overpass API
    let overpass_client = OverpassClient::with_cache(cache);

    let request = Request::builder()
        .query_by_ids(osm_ids)
        .timeout(25)
        .build()?;

    let elements = overpass_client.execute(&request).await?;

    tracing::info!("Fetched {} OSM elements", elements.len());

    Ok(elements)
}

/// Extract Commons categories from Wikidata entities (P373) and fetch via Wikicommons API
///
/// This function looks for the Commons category property (P373) in Wikidata entities
/// and fetches the category members from Wikimedia Commons.
///
/// Returns (category results, imageinfo) as separate maps depending on depth setting.
pub async fn fetch_commons_from_wikidata_entities(
    entities: &EntityCollection,
    cache: Arc<DiskCacheMiddleware>,
    depth: CommonsTraverseDepth,
) -> Result<
    (
        BTreeMap<String, crate::wikicommons::CategoryResult>,
        std::collections::HashMap<String, crate::wikicommons::PageImageInfo>,
    ),
    Error,
> {
    // Define Commons category property ID
    let commons_category_prop = WikidataId::try_from("P373")?;

    let mut category_names = Vec::new();

    // Extract Commons category names from all entities
    for entity in entities.values() {
        if let Some(property) = entity.properties.get(&commons_category_prop) {
            for stmt in &property.statements {
                if stmt.rank != StatementRank::Deprecated
                    && let EntityValue::String(category_name) = &stmt.value
                {
                    category_names.push(category_name.clone());
                }
            }
        }
    }

    // Deduplicate category names
    let unique_categories: Vec<String> = category_names
        .into_iter()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    // If no categories found, return empty maps
    if unique_categories.is_empty() {
        return Ok((BTreeMap::new(), std::collections::HashMap::new()));
    }

    tracing::info!(
        "Fetching {} Commons categories from wikidata traversal",
        unique_categories.len()
    );

    // Fetch category members via Wikicommons API
    let wikicommons_client = WikicommonsClient::with_cache(cache);

    let results = wikicommons_client
        .get_category_members(&unique_categories)
        .await?;

    // Convert HashMap to BTreeMap for consistency
    let results_btree: BTreeMap<String, crate::wikicommons::CategoryResult> =
        results.into_iter().collect();

    // Fetch imageinfo separately based on depth setting
    let traverse_pageid = matches!(depth, CommonsTraverseDepth::Page);
    let imageinfo = if traverse_pageid {
        // Collect all unique NS_FILE (ns=6) pageids
        let all_pageids: Vec<u64> = results_btree
            .values()
            .filter_map(|result| result.members.get(&crate::wikicommons::MediaWikiNS::File))
            .flat_map(|file_map| file_map.keys().copied())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        if !all_pageids.is_empty() {
            tracing::info!(
                "  Fetching imageinfo for {} pageids from Commons traversal",
                all_pageids.len()
            );
            wikicommons_client.get_image_info(&all_pageids, &[]).await?
        } else {
            std::collections::HashMap::new()
        }
    } else {
        std::collections::HashMap::new()
    };

    tracing::info!(
        "Fetched {} Commons category results with {} imageinfo entries",
        results_btree.len(),
        imageinfo.len()
    );

    Ok((results_btree, imageinfo))
}
