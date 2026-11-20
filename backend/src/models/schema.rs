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

/// A table constraint (NOT NULL constraints are reported on columns instead).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ConstraintInfo {
    pub name: String,
    /// `primary_key`, `foreign_key`, `unique`, `check` or `exclusion`.
    pub kind: String,
    pub columns: Vec<String>,
    /// As produced by `pg_get_constraintdef`, e.g. `CHECK (price >= 0)`.
    pub definition: String,
    pub deferrable: bool,
    /// Qualified referenced table for foreign keys.
    pub referenced_table: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableDdl {
    pub schema: String,
    pub name: String,
    pub kind: String,
    pub ddl: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct FunctionInfo {
    /// `pg_proc.oid`; stable for the lifetime of the function.
    pub oid: i64,
    pub schema: String,
    pub name: String,
    /// `function`, `procedure`, `aggregate` or `window`.
    pub kind: String,
    /// Identity arguments, e.g. `p_status inventory.stock_status`.
    pub arguments: String,
    pub result: Option<String>,
    pub language: String,
    /// `immutable`, `stable` or `volatile`.
    pub volatility: String,
    pub security_definer: bool,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionDetail {
    #[serde(flatten)]
    pub info: FunctionInfo,
    /// `CREATE OR REPLACE ...` statement; null for aggregates.
    pub definition: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TriggerRow {
    pub schema: String,
    pub table: String,
    pub name: String,
    pub tgtype: i16,
    pub enabled: String,
    pub function: String,
    pub definition: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TriggerInfo {
    pub schema: String,
    pub table: String,
    pub name: String,
    /// `BEFORE`, `AFTER` or `INSTEAD OF`.
    pub timing: String,
    /// Any of `INSERT`, `UPDATE`, `DELETE`, `TRUNCATE`.
    pub events: Vec<String>,
    /// `ROW` or `STATEMENT`.
    pub level: String,
    /// Qualified trigger function.
    pub function: String,
    /// `enabled`, `disabled`, `replica` or `always`.
    pub enabled: String,
    pub definition: String,
}

impl From<TriggerRow> for TriggerInfo {
    fn from(r: TriggerRow) -> Self {
        // Bit layout of pg_trigger.tgtype (see src/include/catalog/pg_trigger.h).
        let t = r.tgtype;
        let timing = if t & (1 << 6) != 0 {
            "INSTEAD OF"
        } else if t & (1 << 1) != 0 {
            "BEFORE"
        } else {
            "AFTER"
        };
        let events = [(2, "INSERT"), (4, "UPDATE"), (3, "DELETE"), (5, "TRUNCATE")]
            .into_iter()
            .filter(|(bit, _)| t & (1 << bit) != 0)
            .map(|(_, e)| e.to_string())
            .collect();
        let enabled = match r.enabled.as_str() {
            "D" => "disabled",
            "R" => "replica",
            "A" => "always",
            _ => "enabled",
        };
        Self {
            schema: r.schema,
            table: r.table,
            name: r.name,
            timing: timing.into(),
            events,
            level: if t & 1 != 0 { "ROW" } else { "STATEMENT" }.into(),
            function: r.function,
            enabled: enabled.into(),
            definition: r.definition,
        }
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SequenceInfo {
    pub schema: String,
    pub name: String,
    pub data_type: String,
    pub start_value: i64,
    pub increment_by: i64,
    /// Null if never used or not visible to the connected role.
    pub last_value: Option<i64>,
    /// `schema.table.column` when the sequence is owned by a column.
    pub owned_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TypeInfo {
    pub schema: String,
    pub name: String,
    /// `enum`, `domain`, `composite` or `range`.
    pub kind: String,
    /// Domain base type (with constraints) or composite attribute list.
    pub detail: Option<String>,
    /// Enum labels in sort order.
    pub values: Vec<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchemaObjects {
    pub sequences: Vec<SequenceInfo>,
    pub types: Vec<TypeInfo>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SearchHit {
    /// `table`, `view`, `materialized_view`, `partitioned_table`,
    /// `foreign_table`, `column` or `function`.
    pub kind: String,
    pub schema: String,
    /// Owning table for columns.
    pub table: Option<String>,
    pub name: String,
    /// Column data type or function arguments.
    pub detail: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

impl Validate for SearchQuery {
    fn validate(&self) -> Result<(), AppError> {
        Validator::new()
            .length("q", self.q.trim(), 1, 100)
            .no_control_chars("q", &self.q)
            .finish()
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct TriggersQuery {
    pub schema: Option<String>,
    pub table: Option<String>,
}

impl Validate for TriggersQuery {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        if let Some(schema) = &self.schema {
            validate_identifier(&mut v, "schema", schema);
        }
        if let Some(table) = &self.table {
            validate_identifier(&mut v, "table", table);
        }
        v.finish()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn trigger(tgtype: i16) -> TriggerInfo {
        TriggerInfo::from(TriggerRow {
            schema: "s".into(),
            table: "t".into(),
            name: "trg".into(),
            tgtype,
            enabled: "O".into(),
            function: "s.f()".into(),
            definition: String::new(),
        })
    }

    #[test]
    fn decodes_trigger_type_bits() {
        // BEFORE UPDATE FOR EACH ROW: ROW(1) | BEFORE(2) | UPDATE(16)
        let t = trigger(1 | 2 | 16);
        assert_eq!((t.timing.as_str(), t.level.as_str()), ("BEFORE", "ROW"));
        assert_eq!(t.events, ["UPDATE"]);
        assert_eq!(t.enabled, "enabled");
        // AFTER INSERT OR DELETE FOR EACH STATEMENT
        let t = trigger(4 | 8);
        assert_eq!(
            (t.timing.as_str(), t.level.as_str()),
            ("AFTER", "STATEMENT")
        );
        assert_eq!(t.events, ["INSERT", "DELETE"]);
        // INSTEAD OF INSERT OR UPDATE (views)
        let t = trigger(1 | 64 | 4 | 16);
        assert_eq!(t.timing, "INSTEAD OF");
        assert_eq!(t.events, ["INSERT", "UPDATE"]);
        // TRUNCATE
        assert_eq!(trigger(32).events, ["TRUNCATE"]);
    }

    #[test]
    fn search_query_validation() {
        assert!(SearchQuery { q: " ".into() }.validate().is_err());
        assert!(SearchQuery { q: "x".repeat(101) }.validate().is_err());
        assert!(SearchQuery { q: "price".into() }.validate().is_ok());
    }
}
