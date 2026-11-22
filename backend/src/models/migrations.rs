//! Migration projects, scripts, runs and per-database status.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    errors::{AppError, SqlErrorInfo},
    models::query::MAX_SQL_BYTES,
    utils::{
        fingerprint::statement_heads,
        validation::{double_option, Validate, Validator},
    },
};

/// SHA-256 of the up script, hex. Any byte change counts as drift.
pub fn checksum(up_sql: &str) -> String {
    Sha256::digest(up_sql.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Migrations run inside QueryLab's transaction; their own transaction
/// control would end it (and release the advisory lock) early.
fn check_script(v: &mut Validator, field: &str, sql: &str, required: bool) {
    v.check(sql.len() <= MAX_SQL_BYTES, field, "must be at most 1 MiB")
        .check(!sql.contains('\0'), field, "must not contain NUL bytes");
    if required {
        v.check(!sql.trim().is_empty(), field, "is required");
    }
    let control = statement_heads(sql).into_iter().find(|w| {
        matches!(
            w.as_str(),
            "begin" | "commit" | "rollback" | "end" | "abort" | "start"
        )
    });
    if let Some(word) = control {
        v.check(
            false,
            field,
            &format!(
                "must not contain {}: migrations already run in a single transaction",
                word.to_uppercase()
            ),
        );
    }
}

fn check_name(v: &mut Validator, name: &str, max: usize) {
    v.length("name", name.trim(), 1, max)
        .no_control_chars("name", name);
}

/// Default version for new scripts: the UTC time, `YYYYMMDDHHMMSS`.
pub fn timestamp_version(now: DateTime<Utc>) -> i64 {
    now.format("%Y%m%d%H%M%S").to_string().parse().unwrap_or(1)
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
}

impl Validate for CreateProjectRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        check_name(&mut v, &self.name, 100);
        v.finish()
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub migrations: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateMigrationRequest {
    /// Defaults to the current UTC time as `YYYYMMDDHHMMSS`.
    pub version: Option<i64>,
    pub name: String,
    pub up_sql: String,
    #[serde(default)]
    pub down_sql: Option<String>,
}

impl Validate for CreateMigrationRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        check_name(&mut v, &self.name, 200);
        if let Some(version) = self.version {
            v.check(version > 0, "version", "must be a positive number");
        }
        check_script(&mut v, "up_sql", &self.up_sql, true);
        if let Some(down) = &self.down_sql {
            check_script(&mut v, "down_sql", down, false);
        }
        v.finish()
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct UpdateMigrationRequest {
    pub version: Option<i64>,
    pub name: Option<String>,
    pub up_sql: Option<String>,
    /// Explicit `null` removes the down script.
    #[serde(default, deserialize_with = "double_option")]
    pub down_sql: Option<Option<String>>,
}

impl Validate for UpdateMigrationRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        if let Some(name) = &self.name {
            check_name(&mut v, name, 200);
        }
        if let Some(version) = self.version {
            v.check(version > 0, "version", "must be a positive number");
        }
        if let Some(up) = &self.up_sql {
            check_script(&mut v, "up_sql", up, true);
        }
        if let Some(Some(down)) = &self.down_sql {
            check_script(&mut v, "down_sql", down, false);
        }
        v.finish()
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Migration {
    pub id: Uuid,
    pub project_id: Uuid,
    pub version: i64,
    pub name: String,
    pub up_sql: String,
    pub down_sql: Option<String>,
    pub checksum: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Applied (per QueryLab's run history) on at least one connection;
    /// such scripts can no longer be edited or deleted.
    pub locked: bool,
}

#[derive(Debug, Deserialize)]
pub struct ApplyRequest {
    pub connection_id: Uuid,
    /// Connection name for production, any text for staging.
    #[serde(default)]
    pub confirm: Option<String>,
    /// Run the script and roll back: checks it without changing anything.
    #[serde(default)]
    pub dry_run: bool,
}

impl Validate for ApplyRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        if let Some(c) = &self.confirm {
            v.length("confirm", c, 0, 200);
        }
        v.finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Up,
    Down,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Up => "up",
            Direction::Down => "down",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RunOutcome {
    pub dry_run: bool,
    pub direction: Direction,
    /// `success` or `error` (the script failed and was rolled back).
    pub status: &'static str,
    pub duration_ms: u64,
    pub error: Option<SqlErrorInfo>,
    /// Recorded run (absent for dry runs).
    pub run_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MigrationRun {
    pub id: Uuid,
    pub migration_id: Uuid,
    pub project_id: Uuid,
    pub version: i64,
    pub name: String,
    pub connection_id: Option<Uuid>,
    pub connection_name: Option<String>,
    pub direction: String,
    pub status: String,
    pub error_message: Option<String>,
    pub duration_ms: i64,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Default)]
pub struct RunsQuery {
    pub project_id: Option<Uuid>,
    pub connection_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct StatusQuery {
    pub connection_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationState {
    Applied,
    Pending,
    /// Applied, but the script changed since (checksum mismatch).
    Drift,
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationStatus {
    pub id: Uuid,
    pub version: i64,
    pub name: String,
    pub state: MigrationState,
    pub applied_at: Option<DateTime<Utc>>,
    pub applied_by: Option<String>,
    pub duration_ms: Option<i64>,
}

/// A row of the database's tracking table.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AppliedRow {
    pub version: i64,
    pub name: String,
    pub checksum: String,
    pub applied_at: DateTime<Utc>,
    pub applied_by: String,
    pub duration_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub connection_id: Uuid,
    /// `querylab.schema_migrations` exists in the database.
    pub tracking_table: bool,
    /// Why the status could not be read completely.
    pub note: Option<String>,
    pub migrations: Vec<MigrationStatus>,
    /// Versions recorded in the database without a script in this project.
    pub unknown: Vec<AppliedRow>,
}

/// Merges the project's scripts with the database's applied rows.
pub fn merge_status(
    migrations: &[Migration],
    applied: &[AppliedRow],
) -> (Vec<MigrationStatus>, Vec<AppliedRow>) {
    let statuses = migrations
        .iter()
        .map(|m| {
            let row = applied.iter().find(|a| a.version == m.version);
            MigrationStatus {
                id: m.id,
                version: m.version,
                name: m.name.clone(),
                state: match row {
                    None => MigrationState::Pending,
                    Some(a) if a.checksum == m.checksum => MigrationState::Applied,
                    Some(_) => MigrationState::Drift,
                },
                applied_at: row.map(|a| a.applied_at),
                applied_by: row.map(|a| a.applied_by.clone()),
                duration_ms: row.map(|a| a.duration_ms),
            }
        })
        .collect();
    let unknown = applied
        .iter()
        .filter(|a| !migrations.iter().any(|m| m.version == a.version))
        .cloned()
        .collect();
    (statuses, unknown)
}

/// Why `version` cannot be applied given the database state, if anything.
pub fn apply_blocker(
    version: i64,
    migrations: &[Migration],
    applied: &[AppliedRow],
) -> Option<String> {
    let (statuses, _) = merge_status(migrations, applied);
    if let Some(drift) = statuses.iter().find(|s| s.state == MigrationState::Drift) {
        return Some(format!(
            "Version {} was changed after it was applied (checksum mismatch). Restore the applied script before applying more.",
            drift.version
        ));
    }
    if statuses
        .iter()
        .any(|s| s.version == version && s.state == MigrationState::Applied)
    {
        return Some(format!(
            "Version {version} is already applied to this database."
        ));
    }
    statuses
        .iter()
        .filter(|s| s.version < version && s.state == MigrationState::Pending)
        .map(|s| s.version)
        .min()
        .map(|v| format!("Apply version {v} first: migrations are applied in version order."))
}

/// Why `version` cannot be rolled back, if anything.
pub fn rollback_blocker(version: i64, applied: &[AppliedRow]) -> Option<String> {
    let Some(latest) = applied.iter().map(|a| a.version).max() else {
        return Some(format!(
            "Version {version} is not applied to this database."
        ));
    };
    if !applied.iter().any(|a| a.version == version) {
        return Some(format!(
            "Version {version} is not applied to this database."
        ));
    }
    (latest != version).then(|| {
        format!(
            "Roll back version {latest} first: only the latest applied version can be rolled back."
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn migration(version: i64, up: &str) -> Migration {
        Migration {
            id: Uuid::new_v4(),
            project_id: Uuid::nil(),
            version,
            name: format!("m{version}"),
            up_sql: up.into(),
            down_sql: None,
            checksum: checksum(up),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            locked: false,
        }
    }

    fn applied(version: i64, up: &str) -> AppliedRow {
        AppliedRow {
            version,
            name: format!("m{version}"),
            checksum: checksum(up),
            applied_at: Utc::now(),
            applied_by: "me".into(),
            duration_ms: 1,
        }
    }

    #[test]
    fn checksums_are_sha256_hex() {
        assert_eq!(
            checksum(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_ne!(checksum("select 1"), checksum("select 1 "));
    }

    #[test]
    fn status_detects_applied_pending_drift_and_unknown() {
        let ms = [migration(1, "a"), migration(2, "b"), migration(3, "c")];
        let rows = [applied(1, "a"), applied(2, "changed"), applied(9, "z")];
        let (status, unknown) = merge_status(&ms, &rows);
        let states: Vec<_> = status.iter().map(|s| s.state).collect();
        assert_eq!(
            states,
            [
                MigrationState::Applied,
                MigrationState::Drift,
                MigrationState::Pending
            ]
        );
        assert_eq!(unknown.len(), 1);
        assert_eq!(unknown[0].version, 9);
    }

    #[test]
    fn applies_in_order_and_never_over_drift() {
        let ms = [migration(1, "a"), migration(2, "b"), migration(3, "c")];
        assert!(apply_blocker(1, &ms, &[]).is_none());
        assert!(apply_blocker(2, &ms, &[])
            .unwrap()
            .contains("Apply version 1 first"));
        assert!(apply_blocker(1, &ms, &[applied(1, "a")])
            .unwrap()
            .contains("already applied"));
        assert!(apply_blocker(2, &ms, &[applied(1, "a")]).is_none());
        assert!(apply_blocker(3, &ms, &[applied(1, "x"), applied(2, "b")])
            .unwrap()
            .contains("checksum mismatch"));
    }

    #[test]
    fn rolls_back_only_the_latest() {
        let rows = [applied(1, "a"), applied(2, "b")];
        assert!(rollback_blocker(2, &rows).is_none());
        assert!(rollback_blocker(1, &rows)
            .unwrap()
            .contains("Roll back version 2 first"));
        assert!(rollback_blocker(3, &rows).unwrap().contains("not applied"));
        assert!(rollback_blocker(1, &[]).unwrap().contains("not applied"));
    }

    #[test]
    fn scripts_must_not_control_transactions() {
        let req = |up: &str| CreateMigrationRequest {
            version: None,
            name: "x".into(),
            up_sql: up.into(),
            down_sql: None,
        };
        assert!(req("create table t (id int);").validate().is_ok());
        // Function bodies and CASE ... END are fine.
        assert!(req("create function f() returns int language plpgsql as $$ begin return case when true then 1 end; end $$;")
            .validate()
            .is_ok());
        assert!(req("BEGIN; create table t (id int); COMMIT;")
            .validate()
            .is_err());
        assert!(req("create table t (id int); rollback").validate().is_err());
        assert!(req("  ").validate().is_err());
        assert_eq!(
            timestamp_version("2026-11-22T10:15:30Z".parse().unwrap()),
            20261122101530
        );
    }
}
