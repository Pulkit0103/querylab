//! Schema comparison: introspection snapshots of one schema and the typed
//! changes that turn a target schema into the source schema.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    errors::AppError,
    models::schema::validate_identifier,
    utils::validation::{Validate, Validator},
};

// ---------------------------------------------------------------------------
// Snapshot
// ---------------------------------------------------------------------------

/// Objects of one schema. Definitions are read with `search_path` set to
/// that schema, so references inside it are unqualified and two schemas
/// (or two databases) compare equal when only their names differ.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub schema: String,
    pub exists: bool,
    pub tables: Vec<TableDef>,
    pub views: Vec<ViewDef>,
    pub functions: Vec<FunctionDef>,
    pub triggers: Vec<TriggerDef>,
    pub enums: Vec<EnumDef>,
    pub domains: Vec<DomainDef>,
    pub sequences: Vec<SequenceDef>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TableDef {
    pub name: String,
    /// `PARTITION BY` clause for partitioned tables.
    pub partition_key: Option<String>,
    pub columns: Vec<ColumnDef>,
    pub constraints: Vec<ConstraintDef>,
    /// Indexes that do not back a constraint.
    pub indexes: Vec<IndexDef>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ColumnDef {
    pub name: String,
    pub data_type: String,
    pub not_null: bool,
    pub default: Option<String>,
    /// `always` or `by_default`.
    pub identity: Option<String>,
    /// Expression of a stored generated column.
    pub generated: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ConstraintDef {
    pub name: String,
    /// `p`, `u`, `c`, `f` or `x`.
    pub kind: String,
    /// `pg_get_constraintdef`, e.g. `FOREIGN KEY (a_id) REFERENCES a(id)`.
    pub definition: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct IndexDef {
    pub name: String,
    /// `pg_get_indexdef`.
    pub definition: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ViewDef {
    pub name: String,
    pub materialized: bool,
    pub definition: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FunctionDef {
    pub name: String,
    /// Identity arguments, e.g. `p_id bigint, p_qty integer`.
    pub arguments: String,
    /// `function` or `procedure`.
    pub kind: String,
    /// `CREATE OR REPLACE ...` with the function name unqualified.
    pub definition: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TriggerDef {
    pub table: String,
    pub name: String,
    pub definition: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EnumDef {
    pub name: String,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DomainDef {
    pub name: String,
    /// Base type, default, NOT NULL and CHECK constraints, e.g.
    /// `integer CHECK (VALUE >= 0)`.
    pub definition: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SequenceDef {
    pub name: String,
    pub data_type: String,
    pub start: i64,
    pub increment: i64,
    pub min: i64,
    pub max: i64,
    pub cycle: bool,
    /// `[table, column]` for sequences owned by a column (serial).
    pub owned_by: Option<[String; 2]>,
}

// ---------------------------------------------------------------------------
// Changes
// ---------------------------------------------------------------------------

/// Order of execution. Drops come first (reverse dependency order), then
/// creates and alterations in dependency order.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    DropTrigger,
    DropView,
    DropFunction,
    DropForeignKey,
    DropConstraint,
    DropIndex,
    DropColumn,
    DropTable,
    DropSequence,
    DropType,
    CreateSchema,
    CreateType,
    CreateSequence,
    CreateTable,
    AlterColumn,
    AddConstraint,
    AddForeignKey,
    CreateIndex,
    SequenceOwner,
    CreateFunction,
    CreateView,
    CreateTrigger,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObjectType {
    Schema,
    Type,
    Sequence,
    Table,
    Column,
    Constraint,
    Index,
    View,
    Function,
    Trigger,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeAction {
    Create,
    Drop,
    Alter,
}

#[derive(Debug, Clone, Serialize)]
pub struct Change {
    pub phase: Phase,
    pub object_type: ObjectType,
    pub action: ChangeAction,
    /// Display name, e.g. `orders.customer_id` or `orders_pkey`.
    pub object: String,
    /// Statements (each ends with `;`).
    pub sql: String,
    /// Statements that undo this change, when that is possible.
    pub down_sql: Option<String>,
    /// Drops data or objects, or narrows a type.
    pub destructive: bool,
    /// Caveats shown next to the change.
    pub notes: Vec<String>,
    /// Definition in the target before the change.
    pub before: Option<String>,
    /// Definition after the change (from the source).
    pub after: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiffSide {
    pub connection_id: Uuid,
    pub schema: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiffRequest {
    /// Desired state.
    pub source: DiffSide,
    /// Database to change.
    pub target: DiffSide,
}

impl Validate for DiffRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        validate_identifier(&mut v, "source.schema", &self.source.schema);
        validate_identifier(&mut v, "target.schema", &self.target.schema);
        v.check(
            !(self.source.connection_id == self.target.connection_id
                && self.source.schema == self.target.schema),
            "target",
            "must differ from the source",
        );
        v.finish()
    }
}

#[derive(Debug, Serialize)]
pub struct DiffResponse {
    pub source_schema: String,
    pub target_schema: String,
    /// Settings that precede the statements of a generated script.
    pub preamble: String,
    /// Ordered changes; scripts are assembled from the selected ones.
    pub changes: Vec<Change>,
    /// Possible renames (a table dropped and a similar one created).
    pub hints: Vec<String>,
}
