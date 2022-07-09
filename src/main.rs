#[macro_use]
extern crate itertools;
#[macro_use]
extern crate bmp;

mod camera;
mod face;
mod instance;
mod map_generation;
mod spawner;
mod texture;
mod vec_extra;
mod vertex;
mod world;

use cgmath::{prelude::*, Point3};
use futures::executor::block_on;
use itertools::Itertools;
use spawner::Spawner;
use std::{borrow::Cow, future::Future, mem, pin::Pin, task};
use vec_extra::Vec2d;
use wgpu::util::DeviceExt;
use winit::{
    event::{DeviceEvent, ElementState, Event, MouseButton, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
};

use crate::world::{ChunkDataType, MAX_CHUNK_WORLD_WIDTH};

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
    position: [usize; 2],
    annotated_instance_buffers: Vec<AnnotatedInstanceBuffer>,
}

struct Scene {
    vertex_buffers: [wgpu::Buffer; 2],
    index_buf: wgpu::Buffer,
    line_index_buf: wgpu::Buffer,
    index_count: usize,
    texture_bind_group: wgpu::BindGroup,
    camera_bind_group: wgpu::BindGroup,
    camera_buf: wgpu::Buffer,
    camera_staging_buf: wgpu::Buffer,
    chunk_render_data: Vec2d<ChunkRenderDescriptor>,
    chunk_order: Vec<[usize; 2]>,
    depth_texture: texture::Texture,
    pipeline: wgpu::RenderPipeline,
    pipeline_wire: Option<wgpu::RenderPipeline>,
}

async fn setup() -> Setup {
    let event_loop = EventLoop::new();
    let mut builder = winit::window::WindowBuilder::new();
    builder = builder.with_title("Minecrust");
    builder = builder.with_inner_size(winit::dpi::LogicalSize {
        width: 1200,
        height: 800,
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
    let mut camera = camera::Camera::new(
        Point3::<f32>::new(center.x as f32, center.y as f32, center.z as f32),
        // have it look at the origin
        (0.0, 0.0, 0.0).into(),
        // which way is "up"
        cgmath::Vector3::unit_y(),
        cgmath::Vector3::unit_y(),
        config.width as f32 / config.height as f32,
        70.0,
        1.0,
        150.0,
    );

    let mut camera_uniform = camera::CameraUniform::new();
    camera_uniform.update_view_proj(&camera);

    let mut world_state = world::WorldState::new();
    world_state.initial_setup();

    let mut scene = setup_scene(
        &config,
        &adapter,
        &device,
        &queue,
        camera_uniform,
        &world_state,
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

                let mut chunks_modified: Vec<[usize; 2]> = vec![];
                if update_result.did_move_blocks {
                    let [chunk_x, chunk_z] = update_result.new_chunk_location;
                    chunks_modified.push([chunk_x, chunk_z]);
                }

                if update_result.did_move_chunks {
                    scene.chunk_order = world_state.get_chunk_order_by_distance(&camera);
                }

                // Break a block with the camera!
                if left_mouse_clicked || right_mouse_clicked {
                    let construction_chunks_modified = if right_mouse_clicked {
                        world_state.place_block(&camera, world::BlockType::Sand)
                    } else {
                        world_state.break_block(&camera)
                    };
                    left_mouse_clicked = false;
                    right_mouse_clicked = false;

                    chunks_modified.extend(construction_chunks_modified.iter());
                    chunks_modified = chunks_modified.into_iter().unique().collect();

                    let forward = (camera.target - camera.eye).normalize();
                    let horizon_target = camera.target + (forward * 100.0);
                    queue.write_buffer(
                        &scene.vertex_buffers[1],
                        0,
                        bytemuck::cast_slice(&[
                            vertex::Vertex::new_from_pos(camera.eye.into()),
                            vertex::Vertex::new_from_pos(horizon_target.into()),
                            vertex::Vertex::new_from_pos([0.0, 0.0, 0.0]),
                        ]),
                    );
                }

                if !chunks_modified.is_empty() {
                    for chunk_idx in chunks_modified {
                        let chunk_data = world_state.generate_chunk_data(chunk_idx, &camera);
                        let chunk_render_datum = &mut scene.chunk_render_data[chunk_idx];

                        for typed_instances in chunk_data.typed_instances_vec.iter() {
                            let maybe_instance_buffer = chunk_render_datum
                                .annotated_instance_buffers
                                .iter_mut()
                                .find(|ib| ib.data_type == typed_instances.data_type);

                            if let Some(mut instance_buffer) = maybe_instance_buffer {
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

                render_scene(&view, &device, &queue, &scene, &spawner);

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
    camera: &camera::Camera,
) -> Scene {
    let vertex_size = mem::size_of::<vertex::Vertex>();

    let face = face::Face::new();

    let vertex_buffers = [
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&face.vertex_data),
            usage: wgpu::BufferUsages::VERTEX,
        }),
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Line Buffer"),
            contents: bytemuck::cast_slice(&[
                vertex::Vertex::new_from_pos([0.0, 0.0, 0.0]),
                vertex::Vertex::new_from_pos([10.0, 10.0, 10.0]),
                vertex::Vertex::new_from_pos([10.0, 0.0, 10.0]),
            ]),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        }),
    ];

    let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Index Buffer"),
        contents: bytemuck::cast_slice(&face.index_data),
        usage: wgpu::BufferUsages::INDEX,
    });

    let line_index_data: &[u16] = &[0, 1, 2, 2, 1, 0];
    let line_index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Line Index Buffer"),
        contents: bytemuck::cast_slice(&line_index_data),
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

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
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
        ],
    };

    let (all_chunk_data, chunk_order) = world_state.generate_world_data(&camera);
    let chunk_dims = all_chunk_data.dims();

    let mut chunk_render_data_flat: Vec<ChunkRenderDescriptor> = vec![];

    // HACK(aleks): the order needs to be reversed here for collisions to work later -- that's confusing
    for (chunk_z, chunk_x) in iproduct!(0..chunk_dims[0], 0..chunk_dims[1]) {
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
        chunk_render_data_flat.push(ChunkRenderDescriptor {
            position: [chunk_x, chunk_z],
            annotated_instance_buffers,
        })
    }
    let chunk_render_data: Vec2d<ChunkRenderDescriptor> =
        Vec2d::new(chunk_render_data_flat, [chunk_dims[0], chunk_dims[1]]);

    let depth_texture = texture::Texture::create_depth_texture(&device, &config, "depth_texture");

    let buffers = &[vertex_buffer_layout, instance::InstanceRaw::desc()];

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers,
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

    let pipeline_wire = if false
        && device
            .features()
            .contains(wgpu::Features::POLYGON_MODE_LINE)
    {
        let pipeline_wire = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers,
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
        line_index_buf,
        index_count: face.index_data.len(),
        texture_bind_group,
        camera_bind_group,
        camera_buf,
        camera_staging_buf,
        chunk_render_data,
        chunk_order,
        depth_texture,
        pipeline,
        pipeline_wire,
    }
}

fn render_scene(
    view: &wgpu::TextureView,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &Scene,
    spawner: &Spawner,
) {
    static RENDER_WIREFRAME: bool = false;
    static RENDER_CAMERA_RAY: bool = false;

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
        rpass.set_vertex_buffer(0, scene.vertex_buffers[0].slice(..));
        rpass.set_index_buffer(scene.index_buf.slice(..), wgpu::IndexFormat::Uint16);

        for data_type in [ChunkDataType::Opaque, ChunkDataType::Transluscent] {
            for [chunk_x, chunk_z] in scene.chunk_order.iter().rev() {
                let chunk_render_datum = &scene.chunk_render_data[[*chunk_x, *chunk_z]];
                let maybe_instance_buffer = chunk_render_datum
                    .annotated_instance_buffers
                    .iter()
                    .find(|&ib| ib.data_type == data_type);

                if let Some(ref instance_buffer) = maybe_instance_buffer {
                    rpass.set_vertex_buffer(1, instance_buffer.buffer.slice(..));
                    rpass.draw_indexed(0..scene.index_count as u32, 0, 0..instance_buffer.len as _);

                    if RENDER_WIREFRAME || RENDER_CAMERA_RAY {
                        if let Some(ref pipe) = &scene.pipeline_wire {
                            rpass.set_pipeline(pipe);
                            if RENDER_WIREFRAME {
                                rpass.draw_indexed(
                                    0..scene.index_count as u32,
                                    0,
                                    0..instance_buffer.len as _,
                                );
                            }

                            // Draw camera line
                            if RENDER_CAMERA_RAY {
                                rpass.set_index_buffer(
                                    scene.line_index_buf.slice(..),
                                    wgpu::IndexFormat::Uint16,
                                );
                                rpass.set_vertex_buffer(0, scene.vertex_buffers[1].slice(..));
                                rpass.draw_indexed(0..6 as u32, 0, 0..1 as _);
                            }

                            rpass.set_pipeline(&scene.pipeline);
                        }
                    }
                }
            }
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
