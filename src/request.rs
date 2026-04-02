use crate::Error;
use crate::overpass::BoundingBox;
use crate::types::{CommonsTraverseDepth, OutputFormat, ResolveMode};
use serde::{Deserialize, Serialize};
use std::fs;

/// Type alias for resolved OSM IDs (nodes, ways, relations)
pub type ResolvedOsmIds = (Vec<u64>, Vec<u64>, Vec<u64>);

// Default constants shared between CLI and batch processing
// See: https://github.com/clap-rs/clap/discussions/6148
pub const DEFAULT_KEEP_QIDS: bool = true;
pub const DEFAULT_ONLY_VALUES: bool = true;
pub const DEFAULT_TRAVERSE_OSM: bool = true;
pub const DEFAULT_TRAVERSE_COMMONS: bool = true;
pub const DEFAULT_TIMEOUT: u8 = 25;

/// Request for Wikidata wbgetentities action
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct WikidataGetEntitiesRequest {
    pub qids: Vec<String>,

    pub ids_from_file: Option<String>,

    pub format: OutputFormat,

    pub resolve_headers: ResolveMode,

    pub select_headers: Vec<String>,

    pub resolve_data: ResolveMode,

    pub select_data: Vec<String>,

    pub keep_qids: bool,

    pub keep_filename: bool,

    pub only_values: bool,

    pub preserve_qualifiers_for: Vec<String>,

    pub traverse_properties: Vec<String>,

    pub traverse_osm: bool,

    pub traverse_commons: bool,

    pub traverse_commons_depth: CommonsTraverseDepth,
}

impl Default for WikidataGetEntitiesRequest {
    fn default() -> Self {
        Self {
            qids: Vec::new(),
            ids_from_file: None,
            format: OutputFormat::default(),
            resolve_headers: ResolveMode::default(),
            select_headers: Vec::new(),
            resolve_data: ResolveMode::default(),
            select_data: Vec::new(),
            keep_qids: DEFAULT_KEEP_QIDS,
            keep_filename: false,
            only_values: DEFAULT_ONLY_VALUES,
            preserve_qualifiers_for: Vec::new(),
            traverse_properties: Vec::new(),
            traverse_osm: DEFAULT_TRAVERSE_OSM,
            traverse_commons: DEFAULT_TRAVERSE_COMMONS,
            traverse_commons_depth: CommonsTraverseDepth::default(),
        }
    }
}

impl WikidataGetEntitiesRequest {
    /// Resolve QIDs from either inline list or file
    pub fn resolve_qids(&self) -> Result<Vec<String>, Error> {
        if let Some(ref file_path) = self.ids_from_file {
            let contents = fs::read_to_string(file_path).map_err(|e| {
                Error::InvalidInput(format!("Failed to read IDs from file {}: {}", file_path, e))
            })?;
            let qids: Vec<String> = contents
                .lines()
                .map(|line| line.trim().to_string())
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .collect();
            Ok(qids)
        } else {
            Ok(self.qids.clone())
        }
    }
}

/// Request for Wikidata wbgetclaims action
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WikidataGetClaimsRequest {
    pub entity: String,

    #[serde(default)]
    pub property: Option<String>,

    #[serde(default)]
    pub format: OutputFormat,
}

/// Union type for all Wikidata requests
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WikidataRequest {
    Wbgetentities(WikidataGetEntitiesRequest),
    Wbgetclaims(WikidataGetClaimsRequest),
}

/// Request for Wikicommons categorymembers action
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WikicommonsCategorymembersRequest {
    #[serde(default)]
    pub categories: Vec<String>,

    #[serde(default)]
    pub ids_from_file: Option<String>,

    #[serde(default)]
    pub traverse_pageid: bool,

    #[serde(default)]
    pub recurse_subcategory_pattern: Option<String>,

    #[serde(default)]
    pub download_images: bool,
}

impl WikicommonsCategorymembersRequest {
    /// Resolve category names from either inline list or file
    pub fn resolve_categories(&self) -> Result<Vec<String>, Error> {
        if let Some(ref file_path) = self.ids_from_file {
            let contents = fs::read_to_string(file_path).map_err(|e| {
                Error::InvalidInput(format!(
                    "Failed to read categories from file {}: {}",
                    file_path, e
                ))
            })?;
            let categories: Vec<String> = contents
                .lines()
                .map(|line| line.trim().to_string())
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .collect();
            Ok(categories)
        } else {
            Ok(self.categories.clone())
        }
    }
}

/// Request for Wikicommons imageinfo action
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WikicommonsImageinfoRequest {
    #[serde(default)]
    pub pageids: Vec<u64>,

    #[serde(default)]
    pub titles: Vec<String>,

    #[serde(default)]
    pub ids_from_file: Option<String>,
}

impl WikicommonsImageinfoRequest {
    /// Resolve pageids and titles from either inline lists or file
    /// Returns (pageids, titles)
    pub fn resolve_pageids_and_titles(&self) -> Result<(Vec<u64>, Vec<String>), Error> {
        if let Some(ref file_path) = self.ids_from_file {
            let contents = fs::read_to_string(file_path).map_err(|e| {
                Error::InvalidInput(format!("Failed to read IDs from file {}: {}", file_path, e))
            })?;

            let mut pageids = Vec::new();
            let mut titles = Vec::new();

            for line in contents.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                // Try to parse as u64 (pageid), otherwise treat as title
                if let Ok(pageid) = line.parse::<u64>() {
                    pageids.push(pageid);
                } else {
                    titles.push(line.to_string());
                }
            }

            Ok((pageids, titles))
        } else {
            Ok((self.pageids.clone(), self.titles.clone()))
        }
    }
}

/// Union type for all Wikicommons requests
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WikicommonsRequest {
    Categorymembers(WikicommonsCategorymembersRequest),
    Imageinfo(WikicommonsImageinfoRequest),
}

/// Request for Overpass query action
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OverpassQueryRequest {
    #[serde(default)]
    pub bbox: Option<BoundingBox>,

    #[serde(default)]
    pub nodes: Vec<u64>,

    #[serde(default)]
    pub ways: Vec<u64>,

    #[serde(default)]
    pub relations: Vec<u64>,

    #[serde(default)]
    pub ids_from_file: Option<String>,

    #[serde(default = "default_timeout")]
    pub timeout: u8,
}

fn default_timeout() -> u8 {
    DEFAULT_TIMEOUT
}

impl OverpassQueryRequest {
    /// Resolve OSM IDs from either inline lists or file
    /// File format: "node/123", "way/456", "relation/789" (one per line)
    pub fn resolve_ids(&self) -> Result<ResolvedOsmIds, Error> {
        if let Some(ref file_path) = self.ids_from_file {
            let contents = fs::read_to_string(file_path).map_err(|e| {
                Error::InvalidInput(format!("Failed to read IDs from file {}: {}", file_path, e))
            })?;

            let mut nodes = Vec::new();
            let mut ways = Vec::new();
            let mut relations = Vec::new();

            for (line_num, line) in contents.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                let parts: Vec<&str> = line.split('/').collect();
                if parts.len() != 2 {
                    return Err(Error::InvalidInput(format!(
                        "Invalid OSM ID format at line {}: '{}' (expected 'type/id' like 'node/123')",
                        line_num + 1,
                        line
                    )));
                }

                let num = parts[1].parse::<u64>().map_err(|_| {
                    Error::InvalidInput(format!(
                        "Invalid OSM ID number at line {}: '{}'",
                        line_num + 1,
                        parts[1]
                    ))
                })?;

                match parts[0] {
                    "node" => nodes.push(num),
                    "way" => ways.push(num),
                    "relation" => relations.push(num),
                    _ => {
                        return Err(Error::InvalidInput(format!(
                            "Invalid OSM type at line {}: '{}' (expected 'node', 'way', or 'relation')",
                            line_num + 1,
                            parts[0]
                        )));
                    }
                }
            }

            Ok((nodes, ways, relations))
        } else {
            Ok((
                self.nodes.clone(),
                self.ways.clone(),
                self.relations.clone(),
            ))
        }
    }
}
