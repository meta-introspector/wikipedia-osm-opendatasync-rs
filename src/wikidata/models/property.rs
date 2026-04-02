use super::value::{EntityValue, StatementRank};
use crate::wikidata::WikidataId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Represents a property on a Wikidata entity with all its statements/claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Property {
    pub statements: Vec<Statement>,
}

impl Default for Property {
    fn default() -> Self {
        Self::new()
    }
}

impl Property {
    pub fn new() -> Self {
        Self {
            statements: Vec::new(),
        }
    }

    pub fn add_statement(&mut self, statement: Statement) {
        self.statements.push(statement);
    }

    /// Get the primary statement (first preferred, or first normal if no preferred)
    pub fn get_primary_statement(&self) -> Option<&Statement> {
        // First try to find a preferred statement
        for statement in &self.statements {
            if matches!(statement.rank, StatementRank::Preferred) {
                return Some(statement);
            }
        }

        // Fall back to first normal statement
        for statement in &self.statements {
            if matches!(statement.rank, StatementRank::Normal) {
                return Some(statement);
            }
        }

        // Fall back to first statement if no preferred or normal
        self.statements.first()
    }

    /// Collect all WikidataIds referenced in this property's values
    pub fn collect_referenced_ids(&self, ids: &mut BTreeSet<WikidataId>) {
        for statement in &self.statements {
            statement.collect_referenced_ids(ids);
        }
    }

    /// Apply resolved labels from a label map to this property's values
    pub fn apply_resolved_labels_from_map(
        &mut self,
        label_map: &std::collections::BTreeMap<String, crate::wikidata::WikidataId>,
    ) {
        for statement in &mut self.statements {
            statement.apply_resolved_labels_from_map(label_map);
        }
    }
}

/// Represents a single statement/claim within a property
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Statement {
    pub id: Option<String>,
    pub value: EntityValue,
    pub rank: StatementRank,
    pub qualifiers: Vec<Qualifier>,
    pub references: Vec<Reference>,
}

impl Statement {
    pub fn new(value: EntityValue) -> Self {
        Self {
            id: None,
            value,
            rank: StatementRank::Normal,
            qualifiers: Vec::new(),
            references: Vec::new(),
        }
    }

    /// Collect all WikidataIds referenced in this statement
    pub fn collect_referenced_ids(&self, ids: &mut BTreeSet<WikidataId>) {
        self.value.collect_referenced_ids(ids);

        for qualifier in &self.qualifiers {
            qualifier.collect_referenced_ids(ids);
        }

        for reference in &self.references {
            reference.collect_referenced_ids(ids);
        }
    }

    /// Apply resolved labels from a label map to this statement's values
    pub fn apply_resolved_labels_from_map(
        &mut self,
        label_map: &std::collections::BTreeMap<String, crate::wikidata::WikidataId>,
    ) {
        self.value.apply_resolved_labels_from_map(label_map);

        for qualifier in &mut self.qualifiers {
            qualifier.apply_resolved_labels_from_map(label_map);
        }

        for reference in &mut self.references {
            reference.apply_resolved_labels_from_map(label_map);
        }
    }
}

/// Represents a qualifier on a statement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Qualifier {
    pub property: WikidataId,
    pub value: EntityValue,
}

impl Qualifier {
    pub fn collect_referenced_ids(&self, ids: &mut BTreeSet<WikidataId>) {
        ids.insert(self.property.clone());
        self.value.collect_referenced_ids(ids);
    }

    pub fn apply_resolved_labels_from_map(
        &mut self,
        label_map: &std::collections::BTreeMap<String, crate::wikidata::WikidataId>,
    ) {
        let id_str = self.property.to_string();
        if let Some(resolved_id) = label_map.get(&id_str) {
            self.property = resolved_id.clone();
        }
        self.value.apply_resolved_labels_from_map(label_map);
    }
}

/// Represents a reference for a statement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    pub snaks: Vec<Qualifier>, // References use the same structure as qualifiers
}

impl Reference {
    pub fn collect_referenced_ids(&self, ids: &mut BTreeSet<WikidataId>) {
        for snak in &self.snaks {
            snak.collect_referenced_ids(ids);
        }
    }

    pub fn apply_resolved_labels_from_map(
        &mut self,
        label_map: &std::collections::BTreeMap<String, crate::wikidata::WikidataId>,
    ) {
        for snak in &mut self.snaks {
            snak.apply_resolved_labels_from_map(label_map);
        }
    }
}
