#[macro_use]
extern crate itertools;

mod camera;
mod cube;
mod lib;
mod spawner;
mod texture;
mod vertex;
mod world;

use cgmath::prelude::*;
use futures::executor::block_on;
use spawner::Spawner;
use std::{borrow::Cow, future::Future, mem, pin::Pin, task};
use wgpu::util::DeviceExt;
use winit::{
    event::{DeviceEvent, ElementState, Event, MouseButton, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
};

const NUM_INSTANCES_PER_ROW: u32 = 10;
const BLOCK_SIZE: f32 = 2.0;

fn main() {
    let s = block_on(setup());
    start(s);
}

struct Setup {
    window: winit::window::Window,
    event_loop: EventLoop<()>,
    #[allow(dead_code)]
    instance: wgpu::Instance,
    size: winit::dpi::PhysicalSize<u32>,
    surface: wgpu::Surface,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    // #[cfg(target_arch = "wasm32")]
    // offscreen_canvas_setup: Option<OffscreenCanvasSetup>,
}

struct Scene {
    vertex_buffers: [wgpu::Buffer; 2],
    index_buf: wgpu::Buffer,
    index_count: usize,
    texture_bind_group: wgpu::BindGroup,
    camera_bind_group: wgpu::BindGroup,
    camera_buf: wgpu::Buffer,
    camera_staging_buf: wgpu::Buffer,
    instance_data: [Vec<lib::Instance>; 2],
    instance_buffers: [wgpu::Buffer; 2],
    depth_texture: texture::Texture,
    pipeline: wgpu::RenderPipeline,

    camera_ray_buffer: wgpu::Buffer,
    camera_ray_index_buf: wgpu::Buffer,
    camera_ray_index_count: usize,
    pipeline_lines: wgpu::RenderPipeline,

    pipeline_wire: Option<wgpu::RenderPipeline>,
}

async fn setup() -> Setup {
    let event_loop = EventLoop::new();
    let mut builder = winit::window::WindowBuilder::new();
    builder = builder.with_title("Minecrust");
    let window = builder.build(&event_loop).unwrap();

    let backend = wgpu::Backends::PRIMARY;
    let instance = wgpu::Instance::new(backend);

    let size = window.inner_size();
    let surface = unsafe {
        let surface = instance.create_surface(&window);
        surface
    };

    let adapter =
        wgpu::util::initialize_adapter_from_env_or_default(&instance, backend, Some(&surface))
            .await
            .expect("No suitable GPU adapters found on the system!");

    let adapter_info = adapter.get_info();
    println!("Using {} ({:?})", adapter_info.name, adapter_info.backend);

    let trace_dir = std::env::var("WGPU_TRACE");
    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                features: adapter.features(),
                limits: adapter.limits(),
            },
            trace_dir.ok().as_ref().map(std::path::Path::new),
        )
        .await
        .expect("Unable to find a suitable GPU adapter!");

    Setup {
        window,
        event_loop,
        instance,
        size,
        surface,
        adapter,
        device,
        queue,
    }
}

fn start(
    Setup {
        window,
        event_loop,
        instance: _,
        size,
        surface,
        adapter,
        device,
        queue,
    }: Setup,
) {
    let format = *surface
        .get_supported_formats(&adapter)
        .unwrap()
        .first()
        .unwrap();
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: format,
        width: size.width,
        height: size.height,
        present_mode: wgpu::PresentMode::Mailbox,
    };
    surface.configure(&device, &config);

    let mut camera_controller = camera::CameraController::new(0.15, 0.01);
    let mut camera = camera::Camera {
        eye: (3.0, 2.0, 3.0).into(),
        // have it look at the origin
        target: (0.0, 0.0, 0.0).into(),
        // which way is "up"
        up: cgmath::Vector3::unit_y(),
        aspect: config.width as f32 / config.height as f32,
        fovy: 45.0,
        znear: 1.0,
        zfar: 100.0,
    };
    let mut camera_uniform = camera::CameraUniform::new();
    camera_uniform.update_view_proj(&camera);

    let mut world_state = world::WorldState::new();
    world_state.initial_setup();

    let scene = setup_scene(
        &config,
        &adapter,
        &device,
        &queue,
        camera_uniform,
        &world_state,
    );

    let mut instance_lens = [scene.instance_data[0].len(), scene.instance_data[1].len()];

    let mut curr_modifier_state: winit::event::ModifiersState =
        winit::event::ModifiersState::empty();
    let mut cursor_grabbed = false;
    let mut mouse_clicked = false;

    let spawner = Spawner::new();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        match event {
            Event::WindowEvent { event, window_id } => match event {
                WindowEvent::CloseRequested => {
                    if window_id == window.id() {
                        *control_flow = ControlFlow::Exit;
                    }
                }
                WindowEvent::ModifiersChanged(modifiers) => {
                    curr_modifier_state = modifiers;
                }
                WindowEvent::KeyboardInput { input, .. } => {
                    match (input.virtual_keycode, input.state) {
                        (Some(VirtualKeyCode::W), ElementState::Pressed) => {
                            if curr_modifier_state.logo() {
                                *control_flow = ControlFlow::Exit;
                                return;
                            }
                            camera_controller.process_window_event(&event);
                        }
                        _ => {
                            camera_controller.process_window_event(&event);
                        }
                    }
                }
                WindowEvent::CursorMoved { .. } => {
                    if !cursor_grabbed {
                        window.set_cursor_grab(true).expect("Failed to grab curosr");
                        window.set_cursor_visible(false);
                        cursor_grabbed = true;
                    }
                }
                WindowEvent::MouseInput { state, button, .. } => match (state, button) {
                    (ElementState::Pressed, MouseButton::Left) => {
                        println!("Left mouse clicked");
                        mouse_clicked = true;
                    }
                    _ => (),
                },
                _ => (),
            },

            Event::DeviceEvent { event, .. } => match event {
                DeviceEvent::MouseMotion { .. } => {
                    if cursor_grabbed {
                        camera_controller.process_device_event(&event);
                    }
                }
                _ => (),
            },

            Event::RedrawRequested(_) => {
                let frame = match surface.get_current_texture() {
                    Ok(frame) => frame,
                    Err(_) => {
                        surface.configure(&device, &config);
                        surface
                            .get_current_texture()
                            .expect("Failed to acquire next surface texture!")
                    }
                };
                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                camera_controller.update_camera(&mut camera);
                camera_uniform.update_view_proj(&camera);
                queue.write_buffer(
                    &scene.camera_staging_buf,
                    0,
                    bytemuck::cast_slice(&[camera_uniform]),
                );

                // Break a block with the camera!
                if mouse_clicked {
                    mouse_clicked = false;
                    world_state.break_block(&camera);

                    let (grass_instances, dirt_instances, grass_instance_data, dirt_instance_data) =
                        world_state.generate_vertex_data();
                    queue.write_buffer(
                        &scene.instance_buffers[0],
                        0,
                        bytemuck::cast_slice(&grass_instance_data),
                    );
                    queue.write_buffer(
                        &scene.instance_buffers[1],
                        0,
                        bytemuck::cast_slice(&dirt_instance_data),
                    );
                    queue.write_buffer(
                        &scene.camera_ray_buffer,
                        0,
                        bytemuck::cast_slice(&[
                            vertex::Vertex::new_from_pos(camera.eye.into()),
                            vertex::Vertex::new_from_pos(camera.target.into()),
                            vertex::Vertex::new_from_pos(camera.eye.into()),
                        ]),
                    );

                    instance_lens = [grass_instances.len(), dirt_instances.len()];
                }

                render_scene(&view, &device, &queue, &scene, &spawner, instance_lens);

                frame.present();
                camera_controller.reset_mouse_delta();
            }

            Event::MainEventsCleared => {
                // RedrawRequested will only trigger once, unless we manually
                // request it.
                window.request_redraw();

                #[cfg(not(target_arch = "wasm32"))]
                spawner.run_until_stalled();
            }

            _ => (),
        }
    });
}

fn setup_scene(
    config: &wgpu::SurfaceConfiguration,
    _adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    camera_uniform: camera::CameraUniform,
    world_state: &world::WorldState,
) -> Scene {
    let vertex_size = mem::size_of::<vertex::Vertex>();

    let grass_block = cube::Cube::new_grass_block();
    let dirt_block = cube::Cube::new_dirt_block();

    let vertex_buffers = [
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Grass Vertex Buffer"),
            contents: bytemuck::cast_slice(&grass_block.vertex_data),
            usage: wgpu::BufferUsages::VERTEX,
        }),
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Dirt Vertex Buffer"),
            contents: bytemuck::cast_slice(&dirt_block.vertex_data),
            usage: wgpu::BufferUsages::VERTEX,
        }),
    ];
    let camera_ray_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Camera Ray Buffer"),
        contents: bytemuck::cast_slice(&[
            vertex::Vertex::new_from_pos([0.0, 0.0, 0.0]),
            vertex::Vertex::new_from_pos([10.0, 10.0, 10.0]),
            vertex::Vertex::new_from_pos([10.0, 0.0, 10.0]),
        ]),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    });
    let camera_ray_index_data: &[u16] = &[0, 1, 0];
    let camera_ray_index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Camera Ray Index Buffer"),
        contents: bytemuck::cast_slice(&camera_ray_index_data),
        usage: wgpu::BufferUsages::INDEX,
    });

    let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Block Index Buffer"),
        contents: bytemuck::cast_slice(&grass_block.index_data),
        usage: wgpu::BufferUsages::INDEX,
    });

    // Create the texture
    let texture_atlas_bytes = include_bytes!("../assets/minecruft_atlas.png");
    let texture_atlas_bytes = image::load_from_memory(texture_atlas_bytes).unwrap();
    let texture_atlas_rgba = texture_atlas_bytes.to_rgba8();
    let dimensions = texture_atlas_rgba.dimensions();

    let texture_extent = wgpu::Extent3d {
        width: dimensions.0,
        height: dimensions.1,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: texture_extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    });
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &texture_atlas_rgba,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: std::num::NonZeroU32::new(4 * dimensions.0),
            rows_per_image: std::num::NonZeroU32::new(dimensions.1),
        },
        texture_extent,
    );

    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });

    // Create pipeline layout
    let texture_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    // This should match the filterable field of the
                    // corresponding Texture entry above.
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
    let camera_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(64),
                },
                count: None,
            }],
        });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&texture_bind_group_layout, &camera_bind_group_layout],
        push_constant_ranges: &[],
    });

    // Camera
    let camera_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Camera Buffer"),
        contents: bytemuck::cast_slice(&[camera_uniform]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let camera_staging_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Camera Staging Buffer"),
        contents: bytemuck::cast_slice(&[camera_uniform]),
        usage: wgpu::BufferUsages::UNIFORM
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
    });

    // Create bind groups
    let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &texture_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
        label: None,
    });
    let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &camera_bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: camera_buf.as_entire_binding(),
        }],
        label: None,
    });

    let shader = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
        label: Some("Main Shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shader.wgsl"))),
    });

    let vertex_buffer_layout = wgpu::VertexBufferLayout {
        array_stride: vertex_size as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            // position
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 0,
                shader_location: 0,
            },
            // tex_coord
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 4 * 4, // TODO(aleks): use mem to get compute size at compile time
                shader_location: 1,
            },
            // atlas_offset (sprite offset)
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: (4 * 4) + (2 * 4), // TODO(aleks): use mem to get compute size at compile time
                shader_location: 2,
            },
        ],
    };

    let (grass_instances, dirt_instances, grass_instance_data, dirt_instance_data) =
        world_state.generate_vertex_data();

    let instance_buffers = [
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Grass Instance Buffer"),
            contents: bytemuck::cast_slice(&grass_instance_data),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        }),
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Dirt Instance Buffer"),
            contents: bytemuck::cast_slice(&dirt_instance_data),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        }),
    ];

    let depth_texture = texture::Texture::create_depth_texture(&device, &config, "depth_texture");

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[vertex_buffer_layout.clone(), lib::InstanceRaw::desc()],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[config.format.into()],
        }),
        primitive: wgpu::PrimitiveState {
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: texture::Texture::DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    let pipeline_lines = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_line",
            buffers: &[vertex_buffer_layout.clone()],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_wire",
            targets: &[wgpu::ColorTargetState {
                format: config.format,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        operation: wgpu::BlendOperation::Add,
                        src_factor: wgpu::BlendFactor::SrcAlpha,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    },
                    alpha: wgpu::BlendComponent::REPLACE,
                }),
                write_mask: wgpu::ColorWrites::ALL,
            }],
        }),
        primitive: wgpu::PrimitiveState {
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            polygon_mode: wgpu::PolygonMode::Line,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: texture::Texture::DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    let pipeline_wire = if device
        .features()
        .contains(wgpu::Features::POLYGON_MODE_LINE)
    {
        let pipeline_wire = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[vertex_buffer_layout.clone(), lib::InstanceRaw::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_wire",
                targets: &[wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            operation: wgpu::BlendOperation::Add,
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        },
                        alpha: wgpu::BlendComponent::REPLACE,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                }],
            }),
            primitive: wgpu::PrimitiveState {
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Line,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: texture::Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        Some(pipeline_wire)
    } else {
        None
    };

    Scene {
        vertex_buffers,
        index_buf,
        index_count: grass_block.index_data.len(),
        texture_bind_group,
        camera_bind_group,
        camera_buf,
        camera_staging_buf,
        instance_data: [grass_instances, dirt_instances],
        instance_buffers,
        depth_texture,
        pipeline,

        camera_ray_buffer,
        camera_ray_index_buf,
        camera_ray_index_count: camera_ray_index_data.len(),
        pipeline_lines,

        pipeline_wire,
    }
}

fn render_scene(
    view: &wgpu::TextureView,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &Scene,
    spawner: &Spawner,
    instance_lens: [usize; 2],
) {
    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    encoder.copy_buffer_to_buffer(
        &scene.camera_staging_buf,
        0,
        &scene.camera_buf,
        0,
        mem::size_of::<camera::CameraUniform>().try_into().unwrap(),
    );
    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.1,
                        g: 0.2,
                        b: 0.3,
                        a: 1.0,
                    }),
                    store: true,
                },
            }],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &scene.depth_texture.view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: true,
                }),
                stencil_ops: None,
            }),
        });
        rpass.set_pipeline(&scene.pipeline);
        rpass.set_bind_group(0, &scene.texture_bind_group, &[]);
        rpass.set_bind_group(1, &scene.camera_bind_group, &[]);
        rpass.set_index_buffer(scene.index_buf.slice(..), wgpu::IndexFormat::Uint16);

        // Draw grass blocks
        rpass.set_vertex_buffer(0, scene.vertex_buffers[0].slice(..));
        rpass.set_vertex_buffer(1, scene.instance_buffers[0].slice(..));
        rpass.draw_indexed(0..scene.index_count as u32, 0, 0..instance_lens[0] as _);

        if let Some(ref pipe) = &scene.pipeline_wire {
            rpass.set_pipeline(pipe);
            rpass.draw_indexed(0..scene.index_count as u32, 0, 0..instance_lens[0] as _);
            rpass.set_pipeline(&scene.pipeline);
        }

        // Draw dirt blocks
        rpass.set_vertex_buffer(0, scene.vertex_buffers[1].slice(..));
        rpass.set_vertex_buffer(1, scene.instance_buffers[1].slice(..));
        rpass.draw_indexed(0..scene.index_count as u32, 0, 0..instance_lens[1] as _);

        if let Some(ref pipe) = &scene.pipeline_wire {
            rpass.set_pipeline(pipe);
            rpass.draw_indexed(0..scene.index_count as u32, 0, 0..instance_lens[1] as _);
            rpass.set_pipeline(&scene.pipeline);
        }

        // Draw lines
        rpass.set_pipeline(&scene.pipeline_lines);
        rpass.set_index_buffer(
            scene.camera_ray_index_buf.slice(..),
            wgpu::IndexFormat::Uint16,
        );
        rpass.set_vertex_buffer(0, scene.camera_ray_buffer.slice(..));
        rpass.draw_indexed(0..scene.camera_ray_index_count as u32, 0, 0..1 as _);
    }

    queue.submit(Some(encoder.finish()));

    // If an error occurs, report it and panic.
    spawner.spawn_local(ErrorFuture {
        inner: device.pop_error_scope(),
    });
}

/// A wrapper for `pop_error_scope` futures that panics if an error occurs.
///
/// Given a future `inner` of an `Option<E>` for some error type `E`,
/// wait for the future to be ready, and panic if its value is `Some`.
///
/// This can be done simpler with `FutureExt`, but we don't want to add
/// a dependency just for this small case.
struct ErrorFuture<F> {
    inner: F,
}
impl<F: Future<Output = Option<wgpu::Error>>> Future for ErrorFuture<F> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<()> {
        let inner = unsafe { self.map_unchecked_mut(|me| &mut me.inner) };
        inner.poll(cx).map(|error| {
            if let Some(e) = error {
                panic!("Rendering {}", e);
            }
        })
    }
}
