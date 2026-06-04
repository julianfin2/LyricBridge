use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::PathBuf, sync::Mutex};
use tauri::{Emitter, Manager};
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;

const BRIDGE_ADDR: &str = "127.0.0.1:32190";

#[derive(Clone, Serialize)]
struct BridgeServerStatus {
    address: &'static str,
    running: bool,
    error: Option<String>,
}

struct BridgeState {
    server_status: Mutex<BridgeServerStatus>,
}

impl BridgeServerStatus {
    fn offline(error: Option<String>) -> Self {
        Self {
            address: BRIDGE_ADDR,
            running: false,
            error,
        }
    }

    fn online() -> Self {
        Self {
            address: BRIDGE_ADDR,
            running: true,
            error: None,
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LyricBinding {
    video_id: String,
    lyric_file_path: String,
    offset_ms: i32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LyricBindingWithContent {
    video_id: String,
    lyric_file_path: String,
    offset_ms: i32,
    lyric_text: String,
}

#[derive(Default, Deserialize, Serialize)]
struct BindingsStore {
    bindings: BTreeMap<String, LyricBinding>,
}

#[tauri::command]
fn get_bridge_server_status(
    state: tauri::State<'_, BridgeState>,
) -> Result<BridgeServerStatus, String> {
    state
        .server_status
        .lock()
        .map(|status| status.clone())
        .map_err(|error| format!("Failed to read bridge server status: {error}"))
}

#[tauri::command]
fn get_lyric_binding(
    app: tauri::AppHandle,
    video_id: String,
) -> Result<Option<LyricBindingWithContent>, String> {
    let store = read_bindings_store(&app)?;
    let Some(binding) = store.bindings.get(&video_id) else {
        return Ok(None);
    };

    read_binding_content(binding).map(Some)
}

#[tauri::command]
fn save_lyric_binding(
    app: tauri::AppHandle,
    binding: LyricBinding,
) -> Result<LyricBindingWithContent, String> {
    if binding.video_id.trim().is_empty() {
        return Err("Video ID is required".to_string());
    }

    let content = read_binding_content(&binding)?;
    let mut store = read_bindings_store(&app)?;
    store.bindings.insert(binding.video_id.clone(), binding);
    write_bindings_store(&app, &store)?;

    Ok(content)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(BridgeState {
            server_status: Mutex::new(BridgeServerStatus::offline(None)),
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();

            tauri::async_runtime::spawn(async move {
                run_bridge_server(handle).await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_bridge_server_status,
            get_lyric_binding,
            save_lyric_binding
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

async fn run_bridge_server(app: tauri::AppHandle) {
    let listener = match TcpListener::bind(BRIDGE_ADDR).await {
        Ok(listener) => {
            set_bridge_server_status(&app, BridgeServerStatus::online());
            listener
        }
        Err(error) => {
            set_bridge_server_status(&app, BridgeServerStatus::offline(Some(error.to_string())));
            return;
        }
    };

    while let Ok((stream, _addr)) = listener.accept().await {
        let app = app.clone();

        tauri::async_runtime::spawn(async move {
            let Ok(mut socket) = accept_async(stream).await else {
                return;
            };

            while let Some(message) = socket.next().await {
                let Ok(message) = message else {
                    break;
                };

                if !message.is_text() {
                    continue;
                }

                let Ok(text) = message.to_text() else {
                    continue;
                };

                match serde_json::from_str::<serde_json::Value>(text) {
                    Ok(value) => {
                        let _ = app.emit("bridge-message", value);
                    }
                    Err(error) => {
                        let _ = app.emit("bridge-message-error", error.to_string());
                    }
                }
            }
        });
    }
}

fn set_bridge_server_status(app: &tauri::AppHandle, status: BridgeServerStatus) {
    if let Ok(mut current) = app.state::<BridgeState>().server_status.lock() {
        *current = status.clone();
    }

    let _ = app.emit("bridge-server-status", status);
}

fn read_binding_content(binding: &LyricBinding) -> Result<LyricBindingWithContent, String> {
    let lyric_text = fs::read_to_string(&binding.lyric_file_path)
        .map_err(|error| format!("Failed to read lyric file: {error}"))?;

    Ok(LyricBindingWithContent {
        video_id: binding.video_id.clone(),
        lyric_file_path: binding.lyric_file_path.clone(),
        offset_ms: binding.offset_ms,
        lyric_text,
    })
}

fn read_bindings_store(app: &tauri::AppHandle) -> Result<BindingsStore, String> {
    let path = bindings_store_path(app)?;

    if !path.exists() {
        return Ok(BindingsStore::default());
    }

    let text = fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read bindings store: {error}"))?;
    serde_json::from_str(&text).map_err(|error| format!("Failed to parse bindings store: {error}"))
}

fn write_bindings_store(app: &tauri::AppHandle, store: &BindingsStore) -> Result<(), String> {
    let path = bindings_store_path(app)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create app data directory: {error}"))?;
    }

    let text = serde_json::to_string_pretty(store)
        .map_err(|error| format!("Failed to serialize bindings store: {error}"))?;
    fs::write(path, text).map_err(|error| format!("Failed to write bindings store: {error}"))
}

fn bindings_store_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("bindings.json"))
        .map_err(|error| format!("Failed to resolve app data directory: {error}"))
}
