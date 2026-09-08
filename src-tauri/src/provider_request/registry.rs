use super::contracts::{
    ProviderRequestConnection, ProviderRequestPreparation, ProviderRequestResult, REQUEST_PROMPT,
};
use crate::commands::error::CommandError;
use std::collections::BTreeMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::async_runtime::{JoinHandle, Mutex as AsyncMutex};

const PREPARATION_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_REQUESTS: usize = 64;

type TaskResult = Result<ProviderRequestResult, CommandError>;
pub(super) type SharedTask = Arc<AsyncMutex<Option<JoinHandle<TaskResult>>>>;

#[derive(Clone)]
pub(super) struct PreparedProvider {
    pub source: PreparedSource,
    pub created_at: Instant,
}

#[derive(Clone)]
pub(super) enum PreparedSource {
    Saved {
        profile_id: String,
        file_hash: String,
    },
    Draft(ProviderRequestConnection),
}

enum Entry {
    Prepared(PreparedProvider),
    Running {
        abort: Box<dyn Fn() + Send>,
        task: SharedTask,
        cancelled: bool,
    },
}

#[derive(Clone, Default)]
pub(crate) struct ProviderRequests(Arc<Mutex<BTreeMap<String, Entry>>>);

impl ProviderRequests {
    pub(super) fn issue(
        &self,
        connection: &ProviderRequestConnection,
        source: PreparedSource,
        endpoint: String,
    ) -> Result<ProviderRequestPreparation, CommandError> {
        let mut entries = self.0.lock().map_err(|_| registry_error())?;
        entries.retain(|_, entry| match entry {
            Entry::Prepared(prepared) => prepared.created_at.elapsed() < PREPARATION_TTL,
            Entry::Running { .. } => true,
        });
        if entries.len() >= MAX_REQUESTS {
            return Err(CommandError::new(
                "provider-request-limit",
                "待处理请求过多，请关闭其他请求面板后重试",
            ));
        }
        let request_id = uuid::Uuid::new_v4().to_string();
        entries.insert(
            request_id.clone(),
            Entry::Prepared(PreparedProvider {
                source,
                created_at: Instant::now(),
            }),
        );
        Ok(ProviderRequestPreparation {
            request_id,
            endpoint,
            upstream_protocol: connection.upstream_protocol,
            default_model: connection.default_model.clone(),
            prompt: REQUEST_PROMPT,
        })
    }

    pub(super) fn inspect(&self, request_id: &str) -> Result<PreparedProvider, CommandError> {
        let mut entries = self.0.lock().map_err(|_| registry_error())?;
        match entries.get(request_id) {
            Some(Entry::Prepared(prepared)) if prepared.created_at.elapsed() < PREPARATION_TTL => {
                Ok(prepared.clone())
            }
            Some(Entry::Prepared(_)) => {
                entries.remove(request_id);
                Err(unavailable())
            }
            _ => Err(unavailable()),
        }
    }

    pub(super) fn start(
        &self,
        request_id: &str,
        future: impl Future<Output = TaskResult> + Send + 'static,
    ) -> Result<(SharedTask, ActiveRequest), CommandError> {
        let mut entries = self.0.lock().map_err(|_| registry_error())?;
        match entries.get(request_id) {
            Some(Entry::Prepared(prepared)) if prepared.created_at.elapsed() < PREPARATION_TTL => {}
            Some(Entry::Prepared(_)) => {
                entries.remove(request_id);
                return Err(unavailable());
            }
            _ => return Err(unavailable()),
        }
        let handle = tauri::async_runtime::spawn(future);
        let abort_handle = handle.inner().abort_handle();
        let task = Arc::new(AsyncMutex::new(Some(handle)));
        entries.insert(
            request_id.to_string(),
            Entry::Running {
                abort: Box::new(move || abort_handle.abort()),
                task: task.clone(),
                cancelled: false,
            },
        );
        let active = ActiveRequest {
            requests: self.clone(),
            request_id: request_id.to_string(),
        };
        Ok((task, active))
    }

    pub(crate) async fn cancel(&self, request_id: &str) -> Result<bool, CommandError> {
        let task = {
            let mut entries = self.0.lock().map_err(|_| registry_error())?;
            match entries.get_mut(request_id) {
                Some(Entry::Running {
                    abort,
                    task,
                    cancelled,
                }) => {
                    *cancelled = true;
                    abort();
                    Some(task.clone())
                }
                Some(Entry::Prepared(_)) => {
                    entries.remove(request_id);
                    None
                }
                None => return Ok(false),
            }
        };
        if let Some(task) = task {
            // Await destruction of the HTTP future, not just the abort signal.
            let _ = join(&task).await;
        }
        Ok(true)
    }
}

pub(super) async fn join(task: &SharedTask) -> Option<tauri::Result<TaskResult>> {
    let mut task = task.lock().await;
    let result = match task.as_mut() {
        Some(handle) => Some(handle.await),
        None => None,
    };
    task.take();
    result
}

pub(super) struct ActiveRequest {
    requests: ProviderRequests,
    request_id: String,
}

impl ActiveRequest {
    pub(super) fn finish(self) -> Result<bool, CommandError> {
        let mut entries = self.requests.0.lock().map_err(|_| registry_error())?;
        match entries.remove(&self.request_id) {
            Some(Entry::Running { cancelled, .. }) => Ok(cancelled),
            _ => Err(unavailable()),
        }
    }
}

impl Drop for ActiveRequest {
    fn drop(&mut self) {
        if let Ok(mut entries) = self.requests.0.lock() {
            if let Some(Entry::Running { abort, .. }) = entries.remove(&self.request_id) {
                abort();
            }
        }
    }
}

fn unavailable() -> CommandError {
    CommandError::new(
        "provider-request-unavailable",
        "请求准备已失效、已取消或已经使用，请重新准备后发送",
    )
}

fn registry_error() -> CommandError {
    CommandError::new(
        "provider-request-state",
        "请求状态不可用，请重新打开应用后重试",
    )
}

#[cfg(test)]
mod tests;
