use crate::{
    Cacheable, DiskCacheMiddleware, Error,
    erdfa::{self, ErdfaRequest, ErdfaUrl, Seal},
    overpass::{Element, OSMId},
    request::{
        OverpassQueryRequest, WikicommonsCategorymembersRequest, WikidataGetEntitiesRequest,
    },
    wikidata::{EntityCollection, JsonConfig, serialize_entities_to_json},
    zkperf::{self, ZkperfRequest, ZkperfWitness},
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

/// Batch file format parsed from TOML
#[derive(Debug, Deserialize)]
pub struct BatchFile {
    #[serde(default)]
    pub wikidata: Vec<WikidataGetEntitiesRequest>,

    #[serde(default)]
    pub wikicommons: Vec<WikicommonsCategorymembersRequest>,

    #[serde(default)]
    pub overpass: Vec<OverpassQueryRequest>,

    #[serde(default)]
    pub erdfa: Vec<ErdfaRequest>,

    #[serde(default)]
    pub zkperf: Vec<ZkperfRequest>,
}

impl BatchFile {
    pub fn from_file(path: &str) -> Result<Self, Error> {
        let contents = fs::read_to_string(path)
            .map_err(|e| Error::InvalidInput(format!("Failed to read batch file: {}", e)))?;
        let batch: BatchFile = toml::from_str(&contents)
            .map_err(|e| Error::InvalidInput(format!("Failed to parse batch file: {}", e)))?;
        Ok(batch)
    }
}

/// Accumulator for results across all batch sections
pub struct BatchAccumulator {
    /// Wikidata entities keyed by QID string
    pub wikidata_entities: EntityCollection,
    /// OSM elements keyed by OSMId
    pub overpass_elements: BTreeMap<OSMId, Element>,
    /// Wikicommons category results (not enriched)
    pub wikicommons_data: BTreeMap<String, crate::wikicommons::CategoryResult>,
    /// Wikicommons imageinfo keyed by pageid string
    pub wikicommons_imageinfo: std::collections::HashMap<String, crate::wikicommons::PageImageInfo>,
    /// JSON configuration for wikidata output (from last processed request)
    pub wikidata_json_config: Option<JsonConfig>,
    /// eRDFa seals from fetched URLs
    pub erdfa_seals: Vec<Seal>,
    /// zkperf verification witnesses
    pub zkperf_witnesses: Vec<ZkperfWitness>,
}

impl Default for BatchAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

impl BatchAccumulator {
    pub fn new() -> Self {
        Self {
            wikidata_entities: EntityCollection(BTreeMap::new()),
            overpass_elements: BTreeMap::new(),
            wikicommons_data: BTreeMap::new(),
            wikicommons_imageinfo: std::collections::HashMap::new(),
            wikidata_json_config: None,
            erdfa_seals: Vec::new(),
            zkperf_witnesses: Vec::new(),
        }
    }

    /// Merge wikidata entities into accumulator
    /// Also stores the config for later use when writing output
    pub fn merge_wikidata(&mut self, entities: EntityCollection, config: JsonConfig) {
        self.wikidata_entities.extend(entities.0);
        self.wikidata_json_config = Some(config);
    }

    /// Merge OSM elements into accumulator
    pub fn merge_overpass(&mut self, elements: BTreeMap<OSMId, Element>) {
        self.overpass_elements.extend(elements);
    }

    /// Merge wikicommons data into accumulator
    pub fn merge_wikicommons(
        &mut self,
        data: BTreeMap<String, crate::wikicommons::CategoryResult>,
    ) {
        self.wikicommons_data.extend(data);
    }

    /// Merge wikicommons imageinfo into accumulator
    pub fn merge_wikicommons_imageinfo(
        &mut self,
        imageinfo: std::collections::HashMap<String, crate::wikicommons::PageImageInfo>,
    ) {
        self.wikicommons_imageinfo.extend(imageinfo);
    }

    /// Write all accumulated data to output directory
    pub fn write_all(&self, output_dir: &str) -> Result<(), Error> {
        let dir_path = Path::new(output_dir);
        fs::create_dir_all(dir_path)?;

        // Write wikidata entities if any
        if !self.wikidata_entities.is_empty() {
            let file_path = dir_path.join("wikidata-wbgetentities.json");
            let mut file = fs::File::create(&file_path)?;
            // Use stored config from last processed wikidata request, or default if none
            let config = self.wikidata_json_config.clone().unwrap_or_default();
            serialize_entities_to_json(&self.wikidata_entities, &mut file, config)?;
            tracing::info!(
                "Wrote {} wikidata entities to {:?}",
                self.wikidata_entities.len(),
                file_path
            );
        }

        // Write overpass elements if any
        if !self.overpass_elements.is_empty() {
            let file_path = dir_path.join("overpass.json");
            // Convert OSMId keys to strings for JSON serialization
            let elements_with_string_keys: BTreeMap<String, &Element> = self
                .overpass_elements
                .iter()
                .map(|(id, element)| (id.cache_key(), element))
                .collect();
            let json = serde_json::to_string_pretty(&elements_with_string_keys)?;
            fs::write(&file_path, json)?;
            tracing::info!(
                "Wrote {} OSM elements to {:?}",
                self.overpass_elements.len(),
                file_path
            );
        }

        // Write wikicommons data if any
        if !self.wikicommons_data.is_empty() {
            let file_path = dir_path.join("wikicommons-categorymembers.json");
            let json = serde_json::to_string_pretty(&self.wikicommons_data)?;
            fs::write(&file_path, json)?;
            tracing::info!(
                "Wrote {} wikicommons entries to {:?}",
                self.wikicommons_data.len(),
                file_path
            );
        }

        // Write wikicommons imageinfo if any
        if !self.wikicommons_imageinfo.is_empty() {
            let file_path = dir_path.join("wikicommons-imageinfo.json");
            let json = serde_json::to_string_pretty(&self.wikicommons_imageinfo)?;
            fs::write(&file_path, json)?;
            tracing::info!(
                "Wrote {} wikicommons imageinfo entries to {:?}",
                self.wikicommons_imageinfo.len(),
                file_path
            );
        }

        // Write erdfa seals if any
        if !self.erdfa_seals.is_empty() {
            let file_path = dir_path.join("seal_manifest.jsonl");
            let lines: Vec<String> = self.erdfa_seals.iter()
                .map(|s| serde_json::to_string(s).unwrap_or_default())
                .collect();
            fs::write(&file_path, lines.join("\n"))?;
            tracing::info!(
                "Wrote {} erdfa seals to {:?}",
                self.erdfa_seals.len(),
                file_path
            );
        }

        // Write zkperf witnesses if any
        if !self.zkperf_witnesses.is_empty() {
            let file_path = dir_path.join("zkperf_witnesses.jsonl");
            let lines: Vec<String> = self.zkperf_witnesses.iter()
                .map(|w| serde_json::to_string(w).unwrap_or_default())
                .collect();
            fs::write(&file_path, lines.join("\n"))?;
            tracing::info!(
                "Wrote {} zkperf witnesses to {:?}",
                self.zkperf_witnesses.len(),
                file_path
            );
        }

        Ok(())
    }
}

/// Process wikidata batch section
/// Returns (wikidata entities, OSM elements from traversal, Commons categorymembers from traversal, Commons imageinfo from traversal)
pub async fn process_wikidata_batch(
    request: &WikidataGetEntitiesRequest,
    cache: Arc<DiskCacheMiddleware>,
) -> Result<
    (
        EntityCollection,
        BTreeMap<OSMId, Element>,
        BTreeMap<String, crate::wikicommons::CategoryResult>,
        std::collections::HashMap<String, crate::wikicommons::PageImageInfo>,
    ),
    Error,
> {
    // Resolve QIDs from either inline list or file for logging
    let qids = request.resolve_qids()?;

    if !qids.is_empty() {
        tracing::info!("Processing {} wikidata entities...", qids.len());
    }

    // Call shared processing function - this is the SAME code path as CLI
    let result = crate::wikidata::fetch_and_process_wikidata(request, cache).await?;

    tracing::info!("  Completed wikidata processing");
    Ok(result)
}

/// Process overpass batch section
pub async fn process_overpass_batch(
    request: &OverpassQueryRequest,
    cache: Arc<DiskCacheMiddleware>,
) -> Result<BTreeMap<OSMId, Element>, Error> {
    // Resolve OSM IDs from either inline lists or file for logging
    let (nodes, ways, relations) = request.resolve_ids()?;

    let total_count = nodes.len() + ways.len() + relations.len();
    if total_count > 0 {
        tracing::info!("Processing {} OSM elements...", total_count);
    }

    // Call shared processing function - this is the SAME code path as CLI
    let result = crate::overpass::fetch_and_process_overpass(request, cache).await?;

    tracing::info!("  Completed OSM processing");
    Ok(result)
}

/// Process wikicommons batch section
/// Returns (category results, imageinfo)
pub async fn process_wikicommons_batch(
    request: &WikicommonsCategorymembersRequest,
    cache: Arc<DiskCacheMiddleware>,
    output_dir: &str,
) -> Result<
    (
        BTreeMap<String, crate::wikicommons::CategoryResult>,
        std::collections::HashMap<String, crate::wikicommons::PageImageInfo>,
    ),
    Error,
> {
    // Resolve categories from either inline list or file for logging
    let categories = request.resolve_categories()?;
    if !categories.is_empty() {
        tracing::info!("Processing {} wikicommons categories...", categories.len());
    }

    // Convert output_dir to Path for processing function
    let output_path = Path::new(output_dir);

    // Call shared processing function - this is the SAME code path as CLI
    let (data, imageinfo, _download_results) =
        crate::wikicommons::fetch_and_process_categorymembers(request, cache, Some(output_path))
            .await?;

    tracing::info!("  Completed wikicommons processing");
    Ok((data, imageinfo))
}

/// Main batch processing function
pub async fn process_batch_file(
    batch_file_path: &str,
    output_dir: &str,
    cache: Arc<DiskCacheMiddleware>,
) -> Result<(), Error> {
    tracing::info!("Starting batch processing from: {}", batch_file_path);

    let batch = BatchFile::from_file(batch_file_path)?;
    let mut accumulator = BatchAccumulator::new();

    // Process wikidata requests
    for wikidata_request in &batch.wikidata {
        let (entities, osm_elements, commons_data, commons_imageinfo) =
            process_wikidata_batch(wikidata_request, cache.clone()).await?;

        // Convert preserve_qualifiers_for from Vec<String> to Vec<WikidataId>
        let preserve_qualifiers: Vec<crate::wikidata::WikidataId> = wikidata_request
            .preserve_qualifiers_for
            .iter()
            .map(|s| crate::wikidata::WikidataId::try_from(s.as_str()))
            .collect::<Result<Vec<_>, _>>()?;

        let json_config = JsonConfig {
            only_values: wikidata_request.only_values,
            preserve_qualifiers_for: preserve_qualifiers,
        };

        accumulator.merge_wikidata(entities, json_config);
        accumulator.merge_overpass(osm_elements);
        accumulator.merge_wikicommons(commons_data);
        accumulator.merge_wikicommons_imageinfo(commons_imageinfo);
    }

    // Process overpass requests
    for overpass_request in &batch.overpass {
        let elements = process_overpass_batch(overpass_request, cache.clone()).await?;
        accumulator.merge_overpass(elements);
    }

    // Process wikicommons requests
    for wikicommons_request in &batch.wikicommons {
        let (data, imageinfo) =
            process_wikicommons_batch(wikicommons_request, cache.clone(), output_dir).await?;
        accumulator.merge_wikicommons(data);
        accumulator.merge_wikicommons_imageinfo(imageinfo);
    }

    // Process erdfa requests (fetch + seal URLs)
    let http_client = crate::create_http_client();
    for erdfa_request in &batch.erdfa {
        let mut urls = erdfa_request.urls.clone();
        if let Some(ref file) = erdfa_request.urls_from_file {
            urls.extend(erdfa::load_urls_from_file(file)?);
        }
        let out_path = Path::new(output_dir);
        for url in &urls {
            match erdfa::fetch_and_seal(&http_client, url, out_path).await {
                Ok(seal) => accumulator.erdfa_seals.push(seal),
                Err(e) => tracing::warn!("[erdfa] skip {}: {}", url.key(), e),
            }
        }
    }

    // Process zkperf requests (verify seals)
    for zkperf_request in &batch.zkperf {
        let manifest_dir = zkperf_request.manifest_dir.as_deref().unwrap_or(output_dir);
        let manifest_path = Path::new(manifest_dir).join("seal_manifest.jsonl");
        if manifest_path.exists() {
            let seals = zkperf::load_manifest(&manifest_path)?;
            let raw_dir = Path::new(manifest_dir).join("raw");
            for seal in &seals {
                match zkperf::verify_seal(seal, &raw_dir) {
                    Ok(w) => accumulator.zkperf_witnesses.push(w),
                    Err(e) => tracing::warn!("[zkperf] skip {}: {}", seal.key, e),
                }
            }
        }
    }

    // Write all accumulated results
    tracing::info!("Writing batch results to: {}", output_dir);
    accumulator.write_all(output_dir)?;

    tracing::info!("Batch processing completed successfully");
    Ok(())
}
