use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{
    Emitter, Manager, PhysicalPosition, PhysicalSize, Position, Size, WebviewWindow, WindowEvent,
};
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
    connection_status: Mutex<BridgeConnectionStatus>,
    saving_overlay_settings: Mutex<bool>,
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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgeConnectionStatus {
    connected_clients: u32,
    last_message_at: Option<u64>,
}

impl Default for BridgeConnectionStatus {
    fn default() -> Self {
        Self {
            connected_clients: 0,
            last_message_at: None,
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LyricBindingWithContent {
    video_id: String,
    lyric_file_path: String,
    offset_ms: i32,
    lyric_text: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExternalLyricBinding {
    video_id: String,
    lyric_file: String,
    offset_ms: i32,
}

#[derive(Deserialize)]
struct ExternalBindingsStore {
    bindings: Vec<ExternalLyricBinding>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigDirectorySettings {
    directory: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigDirectoryStatus {
    directory: Option<String>,
    binding_count: usize,
    error: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct OverlaySettings {
    locked: bool,
    visible: bool,
    always_on_top: bool,
    x: Option<i32>,
    y: Option<i32>,
    width: u32,
    height: u32,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            locked: false,
            visible: true,
            always_on_top: true,
            x: None,
            y: None,
            width: 1000,
            height: 150,
        }
    }
}

#[tauri::command]
fn get_bridge_server_status(
    state: tauri::State<'_, BridgeState>,
) -> Result<BridgeServerStatus, String> {
    state
        .server_status
        .lock()
        .map(|status| status.clone())
        .map_err(|error| format!("读取桥接服务状态失败：{error}"))
}

#[tauri::command]
fn get_bridge_connection_status(
    state: tauri::State<'_, BridgeState>,
) -> Result<BridgeConnectionStatus, String> {
    state
        .connection_status
        .lock()
        .map(|status| status.clone())
        .map_err(|error| format!("读取扩展连接状态失败：{error}"))
}

#[tauri::command]
fn get_overlay_settings(app: tauri::AppHandle) -> Result<OverlaySettings, String> {
    read_overlay_settings(&app)
}

#[tauri::command]
fn get_config_directory_status(app: tauri::AppHandle) -> ConfigDirectoryStatus {
    config_directory_status(&app)
}

#[tauri::command]
fn set_config_directory(
    app: tauri::AppHandle,
    directory: Option<String>,
) -> Result<ConfigDirectoryStatus, String> {
    let directory = directory.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });

    write_config_directory_settings(&app, &ConfigDirectorySettings { directory })?;
    Ok(config_directory_status(&app))
}

#[tauri::command]
fn set_overlay_visible(app: tauri::AppHandle, visible: bool) -> Result<OverlaySettings, String> {
    let mut settings = read_overlay_settings(&app)?;
    settings.visible = visible;
    write_overlay_settings(&app, &settings)?;
    apply_overlay_settings(&app, &settings)?;
    emit_overlay_settings_changed(&app, &settings);
    Ok(settings)
}

#[tauri::command]
fn set_overlay_locked(app: tauri::AppHandle, locked: bool) -> Result<OverlaySettings, String> {
    let mut settings = read_overlay_settings(&app)?;
    settings.locked = locked;
    write_overlay_settings(&app, &settings)?;
    apply_overlay_settings(&app, &settings)?;
    emit_overlay_settings_changed(&app, &settings);
    Ok(settings)
}

#[tauri::command]
fn set_overlay_always_on_top(
    app: tauri::AppHandle,
    always_on_top: bool,
) -> Result<OverlaySettings, String> {
    let mut settings = read_overlay_settings(&app)?;
    settings.always_on_top = always_on_top;
    write_overlay_settings(&app, &settings)?;
    apply_overlay_settings(&app, &settings)?;
    emit_overlay_settings_changed(&app, &settings);
    Ok(settings)
}

#[tauri::command]
fn reset_overlay_position(app: tauri::AppHandle) -> Result<OverlaySettings, String> {
    let mut settings = read_overlay_settings(&app)?;
    settings.x = None;
    settings.y = None;
    settings.width = 1000;
    settings.height = 150;
    write_overlay_settings(&app, &settings)?;
    apply_overlay_settings(&app, &settings)?;
    emit_overlay_settings_changed(&app, &settings);
    Ok(settings)
}

#[tauri::command]
fn start_overlay_drag(app: tauri::AppHandle) -> Result<(), String> {
    let window = lyrics_window(&app)?;
    window
        .start_dragging()
        .map_err(|error| format!("开始拖动桌面歌词窗口失败：{error}"))
}

#[tauri::command]
fn get_lyric_binding(
    app: tauri::AppHandle,
    video_id: String,
) -> Result<Option<LyricBindingWithContent>, String> {
    read_external_lyric_binding(&app, &video_id)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(BridgeState {
            server_status: Mutex::new(BridgeServerStatus::offline(None)),
            connection_status: Mutex::new(BridgeConnectionStatus::default()),
            saving_overlay_settings: Mutex::new(false),
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();

            tauri::async_runtime::spawn(async move {
                run_bridge_server(handle).await;
            });

            let settings = read_overlay_settings(app.handle())?;
            apply_overlay_settings(app.handle(), &settings)?;
            register_overlay_window_events(app.handle())?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_bridge_connection_status,
            get_bridge_server_status,
            get_config_directory_status,
            get_overlay_settings,
            reset_overlay_position,
            set_config_directory,
            set_overlay_always_on_top,
            set_overlay_locked,
            set_overlay_visible,
            start_overlay_drag,
            get_lyric_binding
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

            update_bridge_client_count(&app, 1);

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
                        mark_bridge_message_received(&app);
                        let _ = app.emit("bridge-message", value);
                    }
                    Err(error) => {
                        let _ = app.emit("bridge-message-error", error.to_string());
                    }
                }
            }

            update_bridge_client_count(&app, -1);
        });
    }
}

fn set_bridge_server_status(app: &tauri::AppHandle, status: BridgeServerStatus) {
    if let Ok(mut current) = app.state::<BridgeState>().server_status.lock() {
        *current = status.clone();
    }

    let _ = app.emit("bridge-server-status", status);
}

fn update_bridge_client_count(app: &tauri::AppHandle, delta: i32) {
    let status = {
        let state = app.state::<BridgeState>();
        let Ok(mut status) = state.connection_status.lock() else {
            return;
        };

        status.connected_clients = if delta.is_negative() {
            status
                .connected_clients
                .saturating_sub(delta.unsigned_abs())
        } else {
            status.connected_clients.saturating_add(delta as u32)
        };

        status.clone()
    };

    let _ = app.emit("bridge-connection-status", status);
}

fn mark_bridge_message_received(app: &tauri::AppHandle) {
    let status = {
        let state = app.state::<BridgeState>();
        let Ok(mut status) = state.connection_status.lock() else {
            return;
        };

        status.last_message_at = Some(now_ms());
        status.clone()
    };

    let _ = app.emit("bridge-connection-status", status);
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn read_external_lyric_binding(
    app: &tauri::AppHandle,
    video_id: &str,
) -> Result<Option<LyricBindingWithContent>, String> {
    let Some(config_dir) = read_config_directory_settings(app)?.directory else {
        return Ok(None);
    };

    let config_dir = PathBuf::from(config_dir);
    let store = read_external_bindings_store(&config_dir)?;
    let Some(binding) = store
        .bindings
        .into_iter()
        .find(|binding| binding.video_id == video_id)
    else {
        return Ok(None);
    };

    let lyric_path = resolve_config_lyric_path(&config_dir, &binding.lyric_file);
    let lyric_text = fs::read_to_string(&lyric_path)
        .map_err(|error| format!("读取配置歌词文件失败：{error}"))?;

    Ok(Some(LyricBindingWithContent {
        video_id: binding.video_id,
        lyric_file_path: lyric_path.to_string_lossy().into_owned(),
        offset_ms: binding.offset_ms,
        lyric_text,
    }))
}

fn read_external_bindings_store(config_dir: &Path) -> Result<ExternalBindingsStore, String> {
    let path = config_dir.join("bindings.json");
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("读取配置目录 bindings.json 失败：{error}"))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("解析配置目录 bindings.json 失败：{error}"))
}

fn resolve_config_lyric_path(config_dir: &Path, lyric_file: &str) -> PathBuf {
    let path = PathBuf::from(lyric_file);
    if path.is_absolute() {
        path
    } else {
        config_dir.join(path)
    }
}

fn config_directory_status(app: &tauri::AppHandle) -> ConfigDirectoryStatus {
    let settings = match read_config_directory_settings(app) {
        Ok(settings) => settings,
        Err(error) => {
            return ConfigDirectoryStatus {
                directory: None,
                binding_count: 0,
                error: Some(error),
            };
        }
    };

    let Some(directory) = settings.directory.clone() else {
        return ConfigDirectoryStatus {
            directory: None,
            binding_count: 0,
            error: None,
        };
    };

    match read_external_bindings_store(Path::new(&directory)) {
        Ok(store) => ConfigDirectoryStatus {
            directory: Some(directory),
            binding_count: store.bindings.len(),
            error: None,
        },
        Err(error) => ConfigDirectoryStatus {
            directory: Some(directory),
            binding_count: 0,
            error: Some(error),
        },
    }
}

fn read_config_directory_settings(
    app: &tauri::AppHandle,
) -> Result<ConfigDirectorySettings, String> {
    let path = config_directory_settings_path(app)?;

    if !path.exists() {
        return Ok(ConfigDirectorySettings { directory: None });
    }

    let text = fs::read_to_string(&path)
        .map_err(|error| format!("读取配置目录设置失败：{error}"))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("解析配置目录设置失败：{error}"))
}

fn write_config_directory_settings(
    app: &tauri::AppHandle,
    settings: &ConfigDirectorySettings,
) -> Result<(), String> {
    let path = config_directory_settings_path(app)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("创建应用数据目录失败：{error}"))?;
    }

    let text = serde_json::to_string_pretty(settings)
        .map_err(|error| format!("序列化配置目录设置失败：{error}"))?;
    fs::write(path, text)
        .map_err(|error| format!("写入配置目录设置失败：{error}"))
}

fn config_directory_settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("config-directory.json"))
        .map_err(|error| format!("解析应用数据目录失败：{error}"))
}

fn read_overlay_settings(app: &tauri::AppHandle) -> Result<OverlaySettings, String> {
    let path = overlay_settings_path(app)?;

    if !path.exists() {
        return Ok(OverlaySettings::default());
    }

    let text = fs::read_to_string(&path)
        .map_err(|error| format!("读取桌面歌词窗口设置失败：{error}"))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("解析桌面歌词窗口设置失败：{error}"))
}

fn write_overlay_settings(
    app: &tauri::AppHandle,
    settings: &OverlaySettings,
) -> Result<(), String> {
    let path = overlay_settings_path(app)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("创建应用数据目录失败：{error}"))?;
    }

    let text = serde_json::to_string_pretty(settings)
        .map_err(|error| format!("序列化桌面歌词窗口设置失败：{error}"))?;
    fs::write(path, text).map_err(|error| format!("写入桌面歌词窗口设置失败：{error}"))
}

fn overlay_settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("overlay.json"))
        .map_err(|error| format!("解析应用数据目录失败：{error}"))
}

fn apply_overlay_settings(
    app: &tauri::AppHandle,
    settings: &OverlaySettings,
) -> Result<(), String> {
    let window = lyrics_window(app)?;
    let state = app.state::<BridgeState>();
    let _guard = OverlaySaveGuard::new(&state);

    window
        .set_size(Size::Physical(PhysicalSize::new(
            settings.width,
            settings.height,
        )))
        .map_err(|error| format!("调整桌面歌词窗口大小失败：{error}"))?;

    let position = if let (Some(x), Some(y)) = (settings.x, settings.y) {
        PhysicalPosition::new(x, y)
    } else {
        default_overlay_position(&window, settings)?
    };

    window
        .set_position(Position::Physical(position))
        .map_err(|error| format!("设置桌面歌词窗口位置失败：{error}"))?;

    window
        .set_always_on_top(settings.always_on_top)
        .map_err(|error| format!("更新桌面歌词窗口置顶状态失败：{error}"))?;
    window
        .set_ignore_cursor_events(settings.locked)
        .map_err(|error| format!("更新桌面歌词窗口鼠标穿透状态失败：{error}"))?;

    if settings.visible {
        window
            .show()
            .map_err(|error| format!("显示桌面歌词窗口失败：{error}"))?;
    } else {
        window
            .hide()
            .map_err(|error| format!("隐藏桌面歌词窗口失败：{error}"))?;
    }

    Ok(())
}

fn register_overlay_window_events(app: &tauri::AppHandle) -> Result<(), String> {
    let window = lyrics_window(app)?;
    let app = app.clone();

    window.on_window_event(move |event| match event {
        WindowEvent::Moved(position) => {
            let _ = update_overlay_geometry(&app, Some(*position), None);
        }
        WindowEvent::Resized(size) => {
            let _ = update_overlay_geometry(&app, None, Some(*size));
        }
        _ => {}
    });

    Ok(())
}

fn default_overlay_position(
    window: &WebviewWindow,
    settings: &OverlaySettings,
) -> Result<PhysicalPosition<i32>, String> {
    let monitor = window
        .current_monitor()
        .map_err(|error| format!("读取当前显示器失败：{error}"))?
        .or(window
            .primary_monitor()
            .map_err(|error| format!("读取主显示器失败：{error}"))?);

    let Some(monitor) = monitor else {
        return Ok(PhysicalPosition::new(0, 0));
    };

    let monitor_position = monitor.position();
    let monitor_size = monitor.size();
    let width = settings.width as i32;
    let height = settings.height as i32;
    let monitor_width = monitor_size.width as i32;
    let monitor_height = monitor_size.height as i32;

    let x = monitor_position.x + ((monitor_width - width) / 2).max(0);
    let y = monitor_position.y + ((monitor_height as f64 * 0.90) as i32 - height / 2);
    let max_y = monitor_position.y + (monitor_height - height).max(0);

    Ok(PhysicalPosition::new(x, y.clamp(monitor_position.y, max_y)))
}

fn update_overlay_geometry(
    app: &tauri::AppHandle,
    position: Option<PhysicalPosition<i32>>,
    size: Option<PhysicalSize<u32>>,
) -> Result<(), String> {
    if app
        .state::<BridgeState>()
        .saving_overlay_settings
        .lock()
        .map(|saving| *saving)
        .unwrap_or(false)
    {
        return Ok(());
    }

    let mut settings = read_overlay_settings(app)?;

    if let Some(position) = position {
        settings.x = Some(position.x);
        settings.y = Some(position.y);
    }

    if let Some(size) = size {
        settings.width = size.width;
        settings.height = size.height;
    }

    write_overlay_settings(app, &settings)
}

fn emit_overlay_settings_changed(app: &tauri::AppHandle, settings: &OverlaySettings) {
    let _ = app.emit("overlay-settings-changed", settings.clone());
}

fn lyrics_window(app: &tauri::AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("lyrics")
        .ok_or_else(|| "桌面歌词窗口不可用".to_string())
}

struct OverlaySaveGuard<'a> {
    state: &'a BridgeState,
}

impl<'a> OverlaySaveGuard<'a> {
    fn new(state: &'a BridgeState) -> Self {
        if let Ok(mut saving) = state.saving_overlay_settings.lock() {
            *saving = true;
        }

        Self { state }
    }
}

impl Drop for OverlaySaveGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut saving) = self.state.saving_overlay_settings.lock() {
            *saving = false;
        }
    }
}
