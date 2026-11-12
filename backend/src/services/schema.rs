//! Schema introspection for user databases, read from `pg_catalog`.
//!
//! Every query is parameterised with exact schema/table names (compared to
//! `pg_namespace.nspname` / `pg_class.relname`), so no identifier quoting or
//! string building is involved. Introspection always runs on a read-only
//! connection with a statement timeout.

use std::time::Duration;

use sqlx::{Connection, PgConnection};
use uuid::Uuid;

use crate::{
    errors::{AppError, AppResult},
    models::schema::{
        CatalogEntry, ColumnInfo, ColumnRow, IndexInfo, Relationship, SchemaInfo, TableInfo,
    },
    services::{
        connections::ConnectionService,
        target_db::{self, TargetSpec},
    },
    state::AppState,
};

const INTROSPECTION_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_TABLES: i64 = 5000;

/// Excludes system schemas. `$1`-free so it can be embedded.
const USER_SCHEMAS: &str = "n.nspname NOT IN ('pg_catalog', 'information_schema') \
     AND n.nspname NOT LIKE 'pg\\_toast%' AND n.nspname NOT LIKE 'pg\\_temp\\_%'";

const RELKIND_LABEL: &str =
    "CASE c.relkind WHEN 'r' THEN 'table' WHEN 'p' THEN 'partitioned_table' \
     WHEN 'v' THEN 'view' WHEN 'm' THEN 'materialized_view' WHEN 'f' THEN 'foreign_table' END";

pub struct SchemaService<'a> {
    state: &'a AppState,
}

impl<'a> SchemaService<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// Opens a read-only introspection connection for a connection the user owns.
    async fn open(&self, user_id: Uuid, connection_id: Uuid) -> AppResult<PgConnection> {
        self.state.connect_limiter.check(&user_id.to_string())?;
        let service = ConnectionService::new(self.state);
        let record = service.get_record(user_id, connection_id).await?;
        let secret = service.decrypt_secret(&record)?;
        let mut spec = TargetSpec::from_record(&record, &secret);
        spec.read_only = true;
        let mut conn = target_db::connect(&self.state.config, &spec).await?;
        target_db::set_statement_timeout(&mut conn, INTROSPECTION_TIMEOUT).await?;
        Ok(conn)
    }

    pub async fn schemas(&self, user_id: Uuid, connection_id: Uuid) -> AppResult<Vec<SchemaInfo>> {
        let mut conn = self.open(user_id, connection_id).await?;
        let sql = format!(
            "SELECT n.nspname::text AS name, \
                    count(c.oid) FILTER (WHERE c.relkind IN ('r','p','v','m','f') AND NOT c.relispartition)::bigint AS table_count \
             FROM pg_namespace n LEFT JOIN pg_class c ON c.relnamespace = n.oid \
             WHERE {USER_SCHEMAS} \
             GROUP BY n.nspname \
             ORDER BY (n.nspname <> 'public'), n.nspname"
        );
        let rows = sqlx::query_as::<_, SchemaInfo>(&sql)
            .fetch_all(&mut conn)
            .await
            .map_err(target_db::query_error)?;
        close(conn).await;
        Ok(rows)
    }

    pub async fn tables(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        schema: Option<&str>,
    ) -> AppResult<Vec<TableInfo>> {
        let mut conn = self.open(user_id, connection_id).await?;
        let sql = format!(
            "SELECT n.nspname::text AS schema, c.relname::text AS name, {RELKIND_LABEL} AS kind, \
                    CASE WHEN c.reltuples < 0 THEN NULL ELSE c.reltuples::bigint END AS estimated_rows, \
                    CASE WHEN c.relkind IN ('r','m') THEN pg_total_relation_size(c.oid) END AS total_bytes, \
                    obj_description(c.oid, 'pg_class') AS comment \
             FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE c.relkind IN ('r','p','v','m','f') AND NOT c.relispartition AND {USER_SCHEMAS} \
               AND ($1::text IS NULL OR n.nspname = $1) \
             ORDER BY 1, 2 \
             LIMIT {MAX_TABLES}"
        );
        let rows = sqlx::query_as::<_, TableInfo>(&sql)
            .bind(schema)
            .fetch_all(&mut conn)
            .await
            .map_err(target_db::query_error)?;
        close(conn).await;
        Ok(rows)
    }

    pub async fn columns(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        schema: &str,
        table: &str,
    ) -> AppResult<Vec<ColumnInfo>> {
        let mut conn = self.open(user_id, connection_id).await?;
        let rows = sqlx::query_as::<_, ColumnRow>(
            "SELECT a.attname::text AS name, a.attnum::int AS ordinal, \
                    format_type(a.atttypid, a.atttypmod) AS data_type, \
                    NOT a.attnotnull AS nullable, \
                    pg_get_expr(d.adbin, d.adrelid) AS default_value, \
                    EXISTS (SELECT 1 FROM pg_index i WHERE i.indrelid = c.oid AND i.indisprimary \
                            AND a.attnum = ANY (i.indkey)) AS is_primary_key, \
                    col_description(c.oid, a.attnum) AS comment, \
                    fk.conname AS fk_constraint, fk.ref_schema AS fk_schema, \
                    fk.ref_table AS fk_table, fk.ref_column AS fk_column \
             FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped \
             LEFT JOIN pg_attrdef d ON d.adrelid = c.oid AND d.adnum = a.attnum \
             LEFT JOIN LATERAL ( \
                 SELECT con.conname::text AS conname, rn.nspname::text AS ref_schema, \
                        rc.relname::text AS ref_table, ra.attname::text AS ref_column \
                 FROM pg_constraint con \
                 JOIN pg_class rc ON rc.oid = con.confrelid \
                 JOIN pg_namespace rn ON rn.oid = rc.relnamespace \
                 JOIN LATERAL unnest(con.conkey, con.confkey) AS k(col, refcol) ON true \
                 JOIN pg_attribute ra ON ra.attrelid = con.confrelid AND ra.attnum = k.refcol \
                 WHERE con.conrelid = c.oid AND con.contype = 'f' AND k.col = a.attnum \
                 ORDER BY con.conname LIMIT 1 \
             ) fk ON true \
             WHERE n.nspname = $1 AND c.relname = $2 AND c.relkind IN ('r','p','v','m','f') \
             ORDER BY a.attnum",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(&mut conn)
        .await
        .map_err(target_db::query_error)?;
        if rows.is_empty() {
            let exists = table_exists(&mut conn, schema, table).await?;
            close(conn).await;
            return if exists {
                Ok(Vec::new())
            } else {
                Err(AppError::NotFound("table"))
            };
        }
        close(conn).await;
        Ok(rows.into_iter().map(ColumnInfo::from).collect())
    }

    pub async fn indexes(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        schema: &str,
        table: &str,
    ) -> AppResult<Vec<IndexInfo>> {
        let mut conn = self.open(user_id, connection_id).await?;
        if !table_exists(&mut conn, schema, table).await? {
            close(conn).await;
            return Err(AppError::NotFound("table"));
        }
        let rows = sqlx::query_as::<_, IndexInfo>(
            "SELECT ic.relname::text AS name, pg_get_indexdef(i.indexrelid) AS definition, \
                    i.indisunique AS is_unique, i.indisprimary AS is_primary \
             FROM pg_index i \
             JOIN pg_class ic ON ic.oid = i.indexrelid \
             JOIN pg_class c ON c.oid = i.indrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relname = $2 \
             ORDER BY i.indisprimary DESC, ic.relname",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(&mut conn)
        .await
        .map_err(target_db::query_error)?;
        close(conn).await;
        Ok(rows)
    }

    pub async fn relationships(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        schema: &str,
        table: &str,
    ) -> AppResult<Vec<Relationship>> {
        let mut conn = self.open(user_id, connection_id).await?;
        if !table_exists(&mut conn, schema, table).await? {
            close(conn).await;
            return Err(AppError::NotFound("table"));
        }
        let rows = sqlx::query_as::<_, Relationship>(
            "WITH t AS ( \
                 SELECT c.oid FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = $1 AND c.relname = $2 \
             ) \
             SELECT con.conname::text AS constraint, \
                    CASE WHEN con.conrelid = t.oid THEN 'outgoing' ELSE 'incoming' END AS direction, \
                    sn.nspname::text AS from_schema, sc.relname::text AS from_table, \
                    ARRAY(SELECT a.attname::text FROM unnest(con.conkey) WITH ORDINALITY AS k(num, ord) \
                          JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.num \
                          ORDER BY k.ord) AS from_columns, \
                    tn.nspname::text AS to_schema, tc.relname::text AS to_table, \
                    ARRAY(SELECT a.attname::text FROM unnest(con.confkey) WITH ORDINALITY AS k(num, ord) \
                          JOIN pg_attribute a ON a.attrelid = con.confrelid AND a.attnum = k.num \
                          ORDER BY k.ord) AS to_columns \
             FROM pg_constraint con \
             JOIN t ON con.conrelid = t.oid OR con.confrelid = t.oid \
             JOIN pg_class sc ON sc.oid = con.conrelid JOIN pg_namespace sn ON sn.oid = sc.relnamespace \
             JOIN pg_class tc ON tc.oid = con.confrelid JOIN pg_namespace tn ON tn.oid = tc.relnamespace \
             WHERE con.contype = 'f' \
             ORDER BY 2 DESC, 1",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(&mut conn)
        .await
        .map_err(target_db::query_error)?;
        close(conn).await;
        Ok(rows)
    }

    /// All tables and their columns in one round trip (editor autocomplete).
    pub async fn catalog(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
    ) -> AppResult<Vec<CatalogEntry>> {
        let mut conn = self.open(user_id, connection_id).await?;
        let sql = format!(
            "SELECT n.nspname::text AS schema, c.relname::text AS table, {RELKIND_LABEL} AS kind, \
                    COALESCE(array_agg(a.attname::text ORDER BY a.attnum) FILTER (WHERE a.attnum IS NOT NULL), '{{}}') AS columns, \
                    COALESCE(array_agg(format_type(a.atttypid, a.atttypmod) ORDER BY a.attnum) FILTER (WHERE a.attnum IS NOT NULL), '{{}}') AS column_types \
             FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             LEFT JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped \
             WHERE c.relkind IN ('r','p','v','m','f') AND NOT c.relispartition AND {USER_SCHEMAS} \
             GROUP BY n.nspname, c.relname, c.relkind \
             ORDER BY 1, 2 \
             LIMIT {MAX_TABLES}"
        );
        let rows = sqlx::query_as::<_, CatalogEntry>(&sql)
            .fetch_all(&mut conn)
            .await
            .map_err(target_db::query_error)?;
        close(conn).await;
        Ok(rows)
    }
}

async fn table_exists(conn: &mut PgConnection, schema: &str, table: &str) -> AppResult<bool> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = $1 AND c.relname = $2 AND c.relkind IN ('r','p','v','m','f'))",
    )
    .bind(schema)
    .bind(table)
    .fetch_one(conn)
    .await
    .map_err(target_db::query_error)
}

async fn close(conn: PgConnection) {
    let _ = tokio::time::timeout(Duration::from_secs(2), conn.close()).await;
}
