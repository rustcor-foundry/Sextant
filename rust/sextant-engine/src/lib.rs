use chrono::{DateTime, Utc};
use distill_core::{DistillOptions, Format};
use rayon::prelude::*;
use reqwest::blocking::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use sextant_bridge::{MultiModalPerception, NeuralBridge};
use sextant_firewall::{FirewallAction, SextantFirewall};
use sextant_privacy;
use std::collections::{HashMap, HashSet};
use std::env;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc};
#[cfg(feature = "servo-backend")]
use std::thread;
use std::time::{Duration, Instant};
use url::Url;
use uuid::Uuid;

#[cfg(feature = "servo-backend")]
mod servo_runtime {
    use super::{
        collapse_whitespace, AsyncFrameCapture, BrowserEvalProbe, BrowserInteraction,
        BrowserInteractionResult, DistilledPage, EngineBackend, EngineStatus, NodeType,
        RenderedFrame, SandboxProfile, SemanticNode,
    };
    use dpi::PhysicalSize;
    use servo::{
        CompositionEvent, CompositionState, DeviceIntPoint, DeviceIntRect, DeviceIntSize,
        DevicePoint, ImeEvent, InputEvent, JSValue, JavaScriptEvaluationError, Key, KeyState,
        KeyboardEvent, LoadStatus, MouseButton, MouseButtonAction, MouseButtonEvent,
        MouseMoveEvent, NamedKey, RenderingContext, Servo, ServoBuilder, SoftwareRenderingContext,
        WebView, WebViewBuilder, WebViewDelegate, WheelDelta, WheelEvent, WheelMode,
    };
    use std::collections::{HashMap, VecDeque};
    use std::env;
    use std::panic::{self, AssertUnwindSafe};
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::OnceLock;
    use std::sync::{mpsc, Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};
    use url::Url;
    use uuid::Uuid;

    const DEFAULT_VIEWPORT_WIDTH: u32 = 1280;
    const DEFAULT_VIEWPORT_HEIGHT: u32 = 720;
    const NAVIGATION_TIMEOUT: Duration = Duration::from_secs(8);
    const LOAD_SETTLE_TIMEOUT: Duration = Duration::from_millis(750);
    const INTERACTION_NAVIGATION_TIMEOUT: Duration = Duration::from_secs(3);
    const SERVICE_REPLY_TIMEOUT: Duration = Duration::from_secs(20);
    const EVENT_LOOP_PAUSE: Duration = Duration::from_millis(10);

    #[derive(Clone)]
    pub struct ServoServiceHandle {
        shared_command_tx: Arc<Mutex<Option<mpsc::Sender<ServoCommand>>>>,
        initial_viewport_size: Arc<Mutex<(u32, u32)>>,
        failed: Arc<AtomicBool>,
    }

    pub struct ServoRenderResult {
        pub final_url: Url,
        pub status: EngineStatus,
    }

    pub struct ServoTabSnapshot {
        pub current_url: Option<Url>,
        pub can_go_back: bool,
        pub can_go_forward: bool,
    }

    pub struct ServoDomSnapshot {
        pub page: DistilledPage,
    }

    pub struct ServoFrameSnapshot {
        pub frame: RenderedFrame,
    }

    enum ServoCommand {
        Navigate {
            tab_id: Uuid,
            url: Url,
            viewport_size: Option<(u32, u32)>,
            reply_tx: mpsc::Sender<Result<ServoRenderResult, String>>,
        },
        Reload {
            tab_id: Uuid,
            reply_tx: mpsc::Sender<Result<ServoRenderResult, String>>,
        },
        GoBack {
            tab_id: Uuid,
            reply_tx: mpsc::Sender<Result<ServoRenderResult, String>>,
        },
        GoForward {
            tab_id: Uuid,
            reply_tx: mpsc::Sender<Result<ServoRenderResult, String>>,
        },
        CloseTab {
            tab_id: Uuid,
            reply_tx: mpsc::Sender<Result<(), String>>,
        },
        InspectTab {
            tab_id: Uuid,
            reply_tx: mpsc::Sender<Result<ServoTabSnapshot, String>>,
        },
        DistillTab {
            tab_id: Uuid,
            enqueued_at: Instant,
            reply_tx: mpsc::Sender<Result<ServoDomSnapshot, String>>,
        },
        EvalProbeTab {
            tab_id: Uuid,
            enqueued_at: Instant,
            reply_tx: mpsc::Sender<Result<BrowserEvalProbe, String>>,
        },
        CaptureFrame {
            tab_id: Uuid,
            reply_tx: mpsc::Sender<Result<ServoFrameSnapshot, String>>,
        },
        ResizeAndCaptureFrame {
            tab_id: Uuid,
            viewport_size: Option<(u32, u32)>,
            reply_tx: mpsc::Sender<Result<AsyncFrameCapture, String>>,
        },
        ResizeTab {
            tab_id: Uuid,
            width: u32,
            height: u32,
            reply_tx: mpsc::Sender<Result<(), String>>,
        },
        Wheel {
            tab_id: Uuid,
            delta_x: f64,
            delta_y: f64,
            pixel_mode: bool,
            reply_tx: mpsc::Sender<Result<(), String>>,
        },
        MouseMove {
            tab_id: Uuid,
            x: f32,
            y: f32,
            reply_tx: mpsc::Sender<Result<(), String>>,
        },
        MouseButton {
            tab_id: Uuid,
            x: f32,
            y: f32,
            pressed: bool,
            reply_tx: mpsc::Sender<Result<(), String>>,
        },
        KeyCharacter {
            tab_id: Uuid,
            text: String,
            pressed: bool,
            reply_tx: mpsc::Sender<Result<(), String>>,
        },
        KeyNamed {
            tab_id: Uuid,
            key: BrowserNamedKey,
            pressed: bool,
            reply_tx: mpsc::Sender<Result<(), String>>,
        },
        Interact {
            tab_id: Uuid,
            interaction: BrowserInteraction,
            reply_tx: mpsc::Sender<Result<BrowserInteractionResult, String>>,
        },
    }

    impl ServoCommand {
        fn is_viewport_input(&self) -> bool {
            matches!(
                self,
                ServoCommand::Wheel { .. }
                    | ServoCommand::MouseMove { .. }
                    | ServoCommand::MouseButton { .. }
                    | ServoCommand::KeyCharacter { .. }
                    | ServoCommand::KeyNamed { .. }
            )
        }

        fn can_yield_to_viewport_input(&self) -> bool {
            matches!(
                self,
                ServoCommand::DistillTab { .. }
                    | ServoCommand::EvalProbeTab { .. }
                    | ServoCommand::CaptureFrame { .. }
                    | ServoCommand::ResizeAndCaptureFrame { .. }
            )
        }
    }

    #[derive(Clone, Copy)]
    pub enum BrowserNamedKey {
        Enter,
        Backspace,
        Tab,
        Escape,
        ArrowLeft,
        ArrowRight,
        ArrowUp,
        ArrowDown,
        Delete,
    }

    struct ServoTabSession {
        webview: WebView,
        viewport_size: (u32, u32),
    }

    struct ServoRuntimeState {
        servo: Servo,
        rendering_context: Rc<dyn RenderingContext>,
        delegate: Rc<HeadlessWebViewDelegate>,
        sessions: HashMap<Uuid, ServoTabSession>,
    }

    #[derive(Default)]
    struct HeadlessWebViewDelegate {
        frame_ready: AtomicBool,
    }

    impl HeadlessWebViewDelegate {
        fn take_frame_ready(&self) -> bool {
            self.frame_ready.swap(false, Ordering::SeqCst)
        }
    }

    impl WebViewDelegate for HeadlessWebViewDelegate {
        fn notify_new_frame_ready(&self, webview: WebView) {
            webview.paint();
            self.frame_ready.store(true, Ordering::SeqCst);
        }
    }

    impl ServoServiceHandle {
        fn new() -> Self {
            Self {
                shared_command_tx: Arc::new(Mutex::new(None)),
                initial_viewport_size: Arc::new(Mutex::new((
                    DEFAULT_VIEWPORT_WIDTH,
                    DEFAULT_VIEWPORT_HEIGHT,
                ))),
                failed: Arc::new(AtomicBool::new(false)),
            }
        }

        fn mark_failed(&self) {
            self.failed.store(true, Ordering::SeqCst);
            if let Ok(mut guard) = self.shared_command_tx.lock() {
                *guard = None;
            }
        }

        fn command_tx(&self) -> Result<mpsc::Sender<ServoCommand>, String> {
            if self.failed.load(Ordering::SeqCst) {
                return Err(
                    "Servo service is unavailable after an internal failure; restart the app."
                        .to_string(),
                );
            }

            let mut guard = self
                .shared_command_tx
                .lock()
                .map_err(|_| "Servo service state lock was poisoned.".to_string())?;
            if let Some(command_tx) = guard.as_ref() {
                return Ok(command_tx.clone());
            }

            static SERVO_SERVICE_STARTED: AtomicBool = AtomicBool::new(false);
            if SERVO_SERVICE_STARTED.swap(true, Ordering::SeqCst) {
                return Err(
                    "Servo service stopped; restart the app to create a new Servo runtime."
                        .to_string(),
                );
            }

            let (command_tx, command_rx) = mpsc::channel();
            let initial_viewport_size = self
                .initial_viewport_size
                .lock()
                .map(|guard| *guard)
                .unwrap_or((DEFAULT_VIEWPORT_WIDTH, DEFAULT_VIEWPORT_HEIGHT));
            eprintln!("[sextant-servo] spawning service thread");
            thread::Builder::new()
                .name("sextant-servo-service".into())
                .spawn(move || run_service(command_rx, initial_viewport_size))
                .map_err(|e| format!("failed to spawn Servo service thread: {}", e))?;
            *guard = Some(command_tx.clone());
            Ok(command_tx)
        }

        fn request<R>(
            &self,
            make_command: impl Fn(mpsc::Sender<Result<R, String>>) -> ServoCommand,
        ) -> Result<R, String> {
            let mut last_error = None;
            for _attempt in 0..1 {
                let (reply_tx, reply_rx) = mpsc::channel();
                let command = make_command(reply_tx);
                match self.command_tx() {
                    Ok(command_tx) => {
                        if let Err(error) = command_tx.send(command) {
                            let message = format!("Servo service unavailable: {}", error);
                            eprintln!(
                                "[sextant-servo] request send failed, marking service failed: {message}"
                            );
                            self.mark_failed();
                            last_error = Some(message);
                            continue;
                        }
                    }
                    Err(error) => {
                        last_error = Some(error);
                        continue;
                    }
                }

                match reply_rx.recv_timeout(SERVICE_REPLY_TIMEOUT) {
                    Ok(result) => return result,
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        let message = "Servo service dropped reply.".to_string();
                        eprintln!(
                            "[sextant-servo] request reply dropped, marking service failed: {message}"
                        );
                        self.mark_failed();
                        last_error = Some(message);
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        let message = format!(
                            "Servo service did not reply within {}s.",
                            SERVICE_REPLY_TIMEOUT.as_secs()
                        );
                        eprintln!(
                            "[sextant-servo] request timed out; keeping service available for follow-up/fallback: {message}"
                        );
                        last_error = Some(message);
                    }
                }
            }

            Err(last_error
                .unwrap_or_else(|| "Servo service request failed without an error.".to_string()))
        }

        fn enqueue(&self, make_command: impl Fn(mpsc::Sender<Result<(), String>>) -> ServoCommand) {
            let (reply_tx, _reply_rx) = mpsc::channel();
            match self.command_tx() {
                Ok(command_tx) => {
                    if let Err(error) = command_tx.send(make_command(reply_tx)) {
                        eprintln!("[sextant-servo] async request send failed: {error}");
                        self.mark_failed();
                    }
                }
                Err(error) => {
                    eprintln!("[sextant-servo] async request unavailable: {error}");
                }
            }
        }

        pub fn configure_initial_viewport(&self, width: u32, height: u32) {
            if let Ok(mut guard) = self.initial_viewport_size.lock() {
                *guard = (width.max(1), height.max(1));
            }
        }

        pub fn navigate(&self, tab_id: Uuid, url: Url) -> Result<ServoRenderResult, String> {
            self.request(|reply_tx| ServoCommand::Navigate {
                tab_id,
                url: url.clone(),
                viewport_size: None,
                reply_tx,
            })
        }

        pub fn navigate_with_viewport(
            &self,
            tab_id: Uuid,
            url: Url,
            viewport_size: Option<(u32, u32)>,
        ) -> Result<ServoRenderResult, String> {
            self.request(|reply_tx| ServoCommand::Navigate {
                tab_id,
                url: url.clone(),
                viewport_size,
                reply_tx,
            })
        }

        pub fn reload(&self, tab_id: Uuid) -> Result<ServoRenderResult, String> {
            self.request(|reply_tx| ServoCommand::Reload { tab_id, reply_tx })
        }

        pub fn go_back(&self, tab_id: Uuid) -> Result<ServoRenderResult, String> {
            self.request(|reply_tx| ServoCommand::GoBack { tab_id, reply_tx })
        }

        pub fn go_forward(&self, tab_id: Uuid) -> Result<ServoRenderResult, String> {
            self.request(|reply_tx| ServoCommand::GoForward { tab_id, reply_tx })
        }

        pub fn close_tab(&self, tab_id: Uuid) -> Result<(), String> {
            self.request(|reply_tx| ServoCommand::CloseTab { tab_id, reply_tx })
        }

        pub fn inspect_tab(&self, tab_id: Uuid) -> Result<ServoTabSnapshot, String> {
            self.request(|reply_tx| ServoCommand::InspectTab { tab_id, reply_tx })
        }

        pub fn distill_tab(&self, tab_id: Uuid) -> Result<ServoDomSnapshot, String> {
            self.request(|reply_tx| ServoCommand::DistillTab {
                tab_id,
                enqueued_at: Instant::now(),
                reply_tx,
            })
        }

        pub fn eval_probe_tab(&self, tab_id: Uuid) -> Result<BrowserEvalProbe, String> {
            self.request(|reply_tx| ServoCommand::EvalProbeTab {
                tab_id,
                enqueued_at: Instant::now(),
                reply_tx,
            })
        }

        pub fn capture_frame(&self, tab_id: Uuid) -> Result<ServoFrameSnapshot, String> {
            self.request(|reply_tx| ServoCommand::CaptureFrame { tab_id, reply_tx })
        }

        pub fn resize_and_capture_frame(
            &self,
            tab_id: Uuid,
            viewport_size: Option<(u32, u32)>,
        ) -> Result<AsyncFrameCapture, String> {
            self.request(|reply_tx| ServoCommand::ResizeAndCaptureFrame {
                tab_id,
                viewport_size,
                reply_tx,
            })
        }

        pub fn resize_tab(&self, tab_id: Uuid, width: u32, height: u32) -> Result<(), String> {
            self.request(|reply_tx| ServoCommand::ResizeTab {
                tab_id,
                width,
                height,
                reply_tx,
            })
        }

        pub fn wheel(
            &self,
            tab_id: Uuid,
            delta_x: f64,
            delta_y: f64,
            pixel_mode: bool,
        ) -> Result<(), String> {
            self.request(|reply_tx| ServoCommand::Wheel {
                tab_id,
                delta_x,
                delta_y,
                pixel_mode,
                reply_tx,
            })
        }

        pub fn enqueue_wheel(&self, tab_id: Uuid, delta_x: f64, delta_y: f64, pixel_mode: bool) {
            self.enqueue(|reply_tx| ServoCommand::Wheel {
                tab_id,
                delta_x,
                delta_y,
                pixel_mode,
                reply_tx,
            });
        }

        pub fn mouse_move(&self, tab_id: Uuid, x: f32, y: f32) -> Result<(), String> {
            self.request(|reply_tx| ServoCommand::MouseMove {
                tab_id,
                x,
                y,
                reply_tx,
            })
        }

        pub fn enqueue_mouse_move(&self, tab_id: Uuid, x: f32, y: f32) {
            self.enqueue(|reply_tx| ServoCommand::MouseMove {
                tab_id,
                x,
                y,
                reply_tx,
            });
        }

        pub fn mouse_button(
            &self,
            tab_id: Uuid,
            x: f32,
            y: f32,
            pressed: bool,
        ) -> Result<(), String> {
            self.request(|reply_tx| ServoCommand::MouseButton {
                tab_id,
                x,
                y,
                pressed,
                reply_tx,
            })
        }

        pub fn enqueue_mouse_button(&self, tab_id: Uuid, x: f32, y: f32, pressed: bool) {
            self.enqueue(|reply_tx| ServoCommand::MouseButton {
                tab_id,
                x,
                y,
                pressed,
                reply_tx,
            });
        }

        pub fn key_character(
            &self,
            tab_id: Uuid,
            text: String,
            pressed: bool,
        ) -> Result<(), String> {
            self.request(|reply_tx| ServoCommand::KeyCharacter {
                tab_id,
                text: text.clone(),
                pressed,
                reply_tx,
            })
        }

        pub fn enqueue_key_character(&self, tab_id: Uuid, text: String, pressed: bool) {
            self.enqueue(|reply_tx| ServoCommand::KeyCharacter {
                tab_id,
                text: text.clone(),
                pressed,
                reply_tx,
            });
        }

        pub fn key_named(
            &self,
            tab_id: Uuid,
            key: BrowserNamedKey,
            pressed: bool,
        ) -> Result<(), String> {
            self.request(|reply_tx| ServoCommand::KeyNamed {
                tab_id,
                key,
                pressed,
                reply_tx,
            })
        }

        pub fn enqueue_key_named(&self, tab_id: Uuid, key: BrowserNamedKey, pressed: bool) {
            self.enqueue(|reply_tx| ServoCommand::KeyNamed {
                tab_id,
                key,
                pressed,
                reply_tx,
            });
        }

        pub fn interact(
            &self,
            tab_id: Uuid,
            interaction: BrowserInteraction,
        ) -> Result<BrowserInteractionResult, String> {
            self.request(|reply_tx| ServoCommand::Interact {
                tab_id,
                interaction: interaction.clone(),
                reply_tx,
            })
        }
    }

    pub fn shared_handle() -> ServoServiceHandle {
        static HANDLE: OnceLock<ServoServiceHandle> = OnceLock::new();
        HANDLE.get_or_init(ServoServiceHandle::new).clone()
    }

    fn run_service(command_rx: mpsc::Receiver<ServoCommand>, initial_viewport_size: (u32, u32)) {
        let mut runtime = match run_servo_command(|| create_runtime(initial_viewport_size)) {
            Ok(runtime) => runtime,
            Err(error) => {
                while let Ok(command) = command_rx.recv() {
                    match command {
                        ServoCommand::Navigate { reply_tx, .. } => {
                            let _ = reply_tx.send(Err(error.clone()));
                        }
                        ServoCommand::Reload { reply_tx, .. } => {
                            let _ = reply_tx.send(Err(error.clone()));
                        }
                        ServoCommand::GoBack { reply_tx, .. } => {
                            let _ = reply_tx.send(Err(error.clone()));
                        }
                        ServoCommand::GoForward { reply_tx, .. } => {
                            let _ = reply_tx.send(Err(error.clone()));
                        }
                        ServoCommand::CloseTab { reply_tx, .. } => {
                            let _ = reply_tx.send(Err(error.clone()));
                        }
                        ServoCommand::InspectTab { reply_tx, .. } => {
                            let _ = reply_tx.send(Err(error.clone()));
                        }
                        ServoCommand::DistillTab { reply_tx, .. } => {
                            let _ = reply_tx.send(Err(error.clone()));
                        }
                        ServoCommand::EvalProbeTab { reply_tx, .. } => {
                            let _ = reply_tx.send(Err(error.clone()));
                        }
                        ServoCommand::CaptureFrame { reply_tx, .. } => {
                            let _ = reply_tx.send(Err(error.clone()));
                        }
                        ServoCommand::ResizeAndCaptureFrame { reply_tx, .. } => {
                            let _ = reply_tx.send(Err(error.clone()));
                        }
                        ServoCommand::ResizeTab { reply_tx, .. } => {
                            let _ = reply_tx.send(Err(error.clone()));
                        }
                        ServoCommand::Wheel { reply_tx, .. } => {
                            let _ = reply_tx.send(Err(error.clone()));
                        }
                        ServoCommand::MouseMove { reply_tx, .. } => {
                            let _ = reply_tx.send(Err(error.clone()));
                        }
                        ServoCommand::MouseButton { reply_tx, .. } => {
                            let _ = reply_tx.send(Err(error.clone()));
                        }
                        ServoCommand::KeyCharacter { reply_tx, .. } => {
                            let _ = reply_tx.send(Err(error.clone()));
                        }
                        ServoCommand::KeyNamed { reply_tx, .. } => {
                            let _ = reply_tx.send(Err(error.clone()));
                        }
                        ServoCommand::Interact { reply_tx, .. } => {
                            let _ = reply_tx.send(Err(error.clone()));
                        }
                    }
                }
                return;
            }
        };

        let mut pending_commands = VecDeque::new();
        while let Ok(command) = next_servo_command(&command_rx, &mut pending_commands) {
            match command {
                ServoCommand::Navigate {
                    tab_id,
                    url,
                    viewport_size,
                    reply_tx,
                } => {
                    let result = run_servo_command(|| {
                        navigate_tab(&mut runtime, tab_id, url, viewport_size)
                    });
                    let _ = reply_tx.send(result);
                }
                ServoCommand::Reload { tab_id, reply_tx } => {
                    let result = run_servo_command(|| reload_tab(&mut runtime, tab_id));
                    let _ = reply_tx.send(result);
                }
                ServoCommand::GoBack { tab_id, reply_tx } => {
                    let result = run_servo_command(|| go_back_tab(&mut runtime, tab_id));
                    let _ = reply_tx.send(result);
                }
                ServoCommand::GoForward { tab_id, reply_tx } => {
                    let result = run_servo_command(|| go_forward_tab(&mut runtime, tab_id));
                    let _ = reply_tx.send(result);
                }
                ServoCommand::CloseTab { tab_id, reply_tx } => {
                    runtime.sessions.remove(&tab_id);
                    let _ = reply_tx.send(Ok(()));
                }
                ServoCommand::InspectTab { tab_id, reply_tx } => {
                    let result = run_servo_command(|| {
                        let session = ensure_session(&mut runtime, tab_id)?;
                        Ok(ServoTabSnapshot {
                            current_url: session.webview.url(),
                            can_go_back: session.webview.can_go_back(),
                            can_go_forward: session.webview.can_go_forward(),
                        })
                    });
                    let _ = reply_tx.send(result);
                }
                ServoCommand::DistillTab {
                    tab_id,
                    enqueued_at,
                    reply_tx,
                } => {
                    let queue_elapsed = enqueued_at.elapsed();
                    let result = run_servo_command(|| {
                        ensure_session(&mut runtime, tab_id)?;
                        let mut session = runtime
                            .sessions
                            .remove(&tab_id)
                            .ok_or_else(|| "Servo session was not created".to_string())?;
                        let result = distill_live_dom(&mut runtime.servo, &mut session);
                        runtime.sessions.insert(tab_id, session);
                        let mut page = result?;
                        page.metadata.insert(
                            "live_dom_queue_ms".to_string(),
                            queue_elapsed.as_millis().to_string(),
                        );
                        Ok(ServoDomSnapshot { page })
                    });
                    let _ = reply_tx.send(result);
                }
                ServoCommand::EvalProbeTab {
                    tab_id,
                    enqueued_at,
                    reply_tx,
                } => {
                    let queue_elapsed = enqueued_at.elapsed();
                    let result = run_servo_command(|| {
                        ensure_session(&mut runtime, tab_id)?;
                        let mut session = runtime
                            .sessions
                            .remove(&tab_id)
                            .ok_or_else(|| "Servo session was not created".to_string())?;
                        let result =
                            eval_probe_tab(&mut runtime.servo, &mut session, queue_elapsed);
                        runtime.sessions.insert(tab_id, session);
                        result
                    });
                    let _ = reply_tx.send(result);
                }
                ServoCommand::CaptureFrame { tab_id, reply_tx } => {
                    let result = run_servo_command(|| capture_frame(&mut runtime, tab_id));
                    let _ = reply_tx.send(result);
                }
                ServoCommand::ResizeAndCaptureFrame {
                    tab_id,
                    viewport_size,
                    reply_tx,
                } => {
                    let result = run_servo_command(|| {
                        resize_and_capture_frame(&mut runtime, tab_id, viewport_size)
                    });
                    let _ = reply_tx.send(result);
                }
                ServoCommand::ResizeTab {
                    tab_id,
                    width,
                    height,
                    reply_tx,
                } => {
                    let result =
                        run_servo_command(|| resize_tab(&mut runtime, tab_id, width, height));
                    let _ = reply_tx.send(result);
                }
                ServoCommand::Wheel {
                    tab_id,
                    delta_x,
                    delta_y,
                    pixel_mode,
                    reply_tx,
                } => {
                    let result = run_servo_command(|| {
                        wheel_tab(&mut runtime, tab_id, delta_x, delta_y, pixel_mode)
                    });
                    let _ = reply_tx.send(result);
                }
                ServoCommand::MouseMove {
                    tab_id,
                    mut x,
                    mut y,
                    reply_tx,
                } => {
                    coalesce_pending_mouse_move(&mut pending_commands, tab_id, &mut x, &mut y);
                    let result = run_servo_command(|| mouse_move_tab(&mut runtime, tab_id, x, y));
                    let _ = reply_tx.send(result);
                }
                ServoCommand::MouseButton {
                    tab_id,
                    x,
                    y,
                    pressed,
                    reply_tx,
                } => {
                    let result =
                        run_servo_command(|| mouse_button_tab(&mut runtime, tab_id, x, y, pressed));
                    let _ = reply_tx.send(result);
                }
                ServoCommand::KeyCharacter {
                    tab_id,
                    text,
                    pressed,
                    reply_tx,
                } => {
                    let mut text = text;
                    if pressed && !text.is_empty() {
                        coalesce_pending_key_text(
                            &command_rx,
                            &mut pending_commands,
                            tab_id,
                            &mut text,
                        );
                    }
                    let result = run_servo_command(|| {
                        key_character_tab(&mut runtime, tab_id, text, pressed)
                    });
                    let _ = reply_tx.send(result);
                }
                ServoCommand::KeyNamed {
                    tab_id,
                    key,
                    pressed,
                    reply_tx,
                } => {
                    let result =
                        run_servo_command(|| key_named_tab(&mut runtime, tab_id, key, pressed));
                    let _ = reply_tx.send(result);
                }
                ServoCommand::Interact {
                    tab_id,
                    interaction,
                    reply_tx,
                } => {
                    let result =
                        run_servo_command(|| interact_tab(&mut runtime, tab_id, &interaction));
                    let _ = reply_tx.send(result);
                }
            }
        }
    }

    fn next_servo_command(
        command_rx: &mpsc::Receiver<ServoCommand>,
        pending_commands: &mut VecDeque<ServoCommand>,
    ) -> Result<ServoCommand, mpsc::RecvError> {
        if pending_commands.is_empty() {
            pending_commands.push_back(command_rx.recv()?);
        }
        while let Ok(command) = command_rx.try_recv() {
            pending_commands.push_back(command);
        }

        if let Some(index) = prioritized_viewport_input_index(pending_commands) {
            return Ok(pending_commands
                .remove(index)
                .expect("pending command index disappeared"));
        }

        Ok(pending_commands
            .pop_front()
            .expect("pending command queue was unexpectedly empty"))
    }

    fn prioritized_viewport_input_index(
        pending_commands: &VecDeque<ServoCommand>,
    ) -> Option<usize> {
        let first = pending_commands.front()?;
        if first.is_viewport_input() {
            return Some(0);
        }
        if !first.can_yield_to_viewport_input() {
            return None;
        }

        for (index, command) in pending_commands.iter().enumerate().skip(1) {
            if command.is_viewport_input() {
                return Some(index);
            }
            if !command.can_yield_to_viewport_input() {
                return None;
            }
        }

        None
    }

    fn coalesce_pending_key_text(
        command_rx: &mpsc::Receiver<ServoCommand>,
        pending_commands: &mut VecDeque<ServoCommand>,
        tab_id: Uuid,
        text: &mut String,
    ) {
        while let Some(command) = pending_commands.pop_front() {
            match command {
                ServoCommand::KeyCharacter {
                    tab_id: next_tab_id,
                    text: next_text,
                    pressed: true,
                    reply_tx,
                } if next_tab_id == tab_id && !next_text.is_empty() => {
                    text.push_str(&next_text);
                    let _ = reply_tx.send(Ok(()));
                }
                other => {
                    pending_commands.push_front(other);
                    return;
                }
            }
        }

        while let Ok(command) = command_rx.try_recv() {
            match command {
                ServoCommand::KeyCharacter {
                    tab_id: next_tab_id,
                    text: next_text,
                    pressed: true,
                    reply_tx,
                } if next_tab_id == tab_id && !next_text.is_empty() => {
                    text.push_str(&next_text);
                    let _ = reply_tx.send(Ok(()));
                }
                other => {
                    pending_commands.push_back(other);
                    return;
                }
            }
        }
    }

    fn coalesce_pending_mouse_move(
        pending_commands: &mut VecDeque<ServoCommand>,
        tab_id: Uuid,
        x: &mut f32,
        y: &mut f32,
    ) {
        let mut index = 0;
        while index < pending_commands.len() {
            let should_coalesce = matches!(
                pending_commands.get(index),
                Some(ServoCommand::MouseMove {
                    tab_id: next_tab_id,
                    ..
                }) if *next_tab_id == tab_id
            );
            if should_coalesce {
                if let Some(ServoCommand::MouseMove {
                    x: next_x,
                    y: next_y,
                    reply_tx,
                    ..
                }) = pending_commands.remove(index)
                {
                    *x = next_x;
                    *y = next_y;
                    let _ = reply_tx.send(Ok(()));
                }
                continue;
            }

            let should_stop = matches!(
                pending_commands.get(index),
                Some(ServoCommand::MouseButton {
                    tab_id: next_tab_id,
                    ..
                }) | Some(ServoCommand::Wheel {
                    tab_id: next_tab_id,
                    ..
                }) | Some(ServoCommand::KeyCharacter {
                    tab_id: next_tab_id,
                    ..
                }) | Some(ServoCommand::KeyNamed {
                    tab_id: next_tab_id,
                    ..
                }) if *next_tab_id == tab_id
            ) || pending_commands
                .get(index)
                .map(|command| !command.can_yield_to_viewport_input())
                .unwrap_or(false);
            if should_stop {
                break;
            }
            index += 1;
        }
    }

    fn ensure_session(
        runtime: &mut ServoRuntimeState,
        tab_id: Uuid,
    ) -> Result<&mut ServoTabSession, String> {
        if !runtime.sessions.contains_key(&tab_id) {
            let session = create_webview_session(runtime, None)?;
            runtime.sessions.insert(tab_id, session);
        }
        runtime
            .sessions
            .get_mut(&tab_id)
            .ok_or_else(|| "Servo session was not created".to_string())
    }

    fn run_servo_command<T>(command: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        panic::catch_unwind(AssertUnwindSafe(command)).map_err(|panic_payload| {
            if let Some(message) = panic_payload.downcast_ref::<&str>() {
                format!("Servo runtime panicked: {}", message)
            } else if let Some(message) = panic_payload.downcast_ref::<String>() {
                format!("Servo runtime panicked: {}", message)
            } else {
                "Servo runtime panicked during initialization.".to_string()
            }
        })?
    }

    fn create_runtime(initial_viewport_size: (u32, u32)) -> Result<ServoRuntimeState, String> {
        install_rustls_crypto_provider();
        prime_windows_angle_runtime()?;
        eprintln!("[sextant-servo] creating runtime");
        let (width, height) = initial_viewport_size;
        let rendering_context = Rc::new(
            SoftwareRenderingContext::new(PhysicalSize::new(width.max(1), height.max(1))).map_err(
                |e| format!("Failed to create Servo software rendering context: {:?}", e),
            )?,
        );
        rendering_context
            .make_current()
            .map_err(|e| format!("Failed to activate Servo rendering context: {:?}", e))?;

        let servo = ServoBuilder::default().build();
        let delegate = Rc::new(HeadlessWebViewDelegate::default());
        eprintln!("[sextant-servo] runtime ready");
        Ok(ServoRuntimeState {
            servo,
            rendering_context,
            delegate,
            sessions: HashMap::new(),
        })
    }

    fn install_rustls_crypto_provider() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }

    fn create_webview_session(
        runtime: &mut ServoRuntimeState,
        initial_url: Option<Url>,
    ) -> Result<ServoTabSession, String> {
        let mut builder = WebViewBuilder::new(&runtime.servo, runtime.rendering_context.clone())
            .delegate(runtime.delegate.clone());
        if let Some(url) = initial_url {
            builder = builder.url(url);
        }
        let webview = builder.build();
        let size = runtime.rendering_context.size();

        Ok(ServoTabSession {
            webview,
            viewport_size: (size.width, size.height),
        })
    }

    fn navigate_tab(
        runtime: &mut ServoRuntimeState,
        tab_id: Uuid,
        url: Url,
        viewport_size: Option<(u32, u32)>,
    ) -> Result<ServoRenderResult, String> {
        if !runtime.sessions.contains_key(&tab_id) {
            eprintln!(
                "[sextant-servo] creating initial session for tab {} at {}",
                tab_id, url
            );
            let session = create_webview_session(
                runtime,
                if viewport_size.is_some() {
                    None
                } else {
                    Some(url.clone())
                },
            )?;
            runtime.sessions.insert(tab_id, session);
        }
        let mut session = runtime
            .sessions
            .remove(&tab_id)
            .ok_or_else(|| "Servo session was not created".to_string())?;
        if let Some((width, height)) = viewport_size {
            let size = PhysicalSize::new(width.max(1), height.max(1));
            if session.viewport_size != (size.width, size.height) {
                session.webview.resize(size);
                session.viewport_size = (size.width, size.height);
                runtime.servo.spin_event_loop();
            }
        }
        let result = navigate_session(&mut runtime.servo, &mut session, url);
        runtime.sessions.insert(tab_id, session);
        result
    }

    fn reload_tab(
        runtime: &mut ServoRuntimeState,
        tab_id: Uuid,
    ) -> Result<ServoRenderResult, String> {
        ensure_session(runtime, tab_id)?;
        let mut session = runtime
            .sessions
            .remove(&tab_id)
            .ok_or_else(|| "Servo session was not created".to_string())?;
        let result = reload_session(&mut runtime.servo, &mut session);
        runtime.sessions.insert(tab_id, session);
        result
    }

    fn go_back_tab(
        runtime: &mut ServoRuntimeState,
        tab_id: Uuid,
    ) -> Result<ServoRenderResult, String> {
        ensure_session(runtime, tab_id)?;
        let mut session = runtime
            .sessions
            .remove(&tab_id)
            .ok_or_else(|| "Servo session was not created".to_string())?;
        let result = go_back_session(&mut runtime.servo, &mut session);
        runtime.sessions.insert(tab_id, session);
        result
    }

    fn go_forward_tab(
        runtime: &mut ServoRuntimeState,
        tab_id: Uuid,
    ) -> Result<ServoRenderResult, String> {
        ensure_session(runtime, tab_id)?;
        let mut session = runtime
            .sessions
            .remove(&tab_id)
            .ok_or_else(|| "Servo session was not created".to_string())?;
        let result = go_forward_session(&mut runtime.servo, &mut session);
        runtime.sessions.insert(tab_id, session);
        result
    }

    fn capture_frame(
        runtime: &mut ServoRuntimeState,
        tab_id: Uuid,
    ) -> Result<ServoFrameSnapshot, String> {
        ensure_session(runtime, tab_id)?;
        let session = runtime
            .sessions
            .get(&tab_id)
            .ok_or_else(|| "Servo session was not created".to_string())?;
        runtime
            .rendering_context
            .make_current()
            .map_err(|e| format!("Failed to make Servo rendering context current: {:?}", e))?;
        if !runtime.delegate.take_frame_ready() {
            session.webview.paint();
        }
        runtime.rendering_context.present();
        let size = runtime.rendering_context.size();
        let rect = DeviceIntRect::from_origin_and_size(
            DeviceIntPoint::new(0, 0),
            DeviceIntSize::new(size.width as i32, size.height as i32),
        );
        let image = runtime
            .rendering_context
            .read_to_image(rect)
            .ok_or_else(|| "Servo rendered no readable frame.".to_string())?;
        let raw = image.into_raw();
        let pixels = raw
            .chunks_exact(4)
            .map(|rgba| {
                let r = rgba[0] as u32;
                let g = rgba[1] as u32;
                let b = rgba[2] as u32;
                (r << 16) | (g << 8) | b
            })
            .collect();
        Ok(ServoFrameSnapshot {
            frame: RenderedFrame {
                width: size.width,
                height: size.height,
                pixels,
            },
        })
    }

    fn resize_and_capture_frame(
        runtime: &mut ServoRuntimeState,
        tab_id: Uuid,
        viewport_size: Option<(u32, u32)>,
    ) -> Result<AsyncFrameCapture, String> {
        let resize = if let Some((width, height)) = viewport_size {
            let started = Instant::now();
            resize_tab(runtime, tab_id, width, height)?;
            Some(started.elapsed())
        } else {
            None
        };
        let capture_started = Instant::now();
        let frame = capture_frame(runtime, tab_id)?.frame;
        Ok(AsyncFrameCapture {
            resize,
            capture: capture_started.elapsed(),
            frame,
        })
    }

    fn resize_tab(
        runtime: &mut ServoRuntimeState,
        tab_id: Uuid,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        ensure_session(runtime, tab_id)?;
        let size = PhysicalSize::new(width.max(1), height.max(1));
        let session = runtime
            .sessions
            .get_mut(&tab_id)
            .ok_or_else(|| "Servo session was not created".to_string())?;
        if session.viewport_size == (size.width, size.height) {
            return Ok(());
        }
        session.webview.resize(size);
        session.viewport_size = (size.width, size.height);
        runtime.servo.spin_event_loop();
        Ok(())
    }

    fn wheel_tab(
        runtime: &mut ServoRuntimeState,
        tab_id: Uuid,
        delta_x: f64,
        delta_y: f64,
        pixel_mode: bool,
    ) -> Result<(), String> {
        ensure_session(runtime, tab_id)?;
        let session = runtime
            .sessions
            .get(&tab_id)
            .ok_or_else(|| "Servo session was not created".to_string())?;
        let mode = if pixel_mode {
            WheelMode::DeltaPixel
        } else {
            WheelMode::DeltaLine
        };
        session
            .webview
            .notify_input_event(InputEvent::Wheel(WheelEvent::new(
                WheelDelta {
                    x: delta_x,
                    y: delta_y,
                    z: 0.0,
                    mode,
                },
                DevicePoint::default().into(),
            )));
        runtime.servo.spin_event_loop();
        Ok(())
    }

    fn mouse_move_tab(
        runtime: &mut ServoRuntimeState,
        tab_id: Uuid,
        x: f32,
        y: f32,
    ) -> Result<(), String> {
        ensure_session(runtime, tab_id)?;
        let session = runtime
            .sessions
            .get(&tab_id)
            .ok_or_else(|| "Servo session was not created".to_string())?;
        session
            .webview
            .notify_input_event(InputEvent::MouseMove(MouseMoveEvent::new(
                DevicePoint::new(x, y).into(),
            )));
        runtime.servo.spin_event_loop();
        Ok(())
    }

    fn mouse_button_tab(
        runtime: &mut ServoRuntimeState,
        tab_id: Uuid,
        x: f32,
        y: f32,
        pressed: bool,
    ) -> Result<(), String> {
        ensure_session(runtime, tab_id)?;
        let session = runtime
            .sessions
            .get(&tab_id)
            .ok_or_else(|| "Servo session was not created".to_string())?;
        let action = if pressed {
            MouseButtonAction::Down
        } else {
            MouseButtonAction::Up
        };
        session
            .webview
            .notify_input_event(InputEvent::MouseButton(MouseButtonEvent::new(
                action,
                MouseButton::Left,
                DevicePoint::new(x, y).into(),
            )));
        runtime.servo.spin_event_loop();
        Ok(())
    }

    fn key_character_tab(
        runtime: &mut ServoRuntimeState,
        tab_id: Uuid,
        text: String,
        pressed: bool,
    ) -> Result<(), String> {
        if !pressed || text.is_empty() {
            return Ok(());
        }
        ensure_session(runtime, tab_id)?;
        let session = runtime
            .sessions
            .get(&tab_id)
            .ok_or_else(|| "Servo session was not created".to_string())?;
        session.webview.notify_input_event(InputEvent::Keyboard(
            KeyboardEvent::from_state_and_key(KeyState::Down, Key::Named(NamedKey::Process)),
        ));
        session
            .webview
            .notify_input_event(InputEvent::Ime(ImeEvent::Composition(CompositionEvent {
                state: CompositionState::End,
                data: text,
            })));
        session.webview.notify_input_event(InputEvent::Keyboard(
            KeyboardEvent::from_state_and_key(KeyState::Up, Key::Named(NamedKey::Process)),
        ));
        runtime.servo.spin_event_loop();
        Ok(())
    }

    fn key_named_tab(
        runtime: &mut ServoRuntimeState,
        tab_id: Uuid,
        key: BrowserNamedKey,
        pressed: bool,
    ) -> Result<(), String> {
        key_tab(
            runtime,
            tab_id,
            Key::Named(to_servo_named_key(key)),
            pressed,
        )
    }

    fn key_tab(
        runtime: &mut ServoRuntimeState,
        tab_id: Uuid,
        key: Key,
        pressed: bool,
    ) -> Result<(), String> {
        ensure_session(runtime, tab_id)?;
        let session = runtime
            .sessions
            .get(&tab_id)
            .ok_or_else(|| "Servo session was not created".to_string())?;
        let state = if pressed {
            KeyState::Down
        } else {
            KeyState::Up
        };
        session.webview.notify_input_event(InputEvent::Keyboard(
            KeyboardEvent::from_state_and_key(state, key),
        ));
        runtime.servo.spin_event_loop();
        Ok(())
    }

    fn to_servo_named_key(key: BrowserNamedKey) -> NamedKey {
        match key {
            BrowserNamedKey::Enter => NamedKey::Enter,
            BrowserNamedKey::Backspace => NamedKey::Backspace,
            BrowserNamedKey::Tab => NamedKey::Tab,
            BrowserNamedKey::Escape => NamedKey::Escape,
            BrowserNamedKey::ArrowLeft => NamedKey::ArrowLeft,
            BrowserNamedKey::ArrowRight => NamedKey::ArrowRight,
            BrowserNamedKey::ArrowUp => NamedKey::ArrowUp,
            BrowserNamedKey::ArrowDown => NamedKey::ArrowDown,
            BrowserNamedKey::Delete => NamedKey::Delete,
        }
    }

    fn interact_tab(
        runtime: &mut ServoRuntimeState,
        tab_id: Uuid,
        interaction: &BrowserInteraction,
    ) -> Result<BrowserInteractionResult, String> {
        ensure_session(runtime, tab_id)?;
        let session = runtime
            .sessions
            .get(&tab_id)
            .ok_or_else(|| "Servo session was not created".to_string())?;
        let previous_url = session.webview.url();
        let _ = wait_for_load(&mut runtime.servo, &session.webview, LOAD_SETTLE_TIMEOUT);
        let script = browser_interaction_script(interaction)?;
        let value = evaluate_javascript_sync(
            &mut runtime.servo,
            &session.webview,
            &script,
            NAVIGATION_TIMEOUT,
        )?;
        let mut result = interaction_result_from_js(value)?;
        runtime.servo.spin_event_loop();
        if result.ok && interaction_may_navigate(interaction) {
            if wait_for_changed_url(
                &mut runtime.servo,
                &session.webview,
                previous_url.as_ref(),
                INTERACTION_NAVIGATION_TIMEOUT,
            )
            .is_ok()
            {
                let _ = wait_for_load(&mut runtime.servo, &session.webview, LOAD_SETTLE_TIMEOUT);
            }
        }
        result.current_url = session.webview.url();
        if result.ok {
            Ok(result)
        } else {
            Err(result.message)
        }
    }

    fn interaction_may_navigate(interaction: &BrowserInteraction) -> bool {
        matches!(
            interaction,
            BrowserInteraction::ClickSelector { .. } | BrowserInteraction::SubmitSelector { .. }
        )
    }

    fn browser_interaction_script(interaction: &BrowserInteraction) -> Result<String, String> {
        let (kind, selector, value) = match interaction {
            BrowserInteraction::ClickSelector { selector } => ("click", selector, None),
            BrowserInteraction::FillSelector { selector, value } => {
                ("fill", selector, Some(value.as_str()))
            }
            BrowserInteraction::SubmitSelector { selector } => ("submit", selector, None),
        };
        let kind = serde_json::to_string(kind)
            .map_err(|e| format!("Failed to encode browser action kind: {}", e))?;
        let selector = serde_json::to_string(selector)
            .map_err(|e| format!("Failed to encode browser selector: {}", e))?;
        let value = serde_json::to_string(value.unwrap_or(""))
            .map_err(|e| format!("Failed to encode browser interaction value: {}", e))?;

        Ok(format!(
            r#"
(() => {{
  const kind = {kind};
  const selector = {selector};
  const value = {value};
  const target = document.querySelector(selector);
  const describe = (node) => {{
    if (!node) return '';
    const tag = (node.tagName || 'node').toLowerCase();
    const id = node.id ? `#${{node.id}}` : '';
    const name = node.getAttribute && node.getAttribute('name') ? `[name="${{node.getAttribute('name')}}"]` : '';
    return `${{tag}}${{id}}${{name}}`;
  }};
  const result = (ok, message, node) => ({{
    ok,
    message,
    selector,
    tag: node && node.tagName ? node.tagName.toLowerCase() : '',
    text: node && typeof node.textContent === 'string' ? node.textContent.replace(/\s+/g, ' ').trim().slice(0, 200) : '',
    value: node && 'value' in node ? String(node.value) : ''
  }});

  if (!target) {{
    return result(false, `No element matched selector "${{selector}}"`, null);
  }}

  target.scrollIntoView?.({{ block: 'center', inline: 'center' }});
  target.focus?.();

  if (kind === 'fill') {{
    if (!('value' in target)) {{
      return result(false, `Element ${{describe(target)}} does not accept a value`, target);
    }}
    target.value = value;
    target.dispatchEvent(new InputEvent('input', {{ bubbles: true, inputType: 'insertText', data: value }}));
    target.dispatchEvent(new Event('change', {{ bubbles: true }}));
    return result(true, `Filled ${{describe(target)}}`, target);
  }}

  if (kind === 'click') {{
    target.click();
    target.dispatchEvent(new MouseEvent('click', {{ bubbles: true, cancelable: true, view: window }}));
    return result(true, `Clicked ${{describe(target)}}`, target);
  }}

  if (kind === 'submit') {{
    const form = target.tagName && target.tagName.toLowerCase() === 'form' ? target : target.closest?.('form');
    if (!form) {{
      return result(false, `Element ${{describe(target)}} is not inside a form`, target);
    }}
    if (typeof form.requestSubmit === 'function') {{
      form.requestSubmit();
    }} else {{
      form.dispatchEvent(new Event('submit', {{ bubbles: true, cancelable: true }}));
    }}
    return result(true, `Submitted ${{describe(form)}}`, form);
  }}

  return result(false, `Unsupported browser interaction "${{kind}}"`, target);
}})()
"#
        ))
    }

    fn extract_bool(value: &JSValue) -> Option<bool> {
        match value {
            JSValue::Boolean(value) => Some(*value),
            _ => None,
        }
    }

    fn interaction_result_from_js(value: JSValue) -> Result<BrowserInteractionResult, String> {
        let object = extract_object(&value, "browser interaction result")?;
        Ok(BrowserInteractionResult {
            ok: object.get("ok").and_then(extract_bool).unwrap_or(false),
            message: object
                .get("message")
                .and_then(extract_string)
                .unwrap_or_else(|| "Browser interaction returned no message.".to_string()),
            selector: object
                .get("selector")
                .and_then(extract_string)
                .unwrap_or_default(),
            tag: object
                .get("tag")
                .and_then(extract_string)
                .unwrap_or_default(),
            text: object
                .get("text")
                .and_then(extract_string)
                .unwrap_or_default(),
            value: object
                .get("value")
                .and_then(extract_string)
                .unwrap_or_default(),
            current_url: None,
        })
    }

    fn navigate_session(
        servo: &mut Servo,
        session: &mut ServoTabSession,
        url: Url,
    ) -> Result<ServoRenderResult, String> {
        let previous_url = session.webview.url();
        if previous_url.as_ref() != Some(&url) {
            session.webview.load(url.clone());
        }
        let mut url_note = wait_for_navigation_url(
            servo,
            &session.webview,
            previous_url.as_ref(),
            &url,
            NAVIGATION_TIMEOUT,
        )
        .err();
        let mut final_url = session.webview.url().unwrap_or_else(|| url.clone());

        if previous_url.as_ref() != Some(&url) && Some(&final_url) == previous_url.as_ref() {
            session.webview.load(url.clone());
            let retry_note = wait_for_navigation_url(
                servo,
                &session.webview,
                previous_url.as_ref(),
                &url,
                NAVIGATION_TIMEOUT,
            )
            .err();
            final_url = session.webview.url().unwrap_or_else(|| url.clone());
            if Some(&final_url) == previous_url.as_ref() {
                return Err(format!(
                    "Servo navigation stayed on {} after requesting {}{}",
                    final_url,
                    url,
                    retry_note
                        .or(url_note)
                        .map(|note| format!(" ({note})"))
                        .unwrap_or_default()
                ));
            }
            url_note = retry_note.or(url_note);
        }

        let load_note = wait_for_load(servo, &session.webview, LOAD_SETTLE_TIMEOUT)
            .err()
            .or(url_note);
        Ok(ServoRenderResult {
            final_url,
            status: servo_engine_status_with_note(load_note),
        })
    }

    fn reload_session(
        servo: &mut Servo,
        session: &mut ServoTabSession,
    ) -> Result<ServoRenderResult, String> {
        session.webview.reload();
        let load_note = wait_for_load(servo, &session.webview, LOAD_SETTLE_TIMEOUT).err();
        let final_url = session
            .webview
            .url()
            .ok_or_else(|| "Reload completed without an active URL.".to_string())?;
        Ok(ServoRenderResult {
            final_url,
            status: servo_engine_status_with_note(load_note),
        })
    }

    fn go_back_session(
        servo: &mut Servo,
        session: &mut ServoTabSession,
    ) -> Result<ServoRenderResult, String> {
        if !session.webview.can_go_back() {
            return Err("Active tab has no back history.".to_string());
        }
        let previous_url = session.webview.url();
        session.webview.go_back(1);
        wait_for_changed_url(
            servo,
            &session.webview,
            previous_url.as_ref(),
            NAVIGATION_TIMEOUT,
        )?;
        let load_note = wait_for_load(servo, &session.webview, LOAD_SETTLE_TIMEOUT).err();
        let final_url = session
            .webview
            .url()
            .ok_or_else(|| "Back navigation completed without an active URL.".to_string())?;
        Ok(ServoRenderResult {
            final_url,
            status: servo_engine_status_with_note(load_note),
        })
    }

    fn go_forward_session(
        servo: &mut Servo,
        session: &mut ServoTabSession,
    ) -> Result<ServoRenderResult, String> {
        if !session.webview.can_go_forward() {
            return Err("Active tab has no forward history.".to_string());
        }
        let previous_url = session.webview.url();
        session.webview.go_forward(1);
        wait_for_changed_url(
            servo,
            &session.webview,
            previous_url.as_ref(),
            NAVIGATION_TIMEOUT,
        )?;
        let load_note = wait_for_load(servo, &session.webview, LOAD_SETTLE_TIMEOUT).err();
        let final_url = session
            .webview
            .url()
            .ok_or_else(|| "Forward navigation completed without an active URL.".to_string())?;
        Ok(ServoRenderResult {
            final_url,
            status: servo_engine_status_with_note(load_note),
        })
    }

    fn servo_engine_status_with_note(load_note: Option<String>) -> EngineStatus {
        EngineStatus {
            active_backend: EngineBackend::Servo,
            is_sandboxed: true,
            memory_usage_mb: 180,
            gpu_accelerated: false,
            sandbox_profile: Some(SandboxProfile {
                pid: std::process::id(),
                restricted_syscalls: vec!["write".into(), "open".into(), "exec".into()],
                memory_limit_mb: 512,
                network_access: true,
            }),
            firewall_status: Some(match load_note {
                Some(note) => format!("Allowed by Servo runtime; load still pending: {}", note),
                None => "Allowed by Servo runtime".into(),
            }),
            layout_time_ms: 0.0,
            parallel_threads: 8,
        }
    }

    fn wait_for_load(
        servo: &mut Servo,
        webview: &WebView,
        timeout: Duration,
    ) -> Result<(), String> {
        wait_for(servo, webview, timeout, |webview| {
            webview.load_status() == LoadStatus::Complete
        })
    }

    fn wait_for_navigation_url(
        servo: &mut Servo,
        webview: &WebView,
        previous_url: Option<&Url>,
        expected_url: &Url,
        timeout: Duration,
    ) -> Result<(), String> {
        wait_for(servo, webview, timeout, |webview| {
            navigation_url_ready(previous_url, expected_url, webview.url().as_ref())
        })
    }

    pub(super) fn navigation_url_ready(
        previous_url: Option<&Url>,
        expected_url: &Url,
        current_url: Option<&Url>,
    ) -> bool {
        let Some(current_url) = current_url else {
            return false;
        };
        current_url == expected_url || Some(current_url) != previous_url
    }

    fn wait_for_changed_url(
        servo: &mut Servo,
        webview: &WebView,
        previous_url: Option<&Url>,
        timeout: Duration,
    ) -> Result<(), String> {
        wait_for(servo, webview, timeout, |webview| {
            webview.url().as_ref() != previous_url
        })
    }

    fn wait_for(
        servo: &mut Servo,
        webview: &WebView,
        timeout: Duration,
        is_ready: impl Fn(&WebView) -> bool,
    ) -> Result<(), String> {
        let started_at = Instant::now();
        loop {
            servo.spin_event_loop();
            if is_ready(webview) {
                return Ok(());
            }

            if started_at.elapsed() >= timeout {
                return Err(format!(
                    "Servo navigation timed out after {}s while waiting for {:?} at {:?}.",
                    timeout.as_secs(),
                    webview.load_status(),
                    webview.url()
                ));
            }

            thread::sleep(EVENT_LOOP_PAUSE);
        }
    }

    fn wait_for_js_result(
        servo: &mut Servo,
        webview: &WebView,
        timeout: Duration,
        result_rx: &mpsc::Receiver<Result<JSValue, JavaScriptEvaluationError>>,
    ) -> Result<JSValue, String> {
        let started_at = Instant::now();
        loop {
            servo.spin_event_loop();
            match result_rx.try_recv() {
                Ok(result) => {
                    return result.map_err(|error| {
                        format!("Servo JavaScript evaluation failed: {:?}", error)
                    });
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("Servo JavaScript evaluation channel disconnected.".to_string());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }

            if started_at.elapsed() >= timeout {
                return Err(format!(
                    "Servo JavaScript evaluation timed out after {}s while waiting for {:?} at {:?}.",
                    timeout.as_secs(),
                    webview.load_status(),
                    webview.url()
                ));
            }

            thread::sleep(EVENT_LOOP_PAUSE);
        }
    }

    fn evaluate_javascript_sync(
        servo: &mut Servo,
        webview: &WebView,
        script: &str,
        timeout: Duration,
    ) -> Result<JSValue, String> {
        let (result_tx, result_rx) = mpsc::channel();
        webview.evaluate_javascript(script, move |result| {
            let _ = result_tx.send(result);
        });
        wait_for_js_result(servo, webview, timeout, &result_rx)
    }

    fn extract_string(value: &JSValue) -> Option<String> {
        match value {
            JSValue::String(value) => Some(value.clone()),
            JSValue::Number(value) => Some(value.to_string()),
            JSValue::Boolean(value) => Some(value.to_string()),
            JSValue::Null | JSValue::Undefined => None,
            _ => None,
        }
    }

    fn extract_object<'a>(
        value: &'a JSValue,
        context: &str,
    ) -> Result<&'a HashMap<String, JSValue>, String> {
        match value {
            JSValue::Object(values) => Ok(values),
            _ => Err(format!(
                "Servo DOM snapshot returned non-object {}",
                context
            )),
        }
    }

    fn extract_array<'a>(value: &'a JSValue, context: &str) -> Result<&'a [JSValue], String> {
        match value {
            JSValue::Array(values) => Ok(values.as_slice()),
            _ => Err(format!("Servo DOM snapshot returned non-array {}", context)),
        }
    }

    fn semantic_node_from_dom(value: &JSValue) -> Result<Option<SemanticNode>, String> {
        let object = extract_object(value, "entry")?;
        let node_type = match object.get("type").and_then(extract_string).as_deref() {
            Some("heading") => NodeType::Heading,
            Some("link") => NodeType::Link,
            Some("image") => NodeType::Image,
            Some("input") => NodeType::Input,
            Some("text") => NodeType::Text,
            Some(_) | None => return Ok(None),
        };
        let id = object
            .get("id")
            .and_then(extract_string)
            .unwrap_or_else(|| "node".to_string());
        let text = object
            .get("text")
            .and_then(extract_string)
            .map(|value| collapse_whitespace(&value))
            .unwrap_or_default();
        let selector = object
            .get("selector")
            .and_then(extract_string)
            .unwrap_or_default();
        let mut attributes = HashMap::new();
        if let Some(href) = object.get("href").and_then(extract_string) {
            attributes.insert("href".to_string(), href);
        }
        if let Some(src) = object.get("src").and_then(extract_string) {
            attributes.insert("src".to_string(), src);
        }
        if let Some(alt) = object.get("alt").and_then(extract_string) {
            attributes.insert("alt".to_string(), alt);
        }
        if let Some(name) = object.get("name").and_then(extract_string) {
            attributes.insert("name".to_string(), name);
        }
        if let Some(input_type) = object.get("inputType").and_then(extract_string) {
            attributes.insert("type".to_string(), input_type);
        }
        if let Some(value) = object.get("value").and_then(extract_string) {
            attributes.insert("value".to_string(), value);
        }
        if let Some(placeholder) = object.get("placeholder").and_then(extract_string) {
            attributes.insert("placeholder".to_string(), placeholder);
        }

        Ok(Some(SemanticNode {
            id,
            node_type,
            text,
            selector,
            attributes,
        }))
    }

    fn distilled_page_from_dom_snapshot(
        fallback_url: Url,
        snapshot: JSValue,
    ) -> Result<DistilledPage, String> {
        let object = extract_object(&snapshot, "snapshot")?;
        let final_url = object
            .get("url")
            .and_then(extract_string)
            .and_then(|value| Url::parse(&value).ok())
            .unwrap_or(fallback_url);
        let title = object
            .get("title")
            .and_then(extract_string)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| final_url.to_string());
        let content = object
            .get("content")
            .and_then(extract_string)
            .map(|value| collapse_whitespace(&value))
            .unwrap_or_default();
        let content_type = object
            .get("contentType")
            .and_then(extract_string)
            .unwrap_or_else(|| "unknown".to_string());

        let mut semantic_map = Vec::new();
        if let Some(nodes_value) = object.get("semanticMap") {
            for node_value in extract_array(nodes_value, "semanticMap")? {
                if let Some(node) = semantic_node_from_dom(node_value)? {
                    semantic_map.push(node);
                }
            }
        }

        let mut metadata = HashMap::new();
        metadata.insert("content_type".to_string(), content_type);
        metadata.insert("fetched_at".to_string(), chrono::Utc::now().to_rfc3339());
        metadata.insert("final_url".to_string(), final_url.to_string());
        metadata.insert("source".to_string(), "servo-live-dom".to_string());
        if let Some(timings) = object
            .get("timings")
            .and_then(|value| extract_object(value, "timings").ok())
        {
            if let Some(script_ms) = timings.get("scriptMs").and_then(extract_string) {
                metadata.insert("live_dom_script_ms".to_string(), script_ms);
            }
        }

        Ok(DistilledPage {
            title,
            url: final_url,
            content: content.chars().take(12000).collect(),
            semantic_map,
            metadata,
        })
    }

    fn distill_live_dom(
        servo: &mut Servo,
        session: &mut ServoTabSession,
    ) -> Result<DistilledPage, String> {
        let current_url = session
            .webview
            .url()
            .ok_or_else(|| "Servo tab has no active URL to distill.".to_string())?;
        let load_status_before = format!("{:?}", session.webview.load_status());
        let script = r#"
(() => {
  const now = () => (window.performance && performance.now) ? performance.now() : Date.now();
  const scriptStarted = now();
  const toText = (value) => typeof value === 'string' ? value.replace(/\s+/g, ' ').trim() : '';
  const headingNodes = Array.from(document.querySelectorAll('h1, h2, h3')).slice(0, 24);
  const linkNodes = Array.from(document.querySelectorAll('a[href]')).slice(0, 24);
  const imageNodes = Array.from(document.querySelectorAll('img[src]')).slice(0, 12);
  const inputNodes = Array.from(document.querySelectorAll('input, textarea, select')).slice(0, 48);
  const textNodes = Array.from(document.querySelectorAll('p, li, pre, blockquote, figcaption, summary')).slice(0, 160);
  const semanticMap = [];
  const seenText = new Set();

  headingNodes.forEach((node, index) => {
    semanticMap.push({
      id: `heading_${index}`,
      type: 'heading',
      text: toText(node.textContent || ''),
      selector: node.tagName.toLowerCase()
    });
  });

  linkNodes.forEach((node, index) => {
    semanticMap.push({
      id: `link_${index}`,
      type: 'link',
      text: toText(node.textContent || ''),
      selector: 'a',
      href: node.href || ''
    });
  });

  imageNodes.forEach((node, index) => {
    semanticMap.push({
      id: `image_${index}`,
      type: 'image',
      text: toText(node.alt || ''),
      selector: 'img',
      src: node.src || '',
      alt: node.alt || ''
    });
  });

  inputNodes.forEach((node, index) => {
    const label = node.labels && node.labels.length
      ? Array.from(node.labels).map(label => toText(label.textContent || '')).filter(Boolean).join(' ')
      : '';
    semanticMap.push({
      id: node.id ? `input_${node.id}` : `input_${index}`,
      type: 'input',
      text: label || node.getAttribute('aria-label') || node.getAttribute('name') || node.getAttribute('placeholder') || '',
      selector: node.id ? `#${node.id}` : node.name ? `[name="${node.name}"]` : node.tagName.toLowerCase(),
      name: node.getAttribute('name') || '',
      inputType: node.getAttribute('type') || node.tagName.toLowerCase(),
      value: 'value' in node ? String(node.value || '') : '',
      placeholder: node.getAttribute('placeholder') || ''
    });
  });

  textNodes.forEach((node, index) => {
    const text = toText(node.textContent || '');
    if (text.length < 3 || seenText.has(text)) {
      return;
    }
    seenText.add(text);
    semanticMap.push({
      id: `text_${index}`,
      type: 'text',
      text,
      selector: node.tagName.toLowerCase()
    });
  });

  const combined = textNodes
    .map(node => toText(node.textContent || ''))
    .filter(Boolean)
    .join('\n');
  const scriptElapsedMs = Math.max(0, Math.round(now() - scriptStarted));

  return {
    url: window.location.href,
    title: document.title || '',
    contentType: document.contentType || '',
    content: combined || toText(document.documentElement?.textContent || ''),
    semanticMap,
    timings: {
      scriptMs: scriptElapsedMs
    }
  };
})()
"#;
        let eval_started = Instant::now();
        let snapshot =
            evaluate_javascript_sync(servo, &session.webview, script, NAVIGATION_TIMEOUT)?;
        let eval_elapsed = eval_started.elapsed();
        let load_status_after = format!("{:?}", session.webview.load_status());
        let parse_started = Instant::now();
        let mut page = distilled_page_from_dom_snapshot(current_url, snapshot)?;
        let parse_elapsed = parse_started.elapsed();
        page.metadata.insert(
            "live_dom_load_status_before".to_string(),
            load_status_before,
        );
        page.metadata
            .insert("live_dom_load_status_after".to_string(), load_status_after);
        page.metadata.insert(
            "live_dom_eval_ms".to_string(),
            eval_elapsed.as_millis().to_string(),
        );
        page.metadata.insert(
            "live_dom_parse_ms".to_string(),
            parse_elapsed.as_millis().to_string(),
        );
        Ok(page)
    }

    fn eval_probe_tab(
        servo: &mut Servo,
        session: &mut ServoTabSession,
        queue_elapsed: Duration,
    ) -> Result<BrowserEvalProbe, String> {
        let load_status_before = format!("{:?}", session.webview.load_status());
        let script = r#"
(() => {
  const now = () => (window.performance && performance.now) ? performance.now() : Date.now();
  const started = now();
  const value = `${document.readyState || ''}:${(document.title || '').length}`;
  const scriptElapsedMs = Math.max(0, Math.round(now() - started));
  return {
    value,
    timings: {
      scriptMs: scriptElapsedMs
    }
  };
})()
"#;
        let eval_started = Instant::now();
        let result = evaluate_javascript_sync(servo, &session.webview, script, NAVIGATION_TIMEOUT)?;
        let eval_elapsed = eval_started.elapsed();
        let load_status_after = format!("{:?}", session.webview.load_status());
        let object = extract_object(&result, "eval probe result")?;
        let script_ms = object
            .get("timings")
            .and_then(|value| extract_object(value, "eval probe timings").ok())
            .and_then(|timings| timings.get("scriptMs"))
            .and_then(extract_string)
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        let value = object
            .get("value")
            .and_then(extract_string)
            .unwrap_or_default();

        Ok(BrowserEvalProbe {
            queue_ms: queue_elapsed.as_millis() as u64,
            eval_ms: eval_elapsed.as_millis() as u64,
            script_ms,
            load_status_before,
            load_status_after,
            value,
        })
    }

    #[cfg(target_os = "windows")]
    fn prime_windows_angle_runtime() -> Result<(), String> {
        let mut current_path = env::var_os("PATH").unwrap_or_default();
        for candidate in windows_angle_runtime_candidates() {
            if candidate.join("libEGL.dll").is_file() && candidate.join("libGLESv2.dll").is_file() {
                let already_present = env::split_paths(&current_path).any(|path| path == candidate);
                if !already_present {
                    let mut paths = vec![candidate.clone()];
                    paths.extend(env::split_paths(&current_path));
                    current_path = env::join_paths(paths).map_err(|e| {
                        format!("Failed to extend PATH for Servo ANGLE runtime: {}", e)
                    })?;
                    env::set_var("PATH", &current_path);
                }
                return Ok(());
            }
        }

        Err(format!(
            "Servo requires libEGL.dll and libGLESv2.dll on PATH. Checked: {}",
            windows_angle_runtime_candidates()
                .into_iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }

    #[cfg(not(target_os = "windows"))]
    fn prime_windows_angle_runtime() -> Result<(), String> {
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn windows_angle_runtime_candidates() -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        candidates.extend(existing_subdirectories(
            r"C:\Program Files (x86)\Microsoft\EdgeWebView\Application",
        ));
        candidates.extend(existing_subdirectories(
            r"C:\Program Files (x86)\Microsoft\EdgeCore",
        ));
        candidates.extend(existing_subdirectories(
            r"C:\Program Files (x86)\Microsoft\Edge\Application",
        ));
        candidates.extend(existing_subdirectories(
            r"C:\Program Files\Google\Chrome\Application",
        ));
        candidates.push(PathBuf::from(
            r"C:\Program Files (x86)\Microsoft\EdgeCore\Optimized",
        ));
        candidates.push(PathBuf::from(r"C:\Program Files\Mozilla Firefox"));
        candidates.push(PathBuf::from(r"C:\Program Files\Firefox Developer Edition"));
        candidates
    }

    #[cfg(target_os = "windows")]
    fn existing_subdirectories(root: &str) -> Vec<PathBuf> {
        std::fs::read_dir(root)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn capture_command(tab_id: Uuid) -> ServoCommand {
            let (reply_tx, _reply_rx) = mpsc::channel();
            ServoCommand::CaptureFrame { tab_id, reply_tx }
        }

        fn inspect_command(tab_id: Uuid) -> ServoCommand {
            let (reply_tx, _reply_rx) = mpsc::channel();
            ServoCommand::InspectTab { tab_id, reply_tx }
        }

        fn distill_command(tab_id: Uuid) -> ServoCommand {
            let (reply_tx, _reply_rx) = mpsc::channel();
            ServoCommand::DistillTab {
                tab_id,
                enqueued_at: Instant::now(),
                reply_tx,
            }
        }

        fn eval_probe_command(tab_id: Uuid) -> ServoCommand {
            let (reply_tx, _reply_rx) = mpsc::channel();
            ServoCommand::EvalProbeTab {
                tab_id,
                enqueued_at: Instant::now(),
                reply_tx,
            }
        }

        fn key_text_command(tab_id: Uuid, text: &str) -> ServoCommand {
            let (reply_tx, _reply_rx) = mpsc::channel();
            ServoCommand::KeyCharacter {
                tab_id,
                text: text.to_string(),
                pressed: true,
                reply_tx,
            }
        }

        fn mouse_move_command(tab_id: Uuid, x: f32, y: f32) -> ServoCommand {
            let (reply_tx, _reply_rx) = mpsc::channel();
            ServoCommand::MouseMove {
                tab_id,
                x,
                y,
                reply_tx,
            }
        }

        #[test]
        fn service_scheduler_prioritizes_input_over_queued_distillation() {
            let (command_tx, command_rx) = mpsc::channel();
            let tab_id = Uuid::new_v4();
            command_tx.send(distill_command(tab_id)).unwrap();
            command_tx.send(key_text_command(tab_id, "a")).unwrap();
            let mut pending_commands = VecDeque::new();

            let command = next_servo_command(&command_rx, &mut pending_commands).unwrap();
            assert!(matches!(command, ServoCommand::KeyCharacter { .. }));
            assert_eq!(pending_commands.len(), 1);

            let command = next_servo_command(&command_rx, &mut pending_commands).unwrap();
            assert!(matches!(command, ServoCommand::DistillTab { .. }));
        }

        #[test]
        fn service_scheduler_prioritizes_input_over_queued_eval_probe() {
            let (command_tx, command_rx) = mpsc::channel();
            let tab_id = Uuid::new_v4();
            command_tx.send(eval_probe_command(tab_id)).unwrap();
            command_tx.send(key_text_command(tab_id, "a")).unwrap();
            let mut pending_commands = VecDeque::new();

            let command = next_servo_command(&command_rx, &mut pending_commands).unwrap();
            assert!(matches!(command, ServoCommand::KeyCharacter { .. }));
            assert_eq!(pending_commands.len(), 1);

            let command = next_servo_command(&command_rx, &mut pending_commands).unwrap();
            assert!(matches!(command, ServoCommand::EvalProbeTab { .. }));
        }

        #[test]
        fn service_scheduler_prioritizes_input_over_queued_frame_capture() {
            let (command_tx, command_rx) = mpsc::channel();
            let tab_id = Uuid::new_v4();
            command_tx.send(capture_command(tab_id)).unwrap();
            command_tx.send(key_text_command(tab_id, "a")).unwrap();
            let mut pending_commands = VecDeque::new();

            let command = next_servo_command(&command_rx, &mut pending_commands).unwrap();
            assert!(matches!(command, ServoCommand::KeyCharacter { .. }));
            assert_eq!(pending_commands.len(), 1);

            let command = next_servo_command(&command_rx, &mut pending_commands).unwrap();
            assert!(matches!(command, ServoCommand::CaptureFrame { .. }));
        }

        #[test]
        fn service_scheduler_keeps_non_render_work_ahead_of_input() {
            let (command_tx, command_rx) = mpsc::channel();
            let tab_id = Uuid::new_v4();
            command_tx.send(capture_command(tab_id)).unwrap();
            command_tx.send(inspect_command(tab_id)).unwrap();
            command_tx.send(key_text_command(tab_id, "a")).unwrap();
            let mut pending_commands = VecDeque::new();

            let command = next_servo_command(&command_rx, &mut pending_commands).unwrap();
            assert!(matches!(command, ServoCommand::CaptureFrame { .. }));

            let command = next_servo_command(&command_rx, &mut pending_commands).unwrap();
            assert!(matches!(command, ServoCommand::InspectTab { .. }));

            let command = next_servo_command(&command_rx, &mut pending_commands).unwrap();
            assert!(matches!(command, ServoCommand::KeyCharacter { .. }));
        }

        #[test]
        fn service_scheduler_coalesces_mouse_moves_across_frame_captures() {
            let tab_id = Uuid::new_v4();
            let mut pending_commands = VecDeque::from([
                capture_command(tab_id),
                mouse_move_command(tab_id, 20.0, 30.0),
                capture_command(tab_id),
                mouse_move_command(tab_id, 40.0, 50.0),
            ]);
            let mut x = 10.0;
            let mut y = 15.0;

            coalesce_pending_mouse_move(&mut pending_commands, tab_id, &mut x, &mut y);

            assert_eq!((x, y), (40.0, 50.0));
            assert_eq!(pending_commands.len(), 2);
            assert!(pending_commands
                .iter()
                .all(|command| matches!(command, ServoCommand::CaptureFrame { .. })));
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum EngineBackend {
    Servo,    // Primary: Parallelized, Memory-Safe
    Gecko,    // Compatibility: Mature, Standard-compliant
    Chromium, // Legacy: "Optimized for Chrome" sites
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SandboxProfile {
    pub pid: u32,
    pub restricted_syscalls: Vec<String>,
    pub memory_limit_mb: u32,
    pub network_access: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LayoutFragment {
    pub id: u32,
    pub selector: String,
    pub depth: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LayoutResult {
    pub fragment_id: u32,
    pub layout_time_ms: f64,
    pub thread_id: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EngineStatus {
    pub active_backend: EngineBackend,
    pub is_sandboxed: bool,
    pub memory_usage_mb: u32,
    pub gpu_accelerated: bool,
    pub sandbox_profile: Option<SandboxProfile>,
    pub firewall_status: Option<String>,
    pub layout_time_ms: f64,
    pub parallel_threads: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum NodeType {
    Link,
    Button,
    Input,
    Text,
    Heading,
    Image,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SemanticNode {
    pub id: String,
    pub node_type: NodeType,
    pub text: String,
    pub selector: String,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DistilledPage {
    pub title: String,
    pub url: Url,
    pub content: String,
    pub semantic_map: Vec<SemanticNode>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RenderedFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BrowserEvalProbe {
    pub queue_ms: u64,
    pub eval_ms: u64,
    pub script_ms: u64,
    pub load_status_before: String,
    pub load_status_after: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct AsyncFrameCapture {
    pub resize: Option<Duration>,
    pub capture: Duration,
    pub frame: RenderedFrame,
}

#[derive(Debug, Clone, Default)]
pub struct AsyncNavigationTimings {
    pub firewall: Option<Duration>,
    pub servo_navigation: Option<Duration>,
    pub servo_inspect: Option<Duration>,
    pub reader_fallback: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct AsyncNavigationResult {
    pub tab_id: Uuid,
    pub requested_url: Url,
    pub final_url: Url,
    pub status: EngineStatus,
    pub distilled_page: Option<DistilledPage>,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub timings: AsyncNavigationTimings,
    pub initial_frame: Option<Result<AsyncFrameCapture, String>>,
}

#[derive(Debug, Clone, Copy)]
enum AsyncNavigationControl {
    Reload,
    Back,
    Forward,
}

#[derive(Debug, Clone)]
pub struct AsyncDistillResult {
    pub tab_id: Uuid,
    pub page: DistilledPage,
    pub status_note: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum BrowserInteraction {
    ClickSelector { selector: String },
    FillSelector { selector: String, value: String },
    SubmitSelector { selector: String },
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct BrowserInteractionResult {
    pub ok: bool,
    pub message: String,
    pub selector: String,
    pub tag: String,
    pub text: String,
    pub value: String,
    pub current_url: Option<Url>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum BrowserKey {
    Enter,
    Backspace,
    Tab,
    Escape,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Delete,
}

impl BrowserKey {
    #[cfg(feature = "servo-backend")]
    fn into_servo_key(self) -> servo_runtime::BrowserNamedKey {
        match self {
            BrowserKey::Enter => servo_runtime::BrowserNamedKey::Enter,
            BrowserKey::Backspace => servo_runtime::BrowserNamedKey::Backspace,
            BrowserKey::Tab => servo_runtime::BrowserNamedKey::Tab,
            BrowserKey::Escape => servo_runtime::BrowserNamedKey::Escape,
            BrowserKey::ArrowLeft => servo_runtime::BrowserNamedKey::ArrowLeft,
            BrowserKey::ArrowRight => servo_runtime::BrowserNamedKey::ArrowRight,
            BrowserKey::ArrowUp => servo_runtime::BrowserNamedKey::ArrowUp,
            BrowserKey::ArrowDown => servo_runtime::BrowserNamedKey::ArrowDown,
            BrowserKey::Delete => servo_runtime::BrowserNamedKey::Delete,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Tab {
    pub id: Uuid,
    pub url: Option<Url>,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub status: EngineStatus,
    pub distilled_page: Option<DistilledPage>,
    pub last_active: DateTime<Utc>,
}

pub struct WgpuRenderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub pipeline: wgpu::RenderPipeline,
}

impl WgpuRenderer {
    pub async fn new(instance: &wgpu::Instance) -> Result<Self, String> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or("Failed to find a suitable GPU adapter")?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Sextant GPU Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .map_err(|e| e.to_string())?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Sextant Layout Shader"),
            source: wgpu::ShaderSource::Wgsl("
                @vertex
                fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> @builtin(position) vec4<f32> {
                    let x = f32(i32(in_vertex_index) - 1);
                    let y = f32(i32(in_vertex_index & 1u) * 2 - 1);
                    return vec4<f32>(x, y, 0.0, 1.0);
                }

                @fragment
                fn fs_main() -> @location(0) vec4<f32> {
                    return vec4<f32>(0.0, 1.0, 0.5, 1.0);
                }
            ".into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Sextant Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Sextant Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        Ok(Self {
            device,
            queue,
            pipeline,
        })
    }

    pub fn render_frame(&self) -> Result<(), String> {
        let encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Sextant Render Encoder"),
            });

        // In a real app, we would render to a texture or surface
        // Here we just simulate the command submission
        self.queue.submit(std::iter::once(encoder.finish()));
        Ok(())
    }
}

pub struct ParallelLayoutEngine {
    thread_pool: rayon::ThreadPool,
}

impl ParallelLayoutEngine {
    pub fn new(threads: usize) -> Self {
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        Self { thread_pool }
    }

    pub fn layout_parallel(&self, fragments: Vec<LayoutFragment>) -> Vec<LayoutResult> {
        self.thread_pool.install(|| {
            fragments
                .into_par_iter()
                .map(|f| {
                    // Simulate complex layout calculation
                    let start = std::time::Instant::now();
                    let mut _sum = 0.0;
                    for i in 0..10000 {
                        _sum += (i as f64).sqrt();
                    }
                    LayoutResult {
                        fragment_id: f.id,
                        layout_time_ms: start.elapsed().as_secs_f64() * 1000.0,
                        thread_id: rayon::current_thread_index().unwrap_or(0),
                    }
                })
                .collect()
        })
    }
}

pub struct SextantEngine {
    tabs: HashMap<Uuid, Tab>,
    tab_order: Vec<Uuid>,
    active_tab_id: Option<Uuid>,
    fallback_enabled: bool,
    semantic_cache: HashMap<Url, DistilledPage>,
    gpu_instance: Option<Arc<wgpu::Instance>>,
    renderer: Option<Arc<WgpuRenderer>>,
    firewall: Arc<SextantFirewall>,
    bridge: Arc<NeuralBridge>,
    #[cfg(not(feature = "servo-backend"))]
    layout_engine: Arc<ParallelLayoutEngine>,
    #[cfg(feature = "servo-backend")]
    servo_service: servo_runtime::ServoServiceHandle,
}

struct RenderOutcome {
    final_url: Url,
    status: EngineStatus,
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_selector(pattern: &str) -> Result<Selector, String> {
    Selector::parse(pattern).map_err(|e| format!("Invalid selector '{}': {:?}", pattern, e))
}

fn extract_text(document: &Html, pattern: &str, limit: usize) -> Result<Vec<String>, String> {
    let selector = parse_selector(pattern)?;
    Ok(document
        .select(&selector)
        .filter_map(|node| {
            let text = collapse_whitespace(&node.text().collect::<Vec<_>>().join(" "));
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        })
        .take(limit)
        .collect())
}

fn percent_decode_utf8(input: &str) -> Result<String, String> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err("Percent-encoded input ended early.".to_string());
            }
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
                .map_err(|e| format!("Invalid percent-encoded bytes: {}", e))?;
            let value = u8::from_str_radix(hex, 16)
                .map_err(|e| format!("Invalid percent-encoded sequence '%{hex}': {e}"))?;
            decoded.push(value);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }

    String::from_utf8(decoded).map_err(|e| format!("Decoded text was not valid UTF-8: {}", e))
}

fn distill_html_document(
    final_url: Url,
    content_type: String,
    html: String,
) -> Result<DistilledPage, String> {
    match distill_html_document_with_wsky(&final_url, &content_type, &html) {
        Ok(page) => return Ok(page),
        Err(primary_error) if should_try_render_distiller(&final_url, &primary_error) => {
            if let Ok(page) = distill_with_render_helper(&final_url, &content_type) {
                return Ok(page);
            }
        }
        Err(_) => {}
    }

    distill_html_document_with_scraper(final_url, content_type, html)
}

fn should_try_render_distiller(url: &Url, primary_error: &str) -> bool {
    matches!(url.scheme(), "http" | "https") && primary_error.contains("low-content")
}

fn distill_html_document_with_wsky(
    final_url: &Url,
    content_type: &str,
    html: &str,
) -> Result<DistilledPage, String> {
    let options = DistillOptions {
        include_images: false,
        no_frontmatter: true,
        format: Format::Rich,
        fast: false,
    };
    let document = distill_core::distill_html(html, Some(final_url.as_str()), &options)
        .map_err(|e| format!("wsky distill failed: {}", e))?;
    if distill_core::is_low_content_markdown(&document.markdown) {
        return Err("wsky distill returned low-content output".to_string());
    }

    let semantic_map = semantic_map_from_html(&document.article.content)?;
    let mut metadata = HashMap::new();
    metadata.insert("content_type".to_string(), content_type.to_string());
    metadata.insert("fetched_at".to_string(), Utc::now().to_rfc3339());
    metadata.insert("content_bytes".to_string(), html.len().to_string());
    metadata.insert("final_url".to_string(), final_url.to_string());
    metadata.insert(
        "distiller".to_string(),
        "wsky-distiller/distill_core".to_string(),
    );
    if let Some(excerpt) = document.article.excerpt {
        metadata.insert("excerpt".to_string(), excerpt);
    }

    Ok(DistilledPage {
        title: document.article.title,
        url: final_url.clone(),
        content: document.markdown.chars().take(12000).collect(),
        semantic_map,
        metadata,
    })
}

fn distill_with_render_helper(
    final_url: &Url,
    content_type: &str,
) -> Result<DistilledPage, String> {
    let helper = find_distill_render_helper()
        .ok_or_else(|| "distill-render helper is not built or configured".to_string())?;
    let timeout = Duration::from_secs(45);
    let mut child = Command::new(&helper)
        .arg(final_url.as_str())
        .arg("--timeout")
        .arg("25")
        .arg("--wait")
        .arg("networkidle")
        .arg("--no-frontmatter")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to launch {}: {}", helper.display(), e))?;

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => break,
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Ok(None) => {
                let _ = child.kill();
                return Err(format!(
                    "distill-render timed out after {}s",
                    timeout.as_secs()
                ));
            }
            Err(error) => return Err(format!("distill-render wait failed: {}", error)),
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("failed to collect distill-render output: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "distill-render exited with {}: {}",
            output.status,
            collapse_whitespace(&stderr)
        ));
    }

    let markdown = String::from_utf8(output.stdout)
        .map_err(|e| format!("distill-render emitted non-UTF8 output: {}", e))?;
    if distill_core::is_low_content_markdown(&markdown) {
        return Err("distill-render returned low-content output".to_string());
    }

    let title = title_from_markdown(&markdown).unwrap_or_else(|| final_url.to_string());
    let mut metadata = HashMap::new();
    metadata.insert("content_type".to_string(), content_type.to_string());
    metadata.insert("fetched_at".to_string(), Utc::now().to_rfc3339());
    metadata.insert("final_url".to_string(), final_url.to_string());
    metadata.insert(
        "distiller".to_string(),
        "wsky-distiller/distill-render".to_string(),
    );
    metadata.insert("helper".to_string(), helper.display().to_string());

    Ok(DistilledPage {
        title,
        url: final_url.clone(),
        content: markdown.chars().take(12000).collect(),
        semantic_map: semantic_map_from_markdown(&markdown),
        metadata,
    })
}

fn find_distill_render_helper() -> Option<PathBuf> {
    if let Ok(path) = env::var("WSKY_DISTILL_RENDER") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }

    let exe = if cfg!(windows) {
        "distill-render.exe"
    } else {
        "distill-render"
    };
    [
        PathBuf::from("../../../wsky-distiller/distill-render/target/release").join(exe),
        PathBuf::from("../../../wsky-distiller/distill-render/target/debug").join(exe),
        PathBuf::from("../../wsky-distiller/distill-render/target/release").join(exe),
        PathBuf::from("../../wsky-distiller/distill-render/target/debug").join(exe),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

fn title_from_markdown(markdown: &str) -> Option<String> {
    markdown
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|title| !title.is_empty())
        .map(|title| title.to_string())
}

fn semantic_map_from_markdown(markdown: &str) -> Vec<SemanticNode> {
    markdown
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(48)
        .enumerate()
        .map(|(index, line)| {
            let node_type = if line.starts_with('#') {
                NodeType::Heading
            } else {
                NodeType::Text
            };
            SemanticNode {
                id: format!("rendered_markdown_{}", index),
                node_type,
                text: line.trim_start_matches('#').trim().to_string(),
                selector: "markdown".to_string(),
                attributes: HashMap::new(),
            }
        })
        .collect()
}

#[cfg(feature = "servo-backend")]
fn semantic_signal_count(page: &DistilledPage) -> usize {
    page.semantic_map
        .iter()
        .filter(|node| {
            matches!(
                node.node_type,
                NodeType::Heading | NodeType::Link | NodeType::Input | NodeType::Text
            ) && !node.text.trim().is_empty()
        })
        .count()
}

#[cfg(feature = "servo-backend")]
fn should_try_reader_quality_fallback(page: &DistilledPage) -> bool {
    matches!(page.url.scheme(), "http" | "https") && semantic_signal_count(page) < 3
}

fn semantic_map_from_html(html: &str) -> Result<Vec<SemanticNode>, String> {
    let document = Html::parse_document(&html);
    let heading_text = extract_text(&document, "h1, h2, h3", 24)?;
    let link_selector = parse_selector("a[href]")?;
    let image_selector = parse_selector("img[src]")?;
    let input_selector = parse_selector("input, textarea, select")?;
    let text_selector = parse_selector("p, li, pre, blockquote, figcaption, summary")?;
    let mut semantic_map = Vec::new();

    for (index, text) in heading_text.iter().enumerate() {
        semantic_map.push(SemanticNode {
            id: format!("heading_{}", index),
            node_type: NodeType::Heading,
            text: text.clone(),
            selector: "h1,h2,h3".to_string(),
            attributes: HashMap::new(),
        });
    }

    for (index, node) in document.select(&link_selector).take(24).enumerate() {
        let text = collapse_whitespace(&node.text().collect::<Vec<_>>().join(" "));
        let href = node.value().attr("href").unwrap_or_default();
        let mut attributes = HashMap::new();
        attributes.insert("href".to_string(), href.to_string());
        semantic_map.push(SemanticNode {
            id: format!("link_{}", index),
            node_type: NodeType::Link,
            text,
            selector: "a".to_string(),
            attributes,
        });
    }

    for (index, node) in document.select(&image_selector).take(12).enumerate() {
        let mut attributes = HashMap::new();
        attributes.insert(
            "src".to_string(),
            node.value().attr("src").unwrap_or_default().to_string(),
        );
        if let Some(alt) = node.value().attr("alt") {
            attributes.insert("alt".to_string(), alt.to_string());
        }
        semantic_map.push(SemanticNode {
            id: format!("image_{}", index),
            node_type: NodeType::Image,
            text: node.value().attr("alt").unwrap_or_default().to_string(),
            selector: "img".to_string(),
            attributes,
        });
    }

    for (index, node) in document.select(&input_selector).take(48).enumerate() {
        let mut attributes = HashMap::new();
        if let Some(name) = node.value().attr("name") {
            attributes.insert("name".to_string(), name.to_string());
        }
        if let Some(input_type) = node.value().attr("type") {
            attributes.insert("type".to_string(), input_type.to_string());
        }
        if let Some(value) = node.value().attr("value") {
            attributes.insert("value".to_string(), value.to_string());
        }
        if let Some(placeholder) = node.value().attr("placeholder") {
            attributes.insert("placeholder".to_string(), placeholder.to_string());
        }
        let text = node
            .value()
            .attr("aria-label")
            .or_else(|| node.value().attr("name"))
            .or_else(|| node.value().attr("placeholder"))
            .unwrap_or_default()
            .to_string();
        semantic_map.push(SemanticNode {
            id: node
                .value()
                .attr("id")
                .map(|id| format!("input_{}", id))
                .unwrap_or_else(|| format!("input_{}", index)),
            node_type: NodeType::Input,
            text,
            selector: node
                .value()
                .attr("id")
                .map(|id| format!("#{}", id))
                .or_else(|| {
                    node.value()
                        .attr("name")
                        .map(|name| format!("[name=\"{}\"]", name))
                })
                .unwrap_or_else(|| node.value().name().to_string()),
            attributes,
        });
    }

    let mut seen_text = HashSet::new();
    for (index, node) in document.select(&text_selector).take(160).enumerate() {
        let text = collapse_whitespace(&node.text().collect::<Vec<_>>().join(" "));
        if text.len() < 3 || !seen_text.insert(text.clone()) {
            continue;
        }
        semantic_map.push(SemanticNode {
            id: format!("text_{}", index),
            node_type: NodeType::Text,
            text,
            selector: node.value().name().to_string(),
            attributes: HashMap::new(),
        });
    }

    Ok(semantic_map)
}

fn distill_html_document_with_scraper(
    final_url: Url,
    content_type: String,
    html: String,
) -> Result<DistilledPage, String> {
    let document = Html::parse_document(&html);
    let title = extract_text(&document, "title", 1)?
        .into_iter()
        .next()
        .unwrap_or_else(|| final_url.to_string());
    let body_text = extract_text(&document, "main, article, section, p, li", 400)?;
    let mut semantic_map = semantic_map_from_html(&html)?;

    let link_selector = parse_selector("a[href]")?;
    for (index, node) in document.select(&link_selector).take(24).enumerate() {
        let href = node.value().attr("href").unwrap_or_default();
        let absolute_href = final_url
            .join(href)
            .map(|joined| joined.to_string())
            .unwrap_or_else(|_| href.to_string());
        if let Some(link) = semantic_map
            .iter_mut()
            .filter(|node| matches!(node.node_type, NodeType::Link))
            .nth(index)
        {
            link.attributes.insert("href".to_string(), absolute_href);
        }
    }

    let combined_content = body_text.join("\n");
    let content = if combined_content.is_empty() {
        collapse_whitespace(&document.root_element().text().collect::<Vec<_>>().join(" "))
    } else {
        combined_content
    };

    let mut metadata = HashMap::new();
    metadata.insert("content_type".to_string(), content_type);
    metadata.insert("fetched_at".to_string(), Utc::now().to_rfc3339());
    metadata.insert("content_bytes".to_string(), html.len().to_string());
    metadata.insert("final_url".to_string(), final_url.to_string());
    metadata.insert(
        "distiller".to_string(),
        "sextant-scraper-fallback".to_string(),
    );

    Ok(DistilledPage {
        title,
        url: final_url,
        content: content.chars().take(12000).collect(),
        semantic_map,
        metadata,
    })
}

fn fetch_distilled_page(url: &Url) -> Result<DistilledPage, String> {
    if url.scheme() == "about" {
        return Ok(DistilledPage {
            title: "Blank Page".to_string(),
            url: url.clone(),
            content: String::new(),
            semantic_map: Vec::new(),
            metadata: HashMap::new(),
        });
    }

    if url.scheme() == "data" {
        let payload = url.as_str().strip_prefix("data:").unwrap_or_default();
        let (metadata, encoded_body) = payload
            .split_once(',')
            .ok_or_else(|| format!("Data URL is missing a payload: {}", url))?;
        if metadata
            .split(';')
            .any(|part| part.eq_ignore_ascii_case("base64"))
        {
            return Err(format!(
                "Base64 data URLs are not yet supported for distillation: {}",
                url
            ));
        }
        let content_type = metadata
            .split(';')
            .next()
            .filter(|value| !value.is_empty())
            .unwrap_or("text/plain;charset=US-ASCII")
            .to_string();
        let html = percent_decode_utf8(encoded_body)?;
        return distill_html_document(url.clone(), content_type, html);
    }

    if url.scheme() == "file" {
        let path = url
            .to_file_path()
            .map_err(|_| format!("Unsupported file URL path: {}", url))?;
        let html = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        let content_type = match path.extension().and_then(|ext| ext.to_str()) {
            Some(ext) if ext.eq_ignore_ascii_case("html") || ext.eq_ignore_ascii_case("htm") => {
                "text/html".to_string()
            }
            _ => "text/plain".to_string(),
        };
        return distill_html_document(url.clone(), content_type, html);
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("Sextant/0.1")
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let response = client
        .get(url.clone())
        .send()
        .map_err(|e| format!("Failed to fetch {}: {}", url, e))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("Fetch failed for {} with HTTP {}", url, status));
    }

    let final_url = response.url().clone();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    let html = response
        .text()
        .map_err(|e| format!("Failed to read response body for {}: {}", final_url, e))?;
    distill_html_document(final_url, content_type, html)
}

#[cfg(feature = "servo-backend")]
fn reader_fallback_status(note: String) -> EngineStatus {
    EngineStatus {
        active_backend: EngineBackend::Servo,
        is_sandboxed: true,
        memory_usage_mb: 0,
        gpu_accelerated: false,
        sandbox_profile: Some(SandboxProfile {
            pid: std::process::id(),
            restricted_syscalls: vec!["write".into(), "open".into(), "exec".into()],
            memory_limit_mb: 512,
            network_access: true,
        }),
        firewall_status: Some(note),
        layout_time_ms: 0.0,
        parallel_threads: 1,
    }
}

impl SextantEngine {
    pub fn new() -> Self {
        let mut engine = Self {
            tabs: HashMap::new(),
            tab_order: Vec::new(),
            active_tab_id: None,
            fallback_enabled: true,
            semantic_cache: HashMap::new(),
            gpu_instance: None,
            renderer: None,
            firewall: Arc::new(SextantFirewall::new()),
            bridge: Arc::new(NeuralBridge::new()),
            #[cfg(not(feature = "servo-backend"))]
            layout_engine: Arc::new(ParallelLayoutEngine::new(8)),
            #[cfg(feature = "servo-backend")]
            servo_service: servo_runtime::shared_handle(),
        };
        engine.open_tab();
        engine
    }

    pub async fn initialize_gpu(&mut self) -> Result<(), String> {
        if self.gpu_instance.is_none() {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: wgpu::Backends::all(),
                ..Default::default()
            });
            self.gpu_instance = Some(Arc::new(instance));
        }
        if let Some(instance) = &self.gpu_instance {
            let renderer = WgpuRenderer::new(instance).await?;
            self.renderer = Some(Arc::new(renderer));

            // Update all tabs to reflect GPU acceleration
            for tab in self.tabs.values_mut() {
                if tab.status.active_backend == EngineBackend::Servo {
                    tab.status.gpu_accelerated = true;
                }
            }
        }
        Ok(())
    }

    pub fn configure_initial_servo_viewport(&self, width: u32, height: u32) {
        #[cfg(feature = "servo-backend")]
        self.servo_service.configure_initial_viewport(width, height);
        #[cfg(not(feature = "servo-backend"))]
        let _ = (width, height);
    }

    pub fn open_tab(&mut self) -> Uuid {
        let id = Uuid::new_v4();
        let tab = Tab {
            id,
            url: None,
            can_go_back: false,
            can_go_forward: false,
            status: EngineStatus {
                active_backend: EngineBackend::Servo,
                is_sandboxed: true,
                memory_usage_mb: 0,
                gpu_accelerated: self.renderer.is_some(),
                sandbox_profile: None,
                firewall_status: None,
                layout_time_ms: 0.0,
                parallel_threads: 8,
            },
            distilled_page: None,
            last_active: Utc::now(),
        };
        self.tabs.insert(id, tab);
        self.tab_order.push(id);
        self.active_tab_id = Some(id);
        id
    }

    pub fn close_tab(&mut self, id: &Uuid) -> Result<(), String> {
        self.tabs.remove(id).ok_or("Tab not found")?;
        self.close_servo_tab_session(*id)?;
        let closed_position = self.tab_order.iter().position(|tab_id| tab_id == id);
        if let Some(position) = closed_position {
            self.tab_order.remove(position);
        } else {
            self.tab_order.retain(|tab_id| tab_id != id);
        }
        if self.active_tab_id == Some(*id) {
            self.active_tab_id = closed_position
                .and_then(|position| self.tab_order.get(position).copied())
                .or_else(|| self.tab_order.last().copied());
        }
        Ok(())
    }

    pub fn switch_to_tab(&mut self, id: Uuid) -> Result<(), String> {
        if !self.tabs.contains_key(&id) {
            return Err("Tab not found".into());
        }
        if let Some(tab) = self.tabs.get_mut(&id) {
            tab.last_active = Utc::now();
        }
        self.active_tab_id = Some(id);
        self.sync_tab_navigation_state(id);
        Ok(())
    }

    fn invalidate_tab_distillation(&mut self, tab_id: Uuid, final_url: &Url) {
        let previous_url = self.tabs.get(&tab_id).and_then(|tab| tab.url.clone());
        if let Some(previous_url) = previous_url {
            self.semantic_cache.remove(&previous_url);
        }
        self.semantic_cache.remove(final_url);
        if let Some(tab) = self.tabs.get_mut(&tab_id) {
            tab.distilled_page = None;
        }
    }

    pub fn get_active_tab(&self) -> Option<&Tab> {
        self.active_tab_id.and_then(|id| self.tabs.get(&id))
    }

    pub fn bridge_perceive(&self) -> Result<MultiModalPerception, String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        let frame = self.bridge.capture_frame(&tab_id);
        let audio = self.bridge.capture_audio(&tab_id);

        Ok(self.bridge.bridge_to_neural_engine(vec![frame, audio]))
    }

    pub fn get_tabs(&self) -> Vec<Tab> {
        let mut tabs: Vec<Tab> = self
            .tab_order
            .iter()
            .filter_map(|id| self.tabs.get(id).cloned())
            .collect();
        let ordered_ids: HashSet<Uuid> = self.tab_order.iter().copied().collect();
        let mut unordered_tabs: Vec<Tab> = self
            .tabs
            .iter()
            .filter(|(id, _)| !ordered_ids.contains(id))
            .map(|(_, tab)| tab.clone())
            .collect();
        unordered_tabs.sort_by(|a, b| b.last_active.cmp(&a.last_active));
        tabs.extend(unordered_tabs);
        tabs
    }

    fn close_servo_tab_session(&self, _tab_id: Uuid) -> Result<(), String> {
        #[cfg(feature = "servo-backend")]
        {
            self.servo_service.close_tab(_tab_id)?;
        }
        Ok(())
    }

    fn sync_tab_navigation_state(&mut self, tab_id: Uuid) {
        #[cfg(feature = "servo-backend")]
        if let Some(tab) = self.tabs.get_mut(&tab_id) {
            if tab.status.active_backend == EngineBackend::Servo {
                if tab.url.is_none() {
                    tab.can_go_back = false;
                    tab.can_go_forward = false;
                    return;
                }
                match self.servo_service.inspect_tab(tab_id) {
                    Ok(snapshot) => {
                        tab.can_go_back = snapshot.can_go_back;
                        tab.can_go_forward = snapshot.can_go_forward;
                        if snapshot.current_url.is_some() {
                            tab.url = snapshot.current_url;
                        }
                    }
                    Err(error) => {
                        tab.can_go_back = false;
                        tab.can_go_forward = false;
                        tab.status.firewall_status =
                            Some(format!("Servo navigation state unavailable: {}", error));
                    }
                }
                return;
            }
        }

        if let Some(tab) = self.tabs.get_mut(&tab_id) {
            tab.can_go_back = false;
            tab.can_go_forward = false;
        }
    }

    /// Navigates to a URL in the active tab, automatically falling back if rendering fails.
    pub fn navigate_with_fallback(
        &mut self,
        url: Url,
        persona_id: &str,
    ) -> Result<EngineStatus, String> {
        let tab_id = match self.active_tab_id {
            Some(tab_id) => tab_id,
            None => {
                let tab_id = self.open_tab();
                self.active_tab_id = Some(tab_id);
                tab_id
            }
        };

        // 1. Firewall Check
        let (firewall_action, reason) = self.firewall.check_access(persona_id, &url);
        if firewall_action == FirewallAction::Block {
            if let Some(tab) = self.tabs.get_mut(&tab_id) {
                tab.status.firewall_status = Some(format!("Blocked: {}", reason));
            }
            return Err(format!("Firewall Blocked: {} (Reason: {})", url, reason));
        }

        let mut active_backend = self
            .tabs
            .get(&tab_id)
            .ok_or("Active tab not found")?
            .status
            .active_backend
            .clone();

        match self.try_render(&url, &active_backend, tab_id) {
            Ok(outcome) => {
                self.invalidate_tab_distillation(tab_id, &outcome.final_url);
                if let Some(tab) = self.tabs.get_mut(&tab_id) {
                    tab.url = Some(outcome.final_url.clone());
                    tab.status = outcome.status.clone();
                }
                self.sync_tab_navigation_state(tab_id);
                Ok(outcome.status)
            }
            Err(e) => {
                if let Some(tab) = self.tabs.get_mut(&tab_id) {
                    tab.status.firewall_status =
                        Some(format!("Primary {:?} render failed: {}", active_backend, e));
                }
                #[cfg(feature = "servo-backend")]
                if active_backend == EngineBackend::Servo {
                    let page = fetch_distilled_page(&url)?;
                    self.semantic_cache.insert(page.url.clone(), page.clone());
                    let status = reader_fallback_status(format!(
                        "Servo live navigation failed: {}. Showing distilled reader.",
                        e
                    ));
                    if let Some(tab) = self.tabs.get_mut(&tab_id) {
                        tab.url = Some(page.url.clone());
                        tab.distilled_page = Some(page);
                        tab.status = status.clone();
                    }
                    return Ok(status);
                }

                if self.fallback_enabled
                    && active_backend == EngineBackend::Servo
                    && !cfg!(feature = "servo-backend")
                {
                    println!(
                        "Servo rendering failed for {}: {}. Falling back to Gecko...",
                        url, e
                    );
                    active_backend = EngineBackend::Gecko;
                    let res = self.try_render(&url, &active_backend, tab_id);
                    if let Ok(outcome) = &res {
                        self.invalidate_tab_distillation(tab_id, &outcome.final_url);
                        if let Some(tab) = self.tabs.get_mut(&tab_id) {
                            tab.url = Some(outcome.final_url.clone());
                            tab.status = outcome.status.clone();
                        }
                        self.sync_tab_navigation_state(tab_id);
                    }
                    res.map(|outcome| outcome.status)
                } else if self.fallback_enabled && active_backend == EngineBackend::Gecko {
                    println!("Gecko rendering failed. Falling back to Chromium (Legacy Mode)...");
                    active_backend = EngineBackend::Chromium;
                    let res = self.try_render(&url, &active_backend, tab_id);
                    if let Ok(outcome) = &res {
                        self.invalidate_tab_distillation(tab_id, &outcome.final_url);
                        if let Some(tab) = self.tabs.get_mut(&tab_id) {
                            tab.url = Some(outcome.final_url.clone());
                            tab.status = outcome.status.clone();
                        }
                        self.sync_tab_navigation_state(tab_id);
                    }
                    res.map(|outcome| outcome.status)
                } else {
                    Err(format!("All engine backends failed for {}: {}", url, e))
                }
            }
        }
    }

    pub fn navigate_active_tab_async(
        &self,
        url: Url,
        persona_id: String,
        viewport_size: Option<(u32, u32)>,
    ) -> Result<mpsc::Receiver<Result<AsyncNavigationResult, String>>, String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        let backend = self
            .tabs
            .get(&tab_id)
            .ok_or("Active tab not found")?
            .status
            .active_backend
            .clone();
        let firewall = self.firewall.clone();
        let (result_tx, result_rx) = mpsc::channel();

        #[cfg(feature = "servo-backend")]
        {
            let servo_service = self.servo_service.clone();
            thread::Builder::new()
                .name("sextant-navigation".into())
                .spawn(move || {
                    let requested_url = url.clone();
                    let result = (|| {
                        let mut timings = AsyncNavigationTimings::default();
                        let firewall_started = Instant::now();
                        let (firewall_action, reason) = firewall.check_access(&persona_id, &url);
                        timings.firewall = Some(firewall_started.elapsed());
                        if firewall_action == FirewallAction::Block {
                            return Err(format!("Firewall Blocked: {} (Reason: {})", url, reason));
                        }

                        if backend != EngineBackend::Servo {
                            return Err(
                                "Async visible navigation is only wired for Servo tabs right now."
                                    .to_string(),
                            );
                        }

                        let servo_started = Instant::now();
                        match servo_service.navigate_with_viewport(
                            tab_id,
                            url.clone(),
                            viewport_size,
                        ) {
                            Ok(render) => {
                                timings.servo_navigation = Some(servo_started.elapsed());
                                let initial_frame =
                                    Some(servo_service.resize_and_capture_frame(tab_id, None));
                                let inspect_started = Instant::now();
                                let snapshot = servo_service.inspect_tab(tab_id).ok();
                                timings.servo_inspect = Some(inspect_started.elapsed());
                                Ok(AsyncNavigationResult {
                                    tab_id,
                                    requested_url,
                                    final_url: render.final_url,
                                    status: render.status,
                                    distilled_page: None,
                                    can_go_back: snapshot
                                        .as_ref()
                                        .map(|tab| tab.can_go_back)
                                        .unwrap_or(false),
                                    can_go_forward: snapshot
                                        .as_ref()
                                        .map(|tab| tab.can_go_forward)
                                        .unwrap_or(false),
                                    timings,
                                    initial_frame,
                                })
                            }
                            Err(error) => {
                                timings.servo_navigation = Some(servo_started.elapsed());
                                let reader_started = Instant::now();
                                let page = fetch_distilled_page(&url)?;
                                timings.reader_fallback = Some(reader_started.elapsed());
                                let status = reader_fallback_status(format!(
                                    "Servo live navigation failed: {}. Showing distilled reader.",
                                    error
                                ));
                                Ok(AsyncNavigationResult {
                                    tab_id,
                                    requested_url,
                                    final_url: page.url.clone(),
                                    status,
                                    distilled_page: Some(page),
                                    can_go_back: false,
                                    can_go_forward: false,
                                    timings,
                                    initial_frame: None,
                                })
                            }
                        }
                    })();
                    let _ = result_tx.send(result);
                })
                .map_err(|error| format!("failed to spawn Servo navigation worker: {error}"))?;
            Ok(result_rx)
        }

        #[cfg(not(feature = "servo-backend"))]
        {
            let _ = (tab_id, url, persona_id, viewport_size, backend, firewall);
            let _ = result_tx.send(Err(
                "Servo backend is not enabled in this build.".to_string()
            ));
            Ok(result_rx)
        }
    }

    pub fn reload_active_tab_async(
        &self,
    ) -> Result<mpsc::Receiver<Result<AsyncNavigationResult, String>>, String> {
        self.active_tab_control_async(AsyncNavigationControl::Reload)
    }

    pub fn go_back_active_tab_async(
        &self,
    ) -> Result<mpsc::Receiver<Result<AsyncNavigationResult, String>>, String> {
        self.active_tab_control_async(AsyncNavigationControl::Back)
    }

    pub fn go_forward_active_tab_async(
        &self,
    ) -> Result<mpsc::Receiver<Result<AsyncNavigationResult, String>>, String> {
        self.active_tab_control_async(AsyncNavigationControl::Forward)
    }

    fn active_tab_control_async(
        &self,
        control: AsyncNavigationControl,
    ) -> Result<mpsc::Receiver<Result<AsyncNavigationResult, String>>, String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        let tab = self.tabs.get(&tab_id).ok_or("Active tab not found")?;
        let backend = tab.status.active_backend.clone();
        let requested_url = tab
            .url
            .clone()
            .ok_or("Active tab has no URL for navigation control.")?;
        let (result_tx, result_rx) = mpsc::channel();

        #[cfg(feature = "servo-backend")]
        {
            let servo_service = self.servo_service.clone();
            thread::Builder::new()
                .name("sextant-navigation-control".into())
                .spawn(move || {
                    let result = (|| {
                        let mut timings = AsyncNavigationTimings::default();
                        if backend != EngineBackend::Servo {
                            return Err(
                                "Async visible navigation controls are only wired for Servo tabs right now."
                                    .to_string(),
                            );
                        }

                        let servo_started = Instant::now();
                        let render = match control {
                            AsyncNavigationControl::Reload => servo_service.reload(tab_id),
                            AsyncNavigationControl::Back => servo_service.go_back(tab_id),
                            AsyncNavigationControl::Forward => servo_service.go_forward(tab_id),
                        }?;
                        timings.servo_navigation = Some(servo_started.elapsed());
                        let initial_frame = Some(servo_service.resize_and_capture_frame(tab_id, None));
                        let inspect_started = Instant::now();
                        let snapshot = servo_service.inspect_tab(tab_id).ok();
                        timings.servo_inspect = Some(inspect_started.elapsed());
                        Ok(AsyncNavigationResult {
                            tab_id,
                            requested_url,
                            final_url: render.final_url,
                            status: render.status,
                            distilled_page: None,
                            can_go_back: snapshot
                                .as_ref()
                                .map(|tab| tab.can_go_back)
                                .unwrap_or(false),
                            can_go_forward: snapshot
                                .as_ref()
                                .map(|tab| tab.can_go_forward)
                                .unwrap_or(false),
                            timings,
                            initial_frame,
                        })
                    })();
                    let _ = result_tx.send(result);
                })
                .map_err(|error| {
                    format!("failed to spawn Servo navigation-control worker: {error}")
                })?;
            Ok(result_rx)
        }

        #[cfg(not(feature = "servo-backend"))]
        {
            let _ = (tab_id, requested_url, backend, control);
            let _ = result_tx.send(Err(
                "Servo backend is not enabled in this build.".to_string()
            ));
            Ok(result_rx)
        }
    }

    pub fn apply_async_navigation_result(&mut self, result: AsyncNavigationResult) {
        self.semantic_cache.remove(&result.requested_url);
        self.semantic_cache.remove(&result.final_url);
        if let Some(page) = result.distilled_page.as_ref() {
            self.semantic_cache.insert(page.url.clone(), page.clone());
        }
        if let Some(tab) = self.tabs.get_mut(&result.tab_id) {
            tab.url = Some(result.final_url.clone());
            tab.status = result.status.clone();
            tab.distilled_page = result.distilled_page;
            tab.can_go_back = result.can_go_back;
            tab.can_go_forward = result.can_go_forward;
        }
    }

    pub fn distill_active_tab_async(
        &self,
    ) -> Result<mpsc::Receiver<Result<AsyncDistillResult, String>>, String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        self.distill_tab_async(tab_id)
    }

    pub fn distill_tab_async(
        &self,
        tab_id: Uuid,
    ) -> Result<mpsc::Receiver<Result<AsyncDistillResult, String>>, String> {
        let tab = self.tabs.get(&tab_id).ok_or("Tab not found")?;
        let url = tab
            .url
            .clone()
            .unwrap_or_else(|| Url::parse("about:blank").unwrap());
        let backend = tab.status.active_backend.clone();
        let cached = tab
            .distilled_page
            .as_ref()
            .filter(|page| page.url == url)
            .cloned()
            .or_else(|| self.semantic_cache.get(&url).cloned());
        let (result_tx, result_rx) = mpsc::channel();

        if let Some(page) = cached {
            let _ = result_tx.send(Ok(AsyncDistillResult {
                tab_id,
                page,
                status_note: None,
            }));
            return Ok(result_rx);
        }

        #[cfg(feature = "servo-backend")]
        {
            let servo_service = self.servo_service.clone();
            thread::Builder::new()
                .name("sextant-distill".into())
                .spawn(move || {
                    let result = (|| {
                        let page = if backend == EngineBackend::Servo {
                            match servo_service.distill_tab(tab_id) {
                                Ok(snapshot) if snapshot.page.url.scheme() != "about" => {
                                    let live_page = snapshot.page;
                                    if should_try_reader_quality_fallback(&live_page) {
                                        match fetch_distilled_page(&live_page.url) {
                                            Ok(mut fallback)
                                                if semantic_signal_count(&fallback)
                                                    > semantic_signal_count(&live_page) =>
                                            {
                                                fallback.metadata.insert(
                                                    "distillation_backend".to_string(),
                                                    "reader-fallback-after-weak-live-dom"
                                                        .to_string(),
                                                );
                                                fallback.metadata.insert(
                                                    "live_dom_signal_count".to_string(),
                                                    semantic_signal_count(&live_page).to_string(),
                                                );
                                                fallback
                                            }
                                            _ => live_page,
                                        }
                                    } else {
                                        live_page
                                    }
                                }
                                Ok(snapshot) => fetch_distilled_page(&snapshot.page.url)?,
                                Err(error) => {
                                    let mut page =
                                        fetch_distilled_page(&url).map_err(|fallback| {
                                            format!(
                                                "Servo live DOM distillation failed: {}; reader fallback also failed: {}",
                                                error, fallback
                                            )
                                        })?;
                                    page.metadata.insert(
                                        "distillation_backend".to_string(),
                                        "reader-fallback-after-servo-error".to_string(),
                                    );
                                    return Ok(AsyncDistillResult {
                                        tab_id,
                                        page,
                                        status_note: Some(format!(
                                            "Servo live DOM distillation failed: {}; used reader fallback.",
                                            error
                                        )),
                                    });
                                }
                            }
                        } else {
                            fetch_distilled_page(&url)?
                        };
                        Ok(AsyncDistillResult {
                            tab_id,
                            page,
                            status_note: None,
                        })
                    })();
                    let _ = result_tx.send(result);
                })
                .map_err(|error| format!("failed to spawn Servo distill worker: {error}"))?;
            Ok(result_rx)
        }

        #[cfg(not(feature = "servo-backend"))]
        {
            let page = fetch_distilled_page(&url)?;
            let _ = (backend, tab_id);
            let _ = result_tx.send(Ok(AsyncDistillResult {
                tab_id,
                page,
                status_note: None,
            }));
            Ok(result_rx)
        }
    }

    pub fn apply_async_distill_result(&mut self, result: AsyncDistillResult) {
        self.semantic_cache
            .insert(result.page.url.clone(), result.page.clone());
        if let Some(tab) = self.tabs.get_mut(&result.tab_id) {
            tab.url = Some(result.page.url.clone());
            tab.distilled_page = Some(result.page);
            if let Some(status_note) = result.status_note {
                tab.status.firewall_status = Some(status_note);
            }
        }
    }

    fn try_render(
        &self,
        url: &Url,
        backend: &EngineBackend,
        _tab_id: Uuid,
    ) -> Result<RenderOutcome, String> {
        // In a native build, this would interface with the respective engine's FFI/IPC
        // and spawn a new sandboxed process.
        let pid = rand::random::<u32>() % 10000 + 1000;

        match backend {
            EngineBackend::Servo => {
                #[cfg(feature = "servo-backend")]
                {
                    let render = self.servo_service.navigate(_tab_id, url.clone())?;
                    return Ok(RenderOutcome {
                        final_url: render.final_url,
                        status: EngineStatus {
                            gpu_accelerated: self.renderer.is_some(),
                            layout_time_ms: render.status.layout_time_ms,
                            memory_usage_mb: render.status.memory_usage_mb,
                            sandbox_profile: render.status.sandbox_profile,
                            firewall_status: render.status.firewall_status,
                            active_backend: render.status.active_backend,
                            is_sandboxed: render.status.is_sandboxed,
                            parallel_threads: render.status.parallel_threads,
                        },
                    });
                }

                #[cfg(not(feature = "servo-backend"))]
                {
                    // Simulate Servo's strict standards check
                    if url.domain() == Some("legacy-site.com") {
                        return Err("Servo: Unsupported legacy CSS/JS features detected.".into());
                    }

                    // If GPU is available, simulate a frame render
                    if let Some(renderer) = &self.renderer {
                        renderer.render_frame()?;
                    }

                    // Simulate Servo's parallel layout
                    let fragments = vec![
                        LayoutFragment {
                            id: 1,
                            selector: "header".into(),
                            depth: 1,
                        },
                        LayoutFragment {
                            id: 2,
                            selector: "main".into(),
                            depth: 1,
                        },
                        LayoutFragment {
                            id: 3,
                            selector: "footer".into(),
                            depth: 1,
                        },
                        LayoutFragment {
                            id: 4,
                            selector: "sidebar".into(),
                            depth: 2,
                        },
                    ];
                    let layout_results = self.layout_engine.layout_parallel(fragments);
                    let total_layout_time: f64 =
                        layout_results.iter().map(|r| r.layout_time_ms).sum();

                    Ok(RenderOutcome {
                        final_url: url.clone(),
                        status: EngineStatus {
                            active_backend: EngineBackend::Servo,
                            is_sandboxed: true,
                            memory_usage_mb: 120,
                            gpu_accelerated: self.renderer.is_some(),
                            sandbox_profile: Some(SandboxProfile {
                                pid,
                                restricted_syscalls: vec![
                                    "write".into(),
                                    "open".into(),
                                    "exec".into(),
                                ],
                                memory_limit_mb: 256,
                                network_access: true,
                            }),
                            firewall_status: Some("Allowed by Servo Policy".into()),
                            layout_time_ms: total_layout_time,
                            parallel_threads: 8,
                        },
                    })
                }
            }
            EngineBackend::Gecko => Ok(RenderOutcome {
                final_url: url.clone(),
                status: EngineStatus {
                    active_backend: EngineBackend::Gecko,
                    is_sandboxed: true,
                    memory_usage_mb: 450,
                    gpu_accelerated: false,
                    sandbox_profile: Some(SandboxProfile {
                        pid,
                        restricted_syscalls: vec!["exec".into()],
                        memory_limit_mb: 1024,
                        network_access: true,
                    }),
                    firewall_status: Some("Allowed by Gecko Policy".into()),
                    layout_time_ms: 12.5,
                    parallel_threads: 1,
                },
            }),
            EngineBackend::Chromium => Ok(RenderOutcome {
                final_url: url.clone(),
                status: EngineStatus {
                    active_backend: EngineBackend::Chromium,
                    is_sandboxed: true,
                    memory_usage_mb: 890,
                    gpu_accelerated: false,
                    sandbox_profile: Some(SandboxProfile {
                        pid,
                        restricted_syscalls: vec![],
                        memory_limit_mb: 2048,
                        network_access: true,
                    }),
                    firewall_status: Some("Allowed by Legacy Policy".into()),
                    layout_time_ms: 25.0,
                    parallel_threads: 1,
                },
            }),
        }
    }

    /// Distills the current page into a semantic map for the Pilot.
    pub fn distill_current_page(&mut self) -> Result<DistilledPage, String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        self.distill_tab(tab_id)
    }

    pub fn capture_current_frame(&mut self) -> Result<RenderedFrame, String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        self.capture_tab_frame(tab_id)
    }

    pub fn capture_current_frame_async(
        &self,
    ) -> Result<mpsc::Receiver<Result<RenderedFrame, String>>, String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        self.capture_tab_frame_async(tab_id)
    }

    pub fn capture_tab_frame_async(
        &self,
        tab_id: Uuid,
    ) -> Result<mpsc::Receiver<Result<RenderedFrame, String>>, String> {
        let (result_tx, result_rx) = mpsc::channel();

        #[cfg(feature = "servo-backend")]
        {
            let servo_service = self.servo_service.clone();
            thread::Builder::new()
                .name("sextant-frame-capture".into())
                .spawn(move || {
                    let result = servo_service
                        .capture_frame(tab_id)
                        .map(|snapshot| snapshot.frame);
                    let _ = result_tx.send(result);
                })
                .map_err(|error| format!("failed to spawn Servo frame capture worker: {error}"))?;
            Ok(result_rx)
        }

        #[cfg(not(feature = "servo-backend"))]
        {
            let _ = tab_id;
            let _ = result_tx.send(Err(
                "Servo backend is not enabled in this build.".to_string()
            ));
            Ok(result_rx)
        }
    }

    pub fn capture_tab_frame_with_resize_async(
        &self,
        tab_id: Uuid,
        viewport_size: Option<(u32, u32)>,
    ) -> Result<mpsc::Receiver<Result<AsyncFrameCapture, String>>, String> {
        let (result_tx, result_rx) = mpsc::channel();

        #[cfg(feature = "servo-backend")]
        {
            let servo_service = self.servo_service.clone();
            thread::Builder::new()
                .name("sextant-frame-resize-capture".into())
                .spawn(move || {
                    let result = servo_service.resize_and_capture_frame(tab_id, viewport_size);
                    let _ = result_tx.send(result);
                })
                .map_err(|error| {
                    format!("failed to spawn Servo frame resize/capture worker: {error}")
                })?;
            Ok(result_rx)
        }

        #[cfg(not(feature = "servo-backend"))]
        {
            let _ = (tab_id, viewport_size);
            let _ = result_tx.send(Err(
                "Servo backend is not enabled in this build.".to_string()
            ));
            Ok(result_rx)
        }
    }

    pub fn resize_current_viewport(&mut self, width: u32, height: u32) -> Result<(), String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        self.resize_tab_viewport(tab_id, width, height)
    }

    pub fn resize_tab_viewport(
        &mut self,
        tab_id: Uuid,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        #[cfg(feature = "servo-backend")]
        {
            self.servo_service.resize_tab(tab_id, width, height)
        }

        #[cfg(not(feature = "servo-backend"))]
        {
            let _ = (tab_id, width, height);
            Ok(())
        }
    }

    pub fn wheel_current_viewport(
        &mut self,
        delta_x: f64,
        delta_y: f64,
        pixel_mode: bool,
    ) -> Result<(), String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        #[cfg(feature = "servo-backend")]
        {
            self.servo_service
                .wheel(tab_id, delta_x, delta_y, pixel_mode)
        }

        #[cfg(not(feature = "servo-backend"))]
        {
            let _ = (tab_id, delta_x, delta_y, pixel_mode);
            Err("Servo backend is not enabled in this build.".to_string())
        }
    }

    pub fn enqueue_wheel_current_viewport(
        &mut self,
        delta_x: f64,
        delta_y: f64,
        pixel_mode: bool,
    ) -> Result<(), String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        #[cfg(feature = "servo-backend")]
        {
            self.servo_service
                .enqueue_wheel(tab_id, delta_x, delta_y, pixel_mode);
            Ok(())
        }

        #[cfg(not(feature = "servo-backend"))]
        {
            let _ = (tab_id, delta_x, delta_y, pixel_mode);
            Err("Servo backend is not enabled in this build.".to_string())
        }
    }

    pub fn mouse_move_current_viewport(&mut self, x: f32, y: f32) -> Result<(), String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        #[cfg(feature = "servo-backend")]
        {
            self.servo_service.mouse_move(tab_id, x, y)
        }

        #[cfg(not(feature = "servo-backend"))]
        {
            let _ = (tab_id, x, y);
            Err("Servo backend is not enabled in this build.".to_string())
        }
    }

    pub fn enqueue_mouse_move_current_viewport(&mut self, x: f32, y: f32) -> Result<(), String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        #[cfg(feature = "servo-backend")]
        {
            self.servo_service.enqueue_mouse_move(tab_id, x, y);
            Ok(())
        }

        #[cfg(not(feature = "servo-backend"))]
        {
            let _ = (tab_id, x, y);
            Err("Servo backend is not enabled in this build.".to_string())
        }
    }

    pub fn mouse_button_current_viewport(
        &mut self,
        x: f32,
        y: f32,
        pressed: bool,
    ) -> Result<(), String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        #[cfg(feature = "servo-backend")]
        {
            self.servo_service.mouse_button(tab_id, x, y, pressed)
        }

        #[cfg(not(feature = "servo-backend"))]
        {
            let _ = (tab_id, x, y, pressed);
            Err("Servo backend is not enabled in this build.".to_string())
        }
    }

    pub fn enqueue_mouse_button_current_viewport(
        &mut self,
        x: f32,
        y: f32,
        pressed: bool,
    ) -> Result<(), String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        #[cfg(feature = "servo-backend")]
        {
            self.servo_service
                .enqueue_mouse_button(tab_id, x, y, pressed);
            Ok(())
        }

        #[cfg(not(feature = "servo-backend"))]
        {
            let _ = (tab_id, x, y, pressed);
            Err("Servo backend is not enabled in this build.".to_string())
        }
    }

    pub fn key_character_current_viewport(
        &mut self,
        text: String,
        pressed: bool,
    ) -> Result<(), String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        #[cfg(feature = "servo-backend")]
        {
            self.servo_service.key_character(tab_id, text, pressed)
        }

        #[cfg(not(feature = "servo-backend"))]
        {
            let _ = (tab_id, text, pressed);
            Err("Servo backend is not enabled in this build.".to_string())
        }
    }

    pub fn enqueue_key_character_current_viewport(
        &mut self,
        text: String,
        pressed: bool,
    ) -> Result<(), String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        #[cfg(feature = "servo-backend")]
        {
            self.servo_service
                .enqueue_key_character(tab_id, text, pressed);
            Ok(())
        }

        #[cfg(not(feature = "servo-backend"))]
        {
            let _ = (tab_id, text, pressed);
            Err("Servo backend is not enabled in this build.".to_string())
        }
    }

    pub fn key_named_current_viewport(
        &mut self,
        key: BrowserKey,
        pressed: bool,
    ) -> Result<(), String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        #[cfg(feature = "servo-backend")]
        {
            self.servo_service
                .key_named(tab_id, key.into_servo_key(), pressed)
        }

        #[cfg(not(feature = "servo-backend"))]
        {
            let _ = (tab_id, key, pressed);
            Err("Servo backend is not enabled in this build.".to_string())
        }
    }

    pub fn enqueue_key_named_current_viewport(
        &mut self,
        key: BrowserKey,
        pressed: bool,
    ) -> Result<(), String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        #[cfg(feature = "servo-backend")]
        {
            self.servo_service
                .enqueue_key_named(tab_id, key.into_servo_key(), pressed);
            Ok(())
        }

        #[cfg(not(feature = "servo-backend"))]
        {
            let _ = (tab_id, key, pressed);
            Err("Servo backend is not enabled in this build.".to_string())
        }
    }

    pub fn interact_current_page(
        &mut self,
        interaction: BrowserInteraction,
    ) -> Result<BrowserInteractionResult, String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        self.interact_tab(tab_id, interaction)
    }

    pub fn click_selector_current_page(
        &mut self,
        selector: impl Into<String>,
    ) -> Result<BrowserInteractionResult, String> {
        self.interact_current_page(BrowserInteraction::ClickSelector {
            selector: selector.into(),
        })
    }

    pub fn fill_selector_current_page(
        &mut self,
        selector: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<BrowserInteractionResult, String> {
        self.interact_current_page(BrowserInteraction::FillSelector {
            selector: selector.into(),
            value: value.into(),
        })
    }

    pub fn submit_selector_current_page(
        &mut self,
        selector: impl Into<String>,
    ) -> Result<BrowserInteractionResult, String> {
        self.interact_current_page(BrowserInteraction::SubmitSelector {
            selector: selector.into(),
        })
    }

    pub fn eval_probe_tab(&mut self, tab_id: Uuid) -> Result<BrowserEvalProbe, String> {
        #[cfg(feature = "servo-backend")]
        {
            self.servo_service.eval_probe_tab(tab_id)
        }

        #[cfg(not(feature = "servo-backend"))]
        {
            let _ = tab_id;
            Err("Servo backend is not enabled in this build.".to_string())
        }
    }

    pub fn interact_tab(
        &mut self,
        tab_id: Uuid,
        interaction: BrowserInteraction,
    ) -> Result<BrowserInteractionResult, String> {
        #[cfg(feature = "servo-backend")]
        {
            let result = self.servo_service.interact(tab_id, interaction)?;
            if let Some(tab) = self.tabs.get(&tab_id) {
                if let Some(url) = tab.url.as_ref() {
                    self.semantic_cache.remove(url);
                }
            }
            if let Some(tab) = self.tabs.get_mut(&tab_id) {
                if let Some(current_url) = result.current_url.clone() {
                    tab.url = Some(current_url);
                }
                tab.distilled_page = None;
            }
            self.sync_tab_navigation_state(tab_id);
            Ok(result)
        }

        #[cfg(not(feature = "servo-backend"))]
        {
            let _ = (tab_id, interaction);
            Err("Servo backend is not enabled in this build.".to_string())
        }
    }

    pub fn capture_tab_frame(&mut self, tab_id: Uuid) -> Result<RenderedFrame, String> {
        #[cfg(feature = "servo-backend")]
        {
            let frame = self.servo_service.capture_frame(tab_id)?.frame;
            return Ok(frame);
        }

        #[cfg(not(feature = "servo-backend"))]
        {
            let _ = tab_id;
            Err("Servo backend is not enabled in this build.".to_string())
        }
    }

    pub fn distill_tab(&mut self, tab_id: Uuid) -> Result<DistilledPage, String> {
        #[cfg(feature = "servo-backend")]
        if let Some(tab) = self.tabs.get(&tab_id) {
            if tab.status.active_backend == EngineBackend::Servo {
                let Some(current_url) = tab.url.clone() else {
                    return fetch_distilled_page(&Url::parse("about:blank").unwrap());
                };
                if let Some(cached) = tab
                    .distilled_page
                    .as_ref()
                    .filter(|page| page.url == current_url)
                    .cloned()
                    .or_else(|| self.semantic_cache.get(&current_url).cloned())
                {
                    if let Some(tab) = self.tabs.get_mut(&tab_id) {
                        tab.distilled_page = Some(cached.clone());
                    }
                    return Ok(cached);
                }
                let page = match self.servo_service.distill_tab(tab_id) {
                    Ok(snapshot) if snapshot.page.url.scheme() != "about" => {
                        let live_page = snapshot.page;
                        if should_try_reader_quality_fallback(&live_page) {
                            match fetch_distilled_page(&live_page.url) {
                                Ok(mut fallback)
                                    if semantic_signal_count(&fallback)
                                        > semantic_signal_count(&live_page) =>
                                {
                                    fallback.metadata.insert(
                                        "distillation_backend".to_string(),
                                        "reader-fallback-after-weak-live-dom".to_string(),
                                    );
                                    fallback.metadata.insert(
                                        "live_dom_signal_count".to_string(),
                                        semantic_signal_count(&live_page).to_string(),
                                    );
                                    fallback
                                }
                                _ => live_page,
                            }
                        } else {
                            live_page
                        }
                    }
                    Ok(snapshot) => fetch_distilled_page(&snapshot.page.url)?,
                    Err(error) => {
                        let mut page = fetch_distilled_page(&current_url).map_err(|fallback| {
                            format!(
                                "Servo live DOM distillation failed: {}; reader fallback also failed: {}",
                                error, fallback
                            )
                        })?;
                        page.metadata.insert(
                            "distillation_backend".to_string(),
                            "reader-fallback-after-servo-error".to_string(),
                        );
                        if let Some(tab) = self.tabs.get_mut(&tab_id) {
                            tab.status.firewall_status = Some(format!(
                                "Servo live DOM distillation failed: {}; used reader fallback.",
                                error
                            ));
                        }
                        page
                    }
                };
                self.semantic_cache.insert(page.url.clone(), page.clone());
                if let Some(tab) = self.tabs.get_mut(&tab_id) {
                    tab.distilled_page = Some(page.clone());
                    tab.url = Some(page.url.clone());
                }
                return Ok(page);
            }
        }

        let url = self
            .tabs
            .get(&tab_id)
            .and_then(|t| t.url.clone())
            .unwrap_or_else(|| Url::parse("about:blank").unwrap());

        // Optimization: Check semantic cache first
        if let Some(cached) = self.semantic_cache.get(&url) {
            if let Some(tab) = self.tabs.get_mut(&tab_id) {
                tab.distilled_page = Some(cached.clone());
            }
            return Ok(cached.clone());
        }

        let page = fetch_distilled_page(&url)?;

        // Update cache
        self.semantic_cache.insert(page.url.clone(), page.clone());

        if let Some(tab) = self.tabs.get_mut(&tab_id) {
            tab.distilled_page = Some(page.clone());
            tab.url = Some(page.url.clone());
        }

        Ok(page)
    }

    pub fn reload_active_tab(&mut self) -> Result<EngineStatus, String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        let backend = self
            .tabs
            .get(&tab_id)
            .map(|tab| tab.status.active_backend.clone())
            .ok_or("Tab not found")?;

        match backend {
            EngineBackend::Servo => {
                #[cfg(feature = "servo-backend")]
                {
                    let render = self.servo_service.reload(tab_id)?;
                    self.invalidate_tab_distillation(tab_id, &render.final_url);
                    if let Some(tab) = self.tabs.get_mut(&tab_id) {
                        tab.url = Some(render.final_url.clone());
                        tab.status = render.status.clone();
                    }
                    self.sync_tab_navigation_state(tab_id);
                    return Ok(render.status);
                }
                #[cfg(not(feature = "servo-backend"))]
                {
                    return Err("Servo backend is not enabled in this build.".to_string());
                }
            }
            _ => {
                let url = self
                    .tabs
                    .get(&tab_id)
                    .and_then(|tab| tab.url.clone())
                    .ok_or("Active tab has no URL to reload.")?;
                self.navigate_with_fallback(url, "default")
            }
        }
    }

    pub fn go_back_active_tab(&mut self) -> Result<EngineStatus, String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        let backend = self
            .tabs
            .get(&tab_id)
            .map(|tab| tab.status.active_backend.clone())
            .ok_or("Tab not found")?;

        match backend {
            EngineBackend::Servo => {
                #[cfg(feature = "servo-backend")]
                {
                    let render = self.servo_service.go_back(tab_id)?;
                    self.invalidate_tab_distillation(tab_id, &render.final_url);
                    if let Some(tab) = self.tabs.get_mut(&tab_id) {
                        tab.url = Some(render.final_url.clone());
                        tab.status = render.status.clone();
                    }
                    self.sync_tab_navigation_state(tab_id);
                    return Ok(render.status);
                }
                #[cfg(not(feature = "servo-backend"))]
                {
                    return Err("Servo backend is not enabled in this build.".to_string());
                }
            }
            _ => Err("Back navigation is only wired for the Servo backend right now.".to_string()),
        }
    }

    pub fn go_forward_active_tab(&mut self) -> Result<EngineStatus, String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        let backend = self
            .tabs
            .get(&tab_id)
            .map(|tab| tab.status.active_backend.clone())
            .ok_or("Tab not found")?;

        match backend {
            EngineBackend::Servo => {
                #[cfg(feature = "servo-backend")]
                {
                    let render = self.servo_service.go_forward(tab_id)?;
                    self.invalidate_tab_distillation(tab_id, &render.final_url);
                    if let Some(tab) = self.tabs.get_mut(&tab_id) {
                        tab.url = Some(render.final_url.clone());
                        tab.status = render.status.clone();
                    }
                    self.sync_tab_navigation_state(tab_id);
                    return Ok(render.status);
                }
                #[cfg(not(feature = "servo-backend"))]
                {
                    return Err("Servo backend is not enabled in this build.".to_string());
                }
            }
            _ => {
                Err("Forward navigation is only wired for the Servo backend right now.".to_string())
            }
        }
    }

    /// Performs semantic perception across all open tabs in parallel.
    /// This leverages Servo's parallelized architecture for high-performance distillation.
    pub fn perceive_all_tabs(&mut self) -> Vec<DistilledPage> {
        let tab_data: Vec<(Uuid, Url)> = self
            .tabs
            .iter()
            .map(|(id, t)| {
                (
                    *id,
                    t.url
                        .clone()
                        .unwrap_or_else(|| Url::parse("about:blank").unwrap()),
                )
            })
            .collect();

        let distilled_tabs: Vec<(Uuid, DistilledPage)> = tab_data
            .par_iter()
            .filter_map(|(id, url)| fetch_distilled_page(url).ok().map(|page| (*id, page)))
            .collect();

        for (tab_id, page) in &distilled_tabs {
            self.semantic_cache.insert(page.url.clone(), page.clone());
            if let Some(tab) = self.tabs.get_mut(tab_id) {
                tab.distilled_page = Some(page.clone());
                tab.url = Some(page.url.clone());
            }
        }

        distilled_tabs
            .into_iter()
            .map(|(_tab_id, page)| page)
            .collect()
    }

    pub fn switch_engine(&mut self, backend: EngineBackend) {
        if let Some(tab_id) = self.active_tab_id {
            if let Some(tab) = self.tabs.get_mut(&tab_id) {
                tab.status.active_backend = backend;
            }
        }
    }

    pub fn set_privacy_level(&self, level: sextant_privacy::PrivacyLevel) {
        self.bridge.set_privacy_level(level);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perceive_all_tabs_updates_tab_state() {
        let mut engine = SextantEngine::new();
        engine.open_tab();

        let pages = engine.perceive_all_tabs();

        assert_eq!(pages.len(), engine.get_tabs().len());
        let active_tab = engine.get_active_tab().expect("active tab should exist");
        let page = active_tab
            .distilled_page
            .as_ref()
            .expect("perception should update the active tab");
        assert_eq!(page.title, "Blank Page");
        assert_eq!(active_tab.url.as_ref(), Some(&page.url));
    }

    #[test]
    fn tab_order_stays_stable_when_switching() {
        let mut engine = SextantEngine::new();
        let first = engine.get_active_tab().expect("initial tab").id;
        let second = engine.open_tab();
        let third = engine.open_tab();
        let original_order: Vec<Uuid> = engine.get_tabs().iter().map(|tab| tab.id).collect();

        engine
            .switch_to_tab(second)
            .expect("second tab should be selectable");

        let current_order: Vec<Uuid> = engine.get_tabs().iter().map(|tab| tab.id).collect();
        assert_eq!(original_order, current_order);
        assert_eq!(current_order, vec![first, second, third]);
    }

    #[test]
    fn closing_active_tab_selects_adjacent_ordered_tab() {
        let mut engine = SextantEngine::new();
        let first = engine.get_active_tab().expect("initial tab").id;
        let second = engine.open_tab();
        let third = engine.open_tab();

        engine
            .switch_to_tab(second)
            .expect("second tab should be selectable");
        engine
            .close_tab(&second)
            .expect("active tab should be closable");

        assert_eq!(engine.get_active_tab().map(|tab| tab.id), Some(third));
        let current_order: Vec<Uuid> = engine.get_tabs().iter().map(|tab| tab.id).collect();
        assert_eq!(current_order, vec![first, third]);
    }

    #[test]
    fn perceive_all_tabs_keeps_results_attached_to_source_tabs() {
        let mut engine = SextantEngine::new();
        let good_tab = engine.open_tab();
        let good_url =
            Url::parse("data:text/html,<!DOCTYPE html><title>Good Tab</title><main>ok</main>")
                .unwrap();
        engine
            .navigate_with_fallback(good_url.clone(), "default")
            .expect("good tab should navigate");

        let bad_tab = engine.open_tab();
        let bad_url = Url::parse("file:///definitely/missing/sextant-test-page.html").unwrap();
        if let Some(tab) = engine.tabs.get_mut(&bad_tab) {
            tab.url = Some(bad_url);
        }

        let pages = engine.perceive_all_tabs();

        assert!(pages.iter().any(|page| page.title == "Good Tab"));
        let good_tab = engine.tabs.get(&good_tab).expect("good tab should exist");
        assert_eq!(good_tab.url.as_ref(), Some(&good_url));
        assert_eq!(
            good_tab
                .distilled_page
                .as_ref()
                .map(|page| page.title.as_str()),
            Some("Good Tab")
        );
        let bad_tab = engine.tabs.get(&bad_tab).expect("bad tab should exist");
        assert!(
            bad_tab.distilled_page.is_none(),
            "failed distillation should not receive another tab's page"
        );
    }

    #[cfg(feature = "servo-backend")]
    #[test]
    fn servo_navigation_updates_active_tab_state() {
        let mut engine = SextantEngine::new();
        let url = Url::parse("data:text/html,<!DOCTYPE html><title>Servo Test</title><p>Hello</p>")
            .expect("test URL should parse");

        let status = engine
            .navigate_with_fallback(url.clone(), "captain")
            .expect("Servo navigation should succeed");

        let active_tab = engine.get_active_tab().expect("active tab should exist");
        assert_eq!(status.active_backend, EngineBackend::Servo);
        assert_eq!(active_tab.status.active_backend, EngineBackend::Servo);
        assert_eq!(active_tab.url.as_ref(), Some(&url));
    }

    #[cfg(feature = "servo-backend")]
    #[test]
    fn servo_navigation_url_wait_accepts_redirects_without_accepting_stale_url() {
        let previous = Url::parse("https://old.example/").unwrap();
        let requested = Url::parse("https://www.rust-lang.org/").unwrap();
        let redirected = Url::parse("https://rust-lang.org/").unwrap();

        assert!(servo_runtime::navigation_url_ready(
            Some(&previous),
            &requested,
            Some(&requested)
        ));
        assert!(servo_runtime::navigation_url_ready(
            Some(&previous),
            &requested,
            Some(&redirected)
        ));
        assert!(!servo_runtime::navigation_url_ready(
            Some(&previous),
            &requested,
            Some(&previous)
        ));
        assert!(!servo_runtime::navigation_url_ready(
            Some(&previous),
            &requested,
            None
        ));
    }

    #[cfg(feature = "servo-backend")]
    #[test]
    fn servo_navigation_preserves_history_within_a_tab() {
        let mut engine = SextantEngine::new();
        let first = Url::parse("data:text/html,<!DOCTYPE html>page one").unwrap();
        let second = Url::parse("data:text/html,<!DOCTYPE html>page two").unwrap();

        engine
            .navigate_with_fallback(first.clone(), "captain")
            .expect("first Servo navigation should succeed");
        engine
            .navigate_with_fallback(second.clone(), "captain")
            .expect("second Servo navigation should succeed");

        let tab_id = engine.get_active_tab().expect("active tab should exist").id;
        #[cfg(feature = "servo-backend")]
        {
            let session = engine
                .servo_service
                .inspect_tab(tab_id)
                .expect("Servo session should exist for active tab");
            assert_eq!(session.current_url.as_ref(), Some(&second));
            assert!(session.can_go_back);
            assert!(!session.can_go_forward);
        }
    }

    #[cfg(feature = "servo-backend")]
    #[test]
    fn servo_back_and_forward_update_active_tab_url() {
        let mut engine = SextantEngine::new();
        let first = Url::parse("data:text/html,<!DOCTYPE html>page one").unwrap();
        let second = Url::parse("data:text/html,<!DOCTYPE html>page two").unwrap();

        engine
            .navigate_with_fallback(first.clone(), "captain")
            .expect("first Servo navigation should succeed");
        engine
            .navigate_with_fallback(second.clone(), "captain")
            .expect("second Servo navigation should succeed");

        engine
            .go_back_active_tab()
            .expect("back navigation should succeed");
        assert_eq!(
            engine.get_active_tab().and_then(|tab| tab.url.clone()),
            Some(first.clone())
        );

        engine
            .go_forward_active_tab()
            .expect("forward navigation should succeed");
        assert_eq!(
            engine.get_active_tab().and_then(|tab| tab.url.clone()),
            Some(second)
        );
    }

    #[cfg(feature = "servo-backend")]
    #[test]
    fn distill_tab_supports_data_urls() {
        let mut engine = SextantEngine::new();
        let tab_id = engine.open_tab();
        let url = Url::parse(
            "data:text/html,<!DOCTYPE html><title>Data Title</title><main>Hello%20Servo</main>",
        )
        .expect("data URL should parse");

        engine
            .navigate_with_fallback(url.clone(), "default")
            .expect("Servo navigation should succeed");
        let page = engine
            .distill_tab(tab_id)
            .expect("distillation should succeed");

        assert_eq!(page.url, url);
        assert_eq!(page.title, "Data Title");
        assert!(page.content.contains("Hello Servo"));
    }

    #[cfg(feature = "servo-backend")]
    #[test]
    fn live_dom_distillation_exposes_text_nodes() {
        let mut engine = SextantEngine::new();
        let url = Url::parse(
            "data:text/html,<!DOCTYPE html><title>Text Nodes</title><main><p>First useful paragraph.</p><ul><li>Second useful item.</li></ul></main>",
        )
        .expect("data URL should parse");

        engine
            .navigate_with_fallback(url, "default")
            .expect("Servo navigation should succeed");
        let page = engine
            .distill_current_page()
            .expect("distillation should succeed");

        let text_nodes = page
            .semantic_map
            .iter()
            .filter(|node| matches!(node.node_type, NodeType::Text))
            .map(|node| node.text.as_str())
            .collect::<Vec<_>>();
        assert!(
            text_nodes.contains(&"First useful paragraph.")
                && text_nodes.contains(&"Second useful item."),
            "semantic map should expose useful text nodes, got {:?}",
            text_nodes
        );
    }

    #[cfg(feature = "servo-backend")]
    #[test]
    fn navigation_invalidates_cached_distilled_page() {
        let mut engine = SextantEngine::new();
        let first =
            Url::parse("data:text/html,<!DOCTYPE html><title>One</title><main>first</main>")
                .unwrap();
        let second =
            Url::parse("data:text/html,<!DOCTYPE html><title>Two</title><main>second</main>")
                .unwrap();

        engine
            .navigate_with_fallback(first, "default")
            .expect("first Servo navigation should succeed");
        let first_page = engine
            .distill_current_page()
            .expect("first distillation should succeed");
        assert_eq!(first_page.title, "One");

        engine
            .navigate_with_fallback(second.clone(), "default")
            .expect("second Servo navigation should succeed");
        let active_tab = engine.get_active_tab().expect("active tab should exist");
        assert!(
            active_tab.distilled_page.is_none(),
            "navigation should invalidate stale distilled content"
        );

        let refreshed_page = engine
            .distill_current_page()
            .expect("second distillation should succeed");
        assert_eq!(refreshed_page.url, second);
        assert_eq!(refreshed_page.title, "Two");
        assert!(refreshed_page.content.contains("second"));
    }

    #[cfg(feature = "servo-backend")]
    #[test]
    fn distill_tab_reads_live_dom_after_inline_script_mutation() {
        let mut engine = SextantEngine::new();
        let url = Url::parse(
            "data:text/html,\
<!DOCTYPE html><title>Before</title><main id='target'>before</main>\
<script>document.title='After';document.getElementById('target').textContent='after';</script>",
        )
        .expect("mutating data URL should parse");

        engine
            .navigate_with_fallback(url, "default")
            .expect("Servo navigation should succeed");
        let page = engine
            .distill_current_page()
            .expect("live DOM distillation should succeed");

        assert_eq!(page.title, "After");
        assert!(page.content.contains("after"));
        assert!(
            !page.content.contains("before"),
            "distillation should reflect the mutated DOM, not the original source"
        );
    }

    #[cfg(feature = "servo-backend")]
    #[test]
    fn native_browser_interaction_can_fill_and_click_live_dom() {
        let mut engine = SextantEngine::new();
        let url = Url::parse(
            "data:text/html,\
<!DOCTYPE html><title>Native Interaction</title><main>\
<input id='q' value='empty'>\
<button id='go' onclick=\"document.getElementById('out').textContent=document.getElementById('q').value\">Go</button>\
<p id='out'>waiting</p>\
</main>",
        )
        .expect("interaction data URL should parse");

        engine
            .navigate_with_fallback(url, "default")
            .expect("Servo navigation should succeed");

        let fill = engine
            .fill_selector_current_page("#q", "native interaction filled")
            .expect("fill interaction should succeed");
        assert!(fill.ok);
        assert_eq!(fill.tag, "input");
        assert_eq!(fill.value, "native interaction filled");

        let click = engine
            .click_selector_current_page("#go")
            .expect("click interaction should succeed");
        assert!(click.ok);
        assert_eq!(click.tag, "button");

        let page = engine
            .distill_current_page()
            .expect("distillation should read the interacted DOM");
        assert!(
            page.content.contains("native interaction filled"),
            "distilled content should reflect native browser interaction, got '{}'",
            page.content
        );
        assert!(
            page.semantic_map.iter().any(|node| {
                matches!(node.node_type, NodeType::Input)
                    && node
                        .attributes
                        .get("value")
                        .map(|value| value == "native interaction filled")
                        .unwrap_or(false)
            }),
            "semantic map should expose current input values after native interaction"
        );
    }

    #[cfg(feature = "servo-backend")]
    #[test]
    fn native_browser_interaction_reports_missing_selector() {
        let mut engine = SextantEngine::new();
        let url = Url::parse("data:text/html,<!DOCTYPE html><title>Missing</title><main></main>")
            .expect("data URL should parse");

        engine
            .navigate_with_fallback(url, "default")
            .expect("Servo navigation should succeed");

        let error = engine
            .click_selector_current_page("#missing")
            .expect_err("missing selector should be reported as an interaction error");
        assert!(
            error.contains("No element matched selector"),
            "unexpected missing selector error: {}",
            error
        );
    }

    #[cfg(feature = "servo-backend")]
    #[test]
    fn native_browser_click_updates_tab_after_navigation() {
        let mut engine = SextantEngine::new();
        let destination =
            "data:text/html,%3C!DOCTYPE%20html%3E%3Ctitle%3EClicked%20Destination%3C/title%3E%3Cmain%3EArrived%20after%20click%3C/main%3E";
        let normalized_destination =
            "data:text/html,<!DOCTYPE html><title>Clicked Destination</title><main>Arrived after click</main>";
        let url = Url::parse(&format!(
            "data:text/html,<!DOCTYPE html><title>Click Source</title><main><a id='next' href='{}'>Next</a></main>",
            destination
        ))
        .expect("data URL should parse");
        let destination_url =
            Url::parse(normalized_destination).expect("destination URL should parse");

        engine
            .navigate_with_fallback(url, "default")
            .expect("Servo navigation should succeed");

        let click = engine
            .click_selector_current_page("#next")
            .expect("click interaction should succeed");
        assert!(click.ok);
        assert_eq!(click.current_url.as_ref(), Some(&destination_url));
        assert_eq!(
            engine.get_active_tab().and_then(|tab| tab.url.clone()),
            Some(destination_url.clone())
        );

        let page = engine
            .distill_current_page()
            .expect("distillation should read the clicked destination");
        assert_eq!(page.url, destination_url);
        assert_eq!(page.title, "Clicked Destination");
        assert!(page.content.contains("Arrived after click"));
    }

    #[cfg(feature = "servo-backend")]
    #[test]
    fn distill_tab_reads_live_dom_after_async_script_mutation() {
        let mut engine = SextantEngine::new();
        let url = Url::parse(
            "data:text/html,\
<!DOCTYPE html><title>Before</title><main id='target'>before</main>\
<script>setTimeout(() => { document.title='After Async'; document.getElementById('target').textContent='after async'; }, 0);</script>",
        )
        .expect("async mutating data URL should parse");

        engine
            .navigate_with_fallback(url, "default")
            .expect("Servo navigation should succeed");

        let started_at = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(2);
        loop {
            let page = engine
                .distill_current_page()
                .expect("live DOM distillation should succeed");
            if page.title == "After Async" && page.content.contains("after async") {
                assert!(
                    !page.content.contains("before"),
                    "distillation should reflect async DOM mutations, not stale source content"
                );
                break;
            }

            assert!(
                started_at.elapsed() < timeout,
                "timed out waiting for async DOM mutation; last page was title='{}' content='{}'",
                page.title,
                page.content
            );
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
}
