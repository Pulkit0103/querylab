//! In-memory registry of imports and mock data runs: live progress,
//! cancellation and the bytes of files being imported.
//!
//! Uploaded files only ever live in memory, for as long as their import
//! exists. Every upload reserves its declared size against a global budget
//! up front, so concurrent imports cannot exhaust the server's memory.
//! Finished and abandoned jobs are dropped after [`JOB_TTL`]; finished jobs
//! remain available from the `data_jobs` table.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::{Duration, Instant},
};

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    database::data_jobs as repo,
    errors::{AppError, AppResult},
    models::data_jobs::{JobKind, JobRecord, JobStatus, JobView, JobsQuery},
    services::import::Upload,
    state::AppState,
};

/// Idle time after which a job that is not running is forgotten.
pub const JOB_TTL: Duration = Duration::from_secs(30 * 60);
/// Unfinished imports and runs per user.
pub const MAX_ACTIVE_JOBS_PER_USER: usize = 3;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

pub struct Job {
    pub id: Uuid,
    pub user_id: Uuid,
    pub connection_id: Uuid,
    pub kind: JobKind,
    view: Mutex<JobView>,
    touched: Mutex<Instant>,
    cancel: AtomicBool,
    upload: Mutex<Option<Upload>>,
}

impl Job {
    pub fn view(&self) -> JobView {
        lock(&self.view).clone()
    }

    pub fn status(&self) -> JobStatus {
        lock(&self.view).status
    }

    pub fn update<R>(&self, f: impl FnOnce(&mut JobView) -> R) -> R {
        f(&mut lock(&self.view))
    }

    pub fn touch(&self) {
        *lock(&self.touched) = Instant::now();
    }

    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    pub fn upload(&self) -> MutexGuard<'_, Option<Upload>> {
        lock(&self.upload)
    }

    /// Moves to `Running` unless the job is already running or finished.
    pub fn start(&self, target: Option<String>) -> AppResult<JobView> {
        let mut view = lock(&self.view);
        match view.status {
            JobStatus::Running => Err(AppError::Conflict("this job is already running".into())),
            s if s.is_finished() => Err(AppError::Conflict("this job has already run".into())),
            JobStatus::Uploading => Err(AppError::Conflict(
                "the file has not been uploaded completely".into(),
            )),
            _ => {
                view.status = JobStatus::Running;
                view.started_at = Some(Utc::now());
                if target.is_some() {
                    view.target = target;
                }
                Ok(view.clone())
            }
        }
    }

    /// Final state: frees the upload and stores the report.
    pub async fn finish(&self, db: &PgPool, status: JobStatus, error: Option<String>) -> JobView {
        let view = {
            let mut view = lock(&self.view);
            view.status = status;
            view.phase = None;
            if error.is_some() {
                view.error = error;
            }
            let now = Utc::now();
            view.finished_at = Some(now);
            view.duration_ms = view
                .started_at
                .map(|s| (now - s).num_milliseconds().max(0) as u64);
            view.clone()
        };
        lock(&self.upload).take();
        self.touch();
        if let Err(err) = repo::finish(db, self.user_id, &view).await {
            tracing::warn!(job_id = %self.id, error = %err, "failed to store data job report");
        }
        view
    }
}

/// Bytes reserved against the upload budget; released on drop.
pub struct Reservation {
    registry: Arc<DataJobs>,
    bytes: usize,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let mut reserved = lock(&self.registry.reserved);
        *reserved = reserved.saturating_sub(self.bytes);
    }
}

pub struct DataJobs {
    jobs: Mutex<HashMap<Uuid, Arc<Job>>>,
    reserved: Mutex<usize>,
    budget: usize,
}

impl DataJobs {
    pub fn new(budget: usize) -> Self {
        Self {
            jobs: Mutex::new(HashMap::new()),
            reserved: Mutex::new(0),
            budget,
        }
    }

    fn sweep(map: &mut HashMap<Uuid, Arc<Job>>) {
        map.retain(|_, job| {
            job.status() == JobStatus::Running || lock(&job.touched).elapsed() < JOB_TTL
        });
    }

    /// Reserves upload memory, or fails when the server is at capacity.
    pub fn reserve(self: &Arc<Self>, bytes: usize) -> AppResult<Reservation> {
        // Abandoned uploads give their memory back first.
        Self::sweep(&mut lock(&self.jobs));
        let mut reserved = lock(&self.reserved);
        if reserved.saturating_add(bytes) > self.budget {
            return Err(AppError::Conflict(
                "the server is busy with other imports; try again in a few minutes".into(),
            ));
        }
        *reserved += bytes;
        Ok(Reservation {
            registry: Arc::clone(self),
            bytes,
        })
    }

    pub fn create(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        view: JobView,
        upload: Option<Upload>,
    ) -> AppResult<Arc<Job>> {
        let mut map = lock(&self.jobs);
        Self::sweep(&mut map);
        let active = map
            .values()
            .filter(|j| j.user_id == user_id && !j.status().is_finished())
            .count();
        if active >= MAX_ACTIVE_JOBS_PER_USER {
            return Err(AppError::Conflict(format!(
                "you already have {active} imports or runs in progress; finish or cancel one first"
            )));
        }
        let job = Arc::new(Job {
            id: view.id,
            user_id,
            connection_id,
            kind: view.kind,
            view: Mutex::new(view),
            touched: Mutex::new(Instant::now()),
            cancel: AtomicBool::new(false),
            upload: Mutex::new(upload),
        });
        map.insert(job.id, Arc::clone(&job));
        Ok(job)
    }

    /// A live job of this user. Other users' jobs are indistinguishable
    /// from missing ones.
    pub fn get(&self, user_id: Uuid, id: Uuid) -> Option<Arc<Job>> {
        let job = lock(&self.jobs)
            .get(&id)
            .filter(|j| j.user_id == user_id)
            .cloned()?;
        job.touch();
        Some(job)
    }

    pub fn remove(&self, id: Uuid) {
        lock(&self.jobs).remove(&id);
    }

    #[doc(hidden)]
    pub fn reserved_bytes(&self) -> usize {
        *lock(&self.reserved)
    }
}

fn kind_of(s: &str) -> JobKind {
    match s {
        "import" => JobKind::Import,
        "mock_data" => JobKind::MockData,
        _ => JobKind::Export,
    }
}

/// A job's live state, or its stored report once it has left memory.
pub async fn view(
    state: &AppState,
    user_id: Uuid,
    id: Uuid,
    kind: Option<JobKind>,
) -> AppResult<JobView> {
    if let Some(job) = state
        .jobs
        .get(user_id, id)
        .filter(|j| kind.is_none_or_eq(j.kind))
    {
        return Ok(job.view());
    }
    stored_view(state, user_id, id, kind).await
}

trait KindFilter {
    fn is_none_or_eq(&self, kind: JobKind) -> bool;
}

impl KindFilter for Option<JobKind> {
    fn is_none_or_eq(&self, kind: JobKind) -> bool {
        self.map_or(true, |k| k == kind)
    }
}

/// A finished job's report from `data_jobs`.
pub async fn stored_view(
    state: &AppState,
    user_id: Uuid,
    id: Uuid,
    kind: Option<JobKind>,
) -> AppResult<JobView> {
    let stored = repo::get(&state.db, user_id, id)
        .await?
        .filter(|s| kind.is_none_or_eq(kind_of(&s.record.kind)))
        .ok_or(AppError::NotFound("job"))?;
    if let Some(report) = stored.report {
        return Ok(report.0);
    }
    let r = stored.record;
    let mut view = JobView::new(r.id, kind_of(&r.kind), Uuid::nil(), JobStatus::Failed);
    view.connection_id = r.connection_id;
    view.target = r.target;
    view.rows_processed = r.rows_processed as u64;
    view.rows_failed = r.rows_failed as u64;
    view.started_at = Some(r.started_at);
    view.finished_at = r.finished_at;
    view.status = match r.status.as_str() {
        "completed" => JobStatus::Completed,
        "cancelled" => JobStatus::Cancelled,
        "running" => JobStatus::Running,
        _ => JobStatus::Failed,
    };
    Ok(view)
}

pub async fn list(state: &AppState, user_id: Uuid, q: &JobsQuery) -> AppResult<Vec<JobRecord>> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    Ok(repo::list(&state.db, user_id, q.kind, limit).await?)
}

/// Cancels a running job, or discards an import that has not run.
pub fn cancel(state: &AppState, user_id: Uuid, id: Uuid) -> AppResult<()> {
    let job = state
        .jobs
        .get(user_id, id)
        .ok_or(AppError::NotFound("job"))?;
    match job.status() {
        JobStatus::Running => job.request_cancel(),
        s if s.is_finished() => {
            return Err(AppError::Conflict("the job has already finished".into()))
        }
        _ => state.jobs.remove(id),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(kind: JobKind) -> JobView {
        JobView::new(Uuid::new_v4(), kind, Uuid::new_v4(), JobStatus::Ready)
    }

    #[test]
    fn reservations_respect_the_budget_and_are_released() {
        let jobs = Arc::new(DataJobs::new(100));
        let a = jobs.reserve(60).unwrap();
        assert!(jobs.reserve(50).is_err());
        let b = jobs.reserve(40).unwrap();
        assert_eq!(jobs.reserved_bytes(), 100);
        drop(a);
        drop(b);
        assert_eq!(jobs.reserved_bytes(), 0);
    }

    #[test]
    fn active_jobs_are_limited_per_user_and_private() {
        let jobs = DataJobs::new(100);
        let user = Uuid::new_v4();
        let mut ids = Vec::new();
        for _ in 0..MAX_ACTIVE_JOBS_PER_USER {
            let v = view(JobKind::Import);
            ids.push(v.id);
            jobs.create(user, Uuid::new_v4(), v, None).unwrap();
        }
        assert!(jobs
            .create(user, Uuid::new_v4(), view(JobKind::Import), None)
            .is_err());
        // Another user is unaffected and cannot see the first user's jobs.
        let other = Uuid::new_v4();
        jobs.create(other, Uuid::new_v4(), view(JobKind::MockData), None)
            .unwrap();
        assert!(jobs.get(other, ids[0]).is_none());
        assert!(jobs.get(user, ids[0]).is_some());
    }

    #[test]
    fn jobs_start_once() {
        let jobs = DataJobs::new(100);
        let job = jobs
            .create(Uuid::new_v4(), Uuid::new_v4(), view(JobKind::Import), None)
            .unwrap();
        job.start(Some("s.t".into())).unwrap();
        assert!(job.start(None).is_err());
        assert_eq!(job.view().target.as_deref(), Some("s.t"));
    }
}
