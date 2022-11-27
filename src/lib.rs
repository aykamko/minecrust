#[macro_use]
extern crate itertools;
#[macro_use]
extern crate bmp;

pub mod camera;
pub mod color;
pub mod dom_controls;
pub mod face;
pub mod instance;
pub mod light;
pub mod map_generation;
pub mod spawner;
pub mod texture;
pub mod vec_extra;
pub mod vertex;
pub mod world;

use cgmath::Point3;
use dom_controls::DomControlsUserEvent;
use futures::executor::block_on;
use spawner::Spawner;
use std::{borrow::Cow, collections::HashSet, future::Future, mem, pin::Pin, task};
use wgpu::{util::DeviceExt, SurfaceTexture};
use winit::{
    event::{DeviceEvent, ElementState, Event, MouseButton, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
};
use world::CHUNK_XZ_SIZE;

use crate::world::ChunkDataType;

static RENDER_WIREFRAME: bool = false;
static RENDER_LIGHT_DEBUG_DATA: bool = false;
static RENDER_CHARACTER_ENTITY: bool = true;

#[allow(dead_code)]
const VERBOSE_LOGS: bool = false;

struct State {
    surface_config: wgpu::SurfaceConfiguration,
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,

    camera: camera::Camera,
    camera_controller: camera::CameraController,
    camera_uniform: camera::CameraUniform,
    light_uniform: light::LightUniform,
    world_state: world::WorldState,
}

struct VertexBufers {
    blocks: wgpu::Buffer,
    light_volume: wgpu::Buffer,
    character_entity: wgpu::Buffer,
}

struct IndexBufers {
    blocks: wgpu::Buffer,
    light_volume: wgpu::Buffer,
    character_entity: wgpu::Buffer,
}

struct IndexCounts {
    blocks: usize,
    light_volume: usize,
    character_entity: usize,
}

struct Scene {
    vertex_buffers: VertexBufers,
    index_buffers: IndexBufers,
    index_counts: IndexCounts,
    albedo_only_texture_bind_group: wgpu::BindGroup,
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
    pipeline_solid_color: Option<wgpu::RenderPipeline>,
}

struct Game {
    state: State,
    scene: Scene,
}

struct AnnotatedInstanceBuffer {
    buffer: wgpu::Buffer,
    len: usize,
    data_type: world::ChunkDataType,
}
struct ChunkRenderDescriptor {
    #[allow(dead_code)]
    world_chunk_position: [usize; 2],
    annotated_instance_buffers: Vec<AnnotatedInstanceBuffer>,
}

impl State {
    async fn new(window: &winit::window::Window) -> Self {
        cfg_if::cfg_if! {
            if #[cfg(target_arch = "wasm32")] {
               let backend = wgpu::Backends::SECONDARY;
            } else {
               let backend = wgpu::Backends::PRIMARY;
            }
        };
        let instance = wgpu::Instance::new(backend);

        let size = window.inner_size();
        let surface = unsafe {
            let surface = instance.create_surface(&window);
            surface
        };

        log::warn!("WGPU setup");
        let adapter =
            wgpu::util::initialize_adapter_from_env_or_default(&instance, backend, Some(&surface))
                .await
                .expect("No suitable GPU adapters found on the system!");

        let adapter_info = adapter.get_info();
        println!("Using {} ({:?})", adapter_info.name, adapter_info.backend);

        log::warn!("Requesting device");
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

        let supported_formats = surface.get_supported_formats(&adapter);
        log::warn!("Supported formats: {:?}", supported_formats);

        cfg_if::cfg_if! {
            if #[cfg(target_arch = "wasm32")] {
               let chosen_format = wgpu::TextureFormat::Rgba8UnormSrgb;
            } else {
               let chosen_format = wgpu::TextureFormat::Bgra8UnormSrgb;
            }
        };

        assert!(supported_formats.contains(&chosen_format));

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: chosen_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
        };
        surface.configure(&device, &surface_config);

        let camera_controller = camera::CameraController::new(0.15, 0.01);

        // Start in the center
        let center = world::get_world_center();
        let zfar = 250.0;
        let camera = camera::Camera::new(
            Point3::<f32>::new(center.x as f32, center.y as f32, center.z as f32),
            // have it look at the origin
            (0.0, 0.0, 0.0).into(),
            // which way is "up"
            cgmath::Vector3::unit_y(),
            cgmath::Vector3::unit_y(),
            surface_config.width as f32 / surface_config.height as f32,
            70.0,
            0.1,
            zfar,
        );

        let sunlight_pos = glam::Vec3::new(40.0, 30.0, 40.0);

        let scale_factor = 1.0;
        let sunlight_ortho_proj_coords = vertex::CuboidCoords {
            left: -(CHUNK_XZ_SIZE as f32 * scale_factor * 2.0),
            right: CHUNK_XZ_SIZE as f32 * scale_factor * 2.0,
            bottom: -(CHUNK_XZ_SIZE as f32 * scale_factor * 2.0),
            top: CHUNK_XZ_SIZE as f32 * scale_factor,
            near: 0.0,
            // Can't be too far or z-depth values won't have enough precision
            far: 125.0,
        };

        // Light
        let light_uniform = light::LightUniform::new(
            [0.0, 5.0, 0.0].into(),
            [1.0, 1.0, 1.0].into(),
            sunlight_pos,
            sunlight_ortho_proj_coords,
            [2048, 2048],
        );

        let mut camera_uniform = camera::CameraUniform::new();
        camera_uniform.update_view_proj(&camera);

        let mut world_state = world::WorldState::new();
        world_state.initial_setup(&camera);

        State {
            surface_config,
            surface,
            device,
            queue,

            camera,
            camera_controller,
            camera_uniform,
            light_uniform,
            world_state,
        }
    }
}

impl Scene {
    async fn new(initial_state: &mut State) -> Self {
        let surface_config = &initial_state.surface_config;
        let device = &initial_state.device;
        let queue = &initial_state.queue;
        let camera_uniform = initial_state.camera_uniform;
        let light_uniform = &initial_state.light_uniform;
        let world_state = &mut initial_state.world_state;
        let camera = &&initial_state.camera;

        let albedo_only_texture_bind_group_layout =
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
                ],
            });
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

        log::info!("Creating shadow map render pipeline");
        let shadow_map_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &[
                        &camera_bind_group_layout,
                        &light_bind_group_layout,
                        &albedo_only_texture_bind_group_layout,
                    ],
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
        log::info!("Shadow map render pipeline complete");

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[
                &texture_bind_group_layout,
                &camera_bind_group_layout,
                &light_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        log::info!("Creating forward-pass render pipeline");
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
                    format: surface_config.format,
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

        let pipeline_solid_color = if RENDER_CHARACTER_ENTITY {
            Some(
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: None,
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: "vs_camera_translate_no_instancing",
                        buffers: &[vertex_buffer_layouts[0].clone()],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: "fs_solid_color",
                        targets: &[Some(wgpu::ColorTargetState {
                            format: surface_config.format,
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
                }),
            )
        } else {
            None
        };

        let create_wire_pipeline = |vtx_shader_entry_point: &str, cull_mode: Option<wgpu::Face>| {
            if device
                .features()
                .contains(wgpu::Features::POLYGON_MODE_LINE)
            {
                let pipeline_wire =
                    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                                format: surface_config.format,
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
        let character_vtx_data = world_state.character_entity.vertex_data();

        let vertex_buffers = VertexBufers {
            blocks: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Main Vertex Buffer"),
                contents: bytemuck::cast_slice(&face.vertex_data),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            light_volume: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Light Volume Vertex Buffer"),
                contents: bytemuck::cast_slice(&sunlight_vtx_data.vertex_data),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            }),
            character_entity: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Character Entity Vertex Buffer"),
                contents: bytemuck::cast_slice(&character_vtx_data.vertex_data),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            }),
        };

        let index_buffers = IndexBufers {
            blocks: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Main Index Buffer"),
                contents: bytemuck::cast_slice(&face.index_data),
                usage: wgpu::BufferUsages::INDEX,
            }),
            light_volume: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Light Volume Index Buffer"),
                contents: bytemuck::cast_slice(&sunlight_vtx_data.index_data),
                usage: wgpu::BufferUsages::INDEX,
            }),
            character_entity: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Character Entity Index Buffer"),
                contents: bytemuck::cast_slice(&character_vtx_data.index_data),
                usage: wgpu::BufferUsages::INDEX,
            }),
        };

        let index_counts = IndexCounts {
            blocks: face.index_data.len(),
            light_volume: sunlight_vtx_data.index_data.len(),
            character_entity: character_vtx_data.index_data.len(),
        };

        let texture_atlas = texture::Texture::create_pixel_art_image_texture(
            include_bytes!("../assets/minecruft_atlas.png"),
            device,
            queue,
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
        let shadow_map_texture = texture::Texture::create_depth_texture(
            "shadow_map_texture",
            &device,
            light_uniform.shadow_map_pixel_size,
            &wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                border_color: None,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::FilterMode::Nearest,
                compare: None,
                lod_min_clamp: -100.0,
                lod_max_clamp: 100.0,
                ..Default::default()
            },
        );

        // Create bind groups
        let albedo_only_texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &albedo_only_texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_atlas.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&texture_atlas.sampler),
                },
            ],
            label: None,
        });
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
            [surface_config.width, surface_config.height],
            &wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::FilterMode::Nearest,
                compare: Some(wgpu::CompareFunction::LessEqual),
                lod_min_clamp: -100.0,
                lod_max_clamp: 100.0,
                ..Default::default()
            },
        );

        Scene {
            vertex_buffers,
            index_buffers,
            index_counts,
            albedo_only_texture_bind_group,
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
            pipeline_solid_color,
        }
    }
}

impl Game {
    async fn new(window: &winit::window::Window) -> Self {
        let mut state = State::new(&window).await;
        let scene = Scene::new(&mut state).await;

        Game { state, scene }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.state.surface_config.width = new_size.width;
            self.state.surface_config.height = new_size.height;
            log::info!(
                "Resizing to {}x{}",
                self.state.surface_config.width,
                self.state.surface_config.height
            );
            self.state
                .surface
                .configure(&self.state.device, &self.state.surface_config);
            self.scene.depth_texture = texture::Texture::create_depth_texture(
                "depth_texture",
                &self.state.device,
                [
                    self.state.surface_config.width,
                    self.state.surface_config.height,
                ],
                &wgpu::SamplerDescriptor {
                    address_mode_u: wgpu::AddressMode::ClampToEdge,
                    address_mode_v: wgpu::AddressMode::ClampToEdge,
                    address_mode_w: wgpu::AddressMode::ClampToEdge,
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    mipmap_filter: wgpu::FilterMode::Nearest,
                    compare: Some(wgpu::CompareFunction::LessEqual),
                    lod_min_clamp: -100.0,
                    lod_max_clamp: 100.0,
                    ..Default::default()
                },
            );
            self.state.camera.aspect =
                self.state.surface_config.width as f32 / self.state.surface_config.height as f32;
        }
    }

    pub fn update_tick(&mut self, left_mouse_clicked: &mut bool, right_mouse_clicked: &mut bool) {
        let state = &mut self.state;
        let scene = &mut self.scene;

        let update_result = state
            .camera_controller
            .update_camera(&mut state.camera, &state.world_state);
        state.camera_uniform.update_view_proj(&state.camera);
        state.queue.write_buffer(
            &scene.camera_staging_buf,
            0,
            bytemuck::cast_slice(&[state.camera_uniform]),
        );

        state.light_uniform.update_light_space_proj(&state.camera);
        state.queue.write_buffer(
            &scene.light_buf,
            0,
            bytemuck::cast_slice(&[state.light_uniform.to_raw()]),
        );

        #[derive(PartialEq)]
        struct ChunkModification {
            new_chunk: [usize; 2],
            old_chunk: [usize; 2],
        }
        let mut chunk_mods: Vec<ChunkModification> = vec![];

        if update_result.did_move {
            let chunks_modified = state.world_state.highlight_colliding_block(&state.camera);
            for chunk_idx in chunks_modified {
                chunk_mods.push(ChunkModification {
                    new_chunk: chunk_idx,
                    old_chunk: chunk_idx,
                });
            }

            let sunlight_vtx_data = state.light_uniform.vertex_data_for_sunlight();
            state.queue.write_buffer(
                &scene.vertex_buffers.light_volume,
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
        if *left_mouse_clicked || *right_mouse_clicked {
            let chunks_modified = if *right_mouse_clicked {
                state
                    .world_state
                    .place_block(&state.camera, world::BlockType::Sand)
            } else {
                state.world_state.break_block(&state.camera)
            };
            *left_mouse_clicked = false;
            *right_mouse_clicked = false;

            for chunk_idx in chunks_modified {
                chunk_mods.push(ChunkModification {
                    new_chunk: chunk_idx,
                    old_chunk: chunk_idx,
                });
            }

            if !update_result.did_move {
                let chunks_modified = state.world_state.highlight_colliding_block(&state.camera);
                for chunk_idx in chunks_modified {
                    chunk_mods.push(ChunkModification {
                        new_chunk: chunk_idx,
                        old_chunk: chunk_idx,
                    });
                }
            }
        }

        if update_result.did_move_chunks {
            let new_chunk_order = state.world_state.get_chunk_order_by_distance(&state.camera);

            let new_chunk_order_hashset = new_chunk_order.iter().cloned().collect::<HashSet<_>>();
            let old_chunk_order_hashset = scene.chunk_order.iter().cloned().collect::<HashSet<_>>();

            let new_chunks = (&new_chunk_order_hashset - &old_chunk_order_hashset)
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            let old_chunks = (&old_chunk_order_hashset - &new_chunk_order_hashset)
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            let neighbors_to_new_chunks = state
                .world_state
                .find_chunk_neighbors(&new_chunks, &scene.chunk_order);

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
            #[cfg(not(target_arch = "wasm32"))]
            let chunk_mod_time = std::time::Instant::now();

            chunk_mods.dedup();

            for chunk_mod in chunk_mods.iter() {
                state.world_state.maybe_allocate_chunk(chunk_mod.new_chunk);
            }
            #[cfg(not(target_arch = "wasm32"))]
            if VERBOSE_LOGS && update_result.did_move_chunks {
                println!(
                    "Took {}ms to allocate chunks",
                    chunk_mod_time.elapsed().as_millis()
                );
            }

            let new_chunk_datas = chunk_mods
                .iter()
                .map(|chunk_mod| {
                    let new_chunk_data = state
                        .world_state
                        .compute_chunk_mesh(chunk_mod.new_chunk, &state.camera);

                    let render_descriptor_idx = state
                        .world_state
                        .get_render_descriptor_idx(chunk_mod.old_chunk);
                    if chunk_mod.new_chunk != chunk_mod.old_chunk {
                        state.world_state.set_render_descriptor_idx(
                            chunk_mod.old_chunk,
                            world::NO_RENDER_DESCRIPTOR_INDEX,
                        );
                        state
                            .world_state
                            .set_render_descriptor_idx(chunk_mod.new_chunk, render_descriptor_idx);
                    }

                    (new_chunk_data, render_descriptor_idx)
                })
                .collect::<Vec<_>>();

            #[cfg(not(target_arch = "wasm32"))]
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
                        state.queue.write_buffer(
                            &instance_buffer.buffer,
                            0,
                            bytemuck::cast_slice(&typed_instances.instance_data),
                        );
                        instance_buffer.len = typed_instances.instance_data.len();
                    }
                }
            }
        }

        state.world_state.physics_tick();
        let updated_character_vtx_data = state.world_state.character_entity.vertex_data();
        state.queue.write_buffer(
            &scene.vertex_buffers.character_entity,
            0,
            bytemuck::cast_slice(&updated_character_vtx_data.vertex_data),
        );
    }

    fn render_chunk<'a>(
        &'a self,
        rpass: &mut wgpu::RenderPass<'a>,
        chunk_idx: [usize; 2],
        data_type: ChunkDataType,
    ) {
        let [chunk_x, chunk_z] = chunk_idx;
        let render_descriptor_idx = self
            .state
            .world_state
            .get_render_descriptor_idx([chunk_x, chunk_z]);
        let chunk_render_datum = &self.scene.chunk_render_descriptors[render_descriptor_idx];

        let maybe_instance_buffer = chunk_render_datum
            .annotated_instance_buffers
            .iter()
            .find(|&ib| ib.data_type == data_type);

        if let Some(ref instance_buffer) = maybe_instance_buffer {
            rpass.set_vertex_buffer(1, instance_buffer.buffer.slice(..));
            rpass.draw_indexed(
                0..self.scene.index_counts.blocks as u32,
                0,
                0..instance_buffer.len as _,
            );

            if RENDER_WIREFRAME {
                if let Some(ref pipe) = &self.scene.pipeline_wire {
                    rpass.set_pipeline(pipe);
                    rpass.draw_indexed(
                        0..self.scene.index_counts.blocks as u32,
                        0,
                        0..instance_buffer.len as _,
                    );

                    rpass.set_pipeline(&self.scene.pipeline);
                }
            }
        }
    }

    pub fn render_frame(&mut self, spawner: &Spawner) -> SurfaceTexture {
        let state = &self.state;
        let scene = &self.scene;

        let frame = match state.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(_) => {
                state
                    .surface
                    .configure(&state.device, &state.surface_config);
                state
                    .surface
                    .get_current_texture()
                    .expect("Failed to acquire next surface texture!")
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        state.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut encoder = state
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
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
            rpass.set_bind_group(2, &scene.albedo_only_texture_bind_group, &[]);
            rpass.set_vertex_buffer(0, scene.vertex_buffers.blocks.slice(..));
            rpass.set_index_buffer(
                scene.index_buffers.blocks.slice(..),
                wgpu::IndexFormat::Uint16,
            );

            for data_type in [ChunkDataType::Opaque, ChunkDataType::SemiTransluscent] {
                for chunk_idx in scene.chunk_order.iter().rev() {
                    self.render_chunk(&mut rpass, *chunk_idx, data_type);
                }
            }

            if RENDER_CHARACTER_ENTITY {
                rpass.set_vertex_buffer(0, scene.vertex_buffers.character_entity.slice(..));
                rpass.set_index_buffer(
                    scene.index_buffers.character_entity.slice(..),
                    wgpu::IndexFormat::Uint16,
                );
                rpass.draw_indexed(0..scene.index_counts.character_entity as u32, 0, 0..1);
            }
        }

        let sky_color = wgpu::Color {
            r: color::srgb_to_rgb(120.0 / 255.0),
            g: color::srgb_to_rgb(167.0 / 255.0),
            b: color::srgb_to_rgb(255.0 / 255.0),
            a: 1.0,
        };
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(sky_color),
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
            rpass.set_vertex_buffer(0, scene.vertex_buffers.blocks.slice(..));
            rpass.set_index_buffer(
                scene.index_buffers.blocks.slice(..),
                wgpu::IndexFormat::Uint16,
            );

            for data_type in [
                ChunkDataType::Opaque,
                ChunkDataType::Transluscent,
                ChunkDataType::SemiTransluscent,
            ] {
                for chunk_idx in scene.chunk_order.iter().rev() {
                    self.render_chunk(&mut rpass, *chunk_idx, data_type);
                }
            }

            if RENDER_LIGHT_DEBUG_DATA {
                // Draw light volume wireframe
                if let Some(ref pipe) = &scene.pipeline_wire_no_instancing {
                    rpass.set_pipeline(pipe);
                    rpass.set_vertex_buffer(0, scene.vertex_buffers.light_volume.slice(..));
                    rpass.set_index_buffer(
                        scene.index_buffers.light_volume.slice(..),
                        wgpu::IndexFormat::Uint16,
                    );
                    rpass.draw_indexed(0..scene.index_counts.light_volume as u32, 0, 0..1);

                    rpass.set_pipeline(&scene.pipeline);
                }
            }

            if RENDER_CHARACTER_ENTITY {
                if let Some(ref pipe) = &scene.pipeline_solid_color {
                    rpass.set_pipeline(pipe);
                    rpass.set_bind_group(0, &scene.texture_bind_group, &[]);
                    rpass.set_bind_group(1, &scene.camera_bind_group, &[]);
                    rpass.set_bind_group(2, &scene.light_bind_group, &[]);
                    rpass.set_vertex_buffer(0, scene.vertex_buffers.character_entity.slice(..));
                    rpass.set_index_buffer(
                        scene.index_buffers.character_entity.slice(..),
                        wgpu::IndexFormat::Uint16,
                    );
                    rpass.draw_indexed(0..scene.index_counts.character_entity as u32, 0, 0..1);
                    rpass.set_pipeline(&scene.pipeline);
                }
            }
        }

        state.queue.submit(Some(encoder.finish()));

        // If an error occurs, report it and panic.
        spawner.spawn_local(ErrorFuture {
            inner: state.device.pop_error_scope(),
        });

        return frame;
    }
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn run(width: usize, height: usize) {
    cfg_if::cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            std::panic::set_hook(Box::new(console_error_panic_hook::hook));
            console_log::init_with_level(log::Level::Info).expect("Couldn't initialize logger");
        } else {
            env_logger::init();
        }
    }

    let event_loop = EventLoopBuilder::<DomControlsUserEvent>::with_user_event().build();
    unsafe {
        dom_controls::set_global_event_loop_proxy(&event_loop);
    }

    let window = winit::window::WindowBuilder::new()
        .with_title("Minecrust")
        .with_inner_size(winit::dpi::LogicalSize {
            width: width as i32,
            height: height as i32,
        })
        .build(&event_loop)
        .unwrap();

    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::WindowExtWebSys;
        web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| {
                let dst = doc.get_element_by_id("wasm-container")?;
                let canvas = web_sys::Element::from(window.canvas());
                dst.append_child(&canvas).ok()?;
                Some(())
            })
            .expect("Couldn't append canvas to document body.");
    }

    let mut game = block_on(Game::new(&window));

    let mut curr_modifier_state: winit::event::ModifiersState =
        winit::event::ModifiersState::empty();
    let mut cursor_grabbed = false;

    let mut left_mouse_clicked = false;
    let mut right_mouse_clicked = false;

    // Remove Loader element from DOM
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| {
                let loader_elem = doc.get_element_by_id("loader")?;
                loader_elem.remove();
                Some(())
            });
    }

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
                            game.state.camera_controller.process_window_event(&event);
                        }
                        (Some(VirtualKeyCode::Escape), ElementState::Pressed) => {
                            window.set_cursor_visible(true);
                            window
                                .set_cursor_grab(winit::window::CursorGrabMode::None)
                                .expect("Failed to release curosr");
                            cursor_grabbed = false;
                        }
                        _ => {
                            game.state.camera_controller.process_window_event(&event);
                        }
                    }
                }
                WindowEvent::MouseInput { state, button, .. } => match (state, button) {
                    (ElementState::Pressed, MouseButton::Left) => {
                        if !cursor_grabbed {
                            window
                                .set_cursor_grab(winit::window::CursorGrabMode::Locked)
                                .expect("Failed to grab curosr");
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

                WindowEvent::Resized(physical_size) => {
                    game.resize(physical_size);
                }
                WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                    game.resize(*new_inner_size);
                }
                _ => (),
            },

            Event::DeviceEvent { event, .. } => match event {
                DeviceEvent::MouseMotion { .. } => {
                    if cursor_grabbed {
                        game.state.camera_controller.process_device_event(&event);
                    }
                }
                _ => (),
            },

            Event::UserEvent(event) => match event {
                DomControlsUserEvent::AButtonPressed => {
                    left_mouse_clicked = true;
                }
                DomControlsUserEvent::BButtonPressed => {
                    right_mouse_clicked = true;
                }
                DomControlsUserEvent::WindowResized { size } => {
                    log::info!("Web window resized: {:?}", size);

                    game.resize(size.to_physical(window.scale_factor()));
                    #[cfg(target_arch = "wasm32")]
                    {
                        // Web <canvas> element must be resized explicitly, can't use CSS rules
                        window.set_inner_size(winit::dpi::PhysicalSize::new(
                            game.state.surface_config.width as i32,
                            game.state.surface_config.height as i32,
                        ));
                    }
                }
                _ => {
                    game.state
                        .camera_controller
                        .process_web_dom_button_event(&event);
                }
            },

            Event::RedrawRequested(_) => {
                game.update_tick(&mut left_mouse_clicked, &mut right_mouse_clicked);

                let frame = game.render_frame(&spawner);
                frame.present();

                game.state.camera_controller.reset_mouse_delta();
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
