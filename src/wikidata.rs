pub mod api;
pub mod csv_export;
pub mod id;
pub mod models;
pub mod processing;

pub use api::{
    GetClaimsQuery, GetClaimsQueryBuilderError, GetEntitiesQuery, GetEntitiesQueryBuilderError,
    WikidataClient,
};
pub use id::WikidataId;
pub use models::response::{GetClaimsResponse, GetEntitiesResponse};
pub use models::{
    CommonsMediaPageId, CsvConfig, CsvDenormalizedConfig, Entity, EntityCollection, EntityValue,
    JsonConfig, Property, Statement, StatementRank, serialize_entities_to_csv,
    serialize_entities_to_csv_denormalized, serialize_entities_to_json,
};
// Legacy CSV export function for backward compatibility with raw API responses
pub use csv_export::serialize_claims_to_csv;
// Core processing function shared by CLI and batch modes
pub use processing::fetch_and_process_wikidata;
