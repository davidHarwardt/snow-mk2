use std::{sync::{Arc, RwLock}, time::Instant, ffi::c_void, collections::HashMap};

use rand::prelude::*;
use bytemuck::{Zeroable, Pod};
use icrate::{AppKit::{NSView, NSScreen, self}, Foundation::{NSDictionary, ns_string, NSString, NSNumber}};
use rand::rngs::ThreadRng;
use raw_window_handle::{
    HasRawWindowHandle, RawWindowHandle,
};
use objc2::{rc::{Id, autoreleasepool}, Message, runtime::NSObject};
use winit::{
    monitor::MonitorHandle,
    window::{Window, WindowBuilder, WindowLevel, WindowId},
    event_loop::EventLoop, error::{OsError, ExternalError}, event::WindowEvent
};
use wgpu::{util::{DeviceExt, BufferInitDescriptor}, include_wgsl};
use wrld::{Desc, DescInstance};

use crate::utils::UniformBuffer;

// vertex buffer
#[repr(C)]
#[derive(Pod, Zeroable, Desc, Clone, Copy)]
struct SnowflakeVertex {
    #[f32x2(0)] pos: [f32; 2],
}

// instance buffer
#[repr(C)]
#[derive(Pod, Zeroable, DescInstance, Clone, Copy)]
struct SnowflakeInstance {
    #[f32x2(10)] pos: [f32; 2],
    #[f32x2(11)] vel: [f32; 2],
    #[f32(12)] scale: f32,
    #[f32(13)] age: f32,
}

#[repr(C)]
#[derive(Pod, Zeroable, DescInstance, Clone, Copy)]
struct RectInstance {
    #[f32x2(10)] pos: [f32; 2],
    #[f32x2(11)] dim: [f32; 2],
}

// uniform
#[derive(Pod, Zeroable, Clone, Copy)]
#[repr(C)]
struct FrameData {
    dt: f32,
    time: f32,
    gravity: [f32; 2],
    aspect: f32,
    max_age: f32,
}


#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error(transparent)]
    WindowCreation(#[from] OsError),

    #[error(transparent)]
    WinitExternal(#[from] ExternalError),

    #[error(transparent)]
    SurfaceCreation(#[from] wgpu::CreateSurfaceError),
}

pub struct SnowState {
    running: bool,
    creation: Instant,
    last_draw: Instant,
    rng: ThreadRng,

    particle_count: usize,
    instance_buffer: wgpu::Buffer,
    vertex_count: usize,
    vertex_buffer: wgpu::Buffer,
    frame_data: UniformBuffer<FrameData>,
    window_buffer: wgpu::Buffer,
    windows: HashMap<i64, AppWindow>,
    max_windows: usize,

    fg_surface: wgpu::Surface,
    fg_config: wgpu::SurfaceConfiguration,

    /// the bindgroup containing all uniforms
    uniform_bind_group: wgpu::BindGroup,
    /// the bindgroup containing the storage buffer
    /// of the instances
    compute_bind_group: wgpu::BindGroup,

    render_pipeline: wgpu::RenderPipeline,
    rect_pipeline: wgpu::RenderPipeline,
    sim_pipeline: wgpu::ComputePipeline,

    pub fg_window: Window,
    size: winit::dpi::PhysicalSize<u32>,
    monitor: Id<NSScreen>,
}

impl SnowState {
    pub fn new<E>(
        device: &wgpu::Device,
        instance: &wgpu::Instance,
        adapter: &wgpu::Adapter,

        particle_count: usize,
        monitor: Id<NSScreen>,
        event_loop: &EventLoop<E>,
    ) -> Result<Self, BuildError> {
        dbg!(get_windows());

        let fg_window = WindowBuilder::new()
            .with_title("snow-fg")
            .with_transparent(true)
            .with_decorations(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
        .build(event_loop)?;

        fg_window.set_cursor_hittest(false)?;
        configure_window(&fg_window, monitor.clone());
        
        let size = fg_window.inner_size();

        let aspect = size.width as f32 / size.height as f32;

        let fg_surface = unsafe { instance.create_surface(&fg_window) }?;
        let fg_caps = fg_surface.get_capabilities(adapter);
        let fg_format = fg_caps.formats.iter()
            .copied()
            .filter(|f| f.is_srgb())
            .next()
        .unwrap_or(fg_caps.formats[0]);

        let fg_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: fg_format,
            width: size.width,
            height: size.height,
            present_mode: fg_caps.present_modes[0],
            alpha_mode: wgpu::CompositeAlphaMode::PostMultiplied,
            view_formats: vec![],
        };
        fg_surface.configure(&device, &fg_config);

        let mut rng = rand::thread_rng();

        let vertecies = &[
            SnowflakeVertex { pos: [-1.0, 1.0] },
            SnowflakeVertex { pos: [1.0, 1.0] },
            SnowflakeVertex { pos: [1.0, -1.0] },
            SnowflakeVertex { pos: [1.0, -1.0] },
            SnowflakeVertex { pos: [-1.0, -1.0] },
            SnowflakeVertex { pos: [-1.0, 1.0] },
        ];
        let vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("snow-vertex"),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            contents: bytemuck::cast_slice(vertecies),
        });
        let vertex_count = vertecies.len();

        let instance_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("snow-instance"),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE,
            contents: bytemuck::cast_slice(
                &(0..particle_count).map(|_| {
                    let pos = [
                        rng.gen_range(-1.0..1.0),
                        rng.gen_range(-1.0..1.0) * (1.0 + 0.05),
                    ];

                    SnowflakeInstance {
                        pos,
                        vel: [0.0, 0.0],
                        scale: rng.gen_range(0.1..1.5) * 0.01,
                        age: 0.0,
                    }
                }).collect::<Vec<_>>()
            )
        });

        let max_windows = 100usize;
        let window_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("window instance"),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            size: std::mem::size_of::<RectInstance>() as u64 * max_windows as u64,
            mapped_at_creation: false,
        });
        let windows = HashMap::new();

        let frame_data = UniformBuffer::new(device, FrameData {
            aspect,
            dt: 0.0,
            time: 0.0,
            gravity: [0.1, -1.0],
            max_age: 100.0,
        }, Some("frame data"));


        let uniform_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("uniform bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE | wgpu::ShaderStages::VERTEX,
                    ty: frame_data.binding_ty(),
                    count: None,
                },
            ],
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("uniform bind group"),
            layout: &uniform_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: frame_data.buffer().as_entire_binding(),
                },
            ],
        });

        let compute_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("compute bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let compute_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("compute bind group"),
            layout: &compute_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: instance_buffer.as_entire_binding(),
                },
            ]
        });

        let render_shader = device.create_shader_module(
            include_wgsl!("shaders/render.wgsl")
        );
        let sim_shader = device.create_shader_module(
            include_wgsl!("shaders/simulate.wgsl")
        );
        let rect_shader = device.create_shader_module(
            include_wgsl!("shaders/rect.wgsl")
        );

        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("render pipeline layout"),
            bind_group_layouts: &[&uniform_bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &render_shader,
                buffers: &[SnowflakeVertex::desc(), SnowflakeInstance::desc()],
                entry_point: "vertex_main",
            },
            fragment: Some(wgpu::FragmentState {
                module: &render_shader,
                entry_point: "fragment_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: fg_config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        let rect_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rect pipeline layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        let rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rect pipeline"),
            layout: Some(&rect_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &rect_shader,
                buffers: &[SnowflakeVertex::desc(), RectInstance::desc()],
                entry_point: "vertex_main",
            },
            fragment: Some(wgpu::FragmentState {
                module: &rect_shader,
                entry_point: "fragment_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: fg_config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        let sim_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("compute pipeline layout"),
            bind_group_layouts: &[&uniform_bind_group_layout, &compute_bind_group_layout],
            push_constant_ranges: &[],
        });

        let sim_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("sim pipeline"),
            layout: Some(&sim_pipeline_layout),
            module: &sim_shader,
            entry_point: "main",
        });

        let creation = Instant::now();
        let last_draw = Instant::now();
        // info: maybe set to false?
        let running = true;

        Ok(Self {
            instance_buffer, vertex_buffer,
            vertex_count, particle_count,
            window_buffer, windows,
            fg_surface, fg_config,
            fg_window, size, monitor,
            creation, running, last_draw,
            frame_data, rng,

            uniform_bind_group,
            compute_bind_group,
            render_pipeline,
            rect_pipeline,
            sim_pipeline,
            max_windows,
        })
    }

    pub fn window_id(&self) -> WindowId { self.fg_window.id() }

    pub fn set_running(&mut self, v: bool) {
        tracing::info!("set running: {v}");
        if v { self.redraw() }
        self.running = v;
    }

    pub fn event(&mut self, event: WindowEvent) {
        tracing::info!("{event:?}");
        match event {
            WindowEvent::Occluded(occluded) => self.set_running(!occluded),
            _ => (),
        }
    }


    pub fn redraw(&self) {
        if self.running {
            self.fg_window.request_redraw();
        }
    }

    pub fn update_windows(&mut self, queue: &wgpu::Queue) {
        let dim = self.fg_window.inner_size().cast::<f32>();
        let windows = get_windows();
        let buf_data = windows.iter()
            .filter(|v| v.layer == 0)
            .map(|v| RectInstance {
                pos: [v.pos.0 as f32 / dim.width, v.pos.1 as f32 / dim.height],
                dim: [v.dim.0 as f32 / dim.width, v.dim.1 as f32 / dim.height],
            })
        .collect::<Vec<_>>();

        queue.write_buffer(
            &self.window_buffer,
            0, bytemuck::cast_slice(&buf_data[..buf_data.len().min(self.max_windows)]),
        );

        self.windows = windows.into_iter()
            .filter(|v| v.layer == 0)
            .map(|v| (v.number, v))
        .collect();
    }

    pub fn update(&mut self, queue: &wgpu::Queue) {
        self.frame_data.time = self.creation.elapsed().as_secs_f32();
        self.frame_data.dt = self.last_draw.elapsed().as_secs_f32();
        self.last_draw = Instant::now();
        self.update_windows(queue);
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(), wgpu::SurfaceError> {
        self.frame_data.write(queue);

        let fg_output = self.fg_surface.get_current_texture()?;
        let fg_view = fg_output.texture.create_view(
            &wgpu::TextureViewDescriptor::default(),
        );

        let mut encoder = device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("win-encoder"),
            }
        );

        {
            let mut sim_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("sim pass"),
                timestamp_writes: None,
            });

            sim_pass.set_pipeline(&self.sim_pipeline);
            sim_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            sim_pass.set_bind_group(1, &self.compute_bind_group, &[]);
            #[cfg(debug_assertions)]
            sim_pass.insert_debug_marker("sim pass update");

            let compute_size: usize = 256;
            let n_instances = self.particle_count.div_ceil(compute_size);
            sim_pass.dispatch_workgroups(n_instances as _, 1, 1);
        }

        {
            let mut renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fg-renderpass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &fg_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            // r: self.creation.elapsed().as_secs_f64().sin().abs(),
                            r: 0.0,
                            g: 0.2,
                            b: 0.3, a: 0.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },

                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            // renderpass.set_pipeline(&self.rect_pipeline);
            // renderpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            // renderpass.set_vertex_buffer(1, self.window_buffer.slice(..));
            // renderpass.draw(0..(self.vertex_count as _), 0..(self.windows.len() as _));

            renderpass.set_pipeline(&self.render_pipeline);
            renderpass.set_bind_group(0, &self.uniform_bind_group, &[]);
            renderpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            renderpass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            renderpass.draw(0..(self.vertex_count as _), 0..(self.particle_count as _));
        }
        queue.submit(Some(encoder.finish()));
        fg_output.present();
        Ok(())
    }
}

fn configure_window(window: &Window, monitor: Id<NSScreen>) {
    let ns_view = match window.raw_window_handle() {
        RawWindowHandle::AppKit(handle) => unsafe {
            Id::new(handle.ns_view as *mut NSView)
                .expect("could not get ns_view")
        },
        v => panic!("invalid window handle type: {v:?}"),
    };
    let ns_window = ns_view.window().expect("could not get ns_window");

    ns_window.setMovable(false);
    ns_window.setFrame_display(monitor.frame(), true);
    // disable window shadow to remove artifacts
    ns_window.setHasShadow(false);
    // ns_window.setLevel(99999);
    // ns_window.setLevel(-1);

    unsafe {
        ns_window.setCollectionBehavior(
            0
            // moves with current space + can overlay fullscreen windows
            |  AppKit::NSWindowCollectionBehaviorCanJoinAllSpaces
            // cant tab to window
            | AppKit::NSWindowCollectionBehaviorIgnoresCycle
            // stays even while mission control
            | AppKit::NSWindowCollectionBehaviorStationary
            // dont show in fullscreen
            | AppKit::NSWindowCollectionBehaviorFullScreenNone
        );
    };

    std::mem::forget(ns_view);
}

#[derive(Debug)]
struct AppWindow {
    owner_name: Option<String>,
    name: Option<String>,
    pos: (f64, f64),
    dim: (f64, f64),
    layer: i64,
    number: i64,
}

fn get_windows() -> Vec<AppWindow> {
    use core_graphics::window;
    let arr = unsafe {
        cf_array::<NSDictionary<NSString, NSObject>>(window::CGWindowListCopyWindowInfo(window::kCGWindowListOptionOnScreenOnly, 0))
    };

    arr.into_iter().map(|v| {
        autoreleasepool(|p| {
            let owner_name = v.get(ns_string!("kCGWindowOwnerName")).map(|v| {
                let v: &NSString = unsafe { std::mem::transmute(v) };
                v.as_str(p).to_string()
            });
            let name = v.get(ns_string!("kCGWindowName")).map(|v| {
                let v: &NSString = unsafe { std::mem::transmute(v) };
                v.as_str(p).to_string()
            });
            let bounds: &NSDictionary<NSString, NSNumber> = unsafe {
                std::mem::transmute(v.get(ns_string!("kCGWindowBounds")).unwrap())
            };
            let (x, y, w, h) = (
                bounds[ns_string!("X")].as_f64(),
                bounds[ns_string!("Y")].as_f64(),
                bounds[ns_string!("Width")].as_f64(),
                bounds[ns_string!("Height")].as_f64(),
            );
            let layer: &NSNumber = unsafe {
                std::mem::transmute(v.get(ns_string!("kCGWindowLayer")).unwrap())
            };
            let layer = layer.as_i64();
            let number: &NSNumber = unsafe {
                std::mem::transmute(v.get(ns_string!("kCGWindowNumber")).unwrap())
            };
            let number = number.as_i64();
            AppWindow {
                owner_name, layer, name,
                dim: (w, h),
                pos: (x, y),
                number,
            }
        })
    }).collect()
}

unsafe fn cf_array<T: Message>(array: core_graphics::display::CFArrayRef) -> Vec<Id<T>> {
    (0..core_graphics::display::CFArrayGetCount(array)).flat_map(|i| {
        let unmanaged = core_graphics::display::CFArrayGetValueAtIndex(array, i);
        if unmanaged.is_null() { tracing::warn!("got null from cf_array") }
        let rec = std::mem::transmute(unmanaged);
        Id::new(rec)
    }).collect()
}

