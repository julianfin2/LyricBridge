use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, PhysicalPosition, PhysicalSize, Position, Size, WebviewWindow, WindowEvent,
};
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;

const BRIDGE_ADDR: &str = "127.0.0.1:32190";
const DEFAULT_BINDING_SOURCE_URL: &str = "https://karlblue.github.io/lyricbridge-bindings/";
const TRAY_SHOW_MAIN_ID: &str = "show-main";
const TRAY_TOGGLE_LYRICS_ID: &str = "toggle-lyrics";
const TRAY_EXIT_ID: &str = "exit";

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
    exiting: AtomicBool,
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
async fn get_config_directory_status(app: tauri::AppHandle) -> ConfigDirectoryStatus {
    config_directory_status(&app).await
}

#[tauri::command]
async fn set_config_directory(
    app: tauri::AppHandle,
    directory: Option<String>,
) -> Result<ConfigDirectoryStatus, String> {
    let directory = Some(normalize_binding_source_url(directory));

    write_config_directory_settings(&app, &ConfigDirectorySettings { directory })?;
    Ok(config_directory_status(&app).await)
}

#[tauri::command]
async fn sync_remote_bindings(
    app: tauri::AppHandle,
    source_url: Option<String>,
) -> Result<ConfigDirectoryStatus, String> {
    let source_url = normalize_binding_source_url(source_url);
    write_config_directory_settings(
        &app,
        &ConfigDirectorySettings {
            directory: Some(source_url.clone()),
        },
    )?;
    download_bindings_file(&app, &source_url).await?;
    Ok(config_directory_status(&app).await)
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
async fn get_lyric_binding(
    app: tauri::AppHandle,
    video_id: String,
) -> Result<Option<LyricBindingWithContent>, String> {
    read_external_lyric_binding(&app, &video_id, LyricLoadMode::DownloadMissing).await
}

#[tauri::command]
async fn get_local_lyric_binding(
    app: tauri::AppHandle,
    video_id: String,
) -> Result<Option<LyricBindingWithContent>, String> {
    read_local_external_lyric_binding(&app, &video_id).await
}

#[tauri::command]
async fn update_lyric_binding(
    app: tauri::AppHandle,
    video_id: String,
) -> Result<Option<LyricBindingWithContent>, String> {
    read_external_lyric_binding(&app, &video_id, LyricLoadMode::ForceDownload).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(BridgeState {
            server_status: Mutex::new(BridgeServerStatus::offline(None)),
            connection_status: Mutex::new(BridgeConnectionStatus::default()),
            saving_overlay_settings: Mutex::new(false),
            exiting: AtomicBool::new(false),
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
            setup_tray(app.handle())?;
            register_main_window_events(app.handle())?;
            register_overlay_window_events(app.handle())?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_bridge_connection_status,
            get_bridge_server_status,
            get_config_directory_status,
            get_local_lyric_binding,
            get_overlay_settings,
            reset_overlay_position,
            set_config_directory,
            set_overlay_always_on_top,
            set_overlay_locked,
            set_overlay_visible,
            start_overlay_drag,
            sync_remote_bindings,
            update_lyric_binding,
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

fn setup_tray(app: &tauri::AppHandle) -> Result<(), String> {
    let show_main = MenuItem::with_id(app, TRAY_SHOW_MAIN_ID, "显示主窗口", true, None::<&str>)
        .map_err(|error| format!("创建托盘菜单失败：{error}"))?;
    let toggle_lyrics = MenuItem::with_id(
        app,
        TRAY_TOGGLE_LYRICS_ID,
        "显示/隐藏桌面歌词",
        true,
        None::<&str>,
    )
    .map_err(|error| format!("创建托盘菜单失败：{error}"))?;
    let separator =
        PredefinedMenuItem::separator(app).map_err(|error| format!("创建托盘菜单失败：{error}"))?;
    let exit = MenuItem::with_id(app, TRAY_EXIT_ID, "退出 LyricBridge", true, None::<&str>)
        .map_err(|error| format!("创建托盘菜单失败：{error}"))?;

    let menu = Menu::with_items(app, &[&show_main, &toggle_lyrics, &separator, &exit])
        .map_err(|error| format!("创建托盘菜单失败：{error}"))?;
    let mut tray = TrayIconBuilder::with_id("lyricbridge")
        .menu(&menu)
        .tooltip("LyricBridge")
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = show_main_window(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            TRAY_SHOW_MAIN_ID => {
                let _ = show_main_window(app);
            }
            TRAY_TOGGLE_LYRICS_ID => {
                let _ = toggle_overlay_visible(app);
            }
            TRAY_EXIT_ID => {
                exit_app(app);
            }
            _ => {}
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    tray.build(app)
        .map(|_| ())
        .map_err(|error| format!("创建系统托盘图标失败：{error}"))
}

fn register_main_window_events(app: &tauri::AppHandle) -> Result<(), String> {
    let window = main_window(app)?;
    let app = app.clone();

    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            if app.state::<BridgeState>().exiting.load(Ordering::SeqCst) {
                return;
            }

            api.prevent_close();
            let _ = hide_main_window(&app);
        }
    });

    Ok(())
}

fn show_main_window(app: &tauri::AppHandle) -> Result<(), String> {
    let window = main_window(app)?;
    window
        .show()
        .map_err(|error| format!("显示主窗口失败：{error}"))?;
    window
        .set_focus()
        .map_err(|error| format!("聚焦主窗口失败：{error}"))
}

fn hide_main_window(app: &tauri::AppHandle) -> Result<(), String> {
    main_window(app)?
        .hide()
        .map_err(|error| format!("隐藏主窗口失败：{error}"))
}

fn toggle_overlay_visible(app: &tauri::AppHandle) -> Result<(), String> {
    let mut settings = read_overlay_settings(app)?;
    settings.visible = !settings.visible;
    write_overlay_settings(app, &settings)?;
    apply_overlay_settings(app, &settings)?;
    emit_overlay_settings_changed(app, &settings);
    Ok(())
}

fn exit_app(app: &tauri::AppHandle) {
    app.state::<BridgeState>()
        .exiting
        .store(true, Ordering::SeqCst);
    app.exit(0);
}

enum LyricLoadMode {
    DownloadMissing,
    ForceDownload,
}

async fn read_external_lyric_binding(
    app: &tauri::AppHandle,
    video_id: &str,
    mode: LyricLoadMode,
) -> Result<Option<LyricBindingWithContent>, String> {
    let source_url = configured_binding_source_url(app)?;
    let store = read_external_bindings_store_or_sync(app, &source_url).await?;
    let Some(binding) = store
        .bindings
        .into_iter()
        .find(|binding| binding.video_id == video_id)
    else {
        return Ok(None);
    };

    let lyric_path = resolve_cached_lyric_path(app, &binding.lyric_file)?;

    if matches!(mode, LyricLoadMode::ForceDownload)
        || matches!(mode, LyricLoadMode::DownloadMissing) && !lyric_path.exists()
    {
        download_lyric_file(app, &source_url, &binding.lyric_file).await?;
    }

    read_binding_lyric_content(binding, lyric_path)
}

async fn read_local_external_lyric_binding(
    app: &tauri::AppHandle,
    video_id: &str,
) -> Result<Option<LyricBindingWithContent>, String> {
    let source_url = configured_binding_source_url(app)?;
    let store = read_external_bindings_store_or_sync(app, &source_url).await?;
    let Some(binding) = store
        .bindings
        .into_iter()
        .find(|binding| binding.video_id == video_id)
    else {
        return Ok(None);
    };

    let lyric_path = resolve_cached_lyric_path(app, &binding.lyric_file)?;
    read_binding_lyric_content(binding, lyric_path)
}

fn read_binding_lyric_content(
    binding: ExternalLyricBinding,
    lyric_path: PathBuf,
) -> Result<Option<LyricBindingWithContent>, String> {
    let lyric_text = fs::read_to_string(&lyric_path)
        .map_err(|error| format!("读取本地歌词缓存失败：{error}"))?;

    Ok(Some(LyricBindingWithContent {
        video_id: binding.video_id,
        lyric_file_path: lyric_path.to_string_lossy().into_owned(),
        offset_ms: binding.offset_ms,
        lyric_text,
    }))
}

fn read_external_bindings_store(app: &tauri::AppHandle) -> Result<ExternalBindingsStore, String> {
    let path = bindings_cache_path(app)?;
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("读取本地 bindings.json 失败，请先同步绑定：{error}"))?;
    serde_json::from_str(&text).map_err(|error| format!("解析本地 bindings.json 失败：{error}"))
}

async fn read_external_bindings_store_or_sync(
    app: &tauri::AppHandle,
    source_url: &str,
) -> Result<ExternalBindingsStore, String> {
    if !bindings_cache_path(app)?
        .try_exists()
        .map_err(|error| format!("检查本地 bindings.json 失败：{error}"))?
    {
        download_bindings_file(app, source_url).await?;
    }

    read_external_bindings_store(app)
}

async fn download_bindings_file(app: &tauri::AppHandle, source_url: &str) -> Result<(), String> {
    let url = join_remote_path(source_url, "bindings.json")?;
    let text = download_text(url.as_str()).await?;
    serde_json::from_str::<ExternalBindingsStore>(&text)
        .map_err(|error| format!("远程 bindings.json 格式无效：{error}"))?;

    let path = bindings_cache_path(app)?;
    write_cached_text(&path, &text, "写入 bindings.json 缓存失败")
}

async fn download_lyric_file(
    app: &tauri::AppHandle,
    source_url: &str,
    lyric_file: &str,
) -> Result<(), String> {
    let url = join_remote_path(source_url, lyric_file)?;
    let text = download_text(url.as_str()).await?;
    let path = resolve_cached_lyric_path(app, lyric_file)?;
    write_cached_text(&path, &text, "写入歌词缓存失败")
}

async fn download_text(url: &str) -> Result<String, String> {
    let response = reqwest::get(url)
        .await
        .map_err(|error| format!("下载 {url} 失败：{error}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("下载 {url} 失败：HTTP {status}"));
    }

    response
        .text()
        .await
        .map_err(|error| format!("读取 {url} 响应失败：{error}"))
}

fn write_cached_text(path: &Path, text: &str, message: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建缓存目录失败：{error}"))?;
    }

    fs::write(path, text).map_err(|error| format!("{message}：{error}"))
}

fn join_remote_path(source_url: &str, path: &str) -> Result<reqwest::Url, String> {
    let base = ensure_trailing_slash(source_url);
    let url = reqwest::Url::parse(&base).map_err(|error| format!("绑定源网址无效：{error}"))?;
    url.join(path)
        .map_err(|error| format!("拼接远程文件网址失败：{error}"))
}

fn resolve_cached_lyric_path(app: &tauri::AppHandle, lyric_file: &str) -> Result<PathBuf, String> {
    Ok(bindings_cache_dir(app)?.join(safe_relative_path(lyric_file)?))
}

fn safe_relative_path(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    if path.is_absolute() {
        return Err("lyricFile 不能是绝对路径".to_string());
    }

    let mut safe_path = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => safe_path.push(value),
            Component::CurDir => {}
            _ => return Err("lyricFile 只能使用缓存目录内的相对路径".to_string()),
        }
    }

    if safe_path.as_os_str().is_empty() {
        return Err("lyricFile 不能为空".to_string());
    }

    Ok(safe_path)
}

async fn config_directory_status(app: &tauri::AppHandle) -> ConfigDirectoryStatus {
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

    let source_url = settings.directory.unwrap_or_else(default_binding_source_url);
    let directory = Some(source_url.clone());

    match read_external_bindings_store_or_sync(app, &source_url).await {
        Ok(store) => ConfigDirectoryStatus {
            directory,
            binding_count: store.bindings.len(),
            error: None,
        },
        Err(error) => ConfigDirectoryStatus {
            directory,
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
        return Ok(ConfigDirectorySettings {
            directory: Some(default_binding_source_url()),
        });
    }

    let text =
        fs::read_to_string(&path).map_err(|error| format!("读取配置目录设置失败：{error}"))?;
    serde_json::from_str(&text).map_err(|error| format!("解析配置目录设置失败：{error}"))
}

fn configured_binding_source_url(app: &tauri::AppHandle) -> Result<String, String> {
    Ok(read_config_directory_settings(app)?
        .directory
        .unwrap_or_else(default_binding_source_url))
}

fn normalize_binding_source_url(value: Option<String>) -> String {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(default_binding_source_url)
}

fn default_binding_source_url() -> String {
    DEFAULT_BINDING_SOURCE_URL.to_string()
}

fn ensure_trailing_slash(value: &str) -> String {
    if value.ends_with('/') {
        value.to_string()
    } else {
        format!("{value}/")
    }
}

fn write_config_directory_settings(
    app: &tauri::AppHandle,
    settings: &ConfigDirectorySettings,
) -> Result<(), String> {
    let path = config_directory_settings_path(app)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建应用数据目录失败：{error}"))?;
    }

    let text = serde_json::to_string_pretty(settings)
        .map_err(|error| format!("序列化配置目录设置失败：{error}"))?;
    fs::write(path, text).map_err(|error| format!("写入配置目录设置失败：{error}"))
}

fn config_directory_settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("config-directory.json"))
        .map_err(|error| format!("解析应用数据目录失败：{error}"))
}

fn bindings_cache_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("bindings-cache"))
        .map_err(|error| format!("解析应用数据目录失败：{error}"))
}

fn bindings_cache_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(bindings_cache_dir(app)?.join("bindings.json"))
}

fn read_overlay_settings(app: &tauri::AppHandle) -> Result<OverlaySettings, String> {
    let path = overlay_settings_path(app)?;

    if !path.exists() {
        return Ok(OverlaySettings::default());
    }

    let text =
        fs::read_to_string(&path).map_err(|error| format!("读取桌面歌词窗口设置失败：{error}"))?;
    serde_json::from_str(&text).map_err(|error| format!("解析桌面歌词窗口设置失败：{error}"))
}

fn write_overlay_settings(
    app: &tauri::AppHandle,
    settings: &OverlaySettings,
) -> Result<(), String> {
    let path = overlay_settings_path(app)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建应用数据目录失败：{error}"))?;
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

fn main_window(app: &tauri::AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("main")
        .ok_or_else(|| "主窗口不可用".to_string())
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
