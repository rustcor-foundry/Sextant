// Copyright 2022 the Vello Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Simple helpers for managing wgpu state and surfaces.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::Result;

use wgpu::{
    Adapter, Device, Instance, Limits, Queue, Surface, SurfaceConfiguration, SurfaceTarget,
    TextureFormat, RequestAdapterOptions,
};

/// Simple render context that maintains wgpu state for rendering the pipeline.
pub struct RenderContext {
    pub instance: Instance,
    pub devices: Vec<DeviceHandle>,
}

pub struct DeviceHandle {
    adapter: Adapter,
    pub device: Device,
    pub queue: Queue,
    pub device_lost: Arc<AtomicBool>,
}

impl RenderContext {
    pub fn new() -> Result<Self> {
        eprintln!("[vello] RenderContext::new: begin");
        let instance = Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::util::backend_bits_from_env().unwrap_or(wgpu::Backends::PRIMARY),
            dx12_shader_compiler: wgpu::Dx12Compiler::Fxc,
            ..Default::default()
        });
        eprintln!("[vello] RenderContext::new: instance created");
        Ok(Self {
            instance,
            devices: Vec::new(),
        })
    }

    /// Creates a new surface for the specified window and dimensions.
    pub async fn create_surface<'w>(
        &mut self,
        window: impl Into<SurfaceTarget<'w>>,
        width: u32,
        height: u32,
        present_mode: wgpu::PresentMode,
    ) -> Result<RenderSurface<'w>> {
        eprintln!("[vello] create_surface: begin");
        let surface = self.instance.create_surface(window.into())?;
        eprintln!("[vello] create_surface: surface object created");
        let dev_id = self
            .device(Some(&surface))
            .await
            .ok_or("Error creating device")?;
        eprintln!("[vello] create_surface: device id {dev_id}");

        let device_handle = &self.devices[dev_id];
        let capabilities = surface.get_capabilities(&device_handle.adapter);
        eprintln!(
            "[vello] create_surface: capabilities formats={} present_modes={} requested_present_mode={present_mode:?}",
            capabilities.formats.len(),
            capabilities.present_modes.len()
        );
        let format = capabilities
            .formats
            .into_iter()
            .find(|it| matches!(it, TextureFormat::Rgba8Unorm | TextureFormat::Bgra8Unorm))
            .expect("surface should support Rgba8Unorm or Bgra8Unorm");
        eprintln!("[vello] create_surface: chosen format {:?}", format);
        let present_mode = if capabilities.present_modes.contains(&present_mode) {
            present_mode
        } else if capabilities.present_modes.contains(&wgpu::PresentMode::Fifo) {
            wgpu::PresentMode::Fifo
        } else {
            capabilities
                .present_modes
                .first()
                .copied()
                .expect("surface should expose at least one present mode")
        };
        eprintln!(
            "[vello] create_surface: chosen present_mode {:?}",
            present_mode
        );

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode,
            desired_maximum_frame_latency: 1,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        let surface = RenderSurface {
            surface,
            config,
            dev_id,
            format,
        };
        eprintln!("[vello] create_surface: configuring surface");
        self.configure_surface(&surface);
        eprintln!("[vello] create_surface: complete");
        Ok(surface)
    }

    /// Resizes the surface to the new dimensions.
    pub fn resize_surface(&self, surface: &mut RenderSurface, width: u32, height: u32) {
        surface.config.width = width;
        surface.config.height = height;
        self.configure_surface(surface);
    }

    pub fn set_present_mode(&self, surface: &mut RenderSurface, present_mode: wgpu::PresentMode) {
        surface.config.present_mode = present_mode;
        self.configure_surface(surface);
    }

    fn configure_surface(&self, surface: &RenderSurface) {
        let device = &self.devices[surface.dev_id].device;
        surface.surface.configure(device, &surface.config);
    }

    /// Finds or creates a compatible device handle id.
    pub async fn device(&mut self, compatible_surface: Option<&Surface<'_>>) -> Option<usize> {
        eprintln!(
            "[vello] device: searching existing device compatible_surface={}",
            compatible_surface.is_some()
        );
        let compatible = match compatible_surface {
            Some(s) => self
                .devices
                .iter()
                .enumerate()
                .find(|(_, d)| d.adapter.is_surface_supported(s))
                .map(|(i, _)| i),
            None => (!self.devices.is_empty()).then_some(0),
        };
        if compatible.is_none() {
            eprintln!("[vello] device: no existing device matched, creating a new one");
            return self.new_device(compatible_surface).await;
        }
        eprintln!("[vello] device: reusing device {:?}", compatible);
        compatible
    }

    /// Creates a compatible device handle id.
    async fn new_device(&mut self, compatible_surface: Option<&Surface<'_>>) -> Option<usize> {
        let force_fallback_adapter = matches!(
            std::env::var("SEXTANT_WGPU_FORCE_FALLBACK").as_deref(),
            Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
        );
        eprintln!(
            "[vello] new_device: begin force_fallback_adapter={force_fallback_adapter}"
        );
        let adapter = match wgpu::util::initialize_adapter_from_env_or_default(
            &self.instance,
            compatible_surface,
        )
        .await
        {
            Some(adapter) => {
                let info = adapter.get_info();
                eprintln!(
                    "[vello] new_device: selected adapter name='{}' backend={:?} device_type={:?}",
                    info.name, info.backend, info.device_type
                );
                adapter
            }
            None if force_fallback_adapter => {
                eprintln!("[vello] new_device: requesting explicit fallback adapter");
                let adapter = self.instance
                    .request_adapter(&RequestAdapterOptions {
                        power_preference: wgpu::PowerPreference::LowPower,
                        force_fallback_adapter: true,
                        compatible_surface,
                    })
                    .await?;
                let info = adapter.get_info();
                eprintln!(
                    "[vello] new_device: fallback adapter name='{}' backend={:?} device_type={:?}",
                    info.name, info.backend, info.device_type
                );
                adapter
            }
            None => {
                eprintln!("[vello] new_device: no adapter found");
                return None;
            }
        };
        let features = adapter.features();
        let limits = Limits::default();
        #[allow(unused_mut)]
        let mut maybe_features = wgpu::Features::CLEAR_TEXTURE;
        #[cfg(feature = "wgpu-profiler")]
        {
            maybe_features |= wgpu_profiler::GpuProfiler::ALL_WGPU_TIMER_FEATURES;
        };
        eprintln!(
            "[vello] new_device: requesting device with features {:?}",
            features & maybe_features
        );
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: features & maybe_features,
                    required_limits: limits,
                },
                None,
            )
            .await
            .ok()?;
        eprintln!("[vello] new_device: device request succeeded");
        let device_lost = Arc::new(AtomicBool::new(false));
        let uncaptured_device_lost = device_lost.clone();
        device.on_uncaptured_error(Box::new(move |error| {
            eprintln!("[vello] uncaptured device error: {error}");
            uncaptured_device_lost.store(true, Ordering::SeqCst);
        }));
        let lost_callback_flag = device_lost.clone();
        device.set_device_lost_callback(move |reason, message| {
            eprintln!(
                "[vello] device lost callback: reason={reason:?} message={message}"
            );
            lost_callback_flag.store(true, Ordering::SeqCst);
        });
        let device_handle = DeviceHandle {
            adapter,
            device,
            queue,
            device_lost,
        };
        self.devices.push(device_handle);
        eprintln!("[vello] new_device: stored device index {}", self.devices.len() - 1);
        Some(self.devices.len() - 1)
    }
}

/// Combination of surface and its configuration.
#[derive(Debug)]
pub struct RenderSurface<'s> {
    pub surface: Surface<'s>,
    pub config: SurfaceConfiguration,
    pub dev_id: usize,
    pub format: TextureFormat,
}

struct NullWake;

impl std::task::Wake for NullWake {
    fn wake(self: std::sync::Arc<Self>) {}
}

/// Block on a future, polling the device as needed.
///
/// This will deadlock if the future is awaiting anything other than GPU progress.
pub fn block_on_wgpu<F: Future>(device: &Device, mut fut: F) -> F::Output {
    let waker = std::task::Waker::from(std::sync::Arc::new(NullWake));
    let mut context = std::task::Context::from_waker(&waker);
    // Same logic as `pin_mut!` macro from `pin_utils`.
    let mut fut = unsafe { std::pin::Pin::new_unchecked(&mut fut) };
    loop {
        match fut.as_mut().poll(&mut context) {
            std::task::Poll::Pending => {
                device.poll(wgpu::Maintain::Wait);
            }
            std::task::Poll::Ready(item) => break item,
        }
    }
}
