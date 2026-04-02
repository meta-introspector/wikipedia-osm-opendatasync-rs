//! CSV export for raw Wikidata API responses.
//!
//! This module contains `serialize_claims_to_csv()` which is used by the `wbgetclaims` command.
//! It works directly with raw `GetClaimsResponse` from the Wikidata API.
//!
//! **For entity serialization**, use the modern functions in `models::serialization` that work with `EntityCollection`:
//! - `serialize_entities_to_csv` - basic CSV
//! - `serialize_entities_to_csv_denormalized` - denormalized CSV
//! - `serialize_entities_to_json` - JSON output

use crate::Error;
use serde::Serialize;
use std::io::Write;

#[derive(Debug, Serialize)]
pub struct ClaimCsvRecord {
    pub entity_id: String,
    pub property: String,
    pub claim_id: Option<String>,
    pub value_type: Option<String>,
    pub value: Option<String>,
    pub rank: Option<String>,
}

pub fn serialize_claims_to_csv<W: Write>(
    entity_id: &str,
    claims: &crate::wikidata::GetClaimsResponse,
    writer: W,
) -> Result<(), Error> {
    let mut csv_writer = csv::Writer::from_writer(writer);

    if let Some(ref claims_map) = claims.claims {
        for (property, property_claims) in claims_map {
            for claim in property_claims {
                let record = ClaimCsvRecord {
                    entity_id: entity_id.to_string(),
                    property: property.clone(),
                    claim_id: claim.id.clone(),
                    value_type: claim
                        .mainsnak
                        .as_ref()
                        .and_then(|snak| snak.datatype.clone()),
                    value: claim
                        .mainsnak
                        .as_ref()
                        .and_then(|snak| snak.datavalue.as_ref())
                        .and_then(|datavalue| datavalue.value.as_ref())
                        .map(|v| v.to_string()),
                    rank: claim.rank.clone(),
                };
                csv_writer.serialize(record)?;
            }
        }
    }

    csv_writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wikidata::models::response::{Claim, DataValue, DataValueType, Snak};
    use std::collections::HashMap;

    fn create_test_claim(value: DataValueType) -> Claim {
        Claim {
            id: Some("test-claim-id".to_string()),
            mainsnak: Some(Snak {
                snaktype: Some("value".to_string()),
                property: Some("P31".to_string()),
                datatype: Some("wikibase-item".to_string()),
                datavalue: Some(DataValue {
                    value: Some(value),
                    r#type: Some("wikibase-entityid".to_string()),
                }),
            }),
            rank: Some("normal".to_string()),
            qualifiers: None,
            references: None,
        }
    }

    #[test]
    fn test_serialize_claims_to_csv() {
        let claim = create_test_claim(DataValueType::String("test value".to_string()));

        let mut claims_map = HashMap::new();
        claims_map.insert("P31".to_string(), vec![claim]);

        let claims_response = crate::wikidata::GetClaimsResponse {
            claims: Some(claims_map),
            error: None,
        };

        let mut buffer = Vec::new();
        serialize_claims_to_csv("Q42", &claims_response, &mut buffer).unwrap();

        let output = String::from_utf8(buffer).unwrap();
        assert!(output.contains("Q42"));
        assert!(output.contains("P31"));
    }
}
