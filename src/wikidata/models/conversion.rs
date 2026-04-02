use super::{Entity, EntityValue, Property, Qualifier, Reference, Statement, StatementRank};
use crate::wikidata::models::response as api;
use crate::{Error, wikidata::WikidataId};

impl TryFrom<&api::GetEntitiesResponse> for Vec<Entity> {
    type Error = Error;

    fn try_from(response: &api::GetEntitiesResponse) -> Result<Self, Self::Error> {
        let mut entities = Vec::new();

        if let Some(ref entities_map) = response.entities {
            for api_entity in entities_map.values() {
                let entity = Entity::try_from(api_entity)?;
                entities.push(entity);
            }
        }

        Ok(entities)
    }
}

impl TryFrom<&api::Entity> for Entity {
    type Error = Error;

    fn try_from(api_entity: &api::Entity) -> Result<Self, Self::Error> {
        // Parse the entity ID
        let id_str = api_entity
            .id
            .as_ref()
            .ok_or_else(|| Error::InvalidInput("Entity missing ID".to_string()))?;
        let id = WikidataId::try_from(id_str.as_str())?;

        // Get entity type
        let entity_type = api_entity
            .r#type
            .as_ref()
            .unwrap_or(&"item".to_string())
            .clone();

        let mut entity = Entity::new(id, entity_type);

        // Extract label (default to English, but could be configurable)
        if let Some(ref labels) = api_entity.labels {
            entity.label = labels.get("en").map(|l| l.value.clone());
        }

        // Extract description (default to English)
        if let Some(ref descriptions) = api_entity.descriptions {
            entity.description = descriptions.get("en").map(|d| d.value.clone());
        }

        // Extract aliases (default to English)
        if let Some(ref aliases) = api_entity.aliases
            && let Some(en_aliases) = aliases.get("en")
        {
            entity.aliases = en_aliases.iter().map(|a| a.value.clone()).collect();
        }

        // Extract sitelinks
        if let Some(ref sitelinks) = api_entity.sitelinks {
            entity.sitelinks = sitelinks
                .iter()
                .map(|(site, sitelink)| (site.clone(), sitelink.title.clone()))
                .collect();
        }

        // Convert claims to properties
        if let Some(ref claims) = api_entity.claims {
            for (property_str, api_claims) in claims {
                let property_id = WikidataId::try_from(property_str.as_str())?;
                let mut property = Property::new();

                for api_claim in api_claims {
                    let statement = Statement::try_from(api_claim)?;
                    property.add_statement(statement);
                }

                entity.properties.insert(property_id, property);
            }
        }

        Ok(entity)
    }
}

impl TryFrom<&api::Claim> for Statement {
    type Error = Error;

    fn try_from(api_claim: &api::Claim) -> Result<Self, Self::Error> {
        // Extract the main value
        let value = if let Some(ref mainsnak) = api_claim.mainsnak {
            EntityValue::try_from(mainsnak)?
        } else {
            EntityValue::NoValue
        };

        let mut statement = Statement::new(value);
        statement.id = api_claim.id.clone();

        // Convert rank
        if let Some(ref rank_str) = api_claim.rank {
            statement.rank = match rank_str.as_str() {
                "preferred" => StatementRank::Preferred,
                "normal" => StatementRank::Normal,
                "deprecated" => StatementRank::Deprecated,
                _ => StatementRank::Normal,
            };
        }

        // Convert qualifiers
        if let Some(ref qualifiers) = api_claim.qualifiers {
            for (property_str, snaks) in qualifiers {
                let property_id = WikidataId::try_from(property_str.as_str())?;
                for snak in snaks {
                    let qualifier_value = EntityValue::try_from(snak)?;
                    statement.qualifiers.push(Qualifier {
                        property: property_id.clone(),
                        value: qualifier_value,
                    });
                }
            }
        }

        // Convert references
        if let Some(ref references) = api_claim.references {
            for api_ref in references {
                let mut reference = Reference { snaks: Vec::new() };

                if let Some(ref snaks) = api_ref.snaks {
                    for (property_str, ref_snaks) in snaks {
                        let property_id = WikidataId::try_from(property_str.as_str())?;
                        for snak in ref_snaks {
                            let ref_value = EntityValue::try_from(snak)?;
                            reference.snaks.push(Qualifier {
                                property: property_id.clone(),
                                value: ref_value,
                            });
                        }
                    }
                }

                statement.references.push(reference);
            }
        }

        Ok(statement)
    }
}

impl TryFrom<&api::Snak> for EntityValue {
    type Error = Error;

    fn try_from(snak: &api::Snak) -> Result<Self, Self::Error> {
        match snak.snaktype.as_deref() {
            Some("novalue") => return Ok(EntityValue::NoValue),
            Some("somevalue") => return Ok(EntityValue::SomeValue),
            Some("value") => {
                // Continue to process the actual value
            }
            _ => return Ok(EntityValue::NoValue),
        }

        if let Some(ref datavalue) = snak.datavalue
            && let Some(ref value) = datavalue.value
        {
            // Check if this is a commonsMedia type
            if snak.datatype.as_deref() == Some("commonsMedia")
                && let api::DataValueType::String(filename) = value
            {
                return Ok(EntityValue::CommonsMedia(filename.clone()));
            }

            return EntityValue::try_from(value);
        }

        Ok(EntityValue::NoValue)
    }
}

impl TryFrom<&api::DataValueType> for EntityValue {
    type Error = Error;

    fn try_from(api_value: &api::DataValueType) -> Result<Self, Self::Error> {
        let value = match api_value {
            api::DataValueType::WikibaseItem(id) => EntityValue::WikidataItem(id.clone()),
            api::DataValueType::String(s) => EntityValue::String(s.clone()),
            api::DataValueType::MonolingualText { language, text } => {
                EntityValue::MonolingualText {
                    language: language.clone(),
                    text: text.clone(),
                }
            }
            api::DataValueType::GlobeCoordinate {
                latitude,
                longitude,
                precision,
                ..
            } => EntityValue::GlobeCoordinate {
                latitude: *latitude,
                longitude: *longitude,
                precision: *precision,
            },
            api::DataValueType::Time {
                time, precision, ..
            } => EntityValue::Time {
                time: time.clone(),
                precision: *precision,
            },
            api::DataValueType::Quantity { amount, unit } => {
                let unit_id = if let Some(unit_str) = unit {
                    // Try to parse unit as WikidataId (units are often Wikidata items)
                    WikidataId::try_from(unit_str.as_str()).ok()
                } else {
                    None
                };
                EntityValue::Quantity {
                    amount: amount.clone(),
                    unit: unit_id,
                }
            }
            api::DataValueType::Raw(json_value) => {
                // Try to extract meaningful values from raw JSON
                if let Some(string_val) = json_value.as_str() {
                    EntityValue::String(string_val.to_string())
                } else {
                    EntityValue::Unknown(json_value.to_string())
                }
            }
        };

        Ok(value)
    }
}
