//! Builds Data Studio SQL from structured intent.
//!
//! Safety rules:
//! * Every identifier comes from the live catalog ([`Table`]) and is quoted
//!   by [`quote_ident`]. A client-supplied name that is not a column of the
//!   table is rejected before any SQL is built.
//! * Every value is bound as a **text** parameter and cast to a type name
//!   that also comes from the catalog (`CAST($1 AS integer)`). Values are
//!   therefore parsed by PostgreSQL's own input functions and never appear
//!   in the statement text.
//! * Rows are read as text (`CAST(col AS text)`), so the values a client
//!   sends back for keys, cursors and stale-write checks round-trip exactly.
//!
//! Each [`Sql`] carries two renderings: `text` (placeholders, executed) and
//! `display` (values inlined as literals, shown to the user). Both are
//! produced by the same calls, so they cannot drift apart.

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    database::library::like_pattern,
    errors::AppError,
    models::data::{
        ChangeKind, ChangeSet, FilterCondition, FilterGroup, FilterNode, FilterOperator, LogicOp,
        Pagination, SortDirection, SortKey, ValueMap,
    },
};

/// Byte values are shown as a hex preview of at most this many bytes.
pub const BYTEA_PREVIEW_BYTES: usize = 2048;
/// Counting stops here; larger totals are estimated.
pub const COUNT_CAP: i64 = 10_000;
/// Offsets beyond this are rejected (use filters or keyset pages instead).
pub const MAX_OFFSET: u64 = 10_000_000;

/// Keywords that cannot be used as bare column or table names (PostgreSQL
/// "reserved" and "non-reserved (cannot be function or type)" keywords).
/// Sorted for binary search.
const KEYWORDS: &[&str] = &[
    "all",
    "analyse",
    "analyze",
    "and",
    "any",
    "array",
    "as",
    "asc",
    "asymmetric",
    "authorization",
    "between",
    "bigint",
    "binary",
    "bit",
    "boolean",
    "both",
    "case",
    "cast",
    "char",
    "character",
    "check",
    "coalesce",
    "collate",
    "collation",
    "column",
    "concurrently",
    "constraint",
    "create",
    "cross",
    "current_catalog",
    "current_date",
    "current_role",
    "current_schema",
    "current_time",
    "current_timestamp",
    "current_user",
    "dec",
    "decimal",
    "default",
    "deferrable",
    "desc",
    "distinct",
    "do",
    "else",
    "end",
    "except",
    "exists",
    "extract",
    "false",
    "fetch",
    "float",
    "for",
    "foreign",
    "freeze",
    "from",
    "full",
    "grant",
    "greatest",
    "group",
    "grouping",
    "having",
    "ilike",
    "in",
    "initially",
    "inner",
    "inout",
    "int",
    "integer",
    "intersect",
    "interval",
    "into",
    "is",
    "isnull",
    "join",
    "json",
    "lateral",
    "leading",
    "least",
    "left",
    "like",
    "limit",
    "localtime",
    "localtimestamp",
    "national",
    "natural",
    "nchar",
    "none",
    "normalize",
    "not",
    "notnull",
    "null",
    "nullif",
    "numeric",
    "offset",
    "on",
    "only",
    "or",
    "order",
    "out",
    "outer",
    "overlaps",
    "overlay",
    "placing",
    "position",
    "precision",
    "primary",
    "real",
    "references",
    "returning",
    "right",
    "row",
    "select",
    "session_user",
    "setof",
    "similar",
    "smallint",
    "some",
    "substring",
    "symmetric",
    "system_user",
    "table",
    "tablesample",
    "then",
    "time",
    "timestamp",
    "to",
    "trailing",
    "treat",
    "trim",
    "true",
    "union",
    "unique",
    "user",
    "using",
    "values",
    "varchar",
    "variadic",
    "verbose",
    "when",
    "where",
    "window",
    "with",
];

/// Quotes an identifier unless it is a plain lower-case, non-keyword name.
pub fn quote_ident(name: &str) -> String {
    let plain = name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && KEYWORDS.binary_search(&name).is_err();
    if plain {
        name.to_string()
    } else {
        format!("\"{}\"", name.replace('"', "\"\""))
    }
}

/// A string literal that is safe whatever `standard_conforming_strings` is.
pub fn quote_literal(value: &str) -> String {
    let body = value.replace('\'', "''");
    if value.contains('\\') {
        format!("E'{}'", body.replace('\\', "\\\\"))
    } else {
        format!("'{body}'")
    }
}

fn is_plain_number(v: &str) -> bool {
    let digits = v.strip_prefix('-').unwrap_or(v);
    !digits.is_empty()
        && digits.chars().any(|c| c.is_ascii_digit())
        && digits
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'))
        && digits.starts_with(|c: char| c.is_ascii_digit() || c == '.')
        && v.parse::<f64>().is_ok_and(f64::is_finite)
}

/// Converts a JSON request value to the text form that is bound.
pub fn value_text(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        other => Some(other.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Catalog model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    /// Declared type with modifiers, e.g. `character varying(50)`.
    pub data_type: String,
    /// Type used to cast bound parameters: the declared type without
    /// modifiers, so over-long values raise an error instead of being
    /// silently truncated by an explicit cast.
    pub cast_type: String,
    /// Base type name (domains resolved), e.g. `int4`, `jsonb`.
    pub base_type: String,
    /// `pg_type.typcategory`.
    pub category: char,
    pub nullable: bool,
    pub has_default: bool,
    /// `a` (always) or `d` (by default).
    pub identity: Option<char>,
    pub generated: bool,
    pub enum_values: Vec<String>,
}

impl Column {
    pub fn kind(&self) -> &'static str {
        match (self.category, self.base_type.as_str()) {
            (_, "json" | "jsonb") => "json",
            (_, "uuid") => "uuid",
            (_, "bytea") => "binary",
            ('A', _) => "array",
            ('B', _) => "boolean",
            ('N', _) => "number",
            ('D', _) => "datetime",
            ('E', _) => "enum",
            ('S', _) => "text",
            _ => "other",
        }
    }

    /// Whether inserts and updates may set this column.
    pub fn editable(&self) -> bool {
        !self.generated && self.identity != Some('a') && !matches!(self.kind(), "array" | "binary")
    }

    pub fn identity_label(&self) -> Option<&'static str> {
        match self.identity {
            Some('a') => Some("always"),
            Some('d') => Some("by_default"),
            _ => None,
        }
    }

    fn ident(&self) -> String {
        quote_ident(&self.name)
    }

    /// The column read as text, as returned to clients.
    fn text_expr(&self) -> String {
        format!("CAST({} AS text)", self.ident())
    }

    fn display_literal(&self, value: Option<&str>) -> String {
        match value {
            None => "NULL".into(),
            Some(v) if self.category == 'N' && is_plain_number(v) => v.to_string(),
            Some(v) if self.category == 'B' && matches!(v, "true" | "false") => v.to_string(),
            Some(v) => quote_literal(v),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    PrimaryKey,
    Unique,
}

impl KeyKind {
    pub fn as_str(self) -> &'static str {
        match self {
            KeyKind::PrimaryKey => "primary_key",
            KeyKind::Unique => "unique",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Table {
    pub schema: String,
    pub name: String,
    /// table, partitioned_table, view, materialized_view or foreign_table.
    pub kind: String,
    pub columns: Vec<Column>,
    /// Indexes into `columns` of the row key.
    pub key: Vec<usize>,
    pub key_kind: Option<KeyKind>,
    pub can_insert: bool,
    pub can_update: bool,
    pub can_delete: bool,
}

impl Table {
    pub fn qualified(&self) -> String {
        format!("{}.{}", quote_ident(&self.schema), quote_ident(&self.name))
    }

    pub fn column(&self, name: &str) -> Result<(usize, &Column), AppError> {
        self.columns
            .iter()
            .enumerate()
            .find(|(_, c)| c.name == name)
            .ok_or_else(|| AppError::validation("column", format!("unknown column \"{name}\"")))
    }

    pub fn key_columns(&self) -> impl Iterator<Item = &Column> {
        self.key.iter().map(|&i| &self.columns[i])
    }

    /// Why rows of this relation cannot be edited, if they cannot.
    pub fn read_only_reason(&self) -> Option<&'static str> {
        if !matches!(self.kind.as_str(), "table" | "partitioned_table") {
            Some("Only tables can be edited; views and foreign tables are read-only.")
        } else if self.key.is_empty() {
            Some("This table has no primary key or unique not-null key, so rows cannot be identified for editing.")
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Statement builder
// ---------------------------------------------------------------------------

/// A statement with bound text parameters, plus a display rendering with the
/// values inlined.
#[derive(Debug, Clone, Default)]
pub struct Sql {
    pub text: String,
    pub display: String,
    pub params: Vec<Option<String>>,
}

impl Sql {
    fn push(&mut self, s: &str) {
        self.text.push_str(s);
        self.display.push_str(s);
    }

    fn push_split(&mut self, text: &str, display: &str) {
        self.text.push_str(text);
        self.display.push_str(display);
    }

    /// Binds `value` cast to `cast` (`text` needs no cast).
    fn bind(&mut self, value: Option<String>, cast: &str, display: String) {
        self.params.push(value);
        let n = self.params.len();
        let placeholder = if cast == "text" {
            format!("${n}")
        } else {
            format!("CAST(${n} AS {cast})")
        };
        self.push_split(&placeholder, &display);
    }

    fn bind_column(&mut self, value: Option<String>, col: &Column) {
        let display = col.display_literal(value.as_deref());
        self.bind(value, &col.cast_type, display);
    }

    /// Adds a text parameter without emitting it; reference it with [`Sql::param_ref`].
    fn text_param(&mut self, value: String) -> usize {
        self.params.push(Some(value));
        self.params.len()
    }

    fn param_ref(&mut self, n: usize) {
        let display = self.params[n - 1]
            .as_deref()
            .map(quote_literal)
            .unwrap_or_else(|| "NULL".into());
        self.push_split(&format!("${n}"), &display);
    }
}

fn filter_error(message: impl Into<String>) -> AppError {
    AppError::validation("filter", message)
}

fn node_is_empty(node: &FilterNode) -> bool {
    match node {
        FilterNode::Condition(_) => false,
        FilterNode::Group(g) => group_is_empty(g),
    }
}

pub fn group_is_empty(group: &FilterGroup) -> bool {
    group.items.iter().all(node_is_empty)
}

fn required_scalar(c: &FilterCondition) -> Result<String, AppError> {
    value_text(&c.value).ok_or_else(|| {
        filter_error(format!(
            "a value is required for \"{}\" (use is_null to match NULL)",
            c.column
        ))
    })
}

fn push_condition(sql: &mut Sql, table: &Table, c: &FilterCondition) -> Result<(), AppError> {
    let (_, col) = table.column(&c.column)?;
    let ident = col.ident();
    // `json` has no equality or ordering operators; compare as jsonb.
    let (lhs, cast) = if col.base_type == "json" {
        (format!("CAST({ident} AS jsonb)"), "jsonb")
    } else {
        (ident.clone(), col.cast_type.as_str())
    };
    let bind = |sql: &mut Sql, v: String| {
        let display = col.display_literal(Some(&v));
        sql.bind(Some(v), cast, display);
    };
    match c.operator {
        FilterOperator::IsNull => sql.push(&format!("{ident} IS NULL")),
        FilterOperator::IsNotNull => sql.push(&format!("{ident} IS NOT NULL")),
        FilterOperator::Like | FilterOperator::Ilike => {
            let pattern = required_scalar(c)?;
            let op = if c.operator == FilterOperator::Like {
                "LIKE"
            } else {
                "ILIKE"
            };
            let subject = if col.cast_type == "text" {
                ident
            } else {
                col.text_expr()
            };
            sql.push(&format!("{subject} {op} "));
            let n = sql.text_param(pattern);
            sql.param_ref(n);
        }
        FilterOperator::In => {
            let items = match &c.value {
                Value::Array(items) if !items.is_empty() => items,
                _ => {
                    return Err(filter_error(format!(
                        "\"in\" on \"{}\" needs a non-empty list of values",
                        c.column
                    )))
                }
            };
            sql.push(&format!("{lhs} IN ("));
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    sql.push(", ");
                }
                let v = value_text(item).ok_or_else(|| {
                    filter_error(format!("\"in\" on \"{}\" cannot contain NULL", c.column))
                })?;
                bind(sql, v);
            }
            sql.push(")");
        }
        FilterOperator::Between => {
            let (low, high) = match &c.value {
                Value::Array(items) if items.len() == 2 => {
                    (value_text(&items[0]), value_text(&items[1]))
                }
                _ => (None, None),
            };
            let (Some(low), Some(high)) = (low, high) else {
                return Err(filter_error(format!(
                    "\"between\" on \"{}\" needs exactly two non-null values",
                    c.column
                )));
            };
            sql.push(&format!("{lhs} BETWEEN "));
            bind(sql, low);
            sql.push(" AND ");
            bind(sql, high);
        }
        op => {
            let symbol = match op {
                FilterOperator::Eq => "=",
                FilterOperator::Ne => "<>",
                FilterOperator::Gt => ">",
                FilterOperator::Gte => ">=",
                FilterOperator::Lt => "<",
                _ => "<=",
            };
            let v = required_scalar(c)?;
            sql.push(&format!("{lhs} {symbol} "));
            bind(sql, v);
        }
    }
    Ok(())
}

fn push_group(
    sql: &mut Sql,
    table: &Table,
    group: &FilterGroup,
    wrap: bool,
) -> Result<(), AppError> {
    let items: Vec<&FilterNode> = group.items.iter().filter(|n| !node_is_empty(n)).collect();
    let wrap = wrap && items.len() > 1;
    let sep = match group.op {
        LogicOp::And => " AND ",
        LogicOp::Or => " OR ",
    };
    if wrap {
        sql.push("(");
    }
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            sql.push(sep);
        }
        match item {
            FilterNode::Condition(c) => push_condition(sql, table, c)?,
            FilterNode::Group(g) => push_group(sql, table, g, true)?,
        }
    }
    if wrap {
        sql.push(")");
    }
    Ok(())
}

/// Equality on every key column, e.g. `id = CAST($1 AS bigint)`.
fn push_key_match(
    sql: &mut Sql,
    table: &Table,
    key: &ValueMap,
    field: &str,
) -> Result<(), AppError> {
    let expected: Vec<&str> = table.key_columns().map(|c| c.name.as_str()).collect();
    if key.len() != expected.len() || !expected.iter().all(|k| key.contains_key(*k)) {
        return Err(AppError::validation(
            field,
            format!(
                "must contain exactly the key columns: {}",
                expected.join(", ")
            ),
        ));
    }
    for (i, col) in table.key_columns().enumerate() {
        if i > 0 {
            sql.push(" AND ");
        }
        let value = value_text(&key[&col.name]).ok_or_else(|| {
            AppError::validation(field, format!("\"{}\" must not be null", col.name))
        })?;
        sql.push(&format!("{} = ", col.ident()));
        sql.bind_column(Some(value), col);
    }
    Ok(())
}

/// The structured query shared by browse, preview, count and export.
#[derive(Debug, Clone, Copy, Default)]
pub struct RowQuery<'a> {
    pub filter: Option<&'a FilterGroup>,
    pub sort: &'a [SortKey],
    pub search: Option<&'a str>,
}

impl RowQuery<'_> {
    fn search_term(&self) -> Option<&str> {
        self.search.map(str::trim).filter(|s| !s.is_empty())
    }
}

/// Opaque page cursor (base64url-encoded JSON).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cursor {
    /// Key values of the last row of the previous page.
    After(Vec<String>),
    Offset(u64),
}

impl Cursor {
    pub fn encode(&self) -> String {
        let json = serde_json::to_vec(self).unwrap_or_default();
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
    }

    pub fn decode(s: &str) -> Result<Self, AppError> {
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(s)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .ok_or_else(|| AppError::validation("cursor", "is invalid"))
    }
}

/// Keyset pagination applies when the table has a row key and the sort is
/// the key itself (or empty), in one direction.
pub fn pagination_for(table: &Table, sort: &[SortKey]) -> (Pagination, SortDirection) {
    if table.key.is_empty() {
        return (Pagination::Offset, SortDirection::Asc);
    }
    if sort.is_empty() {
        return (Pagination::Keyset, SortDirection::Asc);
    }
    let direction = sort[0].direction;
    let matches_key = sort.len() == table.key.len()
        && sort
            .iter()
            .zip(table.key_columns())
            .all(|(s, c)| s.column == c.name && s.direction == direction);
    if matches_key {
        (Pagination::Keyset, direction)
    } else {
        (Pagination::Offset, SortDirection::Asc)
    }
}

fn push_where(
    sql: &mut Sql,
    table: &Table,
    q: &RowQuery<'_>,
    keyset: Option<(&[String], SortDirection)>,
    keys: Option<&[ValueMap]>,
) -> Result<(), AppError> {
    let filter = q.filter.filter(|f| !group_is_empty(f));
    let search = q.search_term();
    let clauses = usize::from(filter.is_some())
        + usize::from(search.is_some())
        + usize::from(keyset.is_some())
        + usize::from(keys.is_some());
    if clauses == 0 {
        return Ok(());
    }
    sql.push(" WHERE ");
    let mut first = true;
    let mut next = |sql: &mut Sql| {
        if !first {
            sql.push(" AND ");
        }
        first = false;
    };
    if let Some(filter) = filter {
        next(sql);
        push_group(sql, table, filter, clauses > 1)?;
    }
    if let Some(term) = search {
        next(sql);
        let searchable: Vec<&Column> = table
            .columns
            .iter()
            .filter(|c| c.kind() != "binary")
            .collect();
        if searchable.is_empty() {
            sql.push("false");
        } else {
            let n = sql.text_param(like_pattern(term));
            sql.push("(");
            for (i, col) in searchable.iter().enumerate() {
                if i > 0 {
                    sql.push(" OR ");
                }
                let subject = if col.cast_type == "text" {
                    col.ident()
                } else {
                    col.text_expr()
                };
                sql.push(&format!("{subject} ILIKE "));
                sql.param_ref(n);
            }
            sql.push(")");
        }
    }
    if let Some((after, direction)) = keyset {
        next(sql);
        if after.len() != table.key.len() {
            return Err(AppError::validation(
                "cursor",
                "does not match the table key",
            ));
        }
        let cols: Vec<&Column> = table.key_columns().collect();
        let tuple = |items: Vec<String>| {
            if items.len() == 1 {
                items.into_iter().next().unwrap_or_default()
            } else {
                format!("({})", items.join(", "))
            }
        };
        sql.push(&tuple(cols.iter().map(|c| c.ident()).collect()));
        sql.push(if direction == SortDirection::Asc {
            " > "
        } else {
            " < "
        });
        if cols.len() > 1 {
            sql.push("(");
        }
        for (i, (col, value)) in cols.iter().zip(after).enumerate() {
            if i > 0 {
                sql.push(", ");
            }
            sql.bind_column(Some(value.clone()), col);
        }
        if cols.len() > 1 {
            sql.push(")");
        }
    }
    if let Some(keys) = keys {
        next(sql);
        sql.push("(");
        for (i, key) in keys.iter().enumerate() {
            if i > 0 {
                sql.push(" OR ");
            }
            sql.push("(");
            push_key_match(sql, table, key, "keys")?;
            sql.push(")");
        }
        sql.push(")");
    }
    Ok(())
}

fn push_order(
    sql: &mut Sql,
    table: &Table,
    sort: &[SortKey],
    mode: Pagination,
    direction: SortDirection,
) -> Result<(), AppError> {
    let dir = |d: SortDirection| if d == SortDirection::Asc { "" } else { " DESC" };
    let mut terms: Vec<String> = Vec::new();
    if mode == Pagination::Keyset {
        terms.extend(
            table
                .key_columns()
                .map(|c| format!("{}{}", c.ident(), dir(direction))),
        );
    } else {
        let mut used = Vec::new();
        for key in sort {
            let (i, col) = table.column(&key.column).map_err(|_| {
                AppError::validation("sort", format!("unknown column \"{}\"", key.column))
            })?;
            if !used.contains(&i) {
                used.push(i);
                terms.push(format!("{}{}", col.ident(), dir(key.direction)));
            }
        }
        // Key columns break ties so offset pages are stable.
        for &i in &table.key {
            if !used.contains(&i) {
                terms.push(table.columns[i].ident());
            }
        }
    }
    if !terms.is_empty() {
        // The executed select list reads `CAST(col AS text)`, whose output
        // column is also called `col`, and ORDER BY prefers output columns.
        // Qualifying the names makes them refer to the table's columns.
        let qualified = table.qualified();
        let text: Vec<String> = terms.iter().map(|t| format!("{qualified}.{t}")).collect();
        sql.push_split(
            &format!(" ORDER BY {}", text.join(", ")),
            &format!(" ORDER BY {}", terms.join(", ")),
        );
    }
    Ok(())
}

fn select_list(table: &Table, columns: &[usize], preview: bool) -> String {
    columns
        .iter()
        .map(|&i| {
            let col = &table.columns[i];
            if preview && col.kind() == "binary" {
                let id = col.ident();
                format!(
                    "E'\\\\x' || encode(substring({id} FROM 1 FOR {BYTEA_PREVIEW_BYTES}), 'hex') || \
                     CASE WHEN octet_length({id}) > {BYTEA_PREVIEW_BYTES} THEN '...' ELSE '' END"
                )
            } else {
                col.text_expr()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn all_columns(table: &Table) -> Vec<usize> {
    (0..table.columns.len()).collect()
}

/// One page of rows. Fetches `page_size + 1` rows to detect a next page; the
/// display text shows `page_size`.
pub fn build_browse(
    table: &Table,
    q: &RowQuery<'_>,
    cursor: Option<&Cursor>,
    page_size: u32,
) -> Result<(Sql, Pagination), AppError> {
    let (mode, direction) = pagination_for(table, q.sort);
    let mut sql = Sql::default();
    sql.push_split(
        &format!(
            "SELECT {} FROM {}",
            select_list(table, &all_columns(table), true),
            table.qualified()
        ),
        &format!("SELECT * FROM {}", table.qualified()),
    );
    let keyset = match (cursor, mode) {
        (None, _) => None,
        (Some(Cursor::After(values)), Pagination::Keyset) => Some((values.as_slice(), direction)),
        (Some(Cursor::Offset(_)), Pagination::Offset) => None,
        _ => {
            return Err(AppError::validation(
                "cursor",
                "does not match the current sort",
            ))
        }
    };
    push_where(&mut sql, table, q, keyset, None)?;
    push_order(&mut sql, table, q.sort, mode, direction)?;
    sql.push_split(
        &format!(" LIMIT {}", u64::from(page_size) + 1),
        &format!(" LIMIT {page_size}"),
    );
    if let Some(Cursor::Offset(offset)) = cursor {
        if *offset > MAX_OFFSET {
            return Err(AppError::validation(
                "cursor",
                "offset is too large; narrow the filter instead",
            ));
        }
        if *offset > 0 {
            sql.push(&format!(" OFFSET {offset}"));
        }
    }
    Ok((sql, mode))
}

/// Counts matching rows up to [`COUNT_CAP`] + 1.
pub fn build_count(table: &Table, q: &RowQuery<'_>) -> Result<Sql, AppError> {
    let mut sql = Sql::default();
    sql.push(&format!(
        "SELECT count(*) FROM (SELECT 1 FROM {}",
        table.qualified()
    ));
    push_where(&mut sql, table, q, None, None)?;
    sql.push(&format!(" LIMIT {}) capped", COUNT_CAP + 1));
    Ok(sql)
}

/// Rows for an export, with full (untruncated) values.
pub fn build_export(
    table: &Table,
    q: &RowQuery<'_>,
    keys: Option<&[ValueMap]>,
    columns: &[usize],
    limit: u64,
) -> Result<Sql, AppError> {
    let mut sql = Sql::default();
    sql.push(&format!(
        "SELECT {} FROM {}",
        select_list(table, columns, false),
        table.qualified()
    ));
    if keys.is_some() && table.key.is_empty() {
        return Err(AppError::validation("keys", "this table has no row key"));
    }
    push_where(&mut sql, table, q, None, keys)?;
    push_order(
        &mut sql,
        table,
        q.sort,
        Pagination::Offset,
        SortDirection::Asc,
    )?;
    sql.push(&format!(" LIMIT {limit}"));
    Ok(sql)
}

/// A staged change rendered as one statement.
#[derive(Debug, Clone)]
pub struct ChangeStatement {
    pub kind: ChangeKind,
    pub index: usize,
    pub sql: Sql,
    /// The update checks original values (stale-write protection).
    pub guarded: bool,
}

fn value_columns<'t>(
    table: &'t Table,
    values: &ValueMap,
    field: &str,
) -> Result<Vec<(&'t Column, Option<String>)>, AppError> {
    for name in values.keys() {
        let (_, col) = table
            .column(name)
            .map_err(|_| AppError::validation(field, format!("unknown column \"{name}\"")))?;
        if !col.editable() {
            return Err(AppError::validation(
                field,
                format!(
                    "\"{name}\" cannot be edited (generated, identity, array or binary column)"
                ),
            ));
        }
    }
    // Table order, so generated SQL is stable and readable.
    Ok(table
        .columns
        .iter()
        .filter_map(|c| values.get(&c.name).map(|v| (c, value_text(v))))
        .collect())
}

/// Builds one statement per change: deletes, then updates, then inserts.
pub fn build_changes(table: &Table, changes: &ChangeSet) -> Result<Vec<ChangeStatement>, AppError> {
    if let Some(reason) = table.read_only_reason() {
        return Err(AppError::Forbidden(reason.into()));
    }
    let returning = format!(
        " RETURNING {}",
        select_list(table, &all_columns(table), true)
    );
    let mut out = Vec::with_capacity(changes.len());

    for (index, change) in changes.deletes.iter().enumerate() {
        let field = format!("changes.deletes[{index}].key");
        let mut sql = Sql::default();
        sql.push(&format!("DELETE FROM {} WHERE ", table.qualified()));
        push_key_match(&mut sql, table, &change.key, &field)?;
        // One returned row per deleted row, so the count can be checked.
        sql.push_split(" RETURNING 1", "");
        sql.push(";");
        out.push(ChangeStatement {
            kind: ChangeKind::Delete,
            index,
            sql,
            guarded: false,
        });
    }

    for (index, change) in changes.updates.iter().enumerate() {
        let field = format!("changes.updates[{index}]");
        let values = value_columns(table, &change.values, &format!("{field}.values"))?;
        if values.is_empty() {
            return Err(AppError::validation(
                format!("{field}.values"),
                "must not be empty",
            ));
        }
        let mut sql = Sql::default();
        sql.push(&format!("UPDATE {} SET ", table.qualified()));
        for (i, (col, value)) in values.iter().enumerate() {
            if i > 0 {
                sql.push(", ");
            }
            sql.push(&format!("{} = ", col.ident()));
            sql.bind_column(value.clone(), col);
        }
        sql.push(" WHERE ");
        push_key_match(&mut sql, table, &change.key, &format!("{field}.key"))?;
        let mut guarded = false;
        if let Some(original) = &change.original {
            for (col, _) in &values {
                if let Some(orig) = original.get(&col.name) {
                    guarded = true;
                    sql.push(&format!(" AND {} IS NOT DISTINCT FROM ", col.text_expr()));
                    let v = value_text(orig);
                    let display = v
                        .as_deref()
                        .map(quote_literal)
                        .unwrap_or_else(|| "NULL".into());
                    sql.bind(v, "text", display);
                }
            }
        }
        sql.push_split(&returning, "");
        sql.push(";");
        out.push(ChangeStatement {
            kind: ChangeKind::Update,
            index,
            sql,
            guarded,
        });
    }

    for (index, change) in changes.inserts.iter().enumerate() {
        let values = value_columns(
            table,
            &change.values,
            &format!("changes.inserts[{index}].values"),
        )?;
        let mut sql = Sql::default();
        if values.is_empty() {
            sql.push(&format!("INSERT INTO {} DEFAULT VALUES", table.qualified()));
        } else {
            let names: Vec<String> = values.iter().map(|(c, _)| c.ident()).collect();
            sql.push(&format!(
                "INSERT INTO {} ({}) VALUES (",
                table.qualified(),
                names.join(", ")
            ));
            for (i, (col, value)) in values.iter().enumerate() {
                if i > 0 {
                    sql.push(", ");
                }
                sql.bind_column(value.clone(), col);
            }
            sql.push(")");
        }
        sql.push_split(&returning, "");
        sql.push(";");
        out.push(ChangeStatement {
            kind: ChangeKind::Insert,
            index,
            sql,
            guarded: false,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Export formats
// ---------------------------------------------------------------------------

/// RFC 4180 field. NULL is an empty field. Text that a spreadsheet would
/// evaluate as a formula is prefixed with `'` (CWE-1236), like the client's
/// result-grid export; plain numbers such as `-5` are left alone.
pub fn csv_field(value: Option<&str>) -> String {
    let neutralised;
    let value = match value {
        Some(v) if v.starts_with(['=', '+', '-', '@', '\t', '\r']) && !is_plain_number(v) => {
            neutralised = format!("'{v}");
            Some(neutralised.as_str())
        }
        other => other,
    };
    match value {
        None => String::new(),
        Some(v) if v.contains([',', '"', '\n', '\r']) || v.starts_with(' ') || v.ends_with(' ') => {
            format!("\"{}\"", v.replace('"', "\"\""))
        }
        Some(v) => v.to_string(),
    }
}

pub fn csv_line<'a>(values: impl Iterator<Item = Option<&'a str>>) -> String {
    let mut line = values.map(csv_field).collect::<Vec<_>>().join(",");
    line.push_str("\r\n");
    line
}

/// Largest integer JavaScript represents exactly (2^53 - 1).
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// A JSON number when the value survives the conversion exactly; `None`
/// keeps it as a string (large integers, high-precision decimals, NaN).
fn json_number(v: &str) -> Option<Value> {
    if !is_plain_number(v) {
        return None;
    }
    if let Ok(i) = v.parse::<i64>() {
        return (i.abs() <= MAX_SAFE_INTEGER).then(|| Value::from(i));
    }
    let significant = v
        .split(['e', 'E'])
        .next()
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>();
    let significant = significant.trim_start_matches('0').trim_end_matches('0');
    if significant.len() > 15 {
        return None;
    }
    serde_json::Number::from_f64(v.parse().ok()?).map(Value::Number)
}

/// A cell as a typed JSON value: numbers, booleans and JSON documents keep
/// their type; everything else is a string.
pub fn json_cell(value: Option<&str>, col: &Column) -> Value {
    let Some(v) = value else {
        return Value::Null;
    };
    match col.kind() {
        "number" => json_number(v).unwrap_or_else(|| Value::String(v.to_string())),
        "boolean" => match v {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            other => Value::String(other.to_string()),
        },
        "json" => serde_json::from_str(v).unwrap_or_else(|_| Value::String(v.to_string())),
        _ => Value::String(v.to_string()),
    }
}

/// `INSERT INTO ... VALUES (...);` for one exported row.
pub fn sql_insert_line(target: &str, columns: &[&Column], values: &[Option<String>]) -> String {
    let names: Vec<String> = columns.iter().map(|c| c.ident()).collect();
    let literals: Vec<String> = columns
        .iter()
        .zip(values)
        .map(|(c, v)| c.display_literal(v.as_deref()))
        .collect();
    let overriding = if columns.iter().any(|c| c.identity == Some('a')) {
        " OVERRIDING SYSTEM VALUE"
    } else {
        ""
    };
    format!(
        "INSERT INTO {target} ({}){overriding} VALUES ({});\n",
        names.join(", "),
        literals.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::data::{DeleteChange, InsertChange, UpdateChange};
    use serde_json::json;

    fn col(name: &str, cast: &str, base: &str, category: char) -> Column {
        Column {
            name: name.into(),
            data_type: cast.into(),
            cast_type: cast.into(),
            base_type: base.into(),
            category,
            nullable: true,
            has_default: false,
            identity: None,
            generated: false,
            enum_values: vec![],
        }
    }

    fn people() -> Table {
        let mut id = col("id", "bigint", "int8", 'N');
        id.identity = Some('a');
        id.nullable = false;
        let mut total = col("total", "numeric", "numeric", 'N');
        total.generated = true;
        Table {
            schema: "public".into(),
            name: "people".into(),
            kind: "table".into(),
            columns: vec![
                id,
                col("name", "text", "text", 'S'),
                col("age", "integer", "int4", 'N'),
                col("Full Name", "character varying", "varchar", 'S'),
                col("profile", "json", "json", 'U'),
                col("active", "boolean", "bool", 'B'),
                col("photo", "bytea", "bytea", 'U'),
                total,
            ],
            key: vec![0],
            key_kind: Some(KeyKind::PrimaryKey),
            can_insert: true,
            can_update: true,
            can_delete: true,
        }
    }

    fn filter(value: Value) -> FilterGroup {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn identifiers_are_quoted_when_needed() {
        assert_eq!(quote_ident("people"), "people");
        assert_eq!(quote_ident("_x1"), "_x1");
        assert_eq!(quote_ident("Full Name"), "\"Full Name\"");
        assert_eq!(quote_ident("order"), "\"order\"");
        assert_eq!(quote_ident("user"), "\"user\"");
        assert_eq!(quote_ident("1abc"), "\"1abc\"");
        assert_eq!(quote_ident("café"), "\"café\"");
        assert_eq!(quote_ident(""), "\"\"");
        assert!(
            KEYWORDS.windows(2).all(|w| w[0] < w[1]),
            "keywords must stay sorted"
        );
    }

    #[test]
    fn hostile_identifiers_cannot_escape_quotes() {
        for hostile in [
            "x\"; DROP TABLE people; --",
            "\"",
            "\"\"",
            "a\" OR \"1\"=\"1",
            "name\") VALUES (1); --",
        ] {
            let quoted = quote_ident(hostile);
            assert!(quoted.starts_with('"') && quoted.ends_with('"'), "{quoted}");
            let inner = &quoted[1..quoted.len() - 1];
            // Every quote inside is doubled, so the identifier never ends early.
            assert_eq!(
                inner.replace("\"\"", "").matches('"').count(),
                0,
                "{quoted}"
            );
            assert_eq!(inner.replace("\"\"", "\""), hostile);
        }
    }

    #[test]
    fn literals_are_escaped() {
        assert_eq!(quote_literal("it's"), "'it''s'");
        assert_eq!(quote_literal("a\\b'"), "E'a\\\\b'''");
        assert_eq!(quote_literal(""), "''");
    }

    #[test]
    fn every_operator_binds_its_values() {
        let t = people();
        let q = |f: Value| {
            let f = filter(json!({ "items": [f] }));
            let mut sql = Sql::default();
            push_group(&mut sql, &t, &f, false).map(|_| sql)
        };
        let cases = [
            (
                json!({"column":"age","operator":"=","value":25}),
                "age = CAST($1 AS integer)",
                "age = 25",
            ),
            (
                json!({"column":"age","operator":"!=","value":"25"}),
                "age <> CAST($1 AS integer)",
                "age <> 25",
            ),
            (
                json!({"column":"age","operator":">","value":25}),
                "age > CAST($1 AS integer)",
                "age > 25",
            ),
            (
                json!({"column":"age","operator":">=","value":25}),
                "age >= CAST($1 AS integer)",
                "age >= 25",
            ),
            (
                json!({"column":"age","operator":"<","value":25}),
                "age < CAST($1 AS integer)",
                "age < 25",
            ),
            (
                json!({"column":"age","operator":"<=","value":25}),
                "age <= CAST($1 AS integer)",
                "age <= 25",
            ),
            (
                json!({"column":"name","operator":"like","value":"A%"}),
                "name LIKE $1",
                "name LIKE 'A%'",
            ),
            (
                json!({"column":"age","operator":"ilike","value":"2%"}),
                "CAST(age AS text) ILIKE $1",
                "CAST(age AS text) ILIKE '2%'",
            ),
            (
                json!({"column":"name","operator":"in","value":["a","b"]}),
                "name IN ($1, $2)",
                "name IN ('a', 'b')",
            ),
            (
                json!({"column":"age","operator":"between","value":[1,9]}),
                "age BETWEEN CAST($1 AS integer) AND CAST($2 AS integer)",
                "age BETWEEN 1 AND 9",
            ),
            (
                json!({"column":"name","operator":"is_null"}),
                "name IS NULL",
                "name IS NULL",
            ),
            (
                json!({"column":"name","operator":"is_not_null"}),
                "name IS NOT NULL",
                "name IS NOT NULL",
            ),
            (
                json!({"column":"active","operator":"=","value":true}),
                "active = CAST($1 AS boolean)",
                "active = true",
            ),
            (
                json!({"column":"profile","operator":"=","value":{"a":1}}),
                "CAST(profile AS jsonb) = CAST($1 AS jsonb)",
                "CAST(profile AS jsonb) = '{\"a\":1}'",
            ),
            (
                json!({"column":"Full Name","operator":"=","value":"x"}),
                "\"Full Name\" = CAST($1 AS character varying)",
                "\"Full Name\" = 'x'",
            ),
        ];
        for (input, text, display) in cases {
            let sql = q(input.clone()).unwrap();
            assert_eq!(sql.text, text, "{input}");
            assert_eq!(sql.display, display, "{input}");
        }
        let sql = q(json!({"column":"age","operator":"between","value":[1,9]})).unwrap();
        assert_eq!(sql.params, vec![Some("1".into()), Some("9".into())]);
    }

    #[test]
    fn invalid_conditions_are_rejected() {
        let t = people();
        for bad in [
            json!({"column":"nope","operator":"=","value":1}),
            json!({"column":"age","operator":"=","value":null}),
            json!({"column":"age","operator":"in","value":[]}),
            json!({"column":"age","operator":"in","value":5}),
            json!({"column":"age","operator":"in","value":[1,null]}),
            json!({"column":"age","operator":"between","value":[1]}),
        ] {
            let f = filter(json!({ "items": [bad.clone()] }));
            let mut sql = Sql::default();
            assert!(push_group(&mut sql, &t, &f, false).is_err(), "{bad}");
        }
    }

    #[test]
    fn hostile_values_are_only_ever_parameters() {
        let t = people();
        let hostile = "'; DROP TABLE people; --";
        let f = filter(json!({ "op": "or", "items": [
            {"column":"name","operator":"=","value":hostile},
            {"column":"name","operator":"ilike","value":hostile}
        ]}));
        let (sql, _) = build_browse(
            &t,
            &RowQuery {
                filter: Some(&f),
                search: Some(hostile),
                ..Default::default()
            },
            None,
            10,
        )
        .unwrap();
        assert!(!sql.text.contains("DROP"), "{}", sql.text);
        assert!(sql
            .params
            .iter()
            .all(|p| p.as_deref().is_some_and(|p| p.contains("DROP"))));
        // The display form escapes the quote.
        assert!(
            sql.display.contains("'''; DROP TABLE people; --'"),
            "{}",
            sql.display
        );
    }

    #[test]
    fn nested_groups_are_parenthesised() {
        let t = people();
        let f = filter(json!({ "op": "and", "items": [
            {"column":"age","operator":">","value":25},
            { "op": "or", "items": [
                {"column":"name","operator":"=","value":"a"},
                {"column":"name","operator":"=","value":"b"}
            ]},
            { "op": "or", "items": [] }
        ]}));
        let (sql, mode) = build_browse(
            &t,
            &RowQuery {
                filter: Some(&f),
                ..Default::default()
            },
            None,
            50,
        )
        .unwrap();
        assert_eq!(mode, Pagination::Keyset);
        assert_eq!(
            sql.display,
            "SELECT * FROM public.people WHERE age > 25 AND (name = 'a' OR name = 'b') ORDER BY id LIMIT 50"
        );
        // ORDER BY names the table column, not the text output column.
        assert!(
            sql.text.ends_with("ORDER BY public.people.id LIMIT 51"),
            "{}",
            sql.text
        );
        assert!(sql
            .text
            .contains("E'\\\\x' || encode(substring(photo FROM 1 FOR 2048), 'hex')"));

        // A top-level OR is wrapped when combined with other clauses.
        let f = filter(json!({ "op": "or", "items": [
            {"column":"age","operator":"=","value":1},
            {"column":"age","operator":"=","value":2}
        ]}));
        let (sql, _) = build_browse(
            &t,
            &RowQuery {
                filter: Some(&f),
                search: Some("x"),
                ..Default::default()
            },
            None,
            10,
        )
        .unwrap();
        assert!(
            sql.display.contains(
                "WHERE (age = 1 OR age = 2) AND (CAST(id AS text) ILIKE '%x%' OR name ILIKE '%x%'"
            ),
            "{}",
            sql.display
        );
        assert!(
            !sql.display.contains("photo ILIKE"),
            "binary columns are not searched"
        );
    }

    #[test]
    fn keyset_and_offset_pagination() {
        let t = people();
        let cursor = Cursor::After(vec!["42".into()]);
        let (sql, mode) = build_browse(&t, &RowQuery::default(), Some(&cursor), 100).unwrap();
        assert_eq!(mode, Pagination::Keyset);
        assert_eq!(
            sql.display,
            "SELECT * FROM public.people WHERE id > 42 ORDER BY id LIMIT 100"
        );

        let desc = [SortKey {
            column: "id".into(),
            direction: SortDirection::Desc,
        }];
        let (sql, mode) = build_browse(
            &t,
            &RowQuery {
                sort: &desc,
                ..Default::default()
            },
            Some(&cursor),
            100,
        )
        .unwrap();
        assert_eq!(mode, Pagination::Keyset);
        assert!(
            sql.display
                .ends_with("WHERE id < 42 ORDER BY id DESC LIMIT 100"),
            "{}",
            sql.display
        );

        let by_name = [SortKey {
            column: "name".into(),
            direction: SortDirection::Desc,
        }];
        let (sql, mode) = build_browse(
            &t,
            &RowQuery {
                sort: &by_name,
                ..Default::default()
            },
            Some(&Cursor::Offset(200)),
            100,
        )
        .unwrap();
        assert_eq!(mode, Pagination::Offset);
        assert!(
            sql.display
                .ends_with("ORDER BY name DESC, id LIMIT 100 OFFSET 200"),
            "{}",
            sql.display
        );

        // Cursors must match the pagination mode and the key.
        assert!(build_browse(
            &t,
            &RowQuery {
                sort: &by_name,
                ..Default::default()
            },
            Some(&cursor),
            100
        )
        .is_err());
        assert!(build_browse(&t, &RowQuery::default(), Some(&Cursor::Offset(5)), 100).is_err());
        assert!(build_browse(&t, &RowQuery::default(), Some(&Cursor::After(vec![])), 100).is_err());
        assert!(build_browse(
            &t,
            &RowQuery {
                sort: &by_name,
                ..Default::default()
            },
            Some(&Cursor::Offset(MAX_OFFSET + 1)),
            100
        )
        .is_err());
        let bad_sort = [SortKey {
            column: "nope".into(),
            direction: SortDirection::Asc,
        }];
        assert!(build_browse(
            &t,
            &RowQuery {
                sort: &bad_sort,
                ..Default::default()
            },
            None,
            100
        )
        .is_err());
    }

    #[test]
    fn composite_keyset_uses_row_comparison() {
        let mut t = people();
        t.key = vec![1, 2];
        let cursor = Cursor::After(vec!["Ada".into(), "36".into()]);
        let (sql, _) = build_browse(&t, &RowQuery::default(), Some(&cursor), 10).unwrap();
        assert!(
            sql.text
                .contains("WHERE (name, age) > ($1, CAST($2 AS integer)) ORDER BY public.people.name, public.people.age"),
            "{}",
            sql.text
        );
    }

    #[test]
    fn cursors_round_trip_and_reject_garbage() {
        for c in [
            Cursor::After(vec!["a\"b".into(), "1".into()]),
            Cursor::Offset(300),
        ] {
            assert_eq!(Cursor::decode(&c.encode()).unwrap(), c);
        }
        assert!(Cursor::decode("not base64!").is_err());
        assert!(Cursor::decode("e30").is_err()); // "{}"
    }

    #[test]
    fn count_is_capped() {
        let t = people();
        let f = filter(json!({ "items": [{"column":"age","operator":">","value":25}] }));
        let sql = build_count(
            &t,
            &RowQuery {
                filter: Some(&f),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            sql.text,
            "SELECT count(*) FROM (SELECT 1 FROM public.people WHERE age > CAST($1 AS integer) LIMIT 10001) capped"
        );
    }

    #[test]
    fn change_sets_build_guarded_statements_in_order() {
        let t = people();
        let changes = ChangeSet {
            inserts: vec![
                InsertChange {
                    values: serde_json::from_value(
                        json!({"name": "Ada", "age": 36, "active": null}),
                    )
                    .unwrap(),
                },
                InsertChange::default(),
            ],
            updates: vec![UpdateChange {
                key: serde_json::from_value(json!({"id": "7"})).unwrap(),
                values: serde_json::from_value(json!({"age": 37, "Full Name": "Ada L."})).unwrap(),
                original: Some(
                    serde_json::from_value(json!({"age": "36", "name": "ignored"})).unwrap(),
                ),
            }],
            deletes: vec![DeleteChange {
                key: serde_json::from_value(json!({"id": 9})).unwrap(),
            }],
        };
        let stmts = build_changes(&t, &changes).unwrap();
        let kinds: Vec<ChangeKind> = stmts.iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            [
                ChangeKind::Delete,
                ChangeKind::Update,
                ChangeKind::Insert,
                ChangeKind::Insert
            ]
        );
        assert_eq!(
            stmts[0].sql.display,
            "DELETE FROM public.people WHERE id = 9;"
        );
        assert_eq!(
            stmts[0].sql.text,
            "DELETE FROM public.people WHERE id = CAST($1 AS bigint) RETURNING 1;"
        );
        assert_eq!(
            stmts[1].sql.display,
            "UPDATE public.people SET age = 37, \"Full Name\" = 'Ada L.' WHERE id = 7 AND CAST(age AS text) IS NOT DISTINCT FROM '36';"
        );
        assert!(stmts[1].guarded);
        assert!(stmts[1]
            .sql
            .text
            .contains(" RETURNING CAST(id AS text), CAST(name AS text)"));
        assert_eq!(
            stmts[2].sql.display,
            "INSERT INTO public.people (name, age, active) VALUES ('Ada', 36, NULL);"
        );
        assert_eq!(
            stmts[2].sql.params,
            vec![Some("Ada".into()), Some("36".into()), None]
        );
        assert_eq!(
            stmts[3].sql.display,
            "INSERT INTO public.people DEFAULT VALUES;"
        );
    }

    #[test]
    fn invalid_changes_are_rejected() {
        let t = people();
        let update = |key: Value, values: Value| ChangeSet {
            updates: vec![UpdateChange {
                key: serde_json::from_value(key).unwrap(),
                values: serde_json::from_value(values).unwrap(),
                original: None,
            }],
            ..Default::default()
        };
        for (key, values) in [
            (json!({"id": 1}), json!({})),                   // nothing to set
            (json!({"id": 1}), json!({"nope": 1})),          // unknown column
            (json!({"id": 1}), json!({"id": 2})),            // identity always
            (json!({"id": 1}), json!({"total": 2})),         // generated
            (json!({"id": 1}), json!({"photo": "\\x00"})),   // binary
            (json!({}), json!({"age": 1})),                  // missing key
            (json!({"id": 1, "age": 2}), json!({"age": 1})), // extra key column
            (json!({"id": null}), json!({"age": 1})),        // null key
        ] {
            assert!(
                build_changes(&t, &update(key.clone(), values.clone())).is_err(),
                "{key} {values}"
            );
        }

        let mut view = people();
        view.kind = "view".into();
        assert!(matches!(
            build_changes(&view, &update(json!({"id": 1}), json!({"age": 1}))),
            Err(AppError::Forbidden(_))
        ));
        let mut keyless = people();
        keyless.key.clear();
        assert!(matches!(
            build_changes(&keyless, &update(json!({"id": 1}), json!({"age": 1}))),
            Err(AppError::Forbidden(_))
        ));
    }

    #[test]
    fn export_formats() {
        assert_eq!(csv_field(None), "");
        assert_eq!(csv_field(Some("plain")), "plain");
        assert_eq!(csv_field(Some("a,b")), "\"a,b\"");
        assert_eq!(
            csv_field(Some("=HYPERLINK(\"x\")")),
            "\"'=HYPERLINK(\"\"x\"\")\""
        );
        assert_eq!(csv_field(Some("@cmd")), "'@cmd");
        assert_eq!(csv_field(Some("-12.5")), "-12.5");
        assert_eq!(csv_field(Some("say \"hi\"\n")), "\"say \"\"hi\"\"\n\"");
        assert_eq!(
            csv_line([Some("1"), None, Some("x y")].into_iter()),
            "1,,x y\r\n"
        );

        let t = people();
        assert_eq!(json_cell(Some("36"), &t.columns[2]), json!(36));
        let numeric = col("n", "numeric", "numeric", 'N');
        assert_eq!(
            json_cell(Some("12345678901234567890.5"), &numeric),
            json!("12345678901234567890.5")
        );
        assert_eq!(
            json_cell(Some("9007199254740993"), &numeric),
            json!("9007199254740993")
        );
        assert_eq!(json_cell(Some("10.50"), &numeric), json!(10.5));
        assert_eq!(
            json_cell(Some("NaN"), &col("n", "numeric", "numeric", 'N')),
            json!("NaN")
        );
        assert_eq!(json_cell(Some("true"), &t.columns[5]), json!(true));
        assert_eq!(
            json_cell(Some("{\"a\": [1]}"), &t.columns[4]),
            json!({"a": [1]})
        );
        assert_eq!(json_cell(Some("x"), &t.columns[1]), json!("x"));
        assert_eq!(json_cell(None, &t.columns[1]), Value::Null);

        let cols = [&t.columns[0], &t.columns[1], &t.columns[2]];
        assert_eq!(
            sql_insert_line(&t.qualified(), &cols, &[Some("1".into()), Some("O'Neil \\".into()), None]),
            "INSERT INTO public.people (id, name, age) OVERRIDING SYSTEM VALUE VALUES (1, E'O''Neil \\\\', NULL);\n"
        );
    }

    #[test]
    fn export_selects_full_values_for_chosen_rows() {
        let t = people();
        let keys: Vec<ValueMap> = vec![
            serde_json::from_value(json!({"id": 1})).unwrap(),
            serde_json::from_value(json!({"id": 2})).unwrap(),
        ];
        let sql = build_export(&t, &RowQuery::default(), Some(&keys), &[1, 6], 100).unwrap();
        assert_eq!(
            sql.text,
            "SELECT CAST(name AS text), CAST(photo AS text) FROM public.people \
             WHERE ((id = CAST($1 AS bigint)) OR (id = CAST($2 AS bigint))) ORDER BY public.people.id LIMIT 100"
        );
    }

    #[test]
    fn numbers_are_only_inlined_when_plain() {
        assert!(is_plain_number("42"));
        assert!(is_plain_number("-3.5e10"));
        assert!(is_plain_number(".5"));
        assert!(!is_plain_number("NaN"));
        assert!(!is_plain_number("inf"));
        assert!(!is_plain_number("1; DROP"));
        assert!(!is_plain_number("e5"));
        assert!(!is_plain_number(""));
        let c = col("n", "integer", "int4", 'N');
        assert_eq!(c.display_literal(Some("1 OR 1=1")), "'1 OR 1=1'");
    }
}
