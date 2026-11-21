//! Live server monitoring: sessions, locks, sizes and statement statistics.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    errors::AppError,
    models::schema::validate_identifier,
    utils::validation::{Validate, Validator},
};

/// What the connected role may see and do. Monitoring degrades gracefully:
/// without `pg_read_all_stats` other sessions' statements are hidden.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Privileges {
    pub role: String,
    pub superuser: bool,
    /// Member of `pg_read_all_stats` (or `pg_monitor`): sees every session's query.
    pub read_all_stats: bool,
    /// Member of `pg_signal_backend`: may cancel other roles' sessions.
    pub signal_backend: bool,
    /// QueryLab-side setting: cancel and terminate need a read-write connection.
    #[sqlx(skip)]
    pub read_only_connection: bool,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Session {
    pub pid: i32,
    pub user: Option<String>,
    pub application: Option<String>,
    pub client: Option<String>,
    pub backend_type: Option<String>,
    /// `active`, `idle`, `idle in transaction`, ... (null for background workers).
    pub state: Option<String>,
    pub wait_event_type: Option<String>,
    pub wait_event: Option<String>,
    pub backend_start: Option<DateTime<Utc>>,
    pub xact_start: Option<DateTime<Utc>>,
    pub query_start: Option<DateTime<Utc>>,
    /// Time since `query_start` (the current or last statement).
    pub query_ms: Option<f64>,
    pub xact_ms: Option<f64>,
    /// Statement text (truncated); null when hidden from this role.
    pub query: Option<String>,
    pub query_hidden: bool,
    /// Sessions this one is waiting for (`pg_blocking_pids`).
    pub blocked_by: Vec<i32>,
    /// The monitoring session itself.
    pub is_self: bool,
}

#[derive(Debug, Serialize)]
pub struct ActivityResponse {
    pub privileges: Privileges,
    pub sessions: Vec<Session>,
    pub server_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct LockInfo {
    pub pid: i32,
    pub locktype: String,
    pub mode: String,
    pub granted: bool,
    /// Locked relation, when the lock is on one.
    pub relation: Option<String>,
}

/// A session in the blocking tree: its children wait for it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BlockingNode {
    pub pid: i32,
    pub blocked_by: Vec<i32>,
    pub children: Vec<BlockingNode>,
}

#[derive(Debug, Serialize)]
pub struct LocksResponse {
    pub privileges: Privileges,
    /// Roots are sessions that block others without waiting themselves (or
    /// the lowest pid of a deadlock cycle).
    pub tree: Vec<BlockingNode>,
    /// Sessions in the tree, keyed by pid (as strings, for JSON).
    pub sessions: BTreeMap<String, Session>,
    pub locks: Vec<LockInfo>,
    pub waiting: usize,
}

/// Builds the blocking forest from `(pid, blocked_by)` pairs. Only sessions
/// that block or wait appear. A session blocked by several others appears
/// under each of them. Cycles are cut where they would repeat a pid.
pub fn blocking_tree(sessions: &[(i32, Vec<i32>)]) -> Vec<BlockingNode> {
    let mut children: HashMap<i32, Vec<i32>> = HashMap::new();
    let waiting: HashSet<i32> = sessions
        .iter()
        .filter(|(_, by)| !by.is_empty())
        .map(|(pid, _)| *pid)
        .collect();
    let blocked_by: HashMap<i32, Vec<i32>> = sessions.iter().cloned().collect();
    for (pid, by) in sessions {
        for b in by {
            children.entry(*b).or_default().push(*pid);
        }
    }
    for list in children.values_mut() {
        list.sort_unstable();
        list.dedup();
    }

    fn build(
        pid: i32,
        children: &HashMap<i32, Vec<i32>>,
        blocked_by: &HashMap<i32, Vec<i32>>,
        path: &mut Vec<i32>,
        seen: &mut HashSet<i32>,
    ) -> BlockingNode {
        path.push(pid);
        seen.insert(pid);
        let next: Vec<i32> = children
            .get(&pid)
            .into_iter()
            .flatten()
            .filter(|c| !path.contains(c))
            .copied()
            .collect();
        let kids = next
            .into_iter()
            .map(|c| build(c, children, blocked_by, path, seen))
            .collect();
        path.pop();
        BlockingNode {
            pid,
            blocked_by: blocked_by.get(&pid).cloned().unwrap_or_default(),
            children: kids,
        }
    }

    let mut roots: Vec<i32> = children
        .keys()
        .filter(|pid| !waiting.contains(pid))
        .copied()
        .collect();
    roots.sort_unstable();
    let mut seen = HashSet::new();
    let mut forest: Vec<BlockingNode> = roots
        .into_iter()
        .map(|r| build(r, &children, &blocked_by, &mut Vec::new(), &mut seen))
        .collect();
    // Deadlocks: every member waits, so none is a root. Start from the
    // lowest pid not reached yet.
    let mut rest: Vec<i32> = waiting
        .iter()
        .filter(|p| !seen.contains(p) && children.contains_key(p))
        .copied()
        .collect();
    rest.sort_unstable();
    for pid in rest {
        if !seen.contains(&pid) {
            forest.push(build(
                pid,
                &children,
                &blocked_by,
                &mut Vec::new(),
                &mut seen,
            ));
        }
    }
    forest
}

// ---------------------------------------------------------------------------
// Sizes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
pub struct SizesQuery {
    pub schema: Option<String>,
}

impl Validate for SizesQuery {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        if let Some(schema) = &self.schema {
            validate_identifier(&mut v, "schema", schema);
        }
        v.finish()
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SchemaSize {
    pub schema: String,
    pub total_bytes: i64,
    pub tables: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TableSize {
    pub schema: String,
    pub name: String,
    pub kind: String,
    /// `pg_total_relation_size` = `table_bytes` + `index_bytes`.
    pub total_bytes: i64,
    /// `pg_table_size`: heap, TOAST, free space and visibility maps.
    pub table_bytes: i64,
    /// `pg_indexes_size`.
    pub index_bytes: i64,
    pub estimated_rows: Option<i64>,
    pub live_rows: Option<i64>,
    pub dead_rows: Option<i64>,
    pub last_vacuum: Option<DateTime<Utc>>,
    pub last_analyze: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct IndexSize {
    pub schema: String,
    pub table: String,
    pub name: String,
    pub bytes: i64,
    /// Index scans since statistics were reset; 0 suggests an unused index.
    pub scans: Option<i64>,
    pub unique: bool,
    pub primary: bool,
}

#[derive(Debug, Serialize)]
pub struct SizesResponse {
    pub database: String,
    pub database_bytes: i64,
    pub schemas: Vec<SchemaSize>,
    pub tables: Vec<TableSize>,
    pub indexes: Vec<IndexSize>,
}

// ---------------------------------------------------------------------------
// Statement statistics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TopQueriesSort {
    #[default]
    Total,
    Mean,
    Calls,
}

#[derive(Debug, Deserialize, Default)]
pub struct TopQueriesQuery {
    #[serde(default)]
    pub sort: TopQueriesSort,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TopQuery {
    /// `pg_stat_statements.queryid` (64-bit, as a string).
    pub query_id: Option<String>,
    pub query: Option<String>,
    pub calls: i64,
    pub total_ms: f64,
    pub mean_ms: f64,
    pub max_ms: f64,
    pub rows: i64,
    pub shared_hit: i64,
    pub shared_read: i64,
}

#[derive(Debug, Serialize)]
pub struct TopQueriesResponse {
    pub available: bool,
    /// Why statistics are unavailable (extension missing, not preloaded, ...).
    pub reason: Option<String>,
    pub privileges: Privileges,
    pub queries: Vec<TopQuery>,
}

// ---------------------------------------------------------------------------
// Cancel / terminate
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Cancel,
    Terminate,
}

impl Signal {
    pub fn function(self) -> &'static str {
        match self {
            Signal::Cancel => "pg_cancel_backend",
            Signal::Terminate => "pg_terminate_backend",
        }
    }

    pub fn verb(self) -> &'static str {
        match self {
            Signal::Cancel => "Cancel",
            Signal::Terminate => "Terminate",
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct SignalRequest {
    /// Connection name for production connections, any text for staging.
    #[serde(default)]
    pub confirm: Option<String>,
}

impl Validate for SignalRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        if let Some(c) = &self.confirm {
            v.length("confirm", c, 0, 200);
        }
        v.finish()
    }
}

#[derive(Debug, Serialize)]
pub struct SignalResponse {
    pub pid: i32,
    pub signal: &'static str,
    pub signalled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pids(nodes: &[BlockingNode]) -> Vec<String> {
        nodes
            .iter()
            .map(|n| {
                if n.children.is_empty() {
                    n.pid.to_string()
                } else {
                    format!("{}[{}]", n.pid, pids(&n.children).join(","))
                }
            })
            .collect()
    }

    #[test]
    fn builds_blocking_chains() {
        // 10 blocks 11 and 12; 12 blocks 13; 20 is idle; 30 blocks 31.
        let sessions = vec![
            (10, vec![]),
            (11, vec![10]),
            (12, vec![10]),
            (13, vec![12]),
            (20, vec![]),
            (31, vec![30]),
            (30, vec![]),
        ];
        assert_eq!(pids(&blocking_tree(&sessions)), ["10[11,12[13]]", "30[31]"]);
    }

    #[test]
    fn handles_multiple_blockers_and_deadlocks() {
        // 3 waits for both 1 and 2.
        let tree = blocking_tree(&[(1, vec![]), (2, vec![]), (3, vec![1, 2])]);
        assert_eq!(pids(&tree), ["1[3]", "2[3]"]);
        assert_eq!(tree[0].children[0].blocked_by, [1, 2]);
        // 5 and 6 wait for each other.
        let tree = blocking_tree(&[(5, vec![6]), (6, vec![5])]);
        assert_eq!(pids(&tree), ["5[6]"]);
        assert!(blocking_tree(&[(1, vec![]), (2, vec![])]).is_empty());
    }

    #[test]
    fn signals_name_their_functions() {
        assert_eq!(Signal::Cancel.function(), "pg_cancel_backend");
        assert_eq!(Signal::Terminate.verb(), "Terminate");
        assert!(SignalRequest {
            confirm: Some("x".repeat(201))
        }
        .validate()
        .is_err());
    }
}
