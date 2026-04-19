use url::Url;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use std::sync::Arc;
use std::time::Duration;
use reqwest::blocking::Client;
use scraper::{Html, Selector};
use sextant_firewall::{SextantFirewall, FirewallAction};
use sextant_bridge::{NeuralBridge, MultiModalPerception};
use sextant_privacy;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum EngineBackend {
    Servo,      // Primary: Parallelized, Memory-Safe
    Gecko,      // Compatibility: Mature, Standard-compliant
    Chromium,   // Legacy: "Optimized for Chrome" sites
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
pub struct Tab {
    pub id: Uuid,
    pub url: Option<Url>,
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
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }).await.ok_or("Failed to find a suitable GPU adapter")?;

        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Sextant GPU Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
            },
            None,
        ).await.map_err(|e| e.to_string())?;

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

        Ok(Self { device, queue, pipeline })
    }

    pub fn render_frame(&self) -> Result<(), String> {
        let encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
            fragments.into_par_iter().map(|f| {
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
            }).collect()
        })
    }
}

pub struct SextantEngine {
    tabs: HashMap<Uuid, Tab>,
    active_tab_id: Option<Uuid>,
    fallback_enabled: bool,
    semantic_cache: HashMap<Url, DistilledPage>,
    gpu_instance: Option<Arc<wgpu::Instance>>,
    renderer: Option<Arc<WgpuRenderer>>,
    firewall: Arc<SextantFirewall>,
    bridge: Arc<NeuralBridge>,
    layout_engine: Arc<ParallelLayoutEngine>,
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

    let document = Html::parse_document(&html);
    let title = extract_text(&document, "title", 1)?
        .into_iter()
        .next()
        .unwrap_or_else(|| final_url.to_string());
    let heading_text = extract_text(&document, "h1, h2, h3", 24)?;
    let body_text = extract_text(&document, "main, article, section, p, li", 400)?;
    let link_selector = parse_selector("a[href]")?;
    let image_selector = parse_selector("img[src]")?;

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
        let absolute_href = final_url
            .join(href)
            .map(|joined| joined.to_string())
            .unwrap_or_else(|_| href.to_string());
        let mut attributes = HashMap::new();
        attributes.insert("href".to_string(), absolute_href);
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

    Ok(DistilledPage {
        title,
        url: final_url,
        content: content.chars().take(8000).collect(),
        semantic_map,
        metadata,
    })
}

impl SextantEngine {
    pub fn new() -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let mut engine = Self {
            tabs: HashMap::new(),
            active_tab_id: None,
            fallback_enabled: true,
            semantic_cache: HashMap::new(),
            gpu_instance: Some(Arc::new(instance)),
            renderer: None,
            firewall: Arc::new(SextantFirewall::new()),
            bridge: Arc::new(NeuralBridge::new()),
            layout_engine: Arc::new(ParallelLayoutEngine::new(8)),
        };
        // Open a default tab
        engine.open_tab();
        engine
    }

    pub async fn initialize_gpu(&mut self) -> Result<(), String> {
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

    pub fn open_tab(&mut self) -> Uuid {
        let id = Uuid::new_v4();
        let tab = Tab {
            id,
            url: None,
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
        if self.active_tab_id.is_none() {
            self.active_tab_id = Some(id);
        }
        id
    }

    pub fn close_tab(&mut self, id: &Uuid) -> Result<(), String> {
        self.tabs.remove(id).ok_or("Tab not found")?;
        if self.active_tab_id == Some(*id) {
            self.active_tab_id = self.tabs.keys().next().cloned();
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
        Ok(())
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
        let mut tabs: Vec<Tab> = self.tabs.values().cloned().collect();
        tabs.sort_by(|a, b| b.last_active.cmp(&a.last_active));
        tabs
    }

    /// Navigates to a URL in the active tab, automatically falling back if rendering fails.
    pub fn navigate_with_fallback(&mut self, url: Url, persona_id: &str) -> Result<EngineStatus, String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;

        // 1. Firewall Check
        let (firewall_action, reason) = self.firewall.check_access(persona_id, &url);
        if firewall_action == FirewallAction::Block {
            if let Some(tab) = self.tabs.get_mut(&tab_id) {
                tab.status.firewall_status = Some(format!("Blocked: {}", reason));
            }
            return Err(format!("Firewall Blocked: {} (Reason: {})", url, reason));
        }

        let mut active_backend = self.tabs.get(&tab_id).unwrap().status.active_backend.clone();

        match self.try_render(&url, &active_backend) {
            Ok(status) => {
                if let Some(tab) = self.tabs.get_mut(&tab_id) {
                    tab.url = Some(url);
                    tab.status = status.clone();
                }
                Ok(status)
            }
            Err(e) => {
                if self.fallback_enabled && active_backend == EngineBackend::Servo {
                    println!("Servo rendering failed for {}: {}. Falling back to Gecko...", url, e);
                    active_backend = EngineBackend::Gecko;
                    let res = self.try_render(&url, &active_backend);
                    if let Ok(status) = &res {
                        if let Some(tab) = self.tabs.get_mut(&tab_id) {
                            tab.url = Some(url);
                            tab.status = status.clone();
                        }
                    }
                    res
                } else if self.fallback_enabled && active_backend == EngineBackend::Gecko {
                    println!("Gecko rendering failed. Falling back to Chromium (Legacy Mode)...");
                    active_backend = EngineBackend::Chromium;
                    let res = self.try_render(&url, &active_backend);
                    if let Ok(status) = &res {
                        if let Some(tab) = self.tabs.get_mut(&tab_id) {
                            tab.url = Some(url);
                            tab.status = status.clone();
                        }
                    }
                    res
                } else {
                    Err(format!("All engine backends failed for {}: {}", url, e))
                }
            }
        }
    }

    fn try_render(&self, url: &Url, backend: &EngineBackend) -> Result<EngineStatus, String> {
        // In a native build, this would interface with the respective engine's FFI/IPC
        // and spawn a new sandboxed process.
        let pid = rand::random::<u32>() % 10000 + 1000;
        
        match backend {
            EngineBackend::Servo => {
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
                    LayoutFragment { id: 1, selector: "header".into(), depth: 1 },
                    LayoutFragment { id: 2, selector: "main".into(), depth: 1 },
                    LayoutFragment { id: 3, selector: "footer".into(), depth: 1 },
                    LayoutFragment { id: 4, selector: "sidebar".into(), depth: 2 },
                ];
                let layout_results = self.layout_engine.layout_parallel(fragments);
                let total_layout_time: f64 = layout_results.iter().map(|r| r.layout_time_ms).sum();

                Ok(EngineStatus { 
                    active_backend: EngineBackend::Servo, 
                    is_sandboxed: true, 
                    memory_usage_mb: 120,
                    gpu_accelerated: self.renderer.is_some(),
                    sandbox_profile: Some(SandboxProfile {
                        pid,
                        restricted_syscalls: vec!["write".into(), "open".into(), "exec".into()],
                        memory_limit_mb: 256,
                        network_access: true,
                    }),
                    firewall_status: Some("Allowed by Servo Policy".into()),
                    layout_time_ms: total_layout_time,
                    parallel_threads: 8,
                })
            }
            EngineBackend::Gecko => {
                Ok(EngineStatus { 
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
                    layout_time_ms: 12.5, // Simulated single-threaded layout
                    parallel_threads: 1,
                })
            }
            EngineBackend::Chromium => {
                Ok(EngineStatus { 
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
                    layout_time_ms: 25.0, // Simulated legacy layout
                    parallel_threads: 1,
                })
            }
        }
    }

    /// Distills the current page into a semantic map for the Pilot.
    pub fn distill_current_page(&mut self) -> Result<DistilledPage, String> {
        let tab_id = self.active_tab_id.ok_or("No active tab")?;
        self.distill_tab(tab_id)
    }

    pub fn distill_tab(&mut self, tab_id: Uuid) -> Result<DistilledPage, String> {
        let url = self.tabs.get(&tab_id).and_then(|t| t.url.clone()).unwrap_or_else(|| Url::parse("about:blank").unwrap());

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

    /// Performs semantic perception across all open tabs in parallel.
    /// This leverages Servo's parallelized architecture for high-performance distillation.
    pub fn perceive_all_tabs(&mut self) -> Vec<DistilledPage> {
        let tab_data: Vec<(Uuid, Url)> = self.tabs.iter()
            .map(|(id, t)| (*id, t.url.clone().unwrap_or_else(|| Url::parse("about:blank").unwrap())))
            .collect();
        
        // Parallel distillation using Rayon
        let perceptions: Vec<DistilledPage> = tab_data.par_iter()
            .filter_map(|(_id, url)| {
                fetch_distilled_page(url).ok()
            })
            .collect();

        // Update state with parallel results
        for (tab_id, page) in tab_data.iter().zip(perceptions.iter()) {
            self.semantic_cache.insert(page.url.clone(), page.clone());
            if let Some(tab) = self.tabs.get_mut(&tab_id.0) {
                tab.distilled_page = Some(page.clone());
                tab.url = Some(page.url.clone());
            }
        }

        perceptions
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
}
