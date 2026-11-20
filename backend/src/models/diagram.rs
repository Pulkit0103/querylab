//! ER diagram models: the schema graph (tables, keys and relationships with
//! inferred cardinality) and saved node positions.

use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    errors::AppError,
    models::schema::validate_identifier,
    utils::validation::{Validate, Validator},
};

/// Tables shown when a schema is opened without a selection. Larger schemas
/// start with the most connected tables and say so (`truncated`).
pub const DEFAULT_GRAPH_LIMIT: usize = 150;
/// Upper bound on tables in one graph (selection or limit).
pub const MAX_GRAPH_TABLES: usize = 500;
/// Upper bound on saved positions per diagram.
pub const MAX_LAYOUT_NODES: usize = 2000;
/// Coordinates are canvas pixels; anything beyond this is a client bug.
pub const MAX_COORDINATE: f64 = 10_000_000.0;

fn default_schema() -> String {
    "public".into()
}

#[derive(Debug, Deserialize)]
pub struct GraphQuery {
    #[serde(default = "default_schema")]
    pub schema: String,
    /// Comma-separated table names. All tables of the schema when absent.
    pub tables: Option<String>,
    /// Include views and materialized views (they have no keys).
    #[serde(default)]
    pub include_views: bool,
    /// How many tables to show when no selection is given.
    pub limit: Option<usize>,
}

impl GraphQuery {
    /// The selected table names, or `None` for the whole schema.
    pub fn table_list(&self) -> Option<Vec<String>> {
        let tables = self.tables.as_deref()?;
        let mut out: Vec<String> = Vec::new();
        for name in tables.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            if !out.iter().any(|t| t == name) {
                out.push(name.to_string());
            }
        }
        Some(out)
    }

    pub fn effective_limit(&self) -> usize {
        self.limit.unwrap_or(DEFAULT_GRAPH_LIMIT)
    }
}

impl Validate for GraphQuery {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        validate_identifier(&mut v, "schema", &self.schema);
        if let Some(tables) = self.table_list() {
            v.check(
                !tables.is_empty() && tables.len() <= MAX_GRAPH_TABLES,
                "tables",
                &format!("must name between 1 and {MAX_GRAPH_TABLES} tables"),
            );
            for table in &tables {
                validate_identifier(&mut v, "tables", table);
            }
        }
        if let Some(limit) = self.limit {
            v.check(
                (1..=MAX_GRAPH_TABLES).contains(&limit),
                "limit",
                &format!("must be between 1 and {MAX_GRAPH_TABLES}"),
            );
        }
        v.finish()
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GraphColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub primary_key: bool,
    /// Part of a foreign key of this table.
    pub foreign_key: bool,
    /// Unique on its own (single-column unique constraint or index).
    pub unique: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphTable {
    pub name: String,
    /// `table`, `partitioned_table`, `foreign_table`, `view` or `materialized_view`.
    pub kind: String,
    pub estimated_rows: Option<i64>,
    pub comment: Option<String>,
    pub columns: Vec<GraphColumn>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    /// Each parent row is referenced by at most one child row.
    OneToOne,
    OneToMany,
}

/// A foreign key from `from_table` (child) to `to_table` (parent).
#[derive(Debug, Clone, Serialize)]
pub struct GraphEdge {
    pub constraint: String,
    pub from_table: String,
    pub from_columns: Vec<String>,
    pub to_table: String,
    pub to_columns: Vec<String>,
    pub cardinality: Cardinality,
    /// A child row may reference no parent (a foreign key column is nullable).
    pub optional: bool,
    pub on_delete: &'static str,
    pub on_update: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchemaGraph {
    pub schema: String,
    pub tables: Vec<GraphTable>,
    pub edges: Vec<GraphEdge>,
    /// Tables in the schema that match the kind filter.
    pub total_tables: usize,
    /// True when only the most connected `limit` tables are returned.
    pub truncated: bool,
    /// Foreign keys between a shown table and one that is not shown
    /// (another schema or outside the selection).
    pub hidden_edges: usize,
}

/// A unique key exists on a subset of the foreign key columns, so each
/// parent row can be referenced at most once.
pub fn infer_cardinality(fk_columns: &[String], unique_keys: &[Vec<String>]) -> Cardinality {
    let one_to_one = unique_keys
        .iter()
        .any(|key| !key.is_empty() && key.iter().all(|c| fk_columns.contains(c)));
    if one_to_one {
        Cardinality::OneToOne
    } else {
        Cardinality::OneToMany
    }
}

/// With MATCH SIMPLE (the default), a row with any null foreign key column
/// is not checked, so the reference is optional.
pub fn is_optional(fk_columns: &[String], nullable: &HashSet<&str>) -> bool {
    fk_columns.iter().any(|c| nullable.contains(c.as_str()))
}

/// Labels for `pg_constraint.confdeltype` / `confupdtype`.
pub fn fk_action(code: &str) -> &'static str {
    match code {
        "r" => "restrict",
        "c" => "cascade",
        "n" => "set_null",
        "d" => "set_default",
        _ => "no_action",
    }
}

/// A table considered for the graph and how many foreign keys touch it.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub oid: i64,
    pub name: String,
    pub degree: i64,
}

/// Keeps the `limit` most connected tables (ties by name), returned in name
/// order, and whether any were dropped.
pub fn pick_tables(mut candidates: Vec<Candidate>, limit: usize) -> (Vec<Candidate>, bool) {
    if candidates.len() <= limit {
        return (candidates, false);
    }
    candidates.sort_by(|a, b| b.degree.cmp(&a.degree).then_with(|| a.name.cmp(&b.name)));
    candidates.truncate(limit);
    candidates.sort_by(|a, b| a.name.cmp(&b.name));
    (candidates, true)
}

// ---------------------------------------------------------------------------
// Saved layouts
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct NodePosition {
    pub x: f64,
    pub y: f64,
}

/// Node positions keyed by table name.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiagramLayout {
    pub positions: BTreeMap<String, NodePosition>,
}

impl Validate for DiagramLayout {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        v.check(
            self.positions.len() <= MAX_LAYOUT_NODES,
            "positions",
            &format!("must contain at most {MAX_LAYOUT_NODES} tables"),
        );
        for (name, p) in &self.positions {
            validate_identifier(&mut v, "positions", name);
            v.check(
                p.x.is_finite()
                    && p.y.is_finite()
                    && p.x.abs() <= MAX_COORDINATE
                    && p.y.abs() <= MAX_COORDINATE,
                "positions",
                "coordinates must be finite numbers within ±10,000,000",
            );
        }
        v.finish()
    }
}

#[derive(Debug, Serialize)]
pub struct LayoutResponse {
    pub positions: BTreeMap<String, NodePosition>,
    /// Null when nothing has been saved (the client lays the graph out).
    pub updated_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cols(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn cardinality_follows_unique_keys() {
        let pk = vec![cols(&["id"])];
        // Plain FK column: many children per parent.
        assert_eq!(
            infer_cardinality(&cols(&["author_id"]), &pk),
            Cardinality::OneToMany
        );
        // FK column is the primary key (shared-key 1:1).
        assert_eq!(
            infer_cardinality(&cols(&["author_id"]), &[cols(&["author_id"])]),
            Cardinality::OneToOne
        );
        // FK column has its own unique constraint.
        assert_eq!(
            infer_cardinality(&cols(&["book_id"]), &[cols(&["id"]), cols(&["book_id"])]),
            Cardinality::OneToOne
        );
        // Composite FK covered by a composite unique key, in any order.
        assert_eq!(
            infer_cardinality(&cols(&["a", "b"]), &[cols(&["b", "a"])]),
            Cardinality::OneToOne
        );
        // A unique key on a subset of the FK columns is enough.
        assert_eq!(
            infer_cardinality(&cols(&["a", "b"]), &[cols(&["a"])]),
            Cardinality::OneToOne
        );
        // A unique key that also needs other columns is not.
        assert_eq!(
            infer_cardinality(&cols(&["a"]), &[cols(&["a", "seq"])]),
            Cardinality::OneToMany
        );
        assert_eq!(
            infer_cardinality(&cols(&["a"]), &[Vec::new()]),
            Cardinality::OneToMany
        );
    }

    #[test]
    fn optional_when_any_fk_column_is_nullable() {
        let nullable: HashSet<&str> = ["manager_id", "b"].into_iter().collect();
        assert!(is_optional(&cols(&["manager_id"]), &nullable));
        assert!(!is_optional(&cols(&["department_id"]), &nullable));
        assert!(is_optional(&cols(&["a", "b"]), &nullable));
    }

    #[test]
    fn labels_referential_actions() {
        assert_eq!(fk_action("c"), "cascade");
        assert_eq!(fk_action("n"), "set_null");
        assert_eq!(fk_action("a"), "no_action");
    }

    #[test]
    fn keeps_the_most_connected_tables() {
        let c = |name: &str, degree| Candidate {
            oid: 0,
            name: name.into(),
            degree,
        };
        let all = vec![c("a", 0), c("b", 3), c("c", 1), c("d", 3)];
        let (kept, truncated) = pick_tables(all.clone(), 2);
        assert!(truncated);
        assert_eq!(
            kept.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["b", "d"]
        );
        let (kept, truncated) = pick_tables(all, 4);
        assert!(!truncated);
        assert_eq!(kept.len(), 4);
    }

    #[test]
    fn graph_query_validation() {
        let q = |tables: Option<&str>, limit| GraphQuery {
            schema: "public".into(),
            tables: tables.map(Into::into),
            include_views: false,
            limit,
        };
        assert!(q(None, None).validate().is_ok());
        assert_eq!(
            q(Some(" a, b ,a,,"), None).table_list().unwrap(),
            ["a", "b"]
        );
        assert!(q(Some(","), None).validate().is_err());
        assert!(q(Some(&"x".repeat(64)), None).validate().is_err());
        assert!(q(None, Some(0)).validate().is_err());
        assert!(q(None, Some(MAX_GRAPH_TABLES + 1)).validate().is_err());
        let many = (0..=MAX_GRAPH_TABLES)
            .map(|i| format!("t{i}"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(q(Some(&many), None).validate().is_err());
    }

    #[test]
    fn layouts_are_bounded() {
        let layout = |x: f64| DiagramLayout {
            positions: [("orders".to_string(), NodePosition { x, y: 0.0 })].into(),
        };
        assert!(layout(120.5).validate().is_ok());
        assert!(layout(f64::INFINITY).validate().is_err());
        assert!(layout(MAX_COORDINATE * 2.0).validate().is_err());
        let mut bad = layout(0.0);
        bad.positions
            .insert(String::new(), NodePosition { x: 0.0, y: 0.0 });
        assert!(bad.validate().is_err());
        let big = DiagramLayout {
            positions: (0..=MAX_LAYOUT_NODES)
                .map(|i| (format!("t{i}"), NodePosition { x: 0.0, y: 0.0 }))
                .collect(),
        };
        assert!(big.validate().is_err());
    }
}
