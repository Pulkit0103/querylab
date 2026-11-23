//! State shared by every API instance, kept in Redis: rate-limit windows
//! and the registry of running executions (owner, connection, backend pid
//! and cancel flag), so limits hold across instances and any instance can
//! cancel any query.
//!
//! Every operation is one Lua script, so it is atomic. Keys carry a prefix
//! (several deployments can share one Redis) and rate-limit keys are hashed,
//! so emails and addresses are not stored in Redis.
//!
//! Callers fall back to their in-memory state when Redis fails; a single
//! instance then keeps working with per-instance limits.

use std::time::Duration;

use redis::{aio::ConnectionManager, Script};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub type SharedResult<T> = Result<T, redis::RedisError>;

const HIT: &str = r#"
local count = redis.call('INCR', KEYS[1])
if count == 1 then redis.call('PEXPIRE', KEYS[1], ARGV[1]) end
local ttl = redis.call('PTTL', KEYS[1])
if ttl < 0 then redis.call('PEXPIRE', KEYS[1], ARGV[1]); ttl = tonumber(ARGV[1]) end
return {count, ttl}
"#;

// KEYS: exec hash, user zset. ARGV: now ms, max, user, connection, ttl ms, id.
const REGISTER: &str = r#"
redis.call('ZREMRANGEBYSCORE', KEYS[2], '-inf', ARGV[1])
if redis.call('EXISTS', KEYS[1]) == 1 then return -1 end
if redis.call('ZCARD', KEYS[2]) >= tonumber(ARGV[2]) then return 0 end
redis.call('HSET', KEYS[1], 'user', ARGV[3], 'connection', ARGV[4], 'pid', 0, 'cancel', 0)
redis.call('PEXPIRE', KEYS[1], ARGV[5])
redis.call('ZADD', KEYS[2], tonumber(ARGV[1]) + tonumber(ARGV[5]), ARGV[6])
redis.call('PEXPIRE', KEYS[2], ARGV[5])
return 1
"#;

// KEYS: exec hash. ARGV: user. Returns {connection, pid} or false.
const REQUEST_CANCEL: &str = r#"
if redis.call('HGET', KEYS[1], 'user') ~= ARGV[1] then return false end
redis.call('HSET', KEYS[1], 'cancel', 1)
return {redis.call('HGET', KEYS[1], 'connection'), redis.call('HGET', KEYS[1], 'pid')}
"#;

/// Outcome of registering an execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Registered {
    Ok,
    /// The user already runs as many executions as allowed.
    LimitReached,
    /// The execution id is already in use.
    Duplicate,
}

#[derive(Clone)]
pub struct SharedStore {
    conn: ConnectionManager,
    prefix: String,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

impl SharedStore {
    /// Connects (bounded to 5 seconds) and checks the server answers.
    pub async fn connect(url: &str, prefix: &str) -> SharedResult<Self> {
        let client = redis::Client::open(url)?;
        let conn = tokio::time::timeout(Duration::from_secs(5), client.get_connection_manager())
            .await
            .map_err(|_| {
                redis::RedisError::from((
                    redis::ErrorKind::IoError,
                    "timed out connecting to Redis",
                ))
            })??;
        let store = Self {
            conn,
            prefix: prefix.trim_end_matches(':').to_string(),
        };
        store.ping().await?;
        Ok(store)
    }

    pub async fn ping(&self) -> SharedResult<()> {
        let mut c = self.conn.clone();
        redis::cmd("PING")
            .query_async::<String>(&mut c)
            .await
            .map(|_| ())
    }

    fn key(&self, parts: &[&str]) -> String {
        format!("{}:{}", self.prefix, parts.join(":"))
    }

    /// Counts a hit in the current window; returns the count and the
    /// milliseconds until the window ends.
    pub async fn hit(&self, bucket: &str, key: &str, period: Duration) -> SharedResult<(u64, u64)> {
        let digest: String = Sha256::digest(key.as_bytes())[..16]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let mut c = self.conn.clone();
        let (count, ttl): (u64, i64) = Script::new(HIT)
            .key(self.key(&["rl", bucket, &digest]))
            .arg(period.as_millis() as u64)
            .invoke_async(&mut c)
            .await?;
        Ok((count, ttl.max(0) as u64))
    }

    fn exec_keys(&self, id: Uuid, user: Uuid) -> (String, String) {
        (
            self.key(&["exec", &id.to_string()]),
            self.key(&["user-exec", &user.to_string()]),
        )
    }

    pub async fn register_execution(
        &self,
        id: Uuid,
        user: Uuid,
        connection: Uuid,
        max_per_user: usize,
        ttl: Duration,
    ) -> SharedResult<Registered> {
        let (exec, set) = self.exec_keys(id, user);
        let mut c = self.conn.clone();
        let result: i64 = Script::new(REGISTER)
            .key(exec)
            .key(set)
            .arg(now_ms())
            .arg(max_per_user as u64)
            .arg(user.to_string())
            .arg(connection.to_string())
            .arg(ttl.as_millis() as u64)
            .arg(id.to_string())
            .invoke_async(&mut c)
            .await?;
        Ok(match result {
            1 => Registered::Ok,
            0 => Registered::LimitReached,
            _ => Registered::Duplicate,
        })
    }

    pub async fn set_pid(&self, id: Uuid, user: Uuid, pid: i32) -> SharedResult<()> {
        let (exec, _) = self.exec_keys(id, user);
        let mut c = self.conn.clone();
        redis::cmd("HSET")
            .arg(exec)
            .arg("pid")
            .arg(pid)
            .query_async::<()>(&mut c)
            .await
    }

    pub async fn was_cancelled(&self, id: Uuid, user: Uuid) -> SharedResult<bool> {
        let (exec, _) = self.exec_keys(id, user);
        let mut c = self.conn.clone();
        let flag: Option<String> = redis::cmd("HGET")
            .arg(exec)
            .arg("cancel")
            .query_async(&mut c)
            .await?;
        Ok(flag.as_deref() == Some("1"))
    }

    /// Flags an execution of `user` as cancelled, wherever it runs, and
    /// returns its connection and backend pid.
    pub async fn request_cancel(&self, id: Uuid, user: Uuid) -> SharedResult<Option<(Uuid, i32)>> {
        let (exec, _) = self.exec_keys(id, user);
        let mut c = self.conn.clone();
        let found: Option<(String, String)> = Script::new(REQUEST_CANCEL)
            .key(exec)
            .arg(user.to_string())
            .invoke_async(&mut c)
            .await?;
        Ok(found.and_then(|(conn, pid)| Some((conn.parse().ok()?, pid.parse().ok()?))))
    }

    pub async fn remove_execution(&self, id: Uuid, user: Uuid) -> SharedResult<()> {
        let (exec, set) = self.exec_keys(id, user);
        let mut c = self.conn.clone();
        redis::pipe()
            .del(exec)
            .ignore()
            .zrem(set, id.to_string())
            .ignore()
            .query_async::<()>(&mut c)
            .await
    }
}
