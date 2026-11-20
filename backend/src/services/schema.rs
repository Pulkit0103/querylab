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
    database::library::like_pattern,
    errors::{AppError, AppResult},
    models::schema::{
        CatalogEntry, ColumnInfo, ColumnRow, ConstraintInfo, FunctionDetail, FunctionInfo,
        IndexInfo, Relationship, SchemaInfo, SchemaObjects, SearchHit, SequenceInfo, TableDdl,
        TableInfo, TriggerInfo, TriggerRow, TypeInfo,
    },
    services::{
        connections::ConnectionService,
        ddl::{self, DdlColumn, DdlTable},
        target_db::{self, TargetSpec},
    },
    state::AppState,
};

const INTROSPECTION_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_TABLES: i64 = 5000;

/// Excludes system schemas. `$1`-free so it can be embedded.
const USER_SCHEMAS: &str = "n.nspname NOT IN ('pg_catalog', 'information_schema') \
     AND n.nspname NOT LIKE 'pg\\_toast%' AND n.nspname NOT LIKE 'pg\\_temp\\_%'";

/// Excludes objects that belong to extensions (e.g. pgcrypto functions).
const NOT_FROM_EXTENSION: &str = "NOT EXISTS (SELECT 1 FROM pg_depend dep \
     WHERE dep.objid = {oid} AND dep.classid = '{catalog}'::regclass AND dep.deptype = 'e')";

fn not_from_extension(oid: &str, catalog: &str) -> String {
    NOT_FROM_EXTENSION
        .replace("{oid}", oid)
        .replace("{catalog}", catalog)
}

const CONTYPE_LABEL: &str =
    "CASE con.contype WHEN 'p' THEN 'primary_key' WHEN 'f' THEN 'foreign_key' \
     WHEN 'u' THEN 'unique' WHEN 'c' THEN 'check' WHEN 'x' THEN 'exclusion' END";
const CONTYPE_ORDER: &str = "CASE con.contype WHEN 'p' THEN 0 WHEN 'u' THEN 1 WHEN 'c' THEN 2 \
     WHEN 'x' THEN 3 ELSE 4 END";

const MAX_SEARCH_HITS: i64 = 100;

pub(crate) const RELKIND_LABEL: &str =
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
    pub(crate) async fn open(&self, user_id: Uuid, connection_id: Uuid) -> AppResult<PgConnection> {
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

impl SchemaService<'_> {
    pub async fn constraints(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        schema: &str,
        table: &str,
    ) -> AppResult<Vec<ConstraintInfo>> {
        let mut conn = self.open(user_id, connection_id).await?;
        if !table_exists(&mut conn, schema, table).await? {
            close(conn).await;
            return Err(AppError::NotFound("table"));
        }
        let rows = sqlx::query_as::<_, ConstraintInfo>(&format!(
            "SELECT con.conname::text AS name, {CONTYPE_LABEL} AS kind, \
                    ARRAY(SELECT a.attname::text FROM unnest(con.conkey) WITH ORDINALITY AS k(num, ord) \
                          JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.num \
                          ORDER BY k.ord) AS columns, \
                    pg_get_constraintdef(con.oid, true) AS definition, \
                    con.condeferrable AS deferrable, \
                    CASE WHEN con.contype = 'f' THEN format('%I.%I', rn.nspname, rc.relname) END AS referenced_table \
             FROM pg_constraint con \
             JOIN pg_class c ON c.oid = con.conrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             LEFT JOIN pg_class rc ON rc.oid = con.confrelid \
             LEFT JOIN pg_namespace rn ON rn.oid = rc.relnamespace \
             WHERE n.nspname = $1 AND c.relname = $2 AND con.contype IN ('p','f','u','c','x') \
             ORDER BY {CONTYPE_ORDER}, con.conname"
        ))
        .bind(schema)
        .bind(table)
        .fetch_all(&mut conn)
        .await
        .map_err(target_db::query_error)?;
        close(conn).await;
        Ok(rows)
    }

    /// Generated DDL for a table, partitioned table, view or materialized view.
    pub async fn ddl(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        schema: &str,
        table: &str,
    ) -> AppResult<TableDdl> {
        let mut conn = self.open(user_id, connection_id).await?;
        let result = build_ddl(&mut conn, schema, table).await;
        close(conn).await;
        result
    }

    pub async fn functions(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        schema: Option<&str>,
    ) -> AppResult<Vec<FunctionInfo>> {
        let mut conn = self.open(user_id, connection_id).await?;
        let sql = format!(
            "{FUNCTION_SELECT} WHERE {USER_SCHEMAS} AND ($1::text IS NULL OR n.nspname = $1) AND {ext} \
             ORDER BY 2, 3, 5 LIMIT {MAX_TABLES}",
            ext = not_from_extension("p.oid", "pg_proc")
        );
        let rows = sqlx::query_as::<_, FunctionInfo>(&sql)
            .bind(schema)
            .fetch_all(&mut conn)
            .await
            .map_err(target_db::query_error)?;
        close(conn).await;
        Ok(rows)
    }

    pub async fn function(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        oid: i64,
    ) -> AppResult<FunctionDetail> {
        let mut conn = self.open(user_id, connection_id).await?;
        let sql = format!("{FUNCTION_SELECT} WHERE {USER_SCHEMAS} AND p.oid = $1::bigint::oid");
        let info = sqlx::query_as::<_, FunctionInfo>(&sql)
            .bind(oid)
            .fetch_optional(&mut conn)
            .await
            .map_err(target_db::query_error)?;
        let Some(info) = info else {
            close(conn).await;
            return Err(AppError::NotFound("function"));
        };
        // pg_get_functiondef rejects aggregates.
        let definition = if info.kind == "aggregate" {
            None
        } else {
            sqlx::query_scalar::<_, String>("SELECT pg_get_functiondef($1::bigint::oid)")
                .bind(oid)
                .fetch_one(&mut conn)
                .await
                .map(Some)
                .map_err(target_db::query_error)?
        };
        close(conn).await;
        Ok(FunctionDetail { info, definition })
    }

    pub async fn triggers(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        schema: Option<&str>,
        table: Option<&str>,
    ) -> AppResult<Vec<TriggerInfo>> {
        let mut conn = self.open(user_id, connection_id).await?;
        let sql = format!(
            "SELECT n.nspname::text AS schema, c.relname::text AS \"table\", t.tgname::text AS name, \
                    t.tgtype::int2 AS tgtype, t.tgenabled::text AS enabled, \
                    format('%I.%I', pn.nspname, p.proname) AS function, \
                    pg_get_triggerdef(t.oid, true) AS definition \
             FROM pg_trigger t \
             JOIN pg_class c ON c.oid = t.tgrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             JOIN pg_proc p ON p.oid = t.tgfoid \
             JOIN pg_namespace pn ON pn.oid = p.pronamespace \
             WHERE NOT t.tgisinternal AND {USER_SCHEMAS} \
               AND ($1::text IS NULL OR n.nspname = $1) AND ($2::text IS NULL OR c.relname = $2) \
             ORDER BY 1, 2, 3 LIMIT {MAX_TABLES}"
        );
        let rows = sqlx::query_as::<_, TriggerRow>(&sql)
            .bind(schema)
            .bind(table)
            .fetch_all(&mut conn)
            .await
            .map_err(target_db::query_error)?;
        close(conn).await;
        Ok(rows.into_iter().map(TriggerInfo::from).collect())
    }

    /// Sequences and user-defined types (enums, domains, composites, ranges).
    pub async fn objects(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        schema: Option<&str>,
    ) -> AppResult<SchemaObjects> {
        let mut conn = self.open(user_id, connection_id).await?;
        let sequences = sqlx::query_as::<_, SequenceInfo>(
            "SELECT s.schemaname::text AS schema, s.sequencename::text AS name, \
                    s.data_type::regtype::text AS data_type, s.start_value, s.increment_by, s.last_value, \
                    (SELECT format('%I.%I.%I', tn.nspname, tc.relname, a.attname) \
                     FROM pg_class sc \
                     JOIN pg_namespace sn ON sn.oid = sc.relnamespace \
                     JOIN pg_depend d ON d.objid = sc.oid AND d.classid = 'pg_class'::regclass \
                          AND d.refclassid = 'pg_class'::regclass AND d.deptype IN ('a', 'i') \
                     JOIN pg_class tc ON tc.oid = d.refobjid \
                     JOIN pg_namespace tn ON tn.oid = tc.relnamespace \
                     JOIN pg_attribute a ON a.attrelid = tc.oid AND a.attnum = d.refobjsubid \
                     WHERE sn.nspname = s.schemaname AND sc.relname = s.sequencename LIMIT 1) AS owned_by \
             FROM pg_sequences s \
             WHERE s.schemaname NOT IN ('pg_catalog', 'information_schema') \
               AND ($1::text IS NULL OR s.schemaname = $1) \
             ORDER BY 1, 2 LIMIT 5000",
        )
        .bind(schema)
        .fetch_all(&mut conn)
        .await
        .map_err(target_db::query_error)?;

        let types_sql = format!(
            "SELECT n.nspname::text AS schema, t.typname::text AS name, \
                    CASE t.typtype WHEN 'e' THEN 'enum' WHEN 'd' THEN 'domain' \
                         WHEN 'c' THEN 'composite' WHEN 'r' THEN 'range' END AS kind, \
                    CASE t.typtype \
                      WHEN 'd' THEN format_type(t.typbasetype, t.typtypmod) || COALESCE(' ' || \
                           (SELECT string_agg(pg_get_constraintdef(con.oid, true), ' ' ORDER BY con.conname) \
                            FROM pg_constraint con WHERE con.contypid = t.oid), '') \
                      WHEN 'c' THEN (SELECT string_agg(quote_ident(a.attname) || ' ' || \
                                     format_type(a.atttypid, a.atttypmod), ', ' ORDER BY a.attnum) \
                                     FROM pg_attribute a \
                                     WHERE a.attrelid = t.typrelid AND a.attnum > 0 AND NOT a.attisdropped) \
                      WHEN 'r' THEN (SELECT format_type(r.rngsubtype, NULL) FROM pg_range r WHERE r.rngtypid = t.oid) \
                    END AS detail, \
                    ARRAY(SELECT e.enumlabel::text FROM pg_enum e WHERE e.enumtypid = t.oid \
                          ORDER BY e.enumsortorder) AS \"values\", \
                    obj_description(t.oid, 'pg_type') AS comment \
             FROM pg_type t \
             JOIN pg_namespace n ON n.oid = t.typnamespace \
             WHERE {USER_SCHEMAS} AND ($1::text IS NULL OR n.nspname = $1) \
               AND (t.typtype IN ('e', 'd', 'r') \
                    OR (t.typtype = 'c' AND (SELECT c.relkind FROM pg_class c WHERE c.oid = t.typrelid) = 'c')) \
               AND {ext} \
             ORDER BY 1, 2 LIMIT 5000",
            ext = not_from_extension("t.oid", "pg_type")
        );
        let types = sqlx::query_as::<_, TypeInfo>(&types_sql)
            .bind(schema)
            .fetch_all(&mut conn)
            .await
            .map_err(target_db::query_error)?;
        close(conn).await;
        Ok(SchemaObjects { sequences, types })
    }

    /// Case-insensitive search across relations, columns and functions.
    /// Exact name matches rank first.
    pub async fn search(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        query: &str,
    ) -> AppResult<Vec<SearchHit>> {
        let mut conn = self.open(user_id, connection_id).await?;
        let sql = format!(
            "SELECT kind, schema, \"table\", name, detail FROM ( \
               (SELECT {RELKIND_LABEL} AS kind, n.nspname::text AS schema, NULL::text AS \"table\", \
                       c.relname::text AS name, NULL::text AS detail, 0 AS rank \
                FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
                WHERE c.relkind IN ('r','p','v','m','f') AND NOT c.relispartition AND {USER_SCHEMAS} \
                  AND c.relname ILIKE $1 ESCAPE '\\' LIMIT 50) \
               UNION ALL \
               (SELECT 'column', n.nspname::text, c.relname::text, a.attname::text, \
                       format_type(a.atttypid, a.atttypmod), 1 \
                FROM pg_attribute a \
                JOIN pg_class c ON c.oid = a.attrelid \
                JOIN pg_namespace n ON n.oid = c.relnamespace \
                WHERE a.attnum > 0 AND NOT a.attisdropped AND c.relkind IN ('r','p','v','m','f') \
                  AND NOT c.relispartition AND {USER_SCHEMAS} AND a.attname ILIKE $1 ESCAPE '\\' LIMIT 50) \
               UNION ALL \
               (SELECT 'function', n.nspname::text, NULL, p.proname::text, \
                       pg_get_function_identity_arguments(p.oid), 2 \
                FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
                WHERE {USER_SCHEMAS} AND {ext} AND p.proname ILIKE $1 ESCAPE '\\' LIMIT 30) \
             ) hits \
             ORDER BY (lower(name) = lower($2)) DESC, rank, length(name), schema, name \
             LIMIT {MAX_SEARCH_HITS}",
            ext = not_from_extension("p.oid", "pg_proc")
        );
        let rows = sqlx::query_as::<_, SearchHit>(&sql)
            .bind(like_pattern(query))
            .bind(query)
            .fetch_all(&mut conn)
            .await
            .map_err(target_db::query_error)?;
        close(conn).await;
        Ok(rows)
    }
}

const FUNCTION_SELECT: &str = "SELECT p.oid::bigint AS oid, n.nspname::text AS schema, p.proname::text AS name, \
     CASE p.prokind WHEN 'f' THEN 'function' WHEN 'p' THEN 'procedure' \
          WHEN 'a' THEN 'aggregate' WHEN 'w' THEN 'window' END AS kind, \
     pg_get_function_identity_arguments(p.oid) AS arguments, \
     CASE WHEN p.prokind <> 'p' THEN pg_get_function_result(p.oid) END AS result, \
     l.lanname::text AS language, \
     CASE p.provolatile WHEN 'i' THEN 'immutable' WHEN 's' THEN 'stable' ELSE 'volatile' END AS volatility, \
     p.prosecdef AS security_definer, obj_description(p.oid, 'pg_proc') AS comment \
     FROM pg_proc p \
     JOIN pg_namespace n ON n.oid = p.pronamespace \
     JOIN pg_language l ON l.oid = p.prolang";

#[derive(sqlx::FromRow)]
struct RelationRow {
    oid: i64,
    kind: String,
    qualified: String,
    unlogged: bool,
    partition_key: Option<String>,
    comment: Option<String>,
}

async fn build_ddl(conn: &mut PgConnection, schema: &str, table: &str) -> AppResult<TableDdl> {
    let rel = sqlx::query_as::<_, RelationRow>(&format!(
        "SELECT c.oid::bigint AS oid, {RELKIND_LABEL} AS kind, format('%I.%I', n.nspname, c.relname) AS qualified, \
                c.relpersistence = 'u' AS unlogged, \
                CASE WHEN c.relkind = 'p' THEN pg_get_partkeydef(c.oid) END AS partition_key, \
                obj_description(c.oid, 'pg_class') AS comment \
         FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = $1 AND c.relname = $2 AND c.relkind IN ('r','p','v','m','f')"
    ))
    .bind(schema)
    .bind(table)
    .fetch_optional(&mut *conn)
    .await
    .map_err(target_db::query_error)?
    .ok_or(AppError::NotFound("table"))?;

    let ddl = match rel.kind.as_str() {
        "view" | "materialized_view" => {
            let definition: String =
                sqlx::query_scalar("SELECT pg_get_viewdef($1::bigint::oid, true)")
                    .bind(rel.oid)
                    .fetch_one(&mut *conn)
                    .await
                    .map_err(target_db::query_error)?;
            ddl::view_ddl(
                &rel.qualified,
                rel.kind == "materialized_view",
                &definition,
                rel.comment.as_deref(),
            )
        }
        "foreign_table" => format!(
            "-- DDL generation is not supported for foreign tables ({}).\n",
            rel.qualified
        ),
        _ => {
            let columns = sqlx::query_as::<_, DdlColumn>(
                "SELECT quote_ident(a.attname) AS name, format_type(a.atttypid, a.atttypmod) AS data_type, \
                        CASE WHEN a.attcollation <> 0 AND a.attcollation <> t.typcollation THEN \
                             (SELECT format('%I.%I', cn.nspname, co.collname) FROM pg_collation co \
                              JOIN pg_namespace cn ON cn.oid = co.collnamespace WHERE co.oid = a.attcollation) \
                        END AS collation, \
                        a.attnotnull AS not_null, pg_get_expr(d.adbin, d.adrelid) AS default_expr, \
                        CASE a.attidentity WHEN 'a' THEN 'always' WHEN 'd' THEN 'by_default' END AS identity, \
                        CASE a.attgenerated WHEN 's' THEN 'stored' WHEN 'v' THEN 'virtual' END AS generated, \
                        col_description(a.attrelid, a.attnum) AS comment \
                 FROM pg_attribute a \
                 JOIN pg_type t ON t.oid = a.atttypid \
                 LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
                 WHERE a.attrelid = $1::bigint::oid AND a.attnum > 0 AND NOT a.attisdropped \
                 ORDER BY a.attnum",
            )
            .bind(rel.oid)
            .fetch_all(&mut *conn)
            .await
            .map_err(target_db::query_error)?;

            let constraints = sqlx::query_as::<_, (String, String)>(&format!(
                "SELECT quote_ident(con.conname), pg_get_constraintdef(con.oid, true) \
                 FROM pg_constraint con \
                 WHERE con.conrelid = $1::bigint::oid AND con.conislocal \
                   AND con.contype IN ('p','u','c','f','x') \
                 ORDER BY {CONTYPE_ORDER}, con.conname"
            ))
            .bind(rel.oid)
            .fetch_all(&mut *conn)
            .await
            .map_err(target_db::query_error)?;

            let indexes = sqlx::query_scalar::<_, String>(
                "SELECT pg_get_indexdef(i.indexrelid) FROM pg_index i \
                 JOIN pg_class ic ON ic.oid = i.indexrelid \
                 WHERE i.indrelid = $1::bigint::oid \
                   AND NOT EXISTS (SELECT 1 FROM pg_constraint con \
                                   WHERE con.conindid = i.indexrelid AND con.conrelid = i.indrelid \
                                     AND con.contype IN ('p','u','x')) \
                 ORDER BY ic.relname",
            )
            .bind(rel.oid)
            .fetch_all(&mut *conn)
            .await
            .map_err(target_db::query_error)?;

            let triggers = sqlx::query_scalar::<_, String>(
                "SELECT pg_get_triggerdef(t.oid, true) FROM pg_trigger t \
                 WHERE t.tgrelid = $1::bigint::oid AND NOT t.tgisinternal ORDER BY t.tgname",
            )
            .bind(rel.oid)
            .fetch_all(&mut *conn)
            .await
            .map_err(target_db::query_error)?;

            ddl::table_ddl(&DdlTable {
                qualified: rel.qualified.clone(),
                unlogged: rel.unlogged,
                partition_key: rel.partition_key.clone(),
                columns,
                constraints,
                indexes,
                triggers,
                comment: rel.comment.clone(),
            })
        }
    };
    Ok(TableDdl {
        schema: schema.to_string(),
        name: table.to_string(),
        kind: rel.kind,
        ddl,
    })
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

pub(crate) async fn close(conn: PgConnection) {
    let _ = tokio::time::timeout(Duration::from_secs(2), conn.close()).await;
}
