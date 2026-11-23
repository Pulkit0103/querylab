//! ER diagrams: the schema graph read from `pg_catalog` and saved layouts.

use std::collections::{HashMap, HashSet};

use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    database::diagrams as repo,
    errors::{AppError, AppResult},
    models::{
        diagram::{
            fk_action, infer_cardinality, is_optional, pick_tables, Candidate, DiagramLayout,
            GraphColumn, GraphEdge, GraphTable, LayoutResponse, SchemaGraph,
        },
        workspaces::Access,
    },
    services::{
        authz,
        schema::{close, SchemaService, RELKIND_LABEL},
        target_db,
    },
    state::AppState,
};

/// Upper bound on relations scanned in one schema.
const MAX_SCHEMA_RELATIONS: i64 = 5000;

#[derive(sqlx::FromRow)]
struct TableRow {
    oid: i64,
    name: String,
    kind: String,
    estimated_rows: Option<i64>,
    comment: Option<String>,
    degree: i64,
}

#[derive(sqlx::FromRow)]
struct ColumnRow {
    oid: i64,
    name: String,
    data_type: String,
    nullable: bool,
}

#[derive(sqlx::FromRow)]
struct UniqueKeyRow {
    oid: i64,
    is_primary: bool,
    columns: Vec<String>,
}

#[derive(sqlx::FromRow)]
struct ForeignKeyRow {
    name: String,
    from_oid: i64,
    to_oid: i64,
    from_columns: Vec<String>,
    to_columns: Vec<String>,
    on_delete: String,
    on_update: String,
}

/// Graph options after validation.
pub struct GraphRequest<'a> {
    pub schema: &'a str,
    pub tables: Option<Vec<String>>,
    pub include_views: bool,
    pub limit: usize,
}

impl SchemaService<'_> {
    /// Tables, columns, keys and foreign keys of one schema (or a selection
    /// of its tables) on a single introspection connection.
    pub async fn graph(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        request: GraphRequest<'_>,
    ) -> AppResult<SchemaGraph> {
        let mut conn = self.open(user_id, connection_id).await?;
        let result = load_graph(&mut conn, &request).await;
        close(conn).await;
        result
    }
}

async fn load_graph(conn: &mut PgConnection, req: &GraphRequest<'_>) -> AppResult<SchemaGraph> {
    let kinds = if req.include_views {
        "('r','p','f','v','m')"
    } else {
        "('r','p','f')"
    };
    let rows = sqlx::query_as::<_, TableRow>(&format!(
        "SELECT c.oid::bigint AS oid, c.relname::text AS name, {RELKIND_LABEL} AS kind, \
                CASE WHEN c.reltuples < 0 THEN NULL ELSE c.reltuples::bigint END AS estimated_rows, \
                obj_description(c.oid, 'pg_class') AS comment, \
                (SELECT count(*) FROM pg_constraint con WHERE con.contype = 'f' AND con.conparentid = 0 \
                   AND (con.conrelid = c.oid OR con.confrelid = c.oid))::bigint AS degree \
         FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = $1 AND c.relkind IN {kinds} AND NOT c.relispartition \
           AND ($2::text[] IS NULL OR c.relname = ANY ($2)) \
         ORDER BY c.relname \
         LIMIT {MAX_SCHEMA_RELATIONS}"
    ))
    .bind(req.schema)
    .bind(req.tables.as_deref())
    .fetch_all(&mut *conn)
    .await
    .map_err(target_db::query_error)?;

    if rows.is_empty() {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = $1)")
                .bind(req.schema)
                .fetch_one(&mut *conn)
                .await
                .map_err(target_db::query_error)?;
        if !exists {
            return Err(AppError::NotFound("schema"));
        }
    }

    let total_tables = rows.len();
    let candidates = rows
        .iter()
        .map(|r| Candidate {
            oid: r.oid,
            name: r.name.clone(),
            degree: r.degree,
        })
        .collect();
    // An explicit selection is shown as asked (it is already bounded).
    let limit = if req.tables.is_some() {
        usize::MAX
    } else {
        req.limit
    };
    let (picked, truncated) = pick_tables(candidates, limit);
    let oids: Vec<i64> = picked.iter().map(|c| c.oid).collect();
    let mut by_oid: HashMap<i64, TableRow> = rows.into_iter().map(|r| (r.oid, r)).collect();

    let columns = sqlx::query_as::<_, ColumnRow>(
        "SELECT a.attrelid::bigint AS oid, a.attname::text AS name, \
                format_type(a.atttypid, a.atttypmod) AS data_type, NOT a.attnotnull AS nullable \
         FROM pg_attribute a \
         WHERE a.attrelid = ANY ($1::bigint[]::oid[]) AND a.attnum > 0 AND NOT a.attisdropped \
         ORDER BY a.attrelid, a.attnum",
    )
    .bind(&oids)
    .fetch_all(&mut *conn)
    .await
    .map_err(target_db::query_error)?;

    // Valid, non-partial unique indexes on plain columns; only key columns
    // count (INCLUDE columns do not make anything unique).
    let unique_keys = sqlx::query_as::<_, UniqueKeyRow>(
        "SELECT i.indrelid::bigint AS oid, i.indisprimary AS is_primary, \
                ARRAY(SELECT a.attname::text \
                      FROM unnest(i.indkey::int2[]) WITH ORDINALITY AS k(num, ord) \
                      JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = k.num \
                      WHERE k.ord <= i.indnkeyatts ORDER BY k.ord) AS columns \
         FROM pg_index i \
         WHERE i.indrelid = ANY ($1::bigint[]::oid[]) AND i.indisunique AND i.indisvalid \
           AND i.indpred IS NULL AND i.indexprs IS NULL",
    )
    .bind(&oids)
    .fetch_all(&mut *conn)
    .await
    .map_err(target_db::query_error)?;

    let foreign_keys = sqlx::query_as::<_, ForeignKeyRow>(
        "SELECT con.conname::text AS name, con.conrelid::bigint AS from_oid, con.confrelid::bigint AS to_oid, \
                ARRAY(SELECT a.attname::text FROM unnest(con.conkey) WITH ORDINALITY AS k(num, ord) \
                      JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.num \
                      ORDER BY k.ord) AS from_columns, \
                ARRAY(SELECT a.attname::text FROM unnest(con.confkey) WITH ORDINALITY AS k(num, ord) \
                      JOIN pg_attribute a ON a.attrelid = con.confrelid AND a.attnum = k.num \
                      ORDER BY k.ord) AS to_columns, \
                con.confdeltype::text AS on_delete, con.confupdtype::text AS on_update \
         FROM pg_constraint con \
         WHERE con.contype = 'f' AND con.conparentid = 0 \
           AND (con.conrelid = ANY ($1::bigint[]::oid[]) OR con.confrelid = ANY ($1::bigint[]::oid[])) \
         ORDER BY con.conrelid, con.conname",
    )
    .bind(&oids)
    .fetch_all(&mut *conn)
    .await
    .map_err(target_db::query_error)?;

    Ok(assemble(
        req.schema,
        &picked,
        &mut by_oid,
        columns,
        unique_keys,
        foreign_keys,
        total_tables,
        truncated,
    ))
}

#[allow(clippy::too_many_arguments)]
fn assemble(
    schema: &str,
    picked: &[Candidate],
    rows: &mut HashMap<i64, TableRow>,
    columns: Vec<ColumnRow>,
    unique_keys: Vec<UniqueKeyRow>,
    foreign_keys: Vec<ForeignKeyRow>,
    total_tables: usize,
    truncated: bool,
) -> SchemaGraph {
    let shown: HashSet<i64> = picked.iter().map(|c| c.oid).collect();

    let mut columns_of: HashMap<i64, Vec<ColumnRow>> = HashMap::new();
    for c in columns {
        columns_of.entry(c.oid).or_default().push(c);
    }
    let mut keys_of: HashMap<i64, Vec<UniqueKeyRow>> = HashMap::new();
    for k in unique_keys {
        keys_of.entry(k.oid).or_default().push(k);
    }
    let mut fk_columns_of: HashMap<i64, HashSet<String>> = HashMap::new();
    for fk in &foreign_keys {
        fk_columns_of
            .entry(fk.from_oid)
            .or_default()
            .extend(fk.from_columns.iter().cloned());
    }

    let name_of = |oid: i64, rows: &HashMap<i64, TableRow>| rows.get(&oid).map(|r| r.name.clone());
    let mut edges = Vec::new();
    let mut hidden_edges = 0;
    for fk in foreign_keys {
        if !(shown.contains(&fk.from_oid) && shown.contains(&fk.to_oid)) {
            hidden_edges += 1;
            continue;
        }
        let (Some(from_table), Some(to_table)) =
            (name_of(fk.from_oid, rows), name_of(fk.to_oid, rows))
        else {
            hidden_edges += 1;
            continue;
        };
        let keys: Vec<Vec<String>> = keys_of
            .get(&fk.from_oid)
            .map(|ks| ks.iter().map(|k| k.columns.clone()).collect())
            .unwrap_or_default();
        let nullable: HashSet<&str> = columns_of
            .get(&fk.from_oid)
            .map(|cs| {
                cs.iter()
                    .filter(|c| c.nullable)
                    .map(|c| c.name.as_str())
                    .collect()
            })
            .unwrap_or_default();
        edges.push(GraphEdge {
            cardinality: infer_cardinality(&fk.from_columns, &keys),
            optional: is_optional(&fk.from_columns, &nullable),
            on_delete: fk_action(&fk.on_delete),
            on_update: fk_action(&fk.on_update),
            constraint: fk.name,
            from_table,
            from_columns: fk.from_columns,
            to_table,
            to_columns: fk.to_columns,
        });
    }

    let tables = picked
        .iter()
        .filter_map(|c| rows.remove(&c.oid))
        .map(|row| {
            let keys = keys_of.get(&row.oid);
            let primary: HashSet<&str> = keys
                .into_iter()
                .flatten()
                .filter(|k| k.is_primary)
                .flat_map(|k| k.columns.iter().map(String::as_str))
                .collect();
            let unique: HashSet<&str> = keys
                .into_iter()
                .flatten()
                .filter(|k| k.columns.len() == 1)
                .map(|k| k.columns[0].as_str())
                .collect();
            let fks = fk_columns_of.get(&row.oid);
            let columns = columns_of
                .remove(&row.oid)
                .unwrap_or_default()
                .into_iter()
                .map(|c| GraphColumn {
                    primary_key: primary.contains(c.name.as_str()),
                    unique: unique.contains(c.name.as_str()),
                    foreign_key: fks.is_some_and(|f| f.contains(&c.name)),
                    name: c.name,
                    data_type: c.data_type,
                    nullable: c.nullable,
                })
                .collect();
            GraphTable {
                name: row.name,
                kind: row.kind,
                estimated_rows: row.estimated_rows,
                comment: row.comment,
                columns,
            }
        })
        .collect();

    SchemaGraph {
        schema: schema.to_string(),
        tables,
        edges,
        total_tables,
        truncated,
        hidden_edges,
    }
}

// ---------------------------------------------------------------------------
// Saved layouts (metadata database)
// ---------------------------------------------------------------------------

pub struct DiagramService<'a> {
    state: &'a AppState,
}

impl<'a> DiagramService<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// Layouts are shared by everyone who can see the connection; saving or
    /// resetting one needs write access.
    async fn ensure_access(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        need: Access,
    ) -> AppResult<()> {
        authz::connection(self.state, user_id, connection_id, need).await?;
        Ok(())
    }

    pub async fn layout(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        schema: &str,
    ) -> AppResult<LayoutResponse> {
        self.ensure_access(user_id, connection_id, Access::Read)
            .await?;
        Ok(
            match repo::get_layout(&self.state.db, connection_id, schema).await? {
                Some((positions, updated_at)) => LayoutResponse {
                    positions,
                    updated_at: Some(updated_at),
                },
                None => LayoutResponse {
                    positions: Default::default(),
                    updated_at: None,
                },
            },
        )
    }

    pub async fn save_layout(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        schema: &str,
        layout: DiagramLayout,
    ) -> AppResult<LayoutResponse> {
        self.ensure_access(user_id, connection_id, Access::Write)
            .await?;
        let updated_at = repo::put_layout(
            &self.state.db,
            user_id,
            connection_id,
            schema,
            &layout.positions,
        )
        .await
        .map_err(|e| match &e {
            // The connection was deleted between the check and the write.
            sqlx::Error::Database(db) if db.is_foreign_key_violation() => {
                AppError::NotFound("connection")
            }
            _ => e.into(),
        })?;
        Ok(LayoutResponse {
            positions: layout.positions,
            updated_at: Some(updated_at),
        })
    }

    /// Forgets saved positions so the client lays the diagram out again.
    pub async fn reset_layout(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        schema: &str,
    ) -> AppResult<()> {
        self.ensure_access(user_id, connection_id, Access::Write)
            .await?;
        repo::delete_layout(&self.state.db, connection_id, schema).await?;
        Ok(())
    }
}
