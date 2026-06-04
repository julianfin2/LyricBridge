use futures_util::StreamExt;
use serde::Serialize;
use tauri::Emitter;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;

const BRIDGE_ADDR: &str = "127.0.0.1:32190";

#[derive(Clone, Serialize)]
struct BridgeServerStatus {
    address: &'static str,
    running: bool,
    error: Option<String>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();

            tauri::async_runtime::spawn(async move {
                run_bridge_server(handle).await;
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

async fn run_bridge_server(app: tauri::AppHandle) {
    let listener = match TcpListener::bind(BRIDGE_ADDR).await {
        Ok(listener) => {
            let _ = app.emit(
                "bridge-server-status",
                BridgeServerStatus {
                    address: BRIDGE_ADDR,
                    running: true,
                    error: None,
                },
            );
            listener
        }
        Err(error) => {
            let _ = app.emit(
                "bridge-server-status",
                BridgeServerStatus {
                    address: BRIDGE_ADDR,
                    running: false,
                    error: Some(error.to_string()),
                },
            );
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
