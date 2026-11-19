use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    /// Human-readable stage, e.g. "Downloading assets".
    pub stage: String,
    pub done: u64,
    pub total: u64,
}

pub type ProgressFn = Arc<dyn Fn(Progress) + Send + Sync>;

pub fn report(p: &ProgressFn, stage: &str, done: u64, total: u64) {
    p(Progress {
        stage: stage.to_string(),
        done,
        total,
    });
}

pub fn noop() -> ProgressFn {
    Arc::new(|_| {})
}
