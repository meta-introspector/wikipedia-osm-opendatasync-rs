use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("HTTP middleware error: {0}")]
    HttpMiddleware(#[from] reqwest_middleware::Error),

    #[error("JSON parsing failed: {0}")]
    Json(#[from] serde_json::Error),

    #[error("CSV serialization failed: {0}")]
    Csv(#[from] csv::Error),

    #[error("Invalid Wikidata ID format: {0}")]
    InvalidId(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Wikidata API error [{code}]: {info}")]
    WikidataApi { code: String, info: String },

    #[error("Overpass API error [status {status}]: {message}")]
    OverpassApi { status: u16, message: String },

    #[error("Invalid bounding box: {0}")]
    InvalidBoundingBox(String),

    #[error("Invalid OSM ID: {0}")]
    InvalidOSMId(String),

    #[error("GetEntitiesQuery builder error: {0}")]
    GetEntitiesQueryBuilder(#[from] crate::wikidata::GetEntitiesQueryBuilderError),

    #[error("GetClaimsQuery builder error: {0}")]
    GetClaimsQueryBuilder(#[from] crate::wikidata::GetClaimsQueryBuilderError),

    #[error("Request builder error: {0}")]
    RequestBuilder(#[from] crate::overpass::RequestBuilderError),

    #[error("Builder error: {0}")]
    Builder(String),
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error::Builder(s)
    }
}
