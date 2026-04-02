//! Core Wikidata processing logic shared by CLI and batch modes

use crate::{
    DiskCacheMiddleware, Error, overpass,
    request::WikidataGetEntitiesRequest,
    traversal,
    types::ResolveMode,
    wikicommons,
    wikidata::{
        CommonsMediaPageId, EntityCollection, EntityValue, GetEntitiesQuery, StatementRank,
        WikidataClient, WikidataId,
    },
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

/// Core Wikidata processing function that handles fetching and traversal
///
/// This function is the single source of truth for Wikidata processing logic.
/// Both CLI and batch modes call this function to ensure identical behavior.
///
/// # Arguments
/// * `request` - The Wikidata request configuration
/// * `cache` - Cache middleware for API responses
///
/// # Returns
/// A tuple containing:
/// * `EntityCollection` - All fetched Wikidata entities (including traversed ones)
/// * `BTreeMap<OSMId, Element>` - OSM elements from traversal
/// * `BTreeMap<String, CategoryResult>` - Commons category data
/// * `HashMap<String, PageImageInfo>` - Commons imageinfo data
pub async fn fetch_and_process_wikidata(
    request: &WikidataGetEntitiesRequest,
    cache: Arc<DiskCacheMiddleware>,
) -> Result<
    (
        EntityCollection,
        BTreeMap<overpass::OSMId, overpass::Element>,
        BTreeMap<String, wikicommons::CategoryResult>,
        HashMap<String, wikicommons::PageImageInfo>,
    ),
    Error,
> {
    // Resolve QIDs from either inline list or file
    let qids = request.resolve_qids()?;

    if qids.is_empty() {
        return Ok((
            EntityCollection(BTreeMap::new()),
            BTreeMap::new(),
            BTreeMap::new(),
            HashMap::new(),
        ));
    }

    // Create client with cache
    let client = WikidataClient::with_cache(cache.clone());

    let wikidata_ids: Result<Vec<WikidataId>, Error> = qids
        .iter()
        .map(|qid| WikidataId::try_from(qid.as_str()))
        .collect();
    let wikidata_ids = wikidata_ids?;

    // Prepare traverse_properties list (mutable copy)
    let mut traverse_properties = request.traverse_properties.clone();

    // Handle --traverse-osm flag by appending OSM properties if not already present
    if request.traverse_osm {
        let osm_properties = vec!["P402", "P10689", "P11693"];
        let existing: HashSet<String> = traverse_properties.iter().cloned().collect();

        for prop in osm_properties {
            if !existing.contains(prop) {
                traverse_properties.push(prop.to_string());
            }
        }
    }

    // Handle --traverse-commons flag by appending P373 if not already present
    if request.traverse_commons {
        let existing: HashSet<String> = traverse_properties.iter().cloned().collect();
        if !existing.contains("P373") {
            traverse_properties.push("P373".to_string());
        }
    }

    // Fetch initial entities
    let query = GetEntitiesQuery::builder()
        .ids(wikidata_ids.clone())
        .languages(vec!["en".to_string()])
        .build()?;

    let entities_map = client.get_entities(&query).await?;
    let mut entities = EntityCollection(entities_map);

    // Traverse properties if requested
    if !traverse_properties.is_empty() {
        let traverse_prop_ids: Result<Vec<WikidataId>, Error> = traverse_properties
            .iter()
            .map(|prop| WikidataId::try_from(prop.as_str()))
            .collect();
        let traverse_prop_ids = traverse_prop_ids?;

        // Separate OSM, Commons, and regular Wikidata properties
        let osm_relation_prop = WikidataId::try_from("P402")?; // OSM relation
        let osm_way_prop = WikidataId::try_from("P10689")?; // OSM way
        let osm_node_prop = WikidataId::try_from("P11693")?; // OSM node
        let commons_category_prop = WikidataId::try_from("P373")?; // Commons category

        let _osm_props: Vec<WikidataId> = traverse_prop_ids
            .iter()
            .filter(|prop| {
                *prop == &osm_relation_prop || *prop == &osm_way_prop || *prop == &osm_node_prop
            })
            .cloned()
            .collect();

        let _commons_props: Vec<WikidataId> = traverse_prop_ids
            .iter()
            .filter(|prop| *prop == &commons_category_prop)
            .cloned()
            .collect();

        let wikidata_props: Vec<WikidataId> = traverse_prop_ids
            .iter()
            .filter(|prop| {
                *prop != &osm_relation_prop
                    && *prop != &osm_way_prop
                    && *prop != &osm_node_prop
                    && *prop != &commons_category_prop
            })
            .cloned()
            .collect();

        // Handle regular Wikidata property traversal
        if !wikidata_props.is_empty() {
            let mut current_wave: Vec<WikidataId> = wikidata_ids.clone();

            loop {
                let mut to_fetch = HashSet::new();

                for qid in &current_wave {
                    if let Some(entity) = entities.get(qid) {
                        for traverse_prop in &wikidata_props {
                            if let Some(property) = entity.properties.get(traverse_prop) {
                                for stmt in &property.statements {
                                    if stmt.rank != StatementRank::Deprecated
                                        && let EntityValue::WikidataItem(target) = &stmt.value
                                        && !entities.contains_key(target)
                                    {
                                        to_fetch.insert(target.clone());
                                    }
                                }
                            }
                        }
                    }
                }

                if to_fetch.is_empty() {
                    break;
                }

                // Batch fetch missing entities
                let to_fetch_vec: Vec<WikidataId> = to_fetch.iter().cloned().collect();
                let query = GetEntitiesQuery::builder()
                    .ids(to_fetch_vec.clone())
                    .languages(vec!["en".to_string()])
                    .build()?;

                let new_entities = client.get_entities(&query).await?;
                entities.extend(new_entities);

                current_wave = to_fetch_vec;
            }
        }
    }

    // Handle OSM property traversal
    let osm_elements = if request.traverse_osm {
        traversal::fetch_osm_from_wikidata_entities(&entities, cache.clone()).await?
    } else {
        BTreeMap::new()
    };

    // Handle Commons category traversal
    let (commons_data, commons_imageinfo) = if request.traverse_commons {
        traversal::fetch_commons_from_wikidata_entities(
            &entities,
            cache.clone(),
            request.traverse_commons_depth,
        )
        .await?
    } else {
        (BTreeMap::new(), HashMap::new())
    };

    // Resolve labels if requested
    if request.resolve_headers != ResolveMode::None || request.resolve_data != ResolveMode::None {
        use std::collections::BTreeSet;

        // Collect all IDs that need labels
        let mut ids_to_resolve: BTreeSet<WikidataId> = BTreeSet::new();

        // Collect header IDs (property IDs)
        match request.resolve_headers {
            ResolveMode::None => {}
            ResolveMode::All => {
                for entity in entities.values() {
                    for property_id in entity.get_property_ids() {
                        ids_to_resolve.insert(property_id);
                    }
                }
            }
            ResolveMode::Select => {
                for id_str in &request.select_headers {
                    if let Ok(id) = WikidataId::try_from(id_str.as_str())
                        && id.is_property()
                    {
                        ids_to_resolve.insert(id);
                    }
                }
            }
        }

        // Collect data IDs (item IDs in values)
        match request.resolve_data {
            ResolveMode::None => {}
            ResolveMode::All => {
                for entity in entities.values() {
                    for referenced_id in entity.get_referenced_ids() {
                        ids_to_resolve.insert(referenced_id);
                    }
                }
            }
            ResolveMode::Select => {
                for id_str in &request.select_data {
                    if let Ok(id) = WikidataId::try_from(id_str.as_str())
                        && id.is_item()
                    {
                        ids_to_resolve.insert(id);
                    }
                }
            }
        }

        if !ids_to_resolve.is_empty() {
            // Fetch labels using WikidataClient
            let query = GetEntitiesQuery::builder()
                .ids(ids_to_resolve.into_iter().collect::<Vec<WikidataId>>())
                .languages(vec!["en".to_string()])
                .props(vec!["labels".to_string()])
                .build()?;

            let label_entities = client.get_entities(&query).await?;

            // Build a label map: ID string -> WikidataId with label
            let label_map: BTreeMap<String, WikidataId> = label_entities
                .into_iter()
                .map(|(id_str, entity)| {
                    let mut wikidata_id = entity.id.clone();
                    wikidata_id.label = entity.label;
                    (id_str.to_string(), wikidata_id)
                })
                .collect();

            // Apply resolved labels to all entities
            for entity in entities.values_mut() {
                entity.apply_resolved_labels_from_map(&label_map);
            }
        }
    }

    // Resolve Commons media filenames to pageids if requested
    if request.resolve_data != ResolveMode::None {
        // Define the target properties for Commons media resolution
        const COMMONS_MEDIA_PROPERTIES: &[&str] = &[
            "P18",    // image
            "P242",   // locator map image
            "P1621",  // detail map
            "P1766",  // place name sign
            "P3451",  // nighttime view
            "P8517",  // view
            "P5775",  // image of interior
            "P1801",  // plaque image
            "P11702", // information sign
        ];

        // Determine which properties should have their Commons media resolved
        let properties_to_resolve: Vec<WikidataId> = match request.resolve_data {
            ResolveMode::None => Vec::new(), // Won't reach here due to outer if
            ResolveMode::All => {
                // Resolve all Commons media properties
                COMMONS_MEDIA_PROPERTIES
                    .iter()
                    .filter_map(|prop| WikidataId::try_from(*prop).ok())
                    .collect()
            }
            ResolveMode::Select => {
                // Only resolve selected Commons media properties
                COMMONS_MEDIA_PROPERTIES
                    .iter()
                    .filter_map(|prop| {
                        let prop_str = prop.to_string();
                        if request.select_data.contains(&prop_str) {
                            WikidataId::try_from(*prop).ok()
                        } else {
                            None
                        }
                    })
                    .collect()
            }
        };

        if !properties_to_resolve.is_empty() {
            // Collect all unique filenames from target properties
            let mut filenames_to_resolve: Vec<String> = Vec::new();
            for entity in entities.values() {
                for property_id in &properties_to_resolve {
                    if let Some(property) = entity.properties.get(property_id) {
                        for stmt in &property.statements {
                            if let EntityValue::CommonsMedia(filename) = &stmt.value
                                && !filenames_to_resolve.contains(filename)
                            {
                                filenames_to_resolve.push(filename.clone());
                            }
                        }
                    }
                }
            }

            if !filenames_to_resolve.is_empty() {
                tracing::info!(
                    "Resolving {} Commons media filenames to pageids",
                    filenames_to_resolve.len()
                );

                // Fetch imageinfo for all filenames
                let commons_client = wikicommons::WikicommonsClient::with_cache(cache.clone());
                let imageinfo_results = commons_client
                    .get_image_info(&[], &filenames_to_resolve)
                    .await?;

                // Build a map: filename -> pageid
                let mut filename_to_pageid: HashMap<String, u64> = HashMap::new();
                for page_info in imageinfo_results.values() {
                    // Extract filename from title (remove "File:" prefix if present)
                    let filename = if page_info.title.starts_with("File:") {
                        page_info.title.strip_prefix("File:").unwrap().to_string()
                    } else {
                        page_info.title.clone()
                    };
                    filename_to_pageid.insert(filename, page_info.pageid);
                }

                // Transform EntityValue::CommonsMedia to EntityValue::CommonsMediaPageId
                for entity in entities.values_mut() {
                    for property_id in &properties_to_resolve {
                        if let Some(property) = entity.properties.get_mut(property_id) {
                            for stmt in &mut property.statements {
                                if let EntityValue::CommonsMedia(filename) = &stmt.value {
                                    if let Some(&pageid) = filename_to_pageid.get(filename) {
                                        // Successfully resolved - transform to CommonsMediaPageId
                                        // Store filename only if keep_filename is true
                                        stmt.value =
                                            EntityValue::CommonsMediaPageId(CommonsMediaPageId {
                                                pageid,
                                                filename: if request.keep_filename {
                                                    Some(filename.clone())
                                                } else {
                                                    None
                                                },
                                            });
                                    } else {
                                        // Failed to resolve - emit warning
                                        tracing::warn!(
                                            "Failed to resolve Commons media filename '{}' for property {} in entity {}",
                                            filename,
                                            property_id,
                                            entity.id
                                        );
                                        // Leave as CommonsMedia - will display as empty in CSV
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok((entities, osm_elements, commons_data, commons_imageinfo))
}
