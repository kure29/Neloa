use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, RecvTimeoutError},
        Arc,
    },
    thread,
    time::Duration,
};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::{
    model::{ClipboardSnapshot, ClipboardSyncEvent, PeerDevice},
    network::NetworkHandle,
    trust::TrustStore,
    unix_millis,
};

pub(crate) const CLIPBOARD_TEXT_LIMIT: usize = 8 * 1024;
const CLIPBOARD_POLL_INTERVAL: Duration = Duration::from_millis(450);
const EVENT_CACHE_LIMIT: usize = 256;

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredSettings {
    clipboard_enabled: bool,
}

#[derive(Clone)]
struct SyncContext {
    network: NetworkHandle,
    peers: Arc<RwLock<HashMap<String, PeerDevice>>>,
    trust: TrustStore,
}

enum ClipboardCommand {
    Configure(SyncContext),
    SetEnabled(bool),
    ApplyRemote {
        event_id: String,
        text: String,
        response: oneshot::Sender<Result<bool, String>>,
    },
    WriteLocal {
        text: String,
        response: oneshot::Sender<Result<(), String>>,
    },
}

#[derive(Clone)]
pub(crate) struct ClipboardService {
    app: AppHandle,
    settings_path: PathBuf,
    enabled: Arc<AtomicBool>,
    sender: mpsc::Sender<ClipboardCommand>,
}

impl ClipboardService {
    pub(crate) fn load(app: AppHandle, settings_path: PathBuf) -> Self {
        let enabled = fs::read(&settings_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<StoredSettings>(&bytes).ok())
            .map(|settings| settings.clipboard_enabled)
            .unwrap_or(false);
        let enabled_flag = Arc::new(AtomicBool::new(enabled));
        let (sender, receiver) = mpsc::channel();
        let thread_app = app.clone();
        let thread_enabled = Arc::clone(&enabled_flag);

        thread::Builder::new()
            .name("neloa-clipboard".into())
            .spawn(move || run_clipboard(thread_app, thread_enabled, receiver))
            .expect("Neloa clipboard thread failed to start");

        Self {
            app,
            settings_path,
            enabled: enabled_flag,
            sender,
        }
    }

    pub(crate) fn configure(
        &self,
        network: NetworkHandle,
        peers: Arc<RwLock<HashMap<String, PeerDevice>>>,
        trust: TrustStore,
    ) -> Result<(), String> {
        self.sender
            .send(ClipboardCommand::Configure(SyncContext {
                network,
                peers,
                trust,
            }))
            .map_err(|_| "系统剪贴板服务未运行".to_string())
    }

    pub(crate) fn snapshot(&self) -> ClipboardSnapshot {
        ClipboardSnapshot {
            enabled: self.enabled.load(Ordering::Relaxed),
            max_bytes: CLIPBOARD_TEXT_LIMIT,
            protect_sensitive: true,
            poll_interval_ms: CLIPBOARD_POLL_INTERVAL.as_millis() as u64,
        }
    }

    pub(crate) fn set_enabled(&self, enabled: bool) -> Result<ClipboardSnapshot, String> {
        persist_settings(&self.settings_path, enabled)?;
        self.enabled.store(enabled, Ordering::Relaxed);
        self.sender
            .send(ClipboardCommand::SetEnabled(enabled))
            .map_err(|_| "系统剪贴板服务未运行".to_string())?;
        let snapshot = self.snapshot();
        self.app
            .emit("clipboard-settings-changed", snapshot.clone())
            .map_err(|error| format!("无法更新剪贴板设置：{error}"))?;
        Ok(snapshot)
    }

    pub(crate) async fn apply_remote(
        &self,
        event_id: String,
        text: String,
    ) -> Result<bool, String> {
        if !self.enabled.load(Ordering::Relaxed) {
            return Err("接收端未开启剪贴板同步".to_string());
        }
        validate_clipboard_text(&text)?;
        let (response, result) = oneshot::channel();
        self.sender
            .send(ClipboardCommand::ApplyRemote {
                event_id,
                text,
                response,
            })
            .map_err(|_| "系统剪贴板服务未运行".to_string())?;
        result
            .await
            .map_err(|_| "系统剪贴板服务没有返回写入结果".to_string())?
    }

    pub(crate) async fn write_local(&self, text: String) -> Result<(), String> {
        let (response, result) = oneshot::channel();
        self.sender
            .send(ClipboardCommand::WriteLocal { text, response })
            .map_err(|_| "系统剪贴板服务未运行".to_string())?;
        result
            .await
            .map_err(|_| "系统剪贴板服务没有返回写入结果".to_string())?
    }
}

fn persist_settings(path: &PathBuf, enabled: bool) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("无法创建设置目录：{error}"))?;
    }
    let bytes = serde_json::to_vec_pretty(&StoredSettings {
        clipboard_enabled: enabled,
    })
    .map_err(|error| format!("无法编码剪贴板设置：{error}"))?;
    fs::write(path, bytes).map_err(|error| format!("无法保存剪贴板设置：{error}"))
}

fn run_clipboard(
    app: AppHandle,
    enabled: Arc<AtomicBool>,
    receiver: mpsc::Receiver<ClipboardCommand>,
) {
    let mut context: Option<SyncContext> = None;
    let mut baseline_ready = false;
    let mut last_text: Option<String> = None;
    let mut seen = EventCache::default();

    loop {
        match receiver.recv_timeout(CLIPBOARD_POLL_INTERVAL) {
            Ok(ClipboardCommand::Configure(value)) => context = Some(value),
            Ok(ClipboardCommand::SetEnabled(value)) => {
                baseline_ready = false;
                if !value {
                    last_text = None;
                }
            }
            Ok(ClipboardCommand::ApplyRemote {
                event_id,
                text,
                response,
            }) => {
                if !enabled.load(Ordering::Relaxed) {
                    let _ = response.send(Err("接收端未开启剪贴板同步".to_string()));
                    continue;
                }
                if seen.contains(&event_id) {
                    let _ = response.send(Ok(false));
                    continue;
                }
                let outcome = app
                    .clipboard()
                    .write_text(text.clone())
                    .map_err(|error| format!("无法写入系统剪贴板：{error}"))
                    .map(|_| {
                        seen.insert(event_id);
                        last_text = Some(canonical_text(&text));
                        baseline_ready = true;
                        true
                    });
                let _ = response.send(outcome);
            }
            Ok(ClipboardCommand::WriteLocal { text, response }) => {
                let outcome = app
                    .clipboard()
                    .write_text(text.clone())
                    .map_err(|error| format!("无法写入系统剪贴板：{error}"))
                    .map(|_| {
                        last_text = Some(canonical_text(&text));
                        baseline_ready = true;
                    });
                let _ = response.send(outcome);
            }
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }

        if !enabled.load(Ordering::Relaxed) {
            continue;
        }

        let current = match app
            .clipboard()
            .read_text()
            .map_err(|error| format!("无法读取系统剪贴板：{error}"))
        {
            Ok(text) => text,
            Err(_) => continue,
        };

        if !baseline_ready {
            last_text = Some(canonical_text(&current));
            baseline_ready = true;
            continue;
        }
        let canonical = canonical_text(&current);
        if last_text.as_deref() == Some(canonical.as_str()) {
            continue;
        }
        last_text = Some(canonical);

        let event_id = Uuid::new_v4().to_string();
        if let Err(message) = validate_clipboard_text(&current) {
            emit_local_status(&app, event_id, current.len(), "blocked", &message);
            continue;
        }
        if let Some(reason) = sensitive_content_reason(&current) {
            emit_local_status(&app, event_id, current.len(), "blocked", reason);
            continue;
        }

        let Some(context) = &context else {
            emit_local_status(
                &app,
                event_id,
                current.len(),
                "failed",
                "加密网络服务尚未就绪",
            );
            continue;
        };
        let peers: Vec<_> = context
            .peers
            .read()
            .values()
            .filter(|peer| context.trust.find(&peer.id).is_some())
            .cloned()
            .collect();
        if peers.is_empty() {
            emit_local_status(
                &app,
                event_id,
                current.len(),
                "waiting",
                "没有在线的可信设备，本次复制未同步",
            );
            continue;
        }
        if let Err(message) = context
            .network
            .broadcast_clipboard(peers, event_id.clone(), current)
        {
            emit_local_status(
                &app,
                event_id,
                last_text.as_ref().map_or(0, String::len),
                "failed",
                &message,
            );
        }
    }
}

fn emit_local_status(app: &AppHandle, id: String, bytes: usize, status: &str, message: &str) {
    let _ = app.emit(
        "clipboard-sync-event",
        ClipboardSyncEvent {
            id,
            peer_id: String::new(),
            peer_name: "附近设备".to_string(),
            direction: "sent".to_string(),
            status: status.to_string(),
            bytes,
            message: message.to_string(),
            at_ms: unix_millis(),
        },
    );
}

pub(crate) fn validate_clipboard_text(text: &str) -> Result<(), String> {
    if text.is_empty() || text.chars().all(char::is_whitespace) {
        return Err("空白剪贴板不会自动同步".to_string());
    }
    if text.len() > CLIPBOARD_TEXT_LIMIT {
        return Err(format!(
            "剪贴板文本超过 {} KB，已阻止自动同步",
            CLIPBOARD_TEXT_LIMIT / 1024
        ));
    }
    Ok(())
}

fn sensitive_content_reason(text: &str) -> Option<&'static str> {
    let lower = text.to_ascii_lowercase();
    const MARKERS: [&str; 8] = [
        "authorization: bearer ",
        "client_secret=",
        "client-secret=",
        "\"private_key\"",
        "github_pat_",
        "ghp_",
        "sk-proj-",
        "xoxb-",
    ];
    (lower.contains("-----begin ") && lower.contains("private key-----")
        || MARKERS.iter().any(|marker| lower.contains(marker)))
    .then_some("检测到疑似私钥或访问凭据，已在本机阻止自动同步")
}

fn canonical_text(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

#[derive(Default)]
struct EventCache {
    ids: HashSet<String>,
    order: VecDeque<String>,
}

impl EventCache {
    fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }

    fn insert(&mut self, id: String) {
        if !self.ids.insert(id.clone()) {
            return;
        }
        self.order.push_back(id);
        while self.order.len() > EVENT_CACHE_LIMIT {
            if let Some(expired) = self.order.pop_front() {
                self.ids.remove(&expired);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_policy_limits_size_and_strong_secret_markers() {
        assert!(validate_clipboard_text("hello from Neloa").is_ok());
        assert!(validate_clipboard_text("   ").is_err());
        assert!(validate_clipboard_text(&"x".repeat(CLIPBOARD_TEXT_LIMIT + 1)).is_err());
        assert!(sensitive_content_reason("-----BEGIN PRIVATE KEY-----\nabc").is_some());
        assert!(sensitive_content_reason("-----BEGIN RSA PRIVATE KEY-----\nabc").is_some());
        assert!(sensitive_content_reason("ordinary copied text").is_none());
    }

    #[test]
    fn clipboard_comparison_normalizes_platform_line_endings() {
        assert_eq!(canonical_text("one\r\ntwo\rthree"), "one\ntwo\nthree");
    }

    #[test]
    fn clipboard_event_cache_is_bounded_and_deduplicates() {
        let mut cache = EventCache::default();
        cache.insert("same".into());
        cache.insert("same".into());
        assert_eq!(cache.order.len(), 1);
        for index in 0..=EVENT_CACHE_LIMIT {
            cache.insert(format!("event-{index}"));
        }
        assert_eq!(cache.order.len(), EVENT_CACHE_LIMIT);
        assert!(!cache.contains("same"));
        assert!(cache.contains(&format!("event-{EVENT_CACHE_LIMIT}")));
    }
}
