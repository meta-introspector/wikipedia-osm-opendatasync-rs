use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Output format for data serialization
#[derive(Debug, Clone, ValueEnum, Deserialize, Serialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Json,
    Csv,
}

/// Resolution mode for property IDs and data values
#[derive(Debug, Clone, ValueEnum, Deserialize, Serialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ResolveMode {
    None,
    #[default]
    All,
    Select,
}

/// Depth of traversal for Wikicommons categories
#[derive(Debug, Clone, Copy, ValueEnum, Deserialize, Serialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum CommonsTraverseDepth {
    Category, // Just category members
    #[default]
    Page, // Category members + imageinfo
}
