//! The synced editor session (open tabs) and per-user workspace settings.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    errors::AppError,
    models::query::MAX_SQL_BYTES,
    utils::validation::{Validate, Validator},
};

pub const MAX_TABS: usize = 50;
/// Upper bound on the stored session (matches the request body limit).
pub const MAX_SESSION_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceTab {
    /// Client-generated tab id.
    pub id: String,
    pub title: String,
    pub sql: String,
    #[serde(default)]
    pub saved_query_id: Option<Uuid>,
    /// SQL at the last save (for the unsaved-changes indicator).
    #[serde(default)]
    pub saved_sql: Option<String>,
}

/// The open tabs, which tab is active and which connection is selected.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceSession {
    pub tabs: Vec<WorkspaceTab>,
    #[serde(default)]
    pub active_tab_id: Option<String>,
    #[serde(default)]
    pub active_connection_id: Option<Uuid>,
}

impl Validate for WorkspaceSession {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        v.check(
            !self.tabs.is_empty() && self.tabs.len() <= MAX_TABS,
            "tabs",
            &format!("must contain between 1 and {MAX_TABS} tabs"),
        );
        for tab in &self.tabs {
            v.length("tabs.id", &tab.id, 1, 64)
                .no_control_chars("tabs.id", &tab.id)
                .length("tabs.title", &tab.title, 0, 200)
                .check(
                    tab.sql.len() <= MAX_SQL_BYTES
                        && tab
                            .saved_sql
                            .as_ref()
                            .map_or(true, |s| s.len() <= MAX_SQL_BYTES),
                    "tabs.sql",
                    "must be at most 1 MiB",
                )
                .check(
                    !tab.sql.contains('\0'),
                    "tabs.sql",
                    "must not contain NUL bytes",
                );
        }
        if let Some(active) = &self.active_tab_id {
            v.check(
                self.tabs.iter().any(|t| &t.id == active),
                "active_tab_id",
                "must be one of the tabs",
            );
        }
        let size = serde_json::to_vec(self)
            .map(|b| b.len())
            .unwrap_or(usize::MAX);
        v.check(
            size <= MAX_SESSION_BYTES,
            "tabs",
            "the session must be at most 2 MiB",
        );
        v.finish()
    }
}

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    /// Null until the first save.
    pub session: Option<WorkspaceSession>,
    pub updated_at: Option<DateTime<Utc>>,
}

pub const MIN_RETENTION_DAYS: i32 = 1;
pub const MAX_RETENTION_DAYS: i32 = 3650;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkspaceSettings {
    /// History older than this is deleted automatically (pinned entries are kept).
    pub history_retention_days: i32,
}

impl Validate for WorkspaceSettings {
    fn validate(&self) -> Result<(), AppError> {
        Validator::new()
            .check(
                (MIN_RETENTION_DAYS..=MAX_RETENTION_DAYS).contains(&self.history_retention_days),
                "history_retention_days",
                &format!("must be between {MIN_RETENTION_DAYS} and {MAX_RETENTION_DAYS}"),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(id: &str, sql: &str) -> WorkspaceTab {
        WorkspaceTab {
            id: id.into(),
            title: "Query".into(),
            sql: sql.into(),
            saved_query_id: None,
            saved_sql: None,
        }
    }

    #[test]
    fn sessions_are_bounded() {
        let ok = WorkspaceSession {
            tabs: vec![tab("a", "select 1")],
            active_tab_id: Some("a".into()),
            active_connection_id: None,
        };
        assert!(ok.validate().is_ok());

        let mut bad = ok.clone();
        bad.tabs.clear();
        assert!(bad.validate().is_err());

        let mut bad = ok.clone();
        bad.active_tab_id = Some("missing".into());
        assert!(bad.validate().is_err());

        let mut bad = ok.clone();
        bad.tabs = (0..=MAX_TABS).map(|i| tab(&i.to_string(), "")).collect();
        assert!(bad.validate().is_err());

        // Three 900 KiB tabs fit individually but not together.
        let mut bad = ok.clone();
        let big = "x".repeat(900 * 1024);
        bad.tabs = vec![tab("a", &big), tab("b", &big), tab("c", &big)];
        bad.active_tab_id = None;
        assert!(bad.validate().is_err());

        let mut bad = ok;
        bad.tabs[0].sql = "x".repeat(MAX_SQL_BYTES + 1);
        assert!(bad.validate().is_err());
    }

    #[test]
    fn retention_is_bounded() {
        let s = |d| WorkspaceSettings {
            history_retention_days: d,
        };
        assert!(s(0).validate().is_err());
        assert!(s(1).validate().is_ok());
        assert!(s(3650).validate().is_ok());
        assert!(s(3651).validate().is_err());
    }
}
