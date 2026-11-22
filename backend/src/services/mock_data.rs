//! Mock data: realistic rows for chosen tables, respecting foreign keys,
//! unique keys and simple CHECK constraints.
//!
//! * Tables are filled in topological order of their foreign keys. A
//!   foreign key to another table of the run draws from the rows generated
//!   for it; a foreign key to any other table draws from its existing rows.
//!   Self-references point at rows generated earlier in the same run (or
//!   existing rows). Cycles between tables are refused with an explanation.
//! * Columns get a generator from their name and type (overridable per
//!   column); CHECK constraints of the form `col > n`, `col BETWEEN …` and
//!   `col IN (…)` narrow it. Unique text values get a per-run suffix and
//!   unique integers become sequences; rows that still collide are skipped
//!   (`ON CONFLICT DO NOTHING`) and counted.
//! * Runs are privileged: read-write connection, typed confirmation, never
//!   on production connections unless `MOCK_DATA_ALLOW_PRODUCTION` is set.
//!   Everything is written in one transaction.

use std::{collections::HashSet, sync::Arc, time::Instant};

use sqlx::{Executor, PgConnection, Row};
use uuid::Uuid;

use crate::{
    database::data_jobs as repo,
    errors::{AppError, AppResult, FieldError},
    models::{
        connection::{ConnectionRecord, Environment},
        data_jobs::{
            Generator, JobKind, JobStatus, JobView, MockColumn, MockPreview, MockRequest,
            MockTablePlan, TableCount,
        },
        library::HistoryKind,
        workspaces::Access,
    },
    services::{
        auth::AuthUser,
        connections::ConnectionService,
        data_jobs::Job,
        data_studio::{check_confirmation, close, load_table, DataStudioService},
        import::unique_keys,
        library::LibraryService,
        mock_gen::{self, Rng, RowContext},
        sql_builder::{quote_ident, Table},
        target_db,
    },
    state::AppState,
};

const BATCH_ROWS: usize = 1000;
/// Rows written before a self-referencing table can reference itself.
const SELF_REF_FIRST_BATCH: u64 = 10;
const SAMPLE_ROWS: u64 = 5;
/// Existing keys sampled from a parent that is not part of the run.
const MAX_EXISTING_KEYS: i64 = 10_000;
const PRODUCTION_REFUSED: &str = "Mock data is never generated on production connections. \
     An administrator can allow it with MOCK_DATA_ALLOW_PRODUCTION.";

/// Where the values of a foreign key come from.
#[derive(Debug, Clone)]
enum KeySource {
    /// Rows generated for another table of the run (index into the plan).
    Generated {
        parent: usize,
    },
    /// Rows generated earlier for the same table, plus existing ones.
    SelfRef {
        existing: Vec<Vec<String>>,
    },
    Existing(Vec<Vec<String>>),
    /// No parent rows: the (nullable) columns stay NULL.
    Null,
}

#[derive(Debug, Clone)]
struct ForeignKey {
    columns: Vec<usize>,
    parent: String,
    parent_columns: Vec<String>,
    nullable: bool,
    unique: bool,
    source: KeySource,
    /// Positions of `parent_columns` in the parent's returned columns.
    positions: Vec<usize>,
}

#[derive(Debug, Clone)]
struct ColumnPlan {
    generator: Generator,
    unique: bool,
    max_len: Option<usize>,
    note: Option<String>,
}

#[derive(Debug, Clone)]
struct TablePlan {
    table: Table,
    rows: u64,
    columns: Vec<ColumnPlan>,
    fks: Vec<ForeignKey>,
    /// Columns returned by inserts (keys needed by children).
    returning: Vec<String>,
    warnings: Vec<String>,
}

impl TablePlan {
    /// Columns given values (others use their defaults).
    fn insert_columns(&self) -> Vec<usize> {
        (0..self.columns.len())
            .filter(|&i| self.columns[i].generator != Generator::Default)
            .collect()
    }

    fn name(&self) -> String {
        self.table.qualified()
    }
}

struct Plan {
    seed: u64,
    run_tag: u64,
    tables: Vec<TablePlan>,
    missing_parents: Vec<String>,
}

#[derive(sqlx::FromRow)]
struct FkRow {
    columns: Vec<String>,
    parent_schema: String,
    parent_table: String,
    parent_columns: Vec<String>,
}

/// Chooses the key tuple of a foreign key (given by index) for a row.
type KeyPicker<'a> = dyn FnMut(usize, &ForeignKey, &mut Rng, u64) -> Option<Vec<String>> + 'a;

/// Values of one row for the insert columns. `pick` resolves foreign keys.
fn row_values(
    tp: &TablePlan,
    rng: &mut Rng,
    fk_rng: &mut Rng,
    ctx: &RowContext,
    pick: &mut KeyPicker,
) -> Vec<Option<String>> {
    let mut values: Vec<Option<String>> = vec![None; tp.columns.len()];
    for (i, c) in tp.columns.iter().enumerate() {
        if matches!(c.generator, Generator::Reference | Generator::Default) {
            continue;
        }
        values[i] = mock_gen::generate(&c.generator, rng, ctx).map(|v| {
            if c.unique && !mock_gen::is_unique(&c.generator) {
                mock_gen::uniquify(v, ctx, c.max_len)
            } else {
                mock_gen::truncate(v, c.max_len)
            }
        });
    }
    for (f, fk) in tp.fks.iter().enumerate() {
        let tuple = pick(f, fk, fk_rng, ctx.index);
        for (k, &col) in fk.columns.iter().enumerate() {
            if tp.columns[col].generator == Generator::Reference {
                values[col] = tuple.as_ref().map(|t| t[k].clone());
            }
        }
    }
    tp.insert_columns()
        .into_iter()
        .map(|i| values[i].take())
        .collect()
}

fn fk_rng(seed: u64, tp: &TablePlan) -> Rng {
    Rng::for_table(seed ^ 0x5eed_f00d, &tp.table.schema, &tp.table.name)
}

/// Picks a tuple from `pool`: in order for one-to-one keys, otherwise at
/// random. Nullable self-references are left NULL now and then.
fn choose(pool: &[Vec<String>], fk: &ForeignKey, rng: &mut Rng, index: u64) -> Option<Vec<String>> {
    if pool.is_empty() {
        return None;
    }
    if fk.unique {
        return pool.get(index as usize).cloned();
    }
    if matches!(fk.source, KeySource::SelfRef { .. }) && fk.nullable && rng.below(10) < 3 {
        return None;
    }
    Some(pool[rng.below(pool.len() as u64) as usize].clone())
}

fn insert_sql(tp: &TablePlan) -> String {
    let t = &tp.table;
    let cols = tp.insert_columns();
    let returning = if tp.returning.is_empty() {
        "1".to_string()
    } else {
        tp.returning
            .iter()
            .map(|c| format!("CAST({} AS text)", quote_ident(c)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    if cols.is_empty() {
        return format!(
            "INSERT INTO {} SELECT FROM generate_series(1, $1::int) ON CONFLICT DO NOTHING RETURNING {returning}",
            t.qualified()
        );
    }
    let overriding = if cols.iter().any(|&i| t.columns[i].identity == Some('a')) {
        " OVERRIDING SYSTEM VALUE"
    } else {
        ""
    };
    let names: Vec<String> = cols
        .iter()
        .map(|&i| quote_ident(&t.columns[i].name))
        .collect();
    let casts: Vec<String> = cols
        .iter()
        .enumerate()
        .map(|(k, &i)| format!("CAST(u.c{k} AS {})", t.columns[i].cast_type))
        .collect();
    let arrays: Vec<String> = (1..=cols.len()).map(|p| format!("${p}::text[]")).collect();
    let aliases: Vec<String> = (0..cols.len()).map(|k| format!("c{k}")).collect();
    format!(
        "INSERT INTO {} ({}){overriding} SELECT {} FROM unnest({}) AS u({}) ON CONFLICT DO NOTHING RETURNING {returning}",
        t.qualified(),
        names.join(", "),
        casts.join(", "),
        arrays.join(", "),
        aliases.join(", ")
    )
}

/// Orders tables so parents come before children. Self-references do not
/// count; any other cycle is an error naming the tables involved.
fn topological_order(names: &[String], edges: &[(usize, usize)]) -> Result<Vec<usize>, String> {
    let n = names.len();
    let mut indegree = vec![0usize; n];
    for &(parent, child) in edges {
        if parent != child {
            indegree[child] += 1;
        }
    }
    let mut order = Vec::with_capacity(n);
    let mut done = vec![false; n];
    while order.len() < n {
        let Some(next) = (0..n).find(|&i| !done[i] && indegree[i] == 0) else {
            let cycle: Vec<&str> = (0..n)
                .filter(|&i| !done[i])
                .map(|i| names[i].as_str())
                .collect();
            return Err(format!(
                "{} reference each other in a cycle. Fill them in separate runs, starting with a \
                 table whose foreign key is nullable, or remove one of them.",
                cycle.join(", ")
            ));
        };
        done[next] = true;
        order.push(next);
        for &(parent, child) in edges {
            if parent == next && child != next {
                indegree[child] -= 1;
            }
        }
    }
    Ok(order)
}

pub struct MockDataService<'a> {
    state: &'a AppState,
}

impl<'a> MockDataService<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    fn production_blocked(&self, record: &ConnectionRecord) -> bool {
        record.environment() == Environment::Production
            && !self.state.config.mock_data_allow_production
    }

    pub async fn preview(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        req: &MockRequest,
    ) -> AppResult<MockPreview> {
        self.state.query_limiter.check(&user_id.to_string()).await?;
        let record = ConnectionService::new(self.state)
            .get_record(user_id, connection_id)
            .await?;
        let mut conn = DataStudioService::new(self.state)
            .connect(&record, true)
            .await?;
        let seed = req.seed.unwrap_or_else(rand_seed);
        let plan = build_plan(&mut conn, req, seed).await;
        close(conn).await;
        let plan = plan?;
        Ok(MockPreview {
            seed,
            total_rows: plan.tables.iter().map(|t| t.rows).sum(),
            tables: plan
                .tables
                .iter()
                .map(|tp| table_preview(tp, &plan))
                .collect(),
            missing_parents: plan.missing_parents,
            production_blocked: self.production_blocked(&record),
        })
    }

    pub async fn run(
        &self,
        user: &AuthUser,
        connection_id: Uuid,
        req: &MockRequest,
    ) -> AppResult<JobView> {
        self.state
            .query_limiter
            .check(&user.user_id.to_string())
            .await?;
        let record = ConnectionService::new(self.state)
            .require(user.user_id, connection_id, Access::Write)
            .await?
            .record;
        if self.production_blocked(&record) {
            return Err(AppError::Forbidden(PRODUCTION_REFUSED.into()));
        }
        if record.read_only {
            return Err(AppError::Forbidden(
                "This connection is read-only. Enable read-write mode in the connection settings to generate data."
                    .into(),
            ));
        }
        check_confirmation(&record, req.confirm.as_deref())?;
        let mut conn = DataStudioService::new(self.state)
            .connect(&record, false)
            .await?;
        let seed = req.seed.unwrap_or_else(rand_seed);
        let plan = match build_plan(&mut conn, req, seed).await {
            Ok(plan) if plan.missing_parents.is_empty() => plan,
            Ok(plan) => {
                close(conn).await;
                return Err(AppError::validation(
                    "tables",
                    format!(
                        "add {} to the run: they have no rows to reference",
                        plan.missing_parents.join(", ")
                    ),
                ));
            }
            Err(err) => {
                close(conn).await;
                return Err(err);
            }
        };
        for tp in &plan.tables {
            if !tp.table.can_insert && tp.rows > 0 {
                close(conn).await;
                return Err(AppError::Forbidden(format!(
                    "The connection's role does not have INSERT privilege on {}.",
                    tp.name()
                )));
            }
        }

        let id = Uuid::new_v4();
        let mut view = JobView::new(id, JobKind::MockData, connection_id, JobStatus::Ready);
        view.rows_total = Some(plan.tables.iter().map(|t| t.rows).sum());
        view.tables = plan
            .tables
            .iter()
            .map(|tp| TableCount {
                table: tp.name(),
                requested: tp.rows,
                inserted: 0,
            })
            .collect();
        let target = plan
            .tables
            .iter()
            .map(TablePlan::name)
            .collect::<Vec<_>>()
            .join(", ");
        let job = match self
            .state
            .jobs
            .create(user.user_id, connection_id, view, None)
            .and_then(|job| job.start(Some(target)).map(|v| (job, v)))
        {
            Ok(j) => j,
            Err(err) => {
                close(conn).await;
                return Err(err);
            }
        };
        let (job, view) = job;
        if let Err(err) = repo::insert_running(&self.state.db, user.user_id, &view).await {
            tracing::warn!(error = %err, "failed to record data job");
        }
        let state = self.state.clone();
        let user_id = user.user_id;
        crate::services::audit::spawn_in_request(async move {
            run_job(state, job, conn, plan, user_id).await
        });
        Ok(view)
    }
}

/// A random seed small enough for JavaScript clients to send back exactly.
fn rand_seed() -> u64 {
    let id = Uuid::new_v4();
    let b = id.as_bytes();
    u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]) & MAX_SAFE_SEED
}

const MAX_SAFE_SEED: u64 = (1 << 53) - 1;

fn table_preview(tp: &TablePlan, plan: &Plan) -> MockTablePlan {
    let mut rng = Rng::for_table(plan.seed, &tp.table.schema, &tp.table.name);
    let mut frng = fk_rng(plan.seed, tp);
    let cols = tp.insert_columns();
    let sample = (0..SAMPLE_ROWS.min(tp.rows.max(1)))
        .map(|index| {
            let ctx = RowContext {
                index,
                run_tag: plan.run_tag,
            };
            let values = row_values(tp, &mut rng, &mut frng, &ctx, &mut |_, fk, r, i| match &fk
                .source
            {
                KeySource::Generated { parent } => Some(
                    fk.columns
                        .iter()
                        .map(|_| format!("→ {}", plan.tables[*parent].name()))
                        .collect(),
                ),
                KeySource::SelfRef { existing } if existing.is_empty() => Some(
                    fk.columns
                        .iter()
                        .map(|_| "→ this table".to_string())
                        .collect(),
                ),
                KeySource::SelfRef { existing } | KeySource::Existing(existing) => {
                    choose(existing, fk, r, i)
                }
                KeySource::Null => None,
            });
            // Full width: NULL for columns left to their defaults.
            let mut row = vec![None; tp.columns.len()];
            for (k, i) in cols.iter().enumerate() {
                row[*i] = values[k].clone();
            }
            row
        })
        .collect();
    MockTablePlan {
        schema: tp.table.schema.clone(),
        table: tp.table.name.clone(),
        rows: tp.rows,
        columns: tp
            .table
            .columns
            .iter()
            .zip(&tp.columns)
            .map(|(c, p)| MockColumn {
                name: c.name.clone(),
                data_type: c.data_type.clone(),
                nullable: c.nullable,
                generator: p.generator.clone(),
                references: tp
                    .fks
                    .iter()
                    .find(|fk| {
                        fk.columns
                            .iter()
                            .any(|&i| tp.table.columns[i].name == c.name)
                    })
                    .map(|fk| fk.parent.clone()),
                note: p.note.clone(),
            })
            .collect(),
        sample,
        warnings: tp.warnings.clone(),
    }
}

async fn existing_keys(
    conn: &mut PgConnection,
    schema: &str,
    table: &str,
    columns: &[String],
) -> AppResult<Vec<Vec<String>>> {
    let select: Vec<String> = columns
        .iter()
        .map(|c| format!("CAST({} AS text)", quote_ident(c)))
        .collect();
    let not_null: Vec<String> = columns
        .iter()
        .map(|c| format!("{} IS NOT NULL", quote_ident(c)))
        .collect();
    let rows = sqlx::query(&format!(
        "SELECT {} FROM {}.{} WHERE {} LIMIT {MAX_EXISTING_KEYS}",
        select.join(", "),
        quote_ident(schema),
        quote_ident(table),
        not_null.join(" AND ")
    ))
    .fetch_all(&mut *conn)
    .await
    .map_err(target_db::query_error)?;
    rows.iter()
        .map(|r| {
            (0..columns.len())
                .map(|i| r.try_get::<String, _>(i))
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<_, _>>()
        .map_err(target_db::query_error)
}

async fn build_plan(conn: &mut PgConnection, req: &MockRequest, seed: u64) -> AppResult<Plan> {
    let mut tables = Vec::with_capacity(req.tables.len());
    let mut fk_rows = Vec::with_capacity(req.tables.len());
    let mut checks = Vec::with_capacity(req.tables.len());
    let mut uniques = Vec::with_capacity(req.tables.len());
    for t in &req.tables {
        let (table, oid) = load_table(conn, &t.schema, &t.table).await?;
        if !matches!(table.kind.as_str(), "table" | "partitioned_table") {
            return Err(AppError::validation(
                "tables",
                format!("{} is not a table", table.qualified()),
            ));
        }
        let fks: Vec<FkRow> = sqlx::query_as(
            "SELECT ARRAY(SELECT a.attname::text FROM unnest(c.conkey) WITH ORDINALITY k(n, o) \
                          JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = k.n ORDER BY k.o) AS columns, \
                    pn.nspname::text AS parent_schema, pc.relname::text AS parent_table, \
                    ARRAY(SELECT a.attname::text FROM unnest(c.confkey) WITH ORDINALITY k(n, o) \
                          JOIN pg_attribute a ON a.attrelid = c.confrelid AND a.attnum = k.n ORDER BY k.o) AS parent_columns \
             FROM pg_constraint c JOIN pg_class pc ON pc.oid = c.confrelid \
             JOIN pg_namespace pn ON pn.oid = pc.relnamespace \
             WHERE c.conrelid = $1::bigint::oid AND c.contype = 'f' ORDER BY c.conname",
        )
        .bind(oid)
        .fetch_all(&mut *conn)
        .await
        .map_err(target_db::query_error)?;
        let defs: Vec<String> = sqlx::query_scalar(
            "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
             WHERE conrelid = $1::bigint::oid AND contype = 'c' ORDER BY conname",
        )
        .bind(oid)
        .fetch_all(&mut *conn)
        .await
        .map_err(target_db::query_error)?;
        uniques.push(unique_keys(conn, oid).await?);
        tables.push(table);
        fk_rows.push(fks);
        checks.push(defs);
    }

    let names: Vec<String> = tables.iter().map(Table::qualified).collect();
    let index_of = |schema: &str, table: &str| {
        tables
            .iter()
            .position(|t| t.schema == schema && t.name == table)
    };
    let mut edges = Vec::new();
    for (child, fks) in fk_rows.iter().enumerate() {
        for fk in fks {
            if let Some(parent) = index_of(&fk.parent_schema, &fk.parent_table) {
                if req.tables[parent].rows > 0 {
                    edges.push((parent, child));
                }
            }
        }
    }
    let order = topological_order(&names, &edges).map_err(|m| AppError::validation("tables", m))?;

    let mut errors: Vec<FieldError> = Vec::new();
    let mut missing_parents: Vec<String> = Vec::new();
    let mut plans: Vec<Option<TablePlan>> = (0..tables.len()).map(|_| None).collect();
    for &ti in &order {
        let table = tables[ti].clone();
        let request = &req.tables[ti];
        let mut warnings = Vec::new();
        let column_names: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
        let single_unique: HashSet<&str> = uniques[ti]
            .iter()
            .filter(|k| k.len() == 1)
            .map(|k| k[0].as_str())
            .collect();

        // Foreign keys.
        let mut fks = Vec::new();
        for row in &fk_rows[ti] {
            let columns: Vec<usize> = row
                .columns
                .iter()
                .filter_map(|n| table.columns.iter().position(|c| &c.name == n))
                .collect();
            let nullable = columns.iter().all(|&i| table.columns[i].nullable);
            let mut sorted = row.columns.clone();
            sorted.sort();
            let unique = uniques[ti].iter().any(|k| {
                let mut k = k.clone();
                k.sort();
                k == sorted
            });
            let parent_name = format!(
                "{}.{}",
                quote_ident(&row.parent_schema),
                quote_ident(&row.parent_table)
            );
            let parent_index = index_of(&row.parent_schema, &row.parent_table);
            let source = match parent_index {
                Some(p) if p == ti => KeySource::SelfRef {
                    existing: existing_keys(
                        conn,
                        &row.parent_schema,
                        &row.parent_table,
                        &row.parent_columns,
                    )
                    .await?,
                },
                Some(p) if req.tables[p].rows > 0 => KeySource::Generated { parent: p },
                _ => {
                    let keys = existing_keys(
                        conn,
                        &row.parent_schema,
                        &row.parent_table,
                        &row.parent_columns,
                    )
                    .await?;
                    if !keys.is_empty() {
                        KeySource::Existing(keys)
                    } else if nullable {
                        warnings.push(format!(
                            "{parent_name} has no rows, so {} stays NULL",
                            row.columns.join(", ")
                        ));
                        KeySource::Null
                    } else {
                        if !missing_parents.contains(&parent_name) {
                            missing_parents.push(parent_name.clone());
                        }
                        KeySource::Null
                    }
                }
            };
            if let KeySource::SelfRef { existing } = &source {
                if existing.is_empty() && !nullable && request.rows > 0 {
                    errors.push(FieldError {
                        field: "tables".into(),
                        message: format!(
                            "{} references itself through NOT NULL columns and has no rows to start from",
                            table.qualified()
                        ),
                    });
                }
            }
            fks.push(ForeignKey {
                columns,
                parent: parent_name,
                parent_columns: row.parent_columns.clone(),
                nullable,
                unique,
                source,
                positions: Vec::new(),
            });
        }
        let fk_columns: HashSet<usize> =
            fks.iter().flat_map(|f| f.columns.iter().copied()).collect();

        // Columns.
        let mut columns = Vec::with_capacity(table.columns.len());
        for (i, c) in table.columns.iter().enumerate() {
            let unique = single_unique.contains(c.name.as_str());
            let (default, note) = if fk_columns.contains(&i) {
                (Generator::Reference, None)
            } else {
                mock_gen::default_generator(&table.name, c, unique)
            };
            let chosen = request.columns.get(&c.name).cloned();
            let overridden = chosen.is_some();
            let mut generator = chosen.unwrap_or(default);
            let field = format!("tables.{}.columns.{}", table.name, c.name);
            if generator == Generator::Reference && !fk_columns.contains(&i) {
                errors.push(FieldError {
                    field: field.clone(),
                    message: format!("{} is not a foreign key column", c.name),
                });
            }
            if c.generated && generator != Generator::Default {
                errors.push(FieldError {
                    field: field.clone(),
                    message: format!("{} is a generated column", c.name),
                });
            }
            // CHECK constraints that mention this column.
            for def in checks[ti].iter().filter(|d| d.contains(c.name.as_str())) {
                match mock_gen::parse_check(def, &c.name, &column_names) {
                    Some(rules) => {
                        let values_only = rules.values.is_some();
                        if !(overridden && values_only) {
                            generator = mock_gen::apply_rules(generator, &rules);
                        }
                    }
                    None => {
                        let mentions_column = def.contains(&format!("{} ", c.name))
                            || def.contains(&format!("({}", c.name));
                        if mentions_column
                            && !warnings.iter().any(|w: &String| w.contains(def.as_str()))
                        {
                            warnings.push(format!(
                                "{def} is not checked by the generator and may reject rows"
                            ));
                        }
                    }
                }
            }
            if unique {
                if let Generator::Integer { min, .. } = generator {
                    generator = Generator::Sequence { start: Some(min) };
                }
            }
            if let Generator::Sequence { start } = &mut generator {
                if c.kind() == "number" {
                    let max: Option<i64> = sqlx::query_scalar(&format!(
                        "SELECT CAST(max({}) AS bigint) FROM {}",
                        quote_ident(&c.name),
                        table.qualified()
                    ))
                    .fetch_one(&mut *conn)
                    .await
                    .map_err(target_db::query_error)?;
                    let next = max.map_or(1, |m| m.saturating_add(1));
                    *start = Some(start.map_or(next, |s| s.max(next)));
                }
            }
            if !c.nullable && generator == Generator::Null && request.rows > 0 {
                errors.push(FieldError {
                    field,
                    message: format!(
                        "{} is NOT NULL and has no generator for {}; choose a constant value",
                        c.name, c.data_type
                    ),
                });
            }
            let max_len = (c.kind() == "text")
                .then(|| {
                    mock_gen::type_modifiers(&c.data_type)
                        .first()
                        .map(|&n| n as usize)
                })
                .flatten();
            columns.push(ColumnPlan {
                generator,
                unique,
                max_len,
                note,
            });
        }
        if !table.can_insert {
            warnings.push(format!(
                "the connection's role cannot insert into {}",
                table.qualified()
            ));
        }
        plans[ti] = Some(TablePlan {
            table,
            rows: request.rows,
            columns,
            fks,
            returning: Vec::new(),
            warnings,
        });
    }
    if !errors.is_empty() {
        return Err(AppError::Validation(errors));
    }

    let mut ordered: Vec<TablePlan> = Vec::with_capacity(order.len());
    let mut position = vec![0usize; order.len()];
    for (k, &ti) in order.iter().enumerate() {
        position[ti] = k;
    }
    for &ti in &order {
        ordered.push(plans[ti].take().expect("planned"));
    }
    // Generated parents now refer to positions in the ordered list; record
    // the columns each parent must return.
    for k in 0..ordered.len() {
        for f in 0..ordered[k].fks.len() {
            let parent = match ordered[k].fks[f].source {
                KeySource::Generated { parent } => position[parent],
                KeySource::SelfRef { .. } => k,
                _ => continue,
            };
            if let KeySource::Generated { parent: p } = &mut ordered[k].fks[f].source {
                *p = parent;
            }
            let wanted = ordered[k].fks[f].parent_columns.clone();
            let mut positions = Vec::new();
            for c in wanted {
                let ret = &mut ordered[parent].returning;
                let pos = match ret.iter().position(|r| *r == c) {
                    Some(p) => p,
                    None => {
                        ret.push(c);
                        ret.len() - 1
                    }
                };
                positions.push(pos);
            }
            ordered[k].fks[f].positions = positions;
        }
    }
    Ok(Plan {
        seed,
        run_tag: seed % 1000,
        tables: ordered,
        missing_parents,
    })
}

async fn run_job(
    state: AppState,
    job: Arc<Job>,
    mut conn: PgConnection,
    plan: Plan,
    user_id: Uuid,
) {
    let started = Instant::now();
    let result = generate_all(&state, &job, &mut conn, &plan).await;
    let (status, error) = match result {
        Ok(true) => {
            close(conn).await;
            (JobStatus::Completed, None)
        }
        Ok(false) => {
            drop(conn);
            (
                JobStatus::Cancelled,
                Some("the run was cancelled".to_string()),
            )
        }
        Err(message) => {
            drop(conn);
            (JobStatus::Failed, Some(message))
        }
    };
    if status != JobStatus::Completed {
        job.update(|v| {
            v.committed = false;
            v.rows_inserted = 0;
            for t in &mut v.tables {
                t.inserted = 0;
            }
        });
    }
    let view = job.finish(&state.db, status, error).await;
    let summary = format!(
        "-- Mock data (seed {}): {}{}",
        plan.seed,
        view.tables
            .iter()
            .map(|t| format!("{} {} rows", t.table, t.inserted))
            .collect::<Vec<_>>()
            .join(", "),
        if view.committed {
            ""
        } else {
            " (nothing committed)"
        }
    );
    LibraryService::new(&state)
        .record_script(
            user_id,
            job.connection_id,
            HistoryKind::MockData,
            &summary,
            started.elapsed().as_millis() as u64,
            view.committed.then_some(view.rows_inserted as i64),
            view.error.as_deref(),
        )
        .await;
    tracing::info!(%user_id, job_id = %job.id, status = status.as_str(), rows = view.rows_inserted, "mock data run finished");
}

/// Writes every table; `Ok(false)` when cancelled.
async fn generate_all(
    state: &AppState,
    job: &Job,
    conn: &mut PgConnection,
    plan: &Plan,
) -> Result<bool, String> {
    let sql_error = |e: sqlx::Error| target_db::query_error(e).to_string();
    conn.execute("BEGIN").await.map_err(sql_error)?;
    target_db::set_local_timeouts(conn, state.config.query_max_timeout)
        .await
        .map_err(|e| e.to_string())?;
    // Keys returned for each table, by position in the plan.
    let mut generated: Vec<Vec<Vec<String>>> = vec![Vec::new(); plan.tables.len()];
    for (k, tp) in plan.tables.iter().enumerate() {
        if tp.rows == 0 {
            continue;
        }
        let name = tp.name();
        job.update(|v| v.phase = Some(format!("Writing {name}")));
        let sql = insert_sql(tp);
        let cols = tp.insert_columns();
        let mut rng = Rng::for_table(plan.seed, &tp.table.schema, &tp.table.name);
        let mut frng = fk_rng(plan.seed, tp);
        // Self-references can only point at rows already written, so such
        // tables start with small batches that double: the first rows are
        // roots and later rows reference earlier ones.
        let self_ref = tp
            .fks
            .iter()
            .any(|fk| matches!(fk.source, KeySource::SelfRef { .. }));
        let mut index = 0u64;
        while index < tp.rows {
            if job.is_cancelled() {
                return Ok(false);
            }
            let batch = if self_ref {
                index.clamp(SELF_REF_FIRST_BATCH, BATCH_ROWS as u64)
            } else {
                BATCH_ROWS as u64
            };
            let n = (tp.rows - index).min(batch);
            // Key pools for this batch: parents are complete; a table's own
            // keys grow batch by batch.
            let pools: Vec<Vec<Vec<String>>> = tp
                .fks
                .iter()
                .map(|fk| {
                    let extract = |rows: &[Vec<String>]| -> Vec<Vec<String>> {
                        rows.iter()
                            .map(|row| fk.positions.iter().map(|&p| row[p].clone()).collect())
                            .collect()
                    };
                    match &fk.source {
                        KeySource::Generated { parent } => extract(&generated[*parent]),
                        KeySource::SelfRef { existing } => {
                            let mut pool = existing.clone();
                            pool.extend(extract(&generated[k]));
                            pool
                        }
                        KeySource::Existing(keys) => keys.clone(),
                        KeySource::Null => Vec::new(),
                    }
                })
                .collect();
            let mut arrays: Vec<Vec<Option<String>>> =
                vec![Vec::with_capacity(n as usize); cols.len()];
            for _ in 0..n {
                let ctx = RowContext {
                    index,
                    run_tag: plan.run_tag,
                };
                let values = row_values(tp, &mut rng, &mut frng, &ctx, &mut |f, fk, r, i| {
                    choose(&pools[f], fk, r, i)
                });
                for (a, v) in arrays.iter_mut().zip(values) {
                    a.push(v);
                }
                index += 1;
            }
            let mut q = sqlx::query(&sql);
            if cols.is_empty() {
                q = q.bind(n as i32);
            } else {
                for a in arrays {
                    q = q.bind(a);
                }
            }
            let rows = q
                .fetch_all(&mut *conn)
                .await
                .map_err(|e| format!("{name}: {}", sql_error(e)))?;
            let inserted = rows.len() as u64;
            if !tp.returning.is_empty() {
                for row in &rows {
                    let values = (0..tp.returning.len())
                        .map(|i| {
                            row.try_get::<Option<String>, _>(i)
                                .ok()
                                .flatten()
                                .unwrap_or_default()
                        })
                        .collect();
                    generated[k].push(values);
                }
            }
            job.update(|v| {
                v.rows_processed += n;
                v.rows_inserted += inserted;
                v.rows_skipped += n - inserted;
                if let Some(t) = v.tables.get_mut(k) {
                    t.inserted += inserted;
                }
            });
        }
    }
    job.update(|v| v.phase = Some("Committing".into()));
    conn.execute("COMMIT").await.map_err(sql_error)?;
    job.update(|v| v.committed = true);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::sql_builder::Column;

    fn col(name: &str, data_type: &str, base: &str, category: char) -> Column {
        Column {
            name: name.into(),
            data_type: data_type.into(),
            cast_type: data_type.into(),
            base_type: base.into(),
            category,
            nullable: false,
            has_default: false,
            identity: None,
            generated: false,
            enum_values: vec![],
        }
    }

    #[test]
    fn tables_are_ordered_parents_first() {
        let names: Vec<String> = ["orders", "customers", "employees"]
            .map(String::from)
            .to_vec();
        // customers → orders, employees → orders, employees → employees (self)
        let order = topological_order(&names, &[(1, 0), (2, 0), (2, 2)]).unwrap();
        assert_eq!(order, [1, 2, 0]);
        let err = topological_order(&names, &[(0, 1), (1, 0)]).unwrap_err();
        assert!(
            err.starts_with("orders, customers reference each other"),
            "{err}"
        );
    }

    fn plan_for(columns: Vec<(Column, Generator)>, returning: Vec<String>) -> TablePlan {
        let (cols, gens): (Vec<_>, Vec<_>) = columns.into_iter().unzip();
        TablePlan {
            table: Table {
                schema: "sales".into(),
                name: "orders".into(),
                kind: "table".into(),
                columns: cols,
                key: vec![0],
                key_kind: None,
                can_insert: true,
                can_update: true,
                can_delete: true,
            },
            rows: 10,
            columns: gens
                .into_iter()
                .map(|generator| ColumnPlan {
                    generator,
                    unique: false,
                    max_len: None,
                    note: None,
                })
                .collect(),
            fks: Vec::new(),
            returning,
            warnings: Vec::new(),
        }
    }

    #[test]
    fn insert_statements_use_unnest_and_skip_conflicts() {
        let mut id = col("id", "bigint", "int8", 'N');
        id.identity = Some('a');
        let tp = plan_for(
            vec![
                (id.clone(), Generator::Default),
                (col("status", "text", "text", 'S'), Generator::Word),
                (
                    col("total", "numeric", "numeric", 'N'),
                    Generator::Decimal {
                        min: 1.0,
                        max: 2.0,
                        scale: 2,
                    },
                ),
            ],
            vec!["id".into()],
        );
        assert_eq!(
            insert_sql(&tp),
            "INSERT INTO sales.orders (status, total) SELECT CAST(u.c0 AS text), CAST(u.c1 AS numeric) \
             FROM unnest($1::text[], $2::text[]) AS u(c0, c1) ON CONFLICT DO NOTHING RETURNING CAST(id AS text)"
        );
        let tp = plan_for(
            vec![(id.clone(), Generator::Sequence { start: Some(5) })],
            vec![],
        );
        assert!(insert_sql(&tp).contains("(id) OVERRIDING SYSTEM VALUE SELECT"));
        let tp = plan_for(vec![(id, Generator::Default)], vec![]);
        assert_eq!(
            insert_sql(&tp),
            "INSERT INTO sales.orders SELECT FROM generate_series(1, $1::int) ON CONFLICT DO NOTHING RETURNING 1"
        );
    }

    #[test]
    fn rows_fill_foreign_keys_from_the_pool() {
        let mut tp = plan_for(
            vec![
                (col("id", "integer", "int4", 'N'), Generator::Default),
                (
                    col("customer_id", "uuid", "uuid", 'U'),
                    Generator::Reference,
                ),
                (
                    col("status", "text", "text", 'S'),
                    Generator::Values {
                        values: vec!["paid".into()],
                    },
                ),
            ],
            vec![],
        );
        tp.fks.push(ForeignKey {
            columns: vec![1],
            parent: "sales.customers".into(),
            parent_columns: vec!["id".into()],
            nullable: false,
            unique: false,
            source: KeySource::Existing(vec![vec!["c1".into()], vec!["c2".into()]]),
            positions: vec![0],
        });
        let mut rng = Rng::new(1);
        let mut frng = Rng::new(2);
        for index in 0..20 {
            let ctx = RowContext { index, run_tag: 0 };
            let values = row_values(&tp, &mut rng, &mut frng, &ctx, &mut |_, fk, r, i| match &fk
                .source
            {
                KeySource::Existing(keys) => choose(keys, fk, r, i),
                _ => None,
            });
            assert_eq!(values.len(), 2);
            assert!(matches!(values[0].as_deref(), Some("c1" | "c2")));
            assert_eq!(values[1].as_deref(), Some("paid"));
        }
        // One-to-one keys are assigned in order and run out.
        tp.fks[0].unique = true;
        let fk = &tp.fks[0];
        let pool = vec![vec!["a".to_string()], vec!["b".to_string()]];
        assert_eq!(choose(&pool, fk, &mut frng, 1), Some(vec!["b".to_string()]));
        assert_eq!(choose(&pool, fk, &mut frng, 2), None);
    }
}
