use std::cell::{Cell, RefCell};
use std::env;
#[cfg(target_os = "windows")]
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use servo::{
    RenderingContext, Servo, ServoBuilder, WebView, WebViewBuilder, WebViewDelegate,
    WindowRenderingContext,
};
use url::Url;
use winit30::application::ApplicationHandler;
use winit30::dpi::PhysicalSize;
use winit30::event::WindowEvent;
use winit30::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit30::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit30::window::{Window, WindowAttributes, WindowId};

const DEFAULT_URL: &str = "https://example.com";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

struct DirectServoApp {
    target: Url,
    smoke: bool,
    timeout: Duration,
    started: Instant,
    state: Option<Rc<DirectServoState>>,
}

struct DirectServoState {
    window: Window,
    servo: Servo,
    rendering_context: Rc<WindowRenderingContext>,
    webview: RefCell<Option<WebView>>,
    frame_ready: Cell<bool>,
    first_present: Cell<Option<Duration>>,
}

impl WebViewDelegate for DirectServoState {
    fn notify_new_frame_ready(&self, _webview: WebView) {
        self.frame_ready.set(true);
        self.window.request_redraw();
    }
}

impl DirectServoApp {
    fn new(target: Url, smoke: bool, timeout: Duration) -> Self {
        Self {
            target,
            smoke,
            timeout,
            started: Instant::now(),
            state: None,
        }
    }
}

impl ApplicationHandler for DirectServoApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let display_handle = event_loop
            .display_handle()
            .expect("failed to get display handle");
        let window = event_loop
            .create_window(
                WindowAttributes::default()
                    .with_title("Sextant Servo Direct")
                    .with_inner_size(PhysicalSize::new(1180, 760)),
            )
            .expect("failed to create direct Servo window");
        let window_handle = window.window_handle().expect("failed to get window handle");
        let rendering_context = Rc::new(
            WindowRenderingContext::new(display_handle, window_handle, window.inner_size())
                .expect("failed to create Servo window rendering context"),
        );
        rendering_context
            .make_current()
            .expect("failed to activate Servo window rendering context");

        let servo = ServoBuilder::default().build();
        servo.setup_logging();

        let state = Rc::new(DirectServoState {
            window,
            servo,
            rendering_context,
            webview: RefCell::new(None),
            frame_ready: Cell::new(false),
            first_present: Cell::new(None),
        });

        let webview = WebViewBuilder::new(&state.servo, state.rendering_context.clone())
            .url(self.target.clone())
            .delegate(state.clone())
            .build();
        state.webview.replace(Some(webview));
        state.window.request_redraw();
        self.state = Some(state);
        println!(
            "[servo-direct] opening {} with Servo WindowRenderingContext",
            self.target
        );
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if let Some(state) = self.state.as_ref() {
            state.servo.spin_event_loop();
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                if let Some(state) = self.state.as_ref() {
                    if let Some(webview) = state.webview.borrow().as_ref() {
                        webview.paint();
                        state.rendering_context.present();
                        if state.first_present.get().is_none() {
                            let elapsed = self.started.elapsed();
                            state.first_present.set(Some(elapsed));
                            println!(
                                "[servo-direct] first direct present in {}",
                                format_duration(elapsed)
                            );
                            if self.smoke {
                                event_loop.exit();
                            }
                        }
                    }
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(state) = self.state.as_ref() {
                    if let Some(webview) = state.webview.borrow().as_ref() {
                        webview.resize(size);
                    }
                    state.window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.as_ref() {
            state.servo.spin_event_loop();
            if state.frame_ready.replace(false) {
                state.window.request_redraw();
            }
        }

        if self.smoke && self.started.elapsed() >= self.timeout {
            eprintln!(
                "[servo-direct] timed out after {} before first direct present",
                format_duration(self.timeout)
            );
            event_loop.exit();
        }
    }
}

fn main() -> Result<(), String> {
    install_rustls_crypto_provider();
    prime_windows_angle_runtime()?;
    let args: Vec<String> = env::args().collect();
    let target = parse_target(&args)?;
    let smoke = args.iter().any(|arg| arg == "--smoke");
    let timeout = parse_timeout(&args)?;

    let event_loop = EventLoop::new().map_err(|error| error.to_string())?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = DirectServoApp::new(target, smoke, timeout);
    event_loop
        .run_app(&mut app)
        .map_err(|error| format!("direct Servo event loop failed: {error}"))
}

fn install_rustls_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
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
                current_path = env::join_paths(paths).map_err(|error| {
                    format!("failed to extend PATH for Servo ANGLE runtime: {error}")
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

fn parse_target(args: &[String]) -> Result<Url, String> {
    if let Some(index) = args.iter().position(|arg| arg == "--url") {
        if let Some(value) = args.get(index + 1) {
            return Url::parse(value).map_err(|error| error.to_string());
        }
        return Err("--url requires a URL value".to_string());
    }
    if let Some(value) = args
        .iter()
        .skip(1)
        .find(|arg| !arg.starts_with("--") && arg.parse::<u64>().is_err())
    {
        return Url::parse(value).map_err(|error| error.to_string());
    }
    Url::parse(DEFAULT_URL).map_err(|error| error.to_string())
}

fn parse_timeout(args: &[String]) -> Result<Duration, String> {
    let Some(index) = args.iter().position(|arg| arg == "--timeout-seconds") else {
        return Ok(DEFAULT_TIMEOUT);
    };
    let value = args
        .get(index + 1)
        .ok_or_else(|| "--timeout-seconds requires a value".to_string())?;
    let seconds = value.parse::<u64>().map_err(|error| error.to_string())?;
    Ok(Duration::from_secs(seconds.max(1)))
}

fn format_duration(duration: Duration) -> String {
    let millis = duration.as_millis();
    if millis >= 1000 {
        format!("{:.1}s", millis as f64 / 1000.0)
    } else {
        format!("{millis}ms")
    }
}
