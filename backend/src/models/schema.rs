//! Database introspection models (schema explorer).

use serde::{Deserialize, Serialize};

use crate::{
    errors::AppError,
    utils::validation::{Validate, Validator},
};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SchemaInfo {
    pub name: String,
    pub table_count: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TableInfo {
    pub schema: String,
    pub name: String,
    /// `table`, `partitioned_table`, `view`, `materialized_view` or `foreign_table`.
    pub kind: String,
    /// Planner estimate from `pg_class.reltuples`; null if never analysed.
    pub estimated_rows: Option<i64>,
    pub total_bytes: Option<i64>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForeignKeyRef {
    pub constraint: String,
    pub schema: String,
    pub table: String,
    pub column: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ColumnRow {
    pub name: String,
    pub ordinal: i32,
    pub data_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub is_primary_key: bool,
    pub comment: Option<String>,
    pub fk_constraint: Option<String>,
    pub fk_schema: Option<String>,
    pub fk_table: Option<String>,
    pub fk_column: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ColumnInfo {
    pub name: String,
    pub ordinal: i32,
    pub data_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub is_primary_key: bool,
    pub foreign_key: Option<ForeignKeyRef>,
    pub comment: Option<String>,
}

impl From<ColumnRow> for ColumnInfo {
    fn from(r: ColumnRow) -> Self {
        let foreign_key = match (r.fk_constraint, r.fk_schema, r.fk_table, r.fk_column) {
            (Some(constraint), Some(schema), Some(table), Some(column)) => Some(ForeignKeyRef {
                constraint,
                schema,
                table,
                column,
            }),
            _ => None,
        };
        Self {
            name: r.name,
            ordinal: r.ordinal,
            data_type: r.data_type,
            nullable: r.nullable,
            default_value: r.default_value,
            is_primary_key: r.is_primary_key,
            foreign_key,
            comment: r.comment,
        }
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct IndexInfo {
    pub name: String,
    pub definition: String,
    pub is_unique: bool,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Relationship {
    pub constraint: String,
    /// `outgoing`: this table references another. `incoming`: referenced by another.
    pub direction: String,
    pub from_schema: String,
    pub from_table: String,
    pub from_columns: Vec<String>,
    pub to_schema: String,
    pub to_table: String,
    pub to_columns: Vec<String>,
}

/// Lightweight catalog for editor autocomplete: every table with its columns.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CatalogEntry {
    pub schema: String,
    pub table: String,
    pub kind: String,
    pub columns: Vec<String>,
    pub column_types: Vec<String>,
}

fn default_schema() -> String {
    "public".into()
}

#[derive(Debug, Deserialize)]
pub struct SchemaQuery {
    #[serde(default = "default_schema")]
    pub schema: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct TablesQuery {
    pub schema: Option<String>,
}

/// PostgreSQL identifiers are at most 63 bytes (NAMEDATALEN - 1).
pub fn validate_identifier(v: &mut Validator, field: &str, value: &str) {
    v.check(
        !value.is_empty() && value.len() <= 63,
        field,
        "must be 1-63 bytes",
    )
    .no_control_chars(field, value);
}

impl Validate for SchemaQuery {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        validate_identifier(&mut v, "schema", &self.schema);
        v.finish()
    }
}

impl Validate for TablesQuery {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        if let Some(schema) = &self.schema {
            validate_identifier(&mut v, "schema", schema);
        }
        v.finish()
    }
}
