#[macro_use]
extern crate itertools;
#[macro_use]
extern crate bmp;

mod camera;
mod face;
mod instance;
mod light;
mod map_generation;
// mod pipeline;
mod spawner;
mod texture;
mod vec_extra;
mod vertex;
mod world;

use cgmath::{prelude::*, Point3};
use futures::executor::block_on;
use itertools::Itertools;
use spawner::Spawner;
use std::{borrow::Cow, collections::HashSet, future::Future, mem, pin::Pin, task};
use vertex::QuadListRenderData;
use wgpu::util::DeviceExt;
use winit::{
    event::{DeviceEvent, ElementState, Event, MouseButton, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
};
use world::{CHUNK_XZ_SIZE, CHUNK_Y_SIZE, VISIBLE_CHUNK_WIDTH};

use crate::world::{Chunk, ChunkDataType, MAX_CHUNK_WORLD_WIDTH};

const VERBOSE_LOGS: bool = false;

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

struct AnnotatedInstanceBuffer {
    buffer: wgpu::Buffer,
    len: usize,
    data_type: world::ChunkDataType,
}
struct ChunkRenderDescriptor {
    world_chunk_position: [usize; 2],
    annotated_instance_buffers: Vec<AnnotatedInstanceBuffer>,
}

struct Scene {
    vertex_buffers: [wgpu::Buffer; 2],
    index_buffers: [wgpu::Buffer; 2],
    index_counts: [usize; 2],
    texture_bind_group: wgpu::BindGroup,
    camera_bind_group: wgpu::BindGroup,
    light_bind_group: wgpu::BindGroup,
    camera_buf: wgpu::Buffer,
    camera_staging_buf: wgpu::Buffer,
    light_buf: wgpu::Buffer,
    chunk_render_descriptors: Vec<ChunkRenderDescriptor>,
    chunk_order: Vec<[usize; 2]>,
    depth_texture: texture::Texture,
    pipeline: wgpu::RenderPipeline,

    shadow_map_texture: texture::Texture,
    shadow_map_pipeline: wgpu::RenderPipeline,

    pipeline_wire: Option<wgpu::RenderPipeline>,
    pipeline_wire_no_instancing: Option<wgpu::RenderPipeline>,
}

async fn setup() -> Setup {
    let event_loop = EventLoop::new();
    let mut builder = winit::window::WindowBuilder::new();
    builder = builder.with_title("Minecrust");
    builder = builder.with_inner_size(winit::dpi::LogicalSize {
        width: 1024,
        height: 1024,
    });
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
    let supported_formats = surface.get_supported_formats(&adapter);
    assert!(supported_formats.contains(&wgpu::TextureFormat::Bgra8Unorm));

    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Bgra8Unorm,
        width: size.width,
        height: size.height,
        present_mode: wgpu::PresentMode::Fifo,
    };
    surface.configure(&device, &config);

    let mut camera_controller = camera::CameraController::new(0.15, 0.01);

    // Start in the center
    let center = world::get_world_center();
    let zfar = 250.0;
    let mut camera = camera::Camera::new(
        Point3::<f32>::new(center.x as f32, center.y as f32, center.z as f32),
        // have it look at the origin
        (0.0, 0.0, 0.0).into(),
        // which way is "up"
        cgmath::Vector3::unit_y(),
        cgmath::Vector3::unit_y(),
        config.width as f32 / config.height as f32,
        70.0,
        0.1,
        zfar,
    );

    let scale_factor = 1.0;
    let sunlight_ortho_proj_coords = vertex::CuboidCoords {
        left: -(CHUNK_XZ_SIZE as f32 * scale_factor),
        right: CHUNK_XZ_SIZE as f32 * scale_factor,
        bottom: -(CHUNK_XZ_SIZE as f32 * scale_factor),
        top: CHUNK_XZ_SIZE as f32 * scale_factor,
        near: 0.0, // -(CHUNK_XZ_SIZE as f32 * scale_factor),
        far: CHUNK_XZ_SIZE as f32 * scale_factor * 8.0,
    };

    // Light
    let mut light_uniform = light::LightUniform::new(
        [0.0, 5.0, 0.0].into(),
        [1.0, 1.0, 1.0].into(),
        [40.0, 30.0, 40.0].into(),
        sunlight_ortho_proj_coords,
    );

    let mut camera_uniform = camera::CameraUniform::new();
    camera_uniform.update_view_proj(&camera);

    let mut world_state = world::WorldState::new();
    world_state.initial_setup(&camera);

    let mut scene = setup_scene(
        &config,
        &adapter,
        &device,
        &queue,
        camera_uniform,
        &light_uniform,
        &mut world_state,
        &camera,
    );

    let mut curr_modifier_state: winit::event::ModifiersState =
        winit::event::ModifiersState::empty();
    let mut cursor_grabbed = false;

    let mut left_mouse_clicked = false;
    let mut right_mouse_clicked = false;

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
                        (Some(VirtualKeyCode::Escape), ElementState::Pressed) => {
                            window.set_cursor_visible(true);
                            window
                                .set_cursor_grab(false)
                                .expect("Failed to release curosr");
                            cursor_grabbed = false;
                        }
                        _ => {
                            camera_controller.process_window_event(&event);
                        }
                    }
                }
                WindowEvent::MouseInput { state, button, .. } => match (state, button) {
                    (ElementState::Pressed, MouseButton::Left) => {
                        if !cursor_grabbed {
                            window.set_cursor_grab(true).expect("Failed to grab curosr");
                            window.set_cursor_visible(false);
                            cursor_grabbed = true;
                        } else {
                            left_mouse_clicked = true;
                        }
                    }
                    (ElementState::Pressed, MouseButton::Right) => {
                        right_mouse_clicked = true;
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

                let update_result = camera_controller.update_camera(&mut camera);
                camera_uniform.update_view_proj(&camera);
                queue.write_buffer(
                    &scene.camera_staging_buf,
                    0,
                    bytemuck::cast_slice(&[camera_uniform]),
                );

                light_uniform.update_light_space_proj(&camera);
                queue.write_buffer(
                    &scene.light_buf,
                    0,
                    bytemuck::cast_slice(&[light_uniform.to_raw()]),
                );

                #[derive(PartialEq)]
                struct ChunkModification {
                    new_chunk: [usize; 2],
                    old_chunk: [usize; 2],
                }
                let mut chunk_mods: Vec<ChunkModification> = vec![];

                if update_result.did_move {
                    let chunks_modified = world_state.highlight_colliding_block(&camera);
                    for chunk_idx in chunks_modified {
                        chunk_mods.push(ChunkModification {
                            new_chunk: chunk_idx,
                            old_chunk: chunk_idx,
                        });
                    }

                    let sunlight_vtx_data = light_uniform.vertex_data_for_sunlight();
                    queue.write_buffer(
                        &scene.vertex_buffers[1],
                        0,
                        bytemuck::cast_slice(&sunlight_vtx_data.vertex_data),
                    );
                }

                if update_result.did_move_blocks {
                    let chunk_idx = update_result.new_chunk_location;
                    chunk_mods.push(ChunkModification {
                        new_chunk: chunk_idx,
                        old_chunk: chunk_idx,
                    });
                }

                // Break a block with the camera!
                if left_mouse_clicked || right_mouse_clicked {
                    let chunks_modified = if right_mouse_clicked {
                        world_state.place_block(&camera, world::BlockType::Sand)
                    } else {
                        world_state.break_block(&camera)
                    };
                    left_mouse_clicked = false;
                    right_mouse_clicked = false;

                    for chunk_idx in chunks_modified {
                        chunk_mods.push(ChunkModification {
                            new_chunk: chunk_idx,
                            old_chunk: chunk_idx,
                        });
                    }

                    if !update_result.did_move {
                        let chunks_modified = world_state.highlight_colliding_block(&camera);
                        for chunk_idx in chunks_modified {
                            chunk_mods.push(ChunkModification {
                                new_chunk: chunk_idx,
                                old_chunk: chunk_idx,
                            });
                        }
                    }
                }

                if update_result.did_move_chunks {
                    let new_chunk_order = world_state.get_chunk_order_by_distance(&camera);

                    let new_chunk_order_hashset =
                        new_chunk_order.iter().cloned().collect::<HashSet<_>>();
                    let old_chunk_order_hashset =
                        scene.chunk_order.iter().cloned().collect::<HashSet<_>>();

                    let new_chunks = (&new_chunk_order_hashset - &old_chunk_order_hashset)
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>();
                    let old_chunks = (&old_chunk_order_hashset - &new_chunk_order_hashset)
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>();
                    let neighbors_to_new_chunks =
                        world_state.find_chunk_neighbors(&new_chunks, &scene.chunk_order);

                    for chunk in neighbors_to_new_chunks {
                        chunk_mods.push(ChunkModification {
                            new_chunk: chunk,
                            old_chunk: chunk,
                        })
                    }
                    for (new_chunk, old_chunk) in izip!(new_chunks, old_chunks) {
                        chunk_mods.push(ChunkModification {
                            new_chunk,
                            old_chunk,
                        });
                    }

                    scene.chunk_order = new_chunk_order;
                }

                if !chunk_mods.is_empty() {
                    let chunk_mod_time = std::time::Instant::now();

                    chunk_mods.dedup();

                    for chunk_mod in chunk_mods.iter() {
                        world_state.maybe_allocate_chunk(chunk_mod.new_chunk);
                    }
                    if VERBOSE_LOGS && update_result.did_move_chunks {
                        println!(
                            "Took {}ms to allocate chunks",
                            chunk_mod_time.elapsed().as_millis()
                        );
                    }

                    let new_chunk_datas = chunk_mods
                        .iter()
                        .map(|chunk_mod| {
                            let new_chunk_data =
                                world_state.generate_chunk_data(chunk_mod.new_chunk, &camera);

                            let render_descriptor_idx =
                                world_state.get_render_descriptor_idx(chunk_mod.old_chunk);
                            if chunk_mod.new_chunk != chunk_mod.old_chunk {
                                world_state.set_render_descriptor_idx(
                                    chunk_mod.old_chunk,
                                    world::NO_RENDER_DESCRIPTOR_INDEX,
                                );
                                world_state.set_render_descriptor_idx(
                                    chunk_mod.new_chunk,
                                    render_descriptor_idx,
                                );
                            }

                            (new_chunk_data, render_descriptor_idx)
                        })
                        .collect::<Vec<_>>();

                    if VERBOSE_LOGS && update_result.did_move_chunks {
                        println!(
                            "Took {}ms to update chunks",
                            chunk_mod_time.elapsed().as_millis()
                        );
                    }

                    for (new_chunk_data, render_descriptor_idx) in new_chunk_datas.into_iter() {
                        let chunk_render_descriptor =
                            &mut scene.chunk_render_descriptors[render_descriptor_idx];

                        for typed_instances in new_chunk_data.typed_instances_vec.iter() {
                            let maybe_instance_buffer = chunk_render_descriptor
                                .annotated_instance_buffers
                                .iter_mut()
                                .find(|ib| ib.data_type == typed_instances.data_type);

                            if let Some(instance_buffer) = maybe_instance_buffer {
                                queue.write_buffer(
                                    &instance_buffer.buffer,
                                    0,
                                    bytemuck::cast_slice(&typed_instances.instance_data),
                                );
                                instance_buffer.len = typed_instances.instance_data.len();
                            }
                        }
                    }
                }

                render_scene(&view, &device, &queue, &scene, &world_state, &spawner);

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
    light_uniform: &light::LightUniform,
    world_state: &mut world::WorldState,
    camera: &camera::Camera,
) -> Scene {
    let texture_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                // Texture Atlas
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
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Shadow Map
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
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
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        mem::size_of::<camera::CameraUniform>() as u64,
                    ),
                },
                count: None,
            }],
        });
    let light_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        mem::size_of::<light::LightUniformRaw>() as u64,
                    ),
                },
                count: None,
            }],
        });

    let vertex_buffer_layouts = &[vertex::Vertex::desc(), instance::InstanceRaw::desc()];

    let shadow_map_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Shadow Map Shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shadow_map.wgsl"))),
    });

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Main Shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shader.wgsl"))),
    });

    let shadow_map_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&camera_bind_group_layout, &light_bind_group_layout],
                push_constant_ranges: &[],
            }),
        ),
        vertex: wgpu::VertexState {
            module: &shadow_map_shader,
            entry_point: "vs_main",
            buffers: vertex_buffer_layouts,
        },
        fragment: Some(wgpu::FragmentState {
            module: &shadow_map_shader,
            entry_point: "fs_main",
            targets: &[],
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

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[
            &texture_bind_group_layout,
            &camera_bind_group_layout,
            &light_bind_group_layout,
        ],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: vertex_buffer_layouts,
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(wgpu::ColorTargetState {
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
            })],
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

    let create_wire_pipeline = |vtx_shader_entry_point: &str, cull_mode: Option<wgpu::Face>| {
        if device
            .features()
            .contains(wgpu::Features::POLYGON_MODE_LINE)
        {
            let pipeline_wire = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: vtx_shader_entry_point,
                    buffers: vertex_buffer_layouts,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: "fs_wire",
                    targets: &[Some(wgpu::ColorTargetState {
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
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode,
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
        }
    };

    let pipeline_wire = create_wire_pipeline("vs_main", Some(wgpu::Face::Back));
    let pipeline_wire_no_instancing = create_wire_pipeline("vs_wire_no_instancing", None);

    let light_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Light VB"),
        contents: bytemuck::cast_slice(&[light_uniform.to_raw()]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let face = face::Face::new();

    let sunlight_vtx_data = light_uniform.vertex_data_for_sunlight();

    let vertex_buffers = [
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Main Vertex Buffer"),
            contents: bytemuck::cast_slice(&face.vertex_data),
            usage: wgpu::BufferUsages::VERTEX,
        }),
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Light Volume Vertex Buffer"),
            contents: bytemuck::cast_slice(&sunlight_vtx_data.vertex_data),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        }),
    ];

    let index_buffers = [
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Main Index Buffer"),
            contents: bytemuck::cast_slice(&face.index_data),
            usage: wgpu::BufferUsages::INDEX,
        }),
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Light Volume Index Buffer"),
            contents: bytemuck::cast_slice(&sunlight_vtx_data.index_data),
            usage: wgpu::BufferUsages::INDEX,
        }),
    ];

    let index_counts = [face.index_data.len(), sunlight_vtx_data.index_data.len()];

    let texture_atlas = texture::Texture::create_pixel_art_image_texture(
        include_bytes!("../assets/minecruft_atlas.png"),
        device,
        queue,
        config,
        "Texture Atlas",
    );

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

    // Shadow Map
    let shadow_map_texture =
        texture::Texture::create_depth_texture("shadow_map_texture", &device, [1024, 1024], None);

    // Create bind groups
    let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &texture_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture_atlas.view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&texture_atlas.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&shadow_map_texture.view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&shadow_map_texture.sampler),
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
    let light_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &light_bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: light_buf.as_entire_binding(),
        }],
        label: None,
    });

    let (all_chunk_data, chunk_order) = world_state.generate_world_data(&camera);
    let chunk_dims = all_chunk_data.dims();

    let mut chunk_render_descriptors: Vec<ChunkRenderDescriptor> = vec![];

    for (chunk_x, chunk_z) in iproduct!(0..chunk_dims[0], 0..chunk_dims[1]) {
        let chunk_data = &all_chunk_data[[chunk_x, chunk_z]];

        let mut annotated_instance_buffers: Vec<AnnotatedInstanceBuffer> = vec![];
        for typed_instances in &chunk_data.typed_instances_vec {
            let instance_byte_contents: &[u8] =
                bytemuck::cast_slice(&typed_instances.instance_data);

            const NUM_FACES: usize = 6;

            // Divide by 2 since worst case is a "3D checkerboard" where every other space is filled
            let mut max_number_faces_possible =
                world::NUM_BLOCKS_IN_CHUNK * instance::InstanceRaw::size() * NUM_FACES / 2;
            // HACK(aleks) divide by 16 because too much memory
            max_number_faces_possible /= 16;

            let unpadded_size: u64 = max_number_faces_possible.try_into().unwrap();

            // Valid vulkan usage is
            // 1. buffer size must be a multiple of COPY_BUFFER_ALIGNMENT.
            // 2. buffer size must be greater than 0.
            // Therefore we round the value up to the nearest multiple, and ensure it's at least COPY_BUFFER_ALIGNMENT.
            let align_mask = wgpu::COPY_BUFFER_ALIGNMENT - 1;
            let padded_size =
                ((unpadded_size + align_mask) & !align_mask).max(wgpu::COPY_BUFFER_ALIGNMENT);

            let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&*format!(
                    "Instance Buffer {:?} {},{}",
                    typed_instances.data_type, chunk_x, chunk_z
                )),
                size: padded_size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: true,
            });

            instance_buffer.slice(..).get_mapped_range_mut()
                [..instance_byte_contents.len() as usize]
                .copy_from_slice(instance_byte_contents);
            instance_buffer.unmap();

            annotated_instance_buffers.push(AnnotatedInstanceBuffer {
                buffer: instance_buffer,
                len: typed_instances.instance_data.len(),
                data_type: typed_instances.data_type.clone(),
            });
        }

        chunk_render_descriptors.push(ChunkRenderDescriptor {
            world_chunk_position: chunk_data.position,
            annotated_instance_buffers,
        });
        let render_descriptor_idx = chunk_render_descriptors.len() - 1;
        world_state.set_render_descriptor_idx(chunk_data.position, render_descriptor_idx);
    }

    let depth_texture = texture::Texture::create_depth_texture(
        "depth_texture",
        &device,
        [config.width, config.height],
        Some(wgpu::CompareFunction::LessEqual),
    );

    Scene {
        vertex_buffers,
        index_buffers,
        index_counts,
        texture_bind_group,
        camera_bind_group,
        light_bind_group,
        camera_buf,
        camera_staging_buf,
        light_buf,
        chunk_render_descriptors,
        chunk_order,
        depth_texture,
        pipeline,

        shadow_map_pipeline,
        shadow_map_texture,

        pipeline_wire,
        pipeline_wire_no_instancing,
    }
}

fn render_scene(
    view: &wgpu::TextureView,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &Scene,
    world_state: &world::WorldState,
    spawner: &Spawner,
) {
    static RENDER_WIREFRAME: bool = false;

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
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &scene.shadow_map_texture.view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: true,
                }),
                stencil_ops: None,
            }),
        });

        rpass.set_pipeline(&scene.shadow_map_pipeline);
        rpass.set_bind_group(0, &scene.camera_bind_group, &[]);
        rpass.set_bind_group(1, &scene.light_bind_group, &[]);
        rpass.set_vertex_buffer(0, scene.vertex_buffers[0].slice(..));
        rpass.set_index_buffer(scene.index_buffers[0].slice(..), wgpu::IndexFormat::Uint16);

        for data_type in [ChunkDataType::Opaque] {
            for [chunk_x, chunk_z] in scene.chunk_order.iter().rev() {
                let render_descriptor_idx =
                    world_state.get_render_descriptor_idx([*chunk_x, *chunk_z]);
                let chunk_render_datum = &scene.chunk_render_descriptors[render_descriptor_idx];

                let maybe_instance_buffer = chunk_render_datum
                    .annotated_instance_buffers
                    .iter()
                    .find(|&ib| ib.data_type == data_type);

                if let Some(ref instance_buffer) = maybe_instance_buffer {
                    rpass.set_vertex_buffer(1, instance_buffer.buffer.slice(..));
                    rpass.draw_indexed(
                        0..scene.index_counts[0] as u32,
                        0,
                        0..instance_buffer.len as _,
                    );

                    if RENDER_WIREFRAME {
                        if let Some(ref pipe) = &scene.pipeline_wire {
                            rpass.set_pipeline(pipe);
                            rpass.draw_indexed(
                                0..scene.index_counts[0] as u32,
                                0,
                                0..instance_buffer.len as _,
                            );

                            rpass.set_pipeline(&scene.pipeline);
                        }
                    }
                }
            }
        }
    }

    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 120.0 / 255.0,
                        g: 167.0 / 255.0,
                        b: 255.0 / 255.0,
                        a: 1.0,
                    }),
                    store: true,
                },
            })],
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
        rpass.set_bind_group(2, &scene.light_bind_group, &[]);
        rpass.set_vertex_buffer(0, scene.vertex_buffers[0].slice(..));
        rpass.set_index_buffer(scene.index_buffers[0].slice(..), wgpu::IndexFormat::Uint16);

        for data_type in [ChunkDataType::Opaque, ChunkDataType::Transluscent] {
            for [chunk_x, chunk_z] in scene.chunk_order.iter().rev() {
                let render_descriptor_idx =
                    world_state.get_render_descriptor_idx([*chunk_x, *chunk_z]);
                let chunk_render_datum = &scene.chunk_render_descriptors[render_descriptor_idx];

                let maybe_instance_buffer = chunk_render_datum
                    .annotated_instance_buffers
                    .iter()
                    .find(|&ib| ib.data_type == data_type);

                if let Some(ref instance_buffer) = maybe_instance_buffer {
                    rpass.set_vertex_buffer(1, instance_buffer.buffer.slice(..));
                    rpass.draw_indexed(
                        0..scene.index_counts[0] as u32,
                        0,
                        0..instance_buffer.len as _,
                    );

                    if RENDER_WIREFRAME {
                        if let Some(ref pipe) = &scene.pipeline_wire {
                            rpass.set_pipeline(pipe);
                            rpass.draw_indexed(
                                0..scene.index_counts[0] as u32,
                                0,
                                0..instance_buffer.len as _,
                            );

                            rpass.set_pipeline(&scene.pipeline);
                        }
                    }
                }
            }
        }

        // Draw light volume wireframe
        if let Some(ref pipe) = &scene.pipeline_wire_no_instancing {
            rpass.set_pipeline(pipe);
            rpass.set_vertex_buffer(0, scene.vertex_buffers[1].slice(..));
            rpass.set_index_buffer(scene.index_buffers[1].slice(..), wgpu::IndexFormat::Uint16);
            rpass.draw_indexed(0..scene.index_counts[1] as u32, 0, 0..1);

            rpass.set_pipeline(&scene.pipeline);
        }
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
