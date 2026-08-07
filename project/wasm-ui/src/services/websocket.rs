use gloo::events::EventListener;
use js_sys::Function;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CloseEvent, ErrorEvent, MessageEvent, WebSocket};

use crate::models::ui::{ConnectionState, ProgressMessage};

/// WebSocket message wrapper for type-safe parsing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsMessage {
    Progress(ProgressMessage),
    Status { job_id: String, status: String, progress: f32 },
    Log { job_id: String, line: String, level: String },
    Complete { job_id: String, success: bool },
    Error { job_id: String, message: String },
    Ping,
    Pong,
}

/// Callback type for WebSocket message handlers
pub type WsCallback = Box<dyn Fn(WsMessage)>;

/// WebSocket client for real-time job updates
pub struct WsClient {
    ws: Option<WebSocket>,
    url: String,
    state: Rc<RefCell<ConnectionState>>,
    _on_message: Option<EventListener>,
    _on_open: Option<EventListener>,
    _on_close: Option<EventListener>,
    _on_error: Option<EventListener>,
    _callback: Rc<RefCell<Option<WsCallback>>>,
}

impl WsClient {
    /// Connect to the WebSocket server
    pub fn connect(url: &str, callback: WsCallback) -> Result<Self, String> {
        log::info!("Connecting to WebSocket: {}", url);

        let ws = WebSocket::new(url).map_err(|e| {
            let msg = format!("Failed to create WebSocket: {:?}", e);
            log::error!("{}", msg);
            msg
        })?;

        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        let state = Rc::new(RefCell::new(ConnectionState::Connecting));
        let callback_rc = Rc::new(RefCell::new(Some(callback)));

        // On open
        let state_open = state.clone();
        let on_open = EventListener::new(&ws, "open", move |_event| {
            log::info!("WebSocket connection opened");
            *state_open.borrow_mut() = ConnectionState::Connected;
        });

        // On message
        let callback_msg = callback_rc.clone();
        let state_msg = state.clone();
        let on_message = EventListener::new(&ws, "message", move |event| {
            let msg_event = event.dyn_ref::<MessageEvent>().unwrap();
            if let Ok(text) = msg_event.data().dyn_into::<js_sys::JsString>() {
                let text_str = String::from(text);
                log::debug!("WebSocket message received: {}", text_str);

                match serde_json::from_str::<WsMessage>(&text_str) {
                    Ok(parsed) => {
                        if let Ok(cb_ref) = callback_msg.try_borrow() {
                            if let Some(cb) = cb_ref.as_ref() {
                                cb(parsed);
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to parse WebSocket message: {} - raw: {}", e, text_str);
                        // Try parsing as generic progress
                        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text_str) {
                            if let Some(job_id) = value.get("job_id").and_then(|v| v.as_str()) {
                                let msg = WsMessage::Progress(ProgressMessage::new(
                                    job_id,
                                    value.get("stage").and_then(|v| v.as_str()).unwrap_or("unknown"),
                                    value.get("stage_label").and_then(|v| v.as_str()).unwrap_or("Processing"),
                                    value.get("progress").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                                    value.get("message").and_then(|v| v.as_str()).unwrap_or(""),
                                ));
                                if let Ok(cb_ref) = callback_msg.try_borrow() {
                                    if let Some(cb) = cb_ref.as_ref() {
                                        cb(msg);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        // On close
        let state_close = state.clone();
        let on_close = EventListener::new(&ws, "close", move |event| {
            let close_event = event.dyn_ref::<CloseEvent>().unwrap();
            log::warn!(
                "WebSocket closed: code={}, reason={}",
                close_event.code(),
                close_event.reason()
            );
            *state_close.borrow_mut() = ConnectionState::Disconnected;
        });

        // On error
        let state_err = state.clone();
        let on_error = EventListener::new(&ws, "error", move |_event| {
            log::error!("WebSocket error occurred");
            *state_err.borrow_mut() = ConnectionState::Error("Connection error".to_string());
        });

        Ok(Self {
            ws: Some(ws),
            url: url.to_string(),
            state,
            _on_message: Some(on_message),
            _on_open: Some(on_open),
            _on_close: Some(on_close),
            _on_error: Some(on_error),
            _callback: callback_rc,
        })
    }

    /// Subscribe to job updates
    pub fn subscribe_job(&self, job_id: &str) {
        if let Some(ref ws) = self.ws {
            let msg = serde_json::json!({
                "type": "subscribe",
                "job_id": job_id
            });
            if let Err(e) = ws.send_with_str(&msg.to_string()) {
                log::error!("Failed to send subscribe message: {:?}", e);
            } else {
                log::info!("Subscribed to job: {}", job_id);
            }
        }
    }

    /// Unsubscribe from job updates
    pub fn unsubscribe_job(&self, job_id: &str) {
        if let Some(ref ws) = self.ws {
            let msg = serde_json::json!({
                "type": "unsubscribe",
                "job_id": job_id
            });
            if let Err(e) = ws.send_with_str(&msg.to_string()) {
                log::error!("Failed to send unsubscribe message: {:?}", e);
            }
        }
    }

    /// Send a ping to keep the connection alive
    pub fn ping(&self) {
        if let Some(ref ws) = self.ws {
            let msg = serde_json::json!({ "type": "ping" });
            if let Err(e) = ws.send_with_str(&msg.to_string()) {
                log::warn!("Failed to send ping: {:?}", e);
            }
        }
    }

    /// Get current connection state
    pub fn state(&self) -> ConnectionState {
        self.state.borrow().clone()
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        matches!(*self.state.borrow(), ConnectionState::Connected)
    }

    /// Disconnect the WebSocket
    pub fn disconnect(&mut self) {
        log::info!("Disconnecting WebSocket");
        if let Some(ref ws) = self.ws {
            let _ = ws.close();
        }
        *self.state.borrow_mut() = ConnectionState::Disconnected;
        self.ws = None;
        // Drop event listeners by taking them
        let _ = self._on_message.take();
        let _ = self._on_open.take();
        let _ = self._on_close.take();
        let _ = self._on_error.take();
    }
}

impl Drop for WsClient {
    fn drop(&mut self) {
        self.disconnect();
    }
}
