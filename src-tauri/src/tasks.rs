use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::Emitter;

#[derive(Clone, Debug, Serialize)]
pub struct TaskSnapshot {
    pub id: String,
    pub kind: String,
    pub state: String,
    pub phase: String,
    pub current: u64,
    pub total: Option<u64>,
    pub path: String,
    pub started_at: u64,
    pub updated_at: u64,
    pub cancellable: bool,
}
struct Entry {
    snapshot: TaskSnapshot,
    cancel: Arc<AtomicBool>,
}
static TASKS: LazyLock<Mutex<HashMap<String, Entry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn running(state: &str) -> bool {
    matches!(state, "running" | "cancelling")
}
fn mutation(kind: &str) -> bool {
    matches!(
        kind,
        "clean_items"
            | "trash_files"
            | "uninstall_app_plan"
            | "uninstall_manual_app"
            | "upgrade_software"
            | "upgrade_tool"
            | "refresh_software_sources"
    )
}
fn cancellable(kind: &str) -> bool {
    matches!(
        kind,
        "scan_junk"
            | "scan_developer_junk"
            | "scan_large_files"
            | "scan_duplicate_files"
            | "analyze_disk"
            | "check_software_updates"
            | "get_installed_apps_report"
    )
}

pub struct TaskContext {
    id: String,
    cancel: Arc<AtomicBool>,
    app: Option<tauri::AppHandle>,
    finished: AtomicBool,
    last_emit: Mutex<Instant>,
}
impl TaskContext {
    pub fn begin(
        kind: &str,
        task_id: Option<String>,
        app: Option<tauri::AppHandle>,
    ) -> Result<Self, String> {
        let id = task_id
            .unwrap_or_else(|| format!("{}-{}", now(), NEXT_ID.fetch_add(1, Ordering::Relaxed)));
        if id.is_empty()
            || id.len() > 100
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_:".contains(c))
        {
            return Err("任务标识无效".into());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let stamp = now();
        let snapshot = TaskSnapshot {
            id: id.clone(),
            kind: kind.into(),
            state: "running".into(),
            phase: "准备中".into(),
            current: 0,
            total: None,
            path: String::new(),
            started_at: stamp,
            updated_at: stamp,
            cancellable: cancellable(kind),
        };
        {
            let mut entries = TASKS.lock().map_err(|e| e.to_string())?;
            if entries.contains_key(&id) {
                return Err("任务标识已经使用".into());
            }
            if entries.values().any(|e| {
                running(&e.snapshot.state)
                    && (e.snapshot.kind == kind || mutation(kind) && mutation(&e.snapshot.kind))
            }) {
                return Err("已有同类任务或处理操作正在运行，请等待完成".into());
            }
            if entries.len() >= 60 {
                let mut completed: Vec<_> = entries
                    .iter()
                    .filter(|(_, e)| !running(&e.snapshot.state))
                    .map(|(id, e)| (id.clone(), e.snapshot.updated_at))
                    .collect();
                completed.sort_by_key(|(_, t)| *t);
                for (id, _) in completed.into_iter().take(20) {
                    entries.remove(&id);
                }
            }
            entries.insert(
                id.clone(),
                Entry {
                    snapshot: snapshot.clone(),
                    cancel: cancel.clone(),
                },
            );
        }
        if let Some(app) = &app {
            let _ = app.emit("task-progress", &snapshot);
        }
        Ok(Self {
            id,
            cancel,
            app,
            finished: AtomicBool::new(false),
            last_emit: Mutex::new(Instant::now() - Duration::from_secs(1)),
        })
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    pub fn checkpoint(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err("扫描已停止，已发现结果保留".into())
        } else {
            Ok(())
        }
    }
    pub fn progress(&self, phase: &str, current: u64, total: Option<u64>, path: Option<&str>) {
        if self.finished.load(Ordering::Relaxed) {
            return;
        }
        let Ok(mut last) = self.last_emit.lock() else {
            return;
        };
        if last.elapsed() < Duration::from_millis(100) && total != Some(current) {
            return;
        }
        *last = Instant::now();
        self.update(None, phase, current, total, path);
    }
    fn update(
        &self,
        state: Option<&str>,
        phase: &str,
        current: u64,
        total: Option<u64>,
        path: Option<&str>,
    ) {
        let snapshot = {
            let Ok(mut entries) = TASKS.lock() else {
                return;
            };
            let Some(entry) = entries.get_mut(&self.id) else {
                return;
            };
            if let Some(state) = state {
                entry.snapshot.state = state.into();
            }
            entry.snapshot.phase = phase.chars().take(300).collect();
            entry.snapshot.current = current;
            entry.snapshot.total = total;
            if let Some(path) = path {
                entry.snapshot.path = path.chars().take(2000).collect();
            }
            entry.snapshot.updated_at = now();
            entry.snapshot.clone()
        };
        if let Some(app) = &self.app {
            let _ = app.emit("task-progress", snapshot);
        }
    }
    pub fn finish(&self, state: &str, message: &str) {
        if self.finished.swap(true, Ordering::Relaxed) {
            return;
        }
        let state = if self.is_cancelled() {
            "cancelled"
        } else if matches!(state, "failed" | "cancelled") {
            state
        } else {
            "completed"
        };
        let (current, total) = TASKS
            .lock()
            .ok()
            .and_then(|entries| {
                entries
                    .get(&self.id)
                    .map(|e| (e.snapshot.current, e.snapshot.total))
            })
            .unwrap_or((0, None));
        self.update(Some(state), message, current, total, None);
    }
}
impl Drop for TaskContext {
    fn drop(&mut self) {
        if !self.finished.load(Ordering::Relaxed) {
            self.finish(
                if std::thread::panicking() {
                    "failed"
                } else {
                    "completed"
                },
                if self.is_cancelled() {
                    "已停止，保留已完成结果"
                } else {
                    "任务完成"
                },
            );
        }
    }
}
pub fn list() -> Vec<TaskSnapshot> {
    let mut rows = TASKS
        .lock()
        .map(|entries| {
            entries
                .values()
                .map(|e| e.snapshot.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    rows.sort_by_key(|e| std::cmp::Reverse(e.started_at));
    rows
}
pub fn cancel(id: &str, app: Option<&tauri::AppHandle>) -> Result<(), String> {
    let snapshot = {
        let mut entries = TASKS.lock().map_err(|e| e.to_string())?;
        let entry = entries.get_mut(id).ok_or("任务不存在")?;
        if !running(&entry.snapshot.state) {
            return Ok(());
        }
        if !entry.snapshot.cancellable {
            return Err("处理已经开始，不能强制中断；可停止队列后续项目".into());
        }
        entry.cancel.store(true, Ordering::Relaxed);
        entry.snapshot.state = "cancelling".into();
        entry.snapshot.phase = "正在停止，保留已完成结果".into();
        entry.snapshot.updated_at = now();
        entry.snapshot.clone()
    };
    if let Some(app) = app {
        let _ = app.emit("task-progress", snapshot);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_is_cooperative_and_releases_kind() {
        let task = TaskContext::begin(
            "scan_duplicate_files",
            Some("task-test-cancel".into()),
            None,
        )
        .unwrap();
        assert!(TaskContext::begin("scan_duplicate_files", None, None).is_err());
        cancel("task-test-cancel", None).unwrap();
        assert!(task.is_cancelled());
        assert!(task.checkpoint().is_err());
        task.finish("completed", "partial");
        assert_eq!(
            list()
                .into_iter()
                .find(|e| e.id == "task-test-cancel")
                .unwrap()
                .state,
            "cancelled"
        );
        let next = TaskContext::begin("scan_duplicate_files", None, None).unwrap();
        next.finish("completed", "done");
    }
    #[test]
    fn mutations_do_not_offer_force_cancel() {
        let task = TaskContext::begin("task-test-ordinary", None, None).unwrap();
        assert!(cancel(&task.id, None).is_err());
        task.finish("failed", "failure");
    }
}
