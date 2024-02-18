use std::collections::HashMap;

use icrate::{Foundation::MainThreadMarker, AppKit::NSScreen};
use winit::{window::{Window, WindowId}, event_loop::EventLoop, event::WindowEvent};

use crate::snow::{SnowState, BuildError};


pub struct State {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,

    states: HashMap<WindowId, SnowState>,
}


impl State {
    pub async fn new<E>(
        main_thread: MainThreadMarker,
        event_loop: &EventLoop<E>,
    ) -> Result<Self, BuildError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
        }).await.expect("could not find adapter");

        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor {
            features: wgpu::Features::empty(),
            limits: wgpu::Limits::default(),
            label: Some("render_device"),
        }, None).await.expect("could not get device");

        let states = NSScreen::screens(main_thread).into_iter()
            .map(|m| {
                let s = SnowState::new(
                    &device, &instance,
                    &adapter, 1000, m,
                    &event_loop,
                )?;
                Ok((s.window_id(), s))
            })
        .collect::<Result<_, BuildError>>()?;

        Ok(Self {
            instance,
            adapter, device, queue,
            states,
        })
    }

    pub fn redraw(&self) {
        for state in self.states.values() {
            state.redraw();
        }
    }

    pub fn event(&mut self, id: &WindowId, event: WindowEvent) {
        if let Some(state) = self.states.get_mut(id) {
            state.event(event);
        } else { tracing::info!("got invalid id for window event") }
    }

    pub fn update(&mut self) {
        for state in self.states.values_mut() {
            state.update(&self.queue);
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        for state in self.states.values_mut() {
            state.render(&self.device, &self.queue)?;
        }
        Ok(())
    }
}

