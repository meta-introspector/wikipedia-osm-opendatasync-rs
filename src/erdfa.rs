//! erdfa — Fetch URLs, seal with SHA-256 + orbifold coords, import as CBOR shards.
//!
//! Integrates erdfa-publish's seal + CFT decomposition into the opendatasync pipeline.

use crate::Error;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::Path;

/// Seal envelope — content-addressed wrapper with orbifold coordinates.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Seal {
    pub key: String,
    pub url: String,
    pub witness: String,
    pub dasl: String,
    pub orbifold: (u8, u8, u8),
    pub size: usize,
    pub timestamp: String,
}

impl Seal {
    pub fn from_bytes(key: &str, url: &str, data: &[u8]) -> Self {
        let h = Sha256::digest(data);
        let v = u64::from_le_bytes(h[0..8].try_into().unwrap());
        Self {
            key: key.to_string(),
            url: url.to_string(),
            witness: hex::encode(h),
            dasl: format!("0xda51{:012x}", v & 0xffffffffffff),
            orbifold: ((v % 71) as u8, (v % 59) as u8, (v % 47) as u8),
            size: data.len(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// Batch request for erdfa URL archival.
#[derive(Debug, Deserialize)]
pub struct ErdfaRequest {
    /// URLs to fetch and witness. Format: ["url1", "url2"] or [{key = "k", url = "u"}]
    #[serde(default)]
    pub urls: Vec<ErdfaUrl>,
    /// File containing URLs (one per line, KEY=URL or just URL, # comments)
    pub urls_from_file: Option<String>,
    /// Max CFT decomposition depth (0=post, 1=paragraph, 2=line)
    #[serde(default = "default_max_depth")]
    pub max_depth: u8,
}

fn default_max_depth() -> u8 {
    2
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ErdfaUrl {
    Simple(String),
    Keyed { key: String, url: String },
}

impl ErdfaUrl {
    pub fn key(&self) -> String {
        match self {
            ErdfaUrl::Simple(u) => u.split('/').last().unwrap_or("unknown")
                .chars().map(|c| if c.is_alphanumeric() || c == '.' { c } else { '_' }).collect(),
            ErdfaUrl::Keyed { key, .. } => key.clone(),
        }
    }
    pub fn url(&self) -> &str {
        match self {
            ErdfaUrl::Simple(u) => u,
            ErdfaUrl::Keyed { url, .. } => url,
        }
    }
}

/// Fetch a URL, seal it, write raw + seal to output_dir.
pub async fn fetch_and_seal(
    client: &reqwest_middleware::ClientWithMiddleware,
    erdfa_url: &ErdfaUrl,
    output_dir: &Path,
) -> Result<Seal, Error> {
    let key = erdfa_url.key();
    let url = erdfa_url.url();

    tracing::info!("[erdfa] {} ← {}", key, url);

    let resp = client.get(url).send().await
        .map_err(|e| Error::InvalidInput(format!("fetch failed: {}", e)))?;
    let bytes = resp.bytes().await
        .map_err(|e| Error::InvalidInput(format!("read failed: {}", e)))?;

    let seal = Seal::from_bytes(&key, url, &bytes);

    // Write raw content
    let raw_dir = output_dir.join("raw");
    std::fs::create_dir_all(&raw_dir)?;
    let raw_path = raw_dir.join(format!("{}.txt", key));

    // Prepend seal header
    let header = format!(
        "--- erdfa-seal ---\nURL: {}\nKey: {}\nWitness: {}\nDASL: {}\nOrbifold: ({},{},{})\nTimestamp: {}\nSize: {}\n---\n",
        seal.url, seal.key, seal.witness, seal.dasl,
        seal.orbifold.0, seal.orbifold.1, seal.orbifold.2,
        seal.timestamp, seal.size,
    );
    let mut content = header.into_bytes();
    content.extend_from_slice(&bytes);
    std::fs::write(&raw_path, &content)?;

    tracing::info!(
        "  witness={} orb=({},{},{}) size={}",
        &seal.witness[..16], seal.orbifold.0, seal.orbifold.1, seal.orbifold.2, seal.size
    );

    Ok(seal)
}

/// Load URLs from a file (KEY=URL or bare URL per line, # comments).
pub fn load_urls_from_file(path: &str) -> Result<Vec<ErdfaUrl>, Error> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| Error::InvalidInput(format!("read urls file: {}", e)))?;
    let mut urls = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        if let Some(idx) = line.find("=http") {
            let key = line[..idx].to_string();
            let url = line[idx + 1..].to_string();
            urls.push(ErdfaUrl::Keyed { key, url });
        } else {
            urls.push(ErdfaUrl::Simple(line.to_string()));
        }
    }
    Ok(urls)
}
