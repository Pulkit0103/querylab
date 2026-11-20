//! Data Studio DTOs: table browsing, structured filters, staged change sets
//! and exports.
//!
//! The client only ever sends *intent* (column names, operators, values).
//! Identifiers are checked against the live catalog and every value is bound
//! as a parameter by [`crate::services::sql_builder`]; nothing here is spliced
//! into SQL.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::{
    errors::{AppError, SqlErrorInfo},
    models::schema::validate_identifier,
    utils::validation::{Validate, Validator},
};

pub const DEFAULT_PAGE_SIZE: u32 = 100;
pub const MAX_PAGE_SIZE: u32 = 1000;
pub const MAX_CHANGES: usize = 500;
pub const MAX_FILTER_CONDITIONS: usize = 50;
pub const MAX_FILTER_DEPTH: usize = 5;
pub const MAX_IN_VALUES: usize = 1000;
pub const MAX_SORT_KEYS: usize = 5;
pub const MAX_SEARCH_CHARS: usize = 200;
pub const MAX_FILTER_VALUE_CHARS: usize = 10_000;
pub const MAX_EXPORT_KEYS: usize = 1000;
pub const MAX_CONFIRM_CHARS: usize = 200;

// ---------------------------------------------------------------------------
// Filters and sorting
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogicOp {
    #[default]
    And,
    Or,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub enum FilterOperator {
    #[serde(rename = "=")]
    Eq,
    #[serde(rename = "!=")]
    Ne,
    #[serde(rename = ">")]
    Gt,
    #[serde(rename = ">=")]
    Gte,
    #[serde(rename = "<")]
    Lt,
    #[serde(rename = "<=")]
    Lte,
    #[serde(rename = "like")]
    Like,
    #[serde(rename = "ilike")]
    Ilike,
    #[serde(rename = "in")]
    In,
    #[serde(rename = "is_null")]
    IsNull,
    #[serde(rename = "is_not_null")]
    IsNotNull,
    #[serde(rename = "between")]
    Between,
}

/// `{op, items}` is a group; `{column, operator, value}` is a condition.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum FilterNode {
    Group(FilterGroup),
    Condition(FilterCondition),
}

/// `items` is required so that an untagged condition can never be mistaken
/// for an empty group.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FilterGroup {
    #[serde(default)]
    pub op: LogicOp,
    pub items: Vec<FilterNode>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FilterCondition {
    pub column: String,
    pub operator: FilterOperator,
    #[serde(default)]
    pub value: Value,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SortKey {
    pub column: String,
    #[serde(default)]
    pub direction: SortDirection,
}

fn value_chars(v: &Value) -> usize {
    match v {
        Value::String(s) => s.chars().count(),
        Value::Null | Value::Bool(_) | Value::Number(_) => 0,
        other => other.to_string().len(),
    }
}

impl FilterGroup {
    fn check(&self, v: &mut Validator, depth: usize, conditions: &mut usize) {
        if depth > MAX_FILTER_DEPTH {
            v.check(
                false,
                "filter",
                &format!("must nest at most {MAX_FILTER_DEPTH} groups deep"),
            );
            return;
        }
        for item in &self.items {
            match item {
                FilterNode::Group(g) => g.check(v, depth + 1, conditions),
                FilterNode::Condition(c) => {
                    *conditions += 1;
                    validate_identifier(v, "filter.column", &c.column);
                    let values: Vec<&Value> = match &c.value {
                        Value::Array(items) if c.operator != FilterOperator::Eq => {
                            items.iter().collect()
                        }
                        other => vec![other],
                    };
                    v.check(
                        values.len() <= MAX_IN_VALUES,
                        "filter.value",
                        &format!("must list at most {MAX_IN_VALUES} values"),
                    );
                    v.check(
                        values
                            .iter()
                            .all(|x| value_chars(x) <= MAX_FILTER_VALUE_CHARS),
                        "filter.value",
                        &format!("must be at most {MAX_FILTER_VALUE_CHARS} characters"),
                    );
                }
            }
        }
    }

    pub fn validate_into(&self, v: &mut Validator) {
        let mut conditions = 0;
        self.check(v, 1, &mut conditions);
        v.check(
            conditions <= MAX_FILTER_CONDITIONS,
            "filter",
            &format!("must have at most {MAX_FILTER_CONDITIONS} conditions"),
        );
    }
}

fn validate_query_shape(
    v: &mut Validator,
    filter: Option<&FilterGroup>,
    sort: &[SortKey],
    search: Option<&str>,
) {
    if let Some(f) = filter {
        f.validate_into(v);
    }
    v.check(
        sort.len() <= MAX_SORT_KEYS,
        "sort",
        &format!("must have at most {MAX_SORT_KEYS} columns"),
    );
    for key in sort {
        validate_identifier(v, "sort.column", &key.column);
    }
    if let Some(s) = search {
        v.length("search", s, 0, MAX_SEARCH_CHARS)
            .no_control_chars("search", s);
    }
}

fn validate_target(v: &mut Validator, schema: &str, table: &str) {
    validate_identifier(v, "schema", schema);
    validate_identifier(v, "table", table);
}

// ---------------------------------------------------------------------------
// Browse
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct BrowseRequest {
    pub schema: String,
    pub table: String,
    #[serde(default)]
    pub filter: Option<FilterGroup>,
    #[serde(default)]
    pub sort: Vec<SortKey>,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub page_size: Option<u32>,
    /// Opaque cursor from a previous page's `next_cursor`.
    #[serde(default)]
    pub cursor: Option<String>,
}

impl Validate for BrowseRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        validate_target(&mut v, &self.schema, &self.table);
        validate_query_shape(
            &mut v,
            self.filter.as_ref(),
            &self.sort,
            self.search.as_deref(),
        );
        if let Some(size) = self.page_size {
            v.check(
                (1..=MAX_PAGE_SIZE).contains(&size),
                "page_size",
                &format!("must be between 1 and {MAX_PAGE_SIZE}"),
            );
        }
        if let Some(c) = &self.cursor {
            v.length("cursor", c, 1, 8192);
        }
        v.finish()
    }
}

/// A column as the Data Studio sees it.
#[derive(Debug, Clone, Serialize)]
pub struct DataColumn {
    pub name: String,
    /// Declared type, e.g. `numeric(10,2)`.
    pub data_type: String,
    /// Base type name (domains resolved), e.g. `numeric`, `jsonb`.
    pub type_name: String,
    /// Editor family: boolean, number, datetime, text, json, uuid, enum,
    /// array, binary or other.
    pub kind: &'static str,
    pub nullable: bool,
    pub has_default: bool,
    pub is_key: bool,
    /// `always` or `by_default` for identity columns.
    pub identity: Option<&'static str>,
    pub generated: bool,
    /// Whether the value can be set by inserts and updates.
    pub editable: bool,
    pub enum_values: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Pagination {
    Keyset,
    Offset,
}

#[derive(Debug, Serialize)]
pub struct BrowseResponse {
    pub schema: String,
    pub table: String,
    /// table, partitioned_table, view, materialized_view or foreign_table.
    pub kind: String,
    pub columns: Vec<DataColumn>,
    /// Columns that identify a row (primary key, or a unique not-null key).
    pub key_columns: Vec<String>,
    /// `primary_key` or `unique`.
    pub key_kind: Option<&'static str>,
    /// Cell values in their PostgreSQL text representation.
    pub rows: Vec<Vec<Option<String>>>,
    pub pagination: Pagination,
    pub next_cursor: Option<String>,
    /// Row count matching the filter (first page only).
    pub total_rows: Option<i64>,
    /// False when the count was capped and `total_rows` is an estimate.
    pub total_exact: bool,
    pub editable: bool,
    pub read_only_reason: Option<String>,
    pub can_insert: bool,
    pub can_update: bool,
    pub can_delete: bool,
    /// The SELECT that produced the page, with values inlined.
    pub sql: String,
    /// The page stopped early because of the response size budget.
    pub truncated: bool,
    pub execution_time_ms: u64,
}

// ---------------------------------------------------------------------------
// Change sets
// ---------------------------------------------------------------------------

pub type ValueMap = Map<String, Value>;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct InsertChange {
    #[serde(default)]
    pub values: ValueMap,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateChange {
    pub key: ValueMap,
    pub values: ValueMap,
    /// Values of the edited columns when the row was loaded. When present,
    /// the update only applies if they are unchanged.
    #[serde(default)]
    pub original: Option<ValueMap>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeleteChange {
    pub key: ValueMap,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ChangeSet {
    #[serde(default)]
    pub inserts: Vec<InsertChange>,
    #[serde(default)]
    pub updates: Vec<UpdateChange>,
    #[serde(default)]
    pub deletes: Vec<DeleteChange>,
}

impl ChangeSet {
    pub fn len(&self) -> usize {
        self.inserts.len() + self.updates.len() + self.deletes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn validate_into(&self, v: &mut Validator) {
        v.check(
            self.len() <= MAX_CHANGES,
            "changes",
            &format!("must contain at most {MAX_CHANGES} changes"),
        );
        let maps = self
            .inserts
            .iter()
            .map(|i| &i.values)
            .chain(
                self.updates
                    .iter()
                    .flat_map(|u| [&u.key, &u.values].into_iter().chain(u.original.as_ref())),
            )
            .chain(self.deletes.iter().map(|d| &d.key));
        for map in maps {
            for name in map.keys() {
                validate_identifier(v, "changes.column", name);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Insert,
    Update,
    Delete,
}

#[derive(Debug, Deserialize)]
pub struct PreviewRequest {
    pub schema: String,
    pub table: String,
    #[serde(default)]
    pub filter: Option<FilterGroup>,
    #[serde(default)]
    pub sort: Vec<SortKey>,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub page_size: Option<u32>,
    #[serde(default)]
    pub changes: Option<ChangeSet>,
}

impl Validate for PreviewRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        validate_target(&mut v, &self.schema, &self.table);
        validate_query_shape(
            &mut v,
            self.filter.as_ref(),
            &self.sort,
            self.search.as_deref(),
        );
        if let Some(size) = self.page_size {
            v.check(
                (1..=MAX_PAGE_SIZE).contains(&size),
                "page_size",
                &format!("must be between 1 and {MAX_PAGE_SIZE}"),
            );
        }
        if let Some(changes) = &self.changes {
            changes.validate_into(&mut v);
        }
        v.finish()
    }
}

#[derive(Debug, Serialize)]
pub struct PreviewStatement {
    pub kind: ChangeKind,
    pub index: usize,
    pub sql: String,
}

#[derive(Debug, Serialize)]
pub struct PreviewResponse {
    /// The SELECT for the current filter, sort and search.
    pub select_sql: String,
    /// One statement per staged change, in execution order.
    pub statements: Vec<PreviewStatement>,
}

#[derive(Debug, Deserialize)]
pub struct ApplyRequest {
    pub schema: String,
    pub table: String,
    pub changes: ChangeSet,
    /// Typed confirmation: the connection name for production connections,
    /// any non-empty value for staging.
    #[serde(default)]
    pub confirm: Option<String>,
}

impl Validate for ApplyRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        validate_target(&mut v, &self.schema, &self.table);
        self.changes.validate_into(&mut v);
        v.check(!self.changes.is_empty(), "changes", "must not be empty");
        if let Some(c) = &self.confirm {
            v.length("confirm", c, 0, MAX_CONFIRM_CHARS);
        }
        v.finish()
    }
}

#[derive(Debug, Serialize)]
pub struct AppliedChange {
    pub kind: ChangeKind,
    pub index: usize,
    /// The row as stored after the change (inserts and updates).
    pub row: Option<Vec<Option<String>>>,
}

#[derive(Debug, Serialize)]
pub struct ApplyFailure {
    /// The failing change; `None` when the commit itself failed (for
    /// example a deferred constraint).
    pub kind: Option<ChangeKind>,
    pub index: Option<usize>,
    /// The row was missing or changed since it was loaded.
    pub conflict: bool,
    pub message: String,
    pub sql_error: Option<SqlErrorInfo>,
    pub statement: String,
}

#[derive(Debug, Serialize)]
pub struct ApplyResponse {
    pub committed: bool,
    pub applied: Vec<AppliedChange>,
    pub failure: Option<ApplyFailure>,
    pub rows_affected: u64,
    pub execution_time_ms: u64,
    pub history_id: Option<Uuid>,
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Csv,
    Json,
    Sql,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Csv => "csv",
            ExportFormat::Json => "json",
            ExportFormat::Sql => "sql",
        }
    }

    pub fn content_type(self) -> &'static str {
        match self {
            ExportFormat::Csv => "text/csv; charset=utf-8",
            ExportFormat::Json => "application/json",
            ExportFormat::Sql => "application/sql; charset=utf-8",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ExportRequest {
    pub schema: String,
    pub table: String,
    #[serde(default)]
    pub filter: Option<FilterGroup>,
    #[serde(default)]
    pub sort: Vec<SortKey>,
    #[serde(default)]
    pub search: Option<String>,
    /// Export only these rows (by key).
    #[serde(default)]
    pub keys: Option<Vec<ValueMap>>,
    /// Export only these columns, in this order. Defaults to all columns.
    #[serde(default)]
    pub columns: Option<Vec<String>>,
    pub format: ExportFormat,
}

impl Validate for ExportRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        validate_target(&mut v, &self.schema, &self.table);
        validate_query_shape(
            &mut v,
            self.filter.as_ref(),
            &self.sort,
            self.search.as_deref(),
        );
        if let Some(keys) = &self.keys {
            v.check(
                !keys.is_empty() && keys.len() <= MAX_EXPORT_KEYS,
                "keys",
                &format!("must list between 1 and {MAX_EXPORT_KEYS} rows"),
            );
            for name in keys.iter().flat_map(|k| k.keys()) {
                validate_identifier(&mut v, "keys.column", name);
            }
        }
        if let Some(columns) = &self.columns {
            v.check(!columns.is_empty(), "columns", "must not be empty");
            for name in columns {
                validate_identifier(&mut v, "columns", name);
            }
        }
        v.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn filter_tree_parses_groups_and_conditions() {
        let group: FilterGroup = serde_json::from_value(json!({
            "op": "or",
            "items": [
                { "column": "age", "operator": ">", "value": 25 },
                { "op": "and", "items": [
                    { "column": "name", "operator": "ilike", "value": "%a%" },
                    { "column": "email", "operator": "is_null" }
                ]}
            ]
        }))
        .unwrap();
        assert_eq!(group.op, LogicOp::Or);
        assert!(
            matches!(&group.items[0], FilterNode::Condition(c) if c.operator == FilterOperator::Gt)
        );
        match &group.items[1] {
            FilterNode::Group(g) => {
                assert_eq!(g.items.len(), 2);
                assert!(matches!(&g.items[1], FilterNode::Condition(c) if c.value.is_null()));
            }
            other => panic!("expected a group, got {other:?}"),
        }
        assert!(serde_json::from_value::<FilterGroup>(json!({
            "items": [{ "column": "a", "operator": "contains", "value": 1 }]
        }))
        .is_err());
    }

    fn browse(filter: Value) -> BrowseRequest {
        serde_json::from_value(json!({ "schema": "public", "table": "t", "filter": filter }))
            .unwrap()
    }

    #[test]
    fn filter_limits_are_enforced() {
        let many: Vec<Value> = (0..=MAX_FILTER_CONDITIONS)
            .map(|_| json!({ "column": "a", "operator": "=", "value": 1 }))
            .collect();
        assert!(browse(json!({ "items": many })).validate().is_err());

        let mut deep = json!({ "column": "a", "operator": "=", "value": 1 });
        for _ in 0..=MAX_FILTER_DEPTH {
            deep = json!({ "op": "and", "items": [deep] });
        }
        assert!(browse(deep).validate().is_err());

        let huge = "x".repeat(MAX_FILTER_VALUE_CHARS + 1);
        assert!(
            browse(json!({ "items": [{ "column": "a", "operator": "=", "value": huge }] }))
                .validate()
                .is_err()
        );
        let list: Vec<i32> = (0..=MAX_IN_VALUES as i32).collect();
        assert!(
            browse(json!({ "items": [{ "column": "a", "operator": "in", "value": list }] }))
                .validate()
                .is_err()
        );
        assert!(
            browse(json!({ "items": [{ "column": "a\0", "operator": "=", "value": 1 }] }))
                .validate()
                .is_err()
        );
        assert!(
            browse(json!({ "items": [{ "column": "a", "operator": "=", "value": 1 }] }))
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn apply_requires_changes_within_limits() {
        let empty: ApplyRequest =
            serde_json::from_value(json!({ "schema": "s", "table": "t", "changes": {} })).unwrap();
        assert!(empty.validate().is_err());
        let deletes: Vec<Value> = (0..=MAX_CHANGES)
            .map(|i| json!({ "key": { "id": i } }))
            .collect();
        let too_many: ApplyRequest = serde_json::from_value(
            json!({ "schema": "s", "table": "t", "changes": { "deletes": deletes } }),
        )
        .unwrap();
        assert!(too_many.validate().is_err());
        let ok: ApplyRequest = serde_json::from_value(json!({
            "schema": "s", "table": "t",
            "changes": { "updates": [{ "key": { "id": 1 }, "values": { "name": "x" } }] }
        }))
        .unwrap();
        assert!(ok.validate().is_ok());
    }
}
