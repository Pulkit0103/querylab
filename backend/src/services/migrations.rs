//! Migration projects and scripts, per-database status, and applying or
//! rolling back scripts.
//!
//! The applied state of a database is recorded in that database, in
//! `querylab.schema_migrations`, so it travels with the database. Applies
//! run in one transaction holding a transaction-level advisory lock: a
//! second apply on the same database fails fast instead of interleaving.

use std::time::Instant;

use chrono::Utc;
use sqlx::{Connection, Executor, PgConnection};
use uuid::Uuid;

use crate::{
    database::{
        migrations::{self as repo, NewMigration, NewRun},
        users,
    },
    errors::{AppError, AppResult},
    models::{
        connection::ConnectionRecord,
        library::HistoryKind,
        migrations::{
            apply_blocker, checksum, merge_status, rollback_blocker, timestamp_version, AppliedRow,
            CreateMigrationRequest, Direction, Migration, MigrationRun, Project, RunOutcome,
            StatusResponse, UpdateMigrationRequest,
        },
        workspaces::Access,
    },
    services::{
        connections::ConnectionService,
        data_studio::check_confirmation,
        library::LibraryService,
        schema::close,
        target_db::{self, TargetSpec},
    },
    state::AppState,
};

/// Advisory lock key: `hashtext('querylab_migrations')`, computed by the server.
const LOCK_SQL: &str = "SELECT pg_try_advisory_xact_lock(hashtext('querylab_migrations'))";

const TRACKING_DDL: &str = "CREATE SCHEMA IF NOT EXISTS querylab; \
     CREATE TABLE IF NOT EXISTS querylab.schema_migrations ( \
         project     text NOT NULL, \
         version     bigint NOT NULL, \
         name        text NOT NULL, \
         checksum    text NOT NULL, \
         applied_at  timestamptz NOT NULL DEFAULT now(), \
         applied_by  text NOT NULL, \
         duration_ms bigint NOT NULL, \
         PRIMARY KEY (project, version))";

const APPLIED_SQL: &str = "SELECT version, name, checksum, applied_at, applied_by, duration_ms \
     FROM querylab.schema_migrations WHERE project = $1 ORDER BY version";

const READ_ONLY_CONNECTION: &str = "This connection is read-only. Enable read-write mode in the \
     connection settings to apply migrations.";

fn unique_violation(e: sqlx::Error, what: &str) -> AppError {
    match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            AppError::Conflict(format!("{what} already exists"))
        }
        _ => e.into(),
    }
}

pub struct MigrationService<'a> {
    state: &'a AppState,
}

impl<'a> MigrationService<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    // -- projects ---------------------------------------------------------------

    pub async fn projects(&self, user_id: Uuid) -> AppResult<Vec<Project>> {
        Ok(repo::list_projects(&self.state.db, user_id).await?)
    }

    pub async fn project(&self, user_id: Uuid, id: Uuid) -> AppResult<Project> {
        repo::get_project(&self.state.db, user_id, id)
            .await?
            .ok_or(AppError::NotFound("migration project"))
    }

    pub async fn create_project(&self, user_id: Uuid, name: &str) -> AppResult<Project> {
        let id = repo::insert_project(&self.state.db, user_id, name.trim())
            .await
            .map_err(|e| unique_violation(e, "a project with this name"))?;
        self.project(user_id, id).await
    }

    /// Deletes a project and its scripts. Tracking rows in databases stay.
    pub async fn delete_project(&self, user_id: Uuid, id: Uuid) -> AppResult<()> {
        if repo::delete_project(&self.state.db, user_id, id).await? {
            Ok(())
        } else {
            Err(AppError::NotFound("migration project"))
        }
    }

    // -- scripts ----------------------------------------------------------------

    pub async fn migrations(&self, user_id: Uuid, project_id: Uuid) -> AppResult<Vec<Migration>> {
        self.project(user_id, project_id).await?;
        Ok(repo::list_migrations(&self.state.db, user_id, project_id).await?)
    }

    pub async fn migration(&self, user_id: Uuid, id: Uuid) -> AppResult<Migration> {
        repo::get_migration(&self.state.db, user_id, id)
            .await?
            .ok_or(AppError::NotFound("migration"))
    }

    pub async fn create_migration(
        &self,
        user_id: Uuid,
        project_id: Uuid,
        req: &CreateMigrationRequest,
    ) -> AppResult<Migration> {
        self.project(user_id, project_id).await?;
        let version = req.version.unwrap_or_else(|| timestamp_version(Utc::now()));
        let id = repo::insert_migration(
            &self.state.db,
            &NewMigration {
                user_id,
                project_id,
                version,
                name: req.name.trim(),
                up_sql: &req.up_sql,
                down_sql: req.down_sql.as_deref().filter(|d| !d.trim().is_empty()),
                checksum: &checksum(&req.up_sql),
            },
        )
        .await
        .map_err(|e| unique_violation(e, &format!("version {version}")))?;
        self.migration(user_id, id).await
    }

    pub async fn update_migration(
        &self,
        user_id: Uuid,
        id: Uuid,
        req: &UpdateMigrationRequest,
    ) -> AppResult<Migration> {
        let current = self.migration(user_id, id).await?;
        if current.locked {
            return Err(AppError::Conflict(
                "this migration has been applied; write a new migration instead of editing it"
                    .into(),
            ));
        }
        let up_sql = req.up_sql.as_deref().unwrap_or(&current.up_sql);
        let down_sql = match &req.down_sql {
            Some(d) => d.as_deref(),
            None => current.down_sql.as_deref(),
        }
        .filter(|d| !d.trim().is_empty());
        let version = req.version.unwrap_or(current.version);
        repo::update_migration(
            &self.state.db,
            user_id,
            id,
            &NewMigration {
                user_id,
                project_id: current.project_id,
                version,
                name: req.name.as_deref().map(str::trim).unwrap_or(&current.name),
                up_sql,
                down_sql,
                checksum: &checksum(up_sql),
            },
        )
        .await
        .map_err(|e| unique_violation(e, &format!("version {version}")))?;
        self.migration(user_id, id).await
    }

    pub async fn delete_migration(&self, user_id: Uuid, id: Uuid) -> AppResult<()> {
        let current = self.migration(user_id, id).await?;
        if current.locked {
            return Err(AppError::Conflict(
                "this migration has been applied and its history is kept; roll it back first"
                    .into(),
            ));
        }
        repo::delete_migration(&self.state.db, user_id, id).await?;
        Ok(())
    }

    pub async fn runs(
        &self,
        user_id: Uuid,
        project_id: Option<Uuid>,
        connection_id: Option<Uuid>,
    ) -> AppResult<Vec<MigrationRun>> {
        Ok(repo::list_runs(&self.state.db, user_id, project_id, connection_id).await?)
    }

    // -- target databases ---------------------------------------------------------

    async fn connect(&self, record: &ConnectionRecord, read_only: bool) -> AppResult<PgConnection> {
        self.state
            .connect_limiter
            .check(&record.user_id.to_string())?;
        let service = ConnectionService::new(self.state);
        let secret = service.decrypt_secret(record)?;
        let mut spec = TargetSpec::from_record(record, &secret);
        spec.read_only = read_only || record.read_only;
        Ok(target_db::connect(&self.state.config, &spec).await?)
    }

    /// Applied, pending and drifted scripts of a project on one database.
    pub async fn status(
        &self,
        user_id: Uuid,
        project_id: Uuid,
        connection_id: Uuid,
    ) -> AppResult<StatusResponse> {
        let project = self.project(user_id, project_id).await?;
        let migrations = repo::list_migrations(&self.state.db, user_id, project_id).await?;
        let record = ConnectionService::new(self.state)
            .get_record(user_id, connection_id)
            .await?;
        let mut conn = self.connect(&record, true).await?;
        let read = async {
            let exists: bool = sqlx::query_scalar(
                "SELECT to_regclass('querylab.schema_migrations') IS NOT NULL",
            )
            .fetch_one(&mut conn)
            .await
            .map_err(target_db::query_error)?;
            if !exists {
                return Ok::<_, AppError>((false, Vec::new(), None));
            }
            match sqlx::query_as::<_, AppliedRow>(APPLIED_SQL)
                .bind(&project.name)
                .fetch_all(&mut conn)
                .await
            {
                Ok(rows) => Ok((true, rows, None)),
                Err(sqlx::Error::Database(db)) if db.code().as_deref() == Some("42501") => Ok((
                    true,
                    Vec::new(),
                    Some(
                        "The connected role cannot read querylab.schema_migrations; grant it SELECT to see the applied state."
                            .to_string(),
                    ),
                )),
                Err(e) => Err(target_db::query_error(e)),
            }
        }
        .await;
        close(conn).await;
        let (tracking_table, applied, note) = read?;
        let (statuses, unknown) = merge_status(&migrations, &applied);
        Ok(StatusResponse {
            connection_id,
            tracking_table,
            note,
            migrations: statuses,
            unknown,
        })
    }

    /// Runs a migration's up script (or, with `dry_run`, runs it and rolls
    /// back) on a connection.
    pub async fn apply(
        &self,
        user_id: Uuid,
        migration_id: Uuid,
        connection_id: Uuid,
        confirm: Option<&str>,
        dry_run: bool,
    ) -> AppResult<RunOutcome> {
        self.run(
            user_id,
            migration_id,
            connection_id,
            confirm,
            dry_run,
            Direction::Up,
        )
        .await
    }

    /// Runs a migration's down script and removes its tracking row.
    pub async fn rollback(
        &self,
        user_id: Uuid,
        migration_id: Uuid,
        connection_id: Uuid,
        confirm: Option<&str>,
        dry_run: bool,
    ) -> AppResult<RunOutcome> {
        self.run(
            user_id,
            migration_id,
            connection_id,
            confirm,
            dry_run,
            Direction::Down,
        )
        .await
    }

    async fn run(
        &self,
        user_id: Uuid,
        migration_id: Uuid,
        connection_id: Uuid,
        confirm: Option<&str>,
        dry_run: bool,
        direction: Direction,
    ) -> AppResult<RunOutcome> {
        let migration = self.migration(user_id, migration_id).await?;
        let project = self.project(user_id, migration.project_id).await?;
        let script = match direction {
            Direction::Up => migration.up_sql.clone(),
            Direction::Down => migration.down_sql.clone().ok_or_else(|| {
                AppError::Conflict("this migration has no down script to roll back with".into())
            })?,
        };
        let record = ConnectionService::new(self.state)
            .require(user_id, connection_id, Access::Write)
            .await?
            .record;
        if record.read_only {
            return Err(AppError::Forbidden(READ_ONLY_CONNECTION.into()));
        }
        if !dry_run {
            check_confirmation(&record, confirm)?;
        }
        let all = repo::list_migrations(&self.state.db, user_id, project.id).await?;
        let applied_by = users::find_by_id(&self.state.db, user_id)
            .await?
            .map(|u| u.email)
            .unwrap_or_else(|| user_id.to_string());

        let mut conn = self.connect(&record, false).await?;
        let outcome = self
            .run_in_transaction(
                &mut conn,
                &project.name,
                &migration,
                &all,
                &script,
                &applied_by,
                dry_run,
                direction,
            )
            .await;
        close(conn).await;
        let (duration_ms, sql_error) = outcome?;

        let status = if sql_error.is_some() {
            "error"
        } else {
            "success"
        };
        let mut run_id = None;
        if !dry_run {
            let message = sql_error.as_ref().map(|e| e.message.clone());
            run_id = Some(
                repo::insert_run(
                    &self.state.db,
                    &NewRun {
                        user_id,
                        migration_id,
                        connection_id,
                        direction: direction.as_str(),
                        status,
                        error_message: message.as_deref(),
                        duration_ms: duration_ms as i64,
                    },
                )
                .await?,
            );
            let header = format!(
                "-- Migration {} {} {}: {}\n",
                if direction == Direction::Up {
                    "apply"
                } else {
                    "rollback"
                },
                project.name,
                migration.version,
                migration.name
            );
            LibraryService::new(self.state)
                .record_script(
                    user_id,
                    connection_id,
                    HistoryKind::Migration,
                    &format!("{header}{script}"),
                    duration_ms,
                    None,
                    message.as_deref(),
                )
                .await;
        }
        Ok(RunOutcome {
            dry_run,
            direction,
            status,
            duration_ms,
            error: sql_error,
            run_id,
        })
    }

    /// Returns the duration and, when the script failed, its error. State
    /// conflicts (lock held, ordering, drift) are errors.
    #[allow(clippy::too_many_arguments)]
    async fn run_in_transaction(
        &self,
        conn: &mut PgConnection,
        project: &str,
        migration: &Migration,
        all: &[Migration],
        script: &str,
        applied_by: &str,
        dry_run: bool,
        direction: Direction,
    ) -> AppResult<(u64, Option<crate::errors::SqlErrorInfo>)> {
        let mut tx = conn.begin().await.map_err(target_db::query_error)?;
        target_db::set_local_timeouts(&mut tx, self.state.config.query_max_timeout).await?;
        let locked: bool = sqlx::query_scalar(LOCK_SQL)
            .fetch_one(&mut *tx)
            .await
            .map_err(target_db::query_error)?;
        if !locked {
            return Err(AppError::Conflict(
                "another migration is being applied to this database; try again when it finishes"
                    .into(),
            ));
        }
        (&mut *tx)
            .execute(TRACKING_DDL)
            .await
            .map_err(target_db::query_error)?;
        let applied: Vec<AppliedRow> = sqlx::query_as(APPLIED_SQL)
            .bind(project)
            .fetch_all(&mut *tx)
            .await
            .map_err(target_db::query_error)?;
        let blocker = match direction {
            Direction::Up => apply_blocker(migration.version, all, &applied),
            Direction::Down => rollback_blocker(migration.version, &applied),
        };
        if let Some(reason) = blocker {
            return Err(AppError::Conflict(reason));
        }

        let started = Instant::now();
        // A plain string runs with the simple query protocol, so the script may
        // hold several statements.
        let result = (&mut *tx).execute(script).await;
        let duration_ms = started.elapsed().as_millis() as u64;
        if let Err(e) = result {
            // Nothing the script did is kept.
            let _ = tx.rollback().await;
            return match target_db::query_error(e) {
                AppError::Sql(mut info) => {
                    info.execution_time_ms = Some(duration_ms);
                    Ok((duration_ms, Some(info)))
                }
                other => Err(other),
            };
        }

        let record =
            match direction {
                Direction::Up => {
                    sqlx::query(
                        "INSERT INTO querylab.schema_migrations \
                     (project, version, name, checksum, applied_by, duration_ms) \
                 VALUES ($1, $2, $3, $4, $5 || ' (' || current_user || ')', $6)",
                    )
                    .bind(project)
                    .bind(migration.version)
                    .bind(&migration.name)
                    .bind(&migration.checksum)
                    .bind(applied_by)
                    .bind(duration_ms as i64)
                    .execute(&mut *tx)
                    .await
                }
                Direction::Down => sqlx::query(
                    "DELETE FROM querylab.schema_migrations WHERE project = $1 AND version = $2",
                )
                .bind(project)
                .bind(migration.version)
                .execute(&mut *tx)
                .await,
            };
        record.map_err(target_db::query_error)?;
        if dry_run {
            tx.rollback().await.map_err(target_db::query_error)?;
        } else {
            tx.commit().await.map_err(target_db::query_error)?;
        }
        Ok((duration_ms, None))
    }
}
