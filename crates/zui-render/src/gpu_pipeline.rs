//! GPU textures, bind groups, transform uniforms, and render pipelines.
//!
//! This module owns backend setup; retained-scene replay remains in the renderer.

use super::*;

pub(crate) fn create_canvas(
    device: &wgpu::Device,
    size: PhysicalSize,
    format: wgpu::TextureFormat,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("zui-render canvas"),
        size: wgpu::Extent3d {
            width: size.width.max(1),
            height: size.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

pub(crate) fn create_stencil(device: &wgpu::Device, size: PhysicalSize) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("zui-render clip stencil"),
        size: wgpu::Extent3d {
            width: size.width.max(1),
            height: size.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth24PlusStencil8,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

pub(crate) fn create_stencil_reset_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    let color = [1.0, 1.0, 1.0, 1.0];
    let vertices = [
        RectVertex {
            position: [-1.0, 1.0],
            color,
        },
        RectVertex {
            position: [1.0, 1.0],
            color,
        },
        RectVertex {
            position: [1.0, -1.0],
            color,
        },
        RectVertex {
            position: [-1.0, 1.0],
            color,
        },
        RectVertex {
            position: [1.0, -1.0],
            color,
        },
        RectVertex {
            position: [-1.0, -1.0],
            color,
        },
    ];
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("zui-render stencil reset"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    })
}

fn stencil_draw_state() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth24PlusStencil8,
        depth_write_enabled: Some(false),
        depth_compare: Some(wgpu::CompareFunction::Always),
        stencil: wgpu::StencilState {
            front: wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Equal,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::Keep,
            },
            back: wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Equal,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::Keep,
            },
            read_mask: 1,
            write_mask: 0,
        },
        bias: wgpu::DepthBiasState::default(),
    }
}

fn stencil_mask_state() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth24PlusStencil8,
        depth_write_enabled: Some(false),
        depth_compare: Some(wgpu::CompareFunction::Always),
        stencil: wgpu::StencilState {
            front: wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Equal,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::IncrementClamp,
            },
            back: wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Equal,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::IncrementClamp,
            },
            read_mask: 0xff,
            write_mask: 1,
        },
        bias: wgpu::DepthBiasState::default(),
    }
}

fn stencil_reset_state() -> wgpu::DepthStencilState {
    let mut state = stencil_mask_state();
    state.stencil.front.compare = wgpu::CompareFunction::Always;
    state.stencil.back.compare = wgpu::CompareFunction::Always;
    state.stencil.read_mask = 0;
    state.stencil.front.pass_op = wgpu::StencilOperation::Replace;
    state.stencil.back.pass_op = wgpu::StencilOperation::Replace;
    state
}

pub(crate) fn create_blit_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("zui-render canvas blit shader"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
            @group(0) @binding(0) var canvas: texture_2d<f32>;
            @group(0) @binding(1) var canvas_sampler: sampler;

            struct VertexOutput {
                @builtin(position) position: vec4<f32>,
                @location(0) uv: vec2<f32>,
            };

            @vertex
            fn vs(@builtin(vertex_index) index: u32) -> VertexOutput {
                var positions = array<vec2<f32>, 3>(
                    vec2<f32>(-1.0, -1.0),
                    vec2<f32>(3.0, -1.0),
                    vec2<f32>(-1.0, 3.0)
                );
                var uvs = array<vec2<f32>, 3>(
                    vec2<f32>(0.0, 1.0),
                    vec2<f32>(2.0, 1.0),
                    vec2<f32>(0.0, -1.0)
                );
                var output: VertexOutput;
                output.position = vec4<f32>(positions[index], 0.0, 1.0);
                output.uv = uvs[index];
                return output;
            }

            @fragment
            fn fs(input: VertexOutput) -> @location(0) vec4<f32> {
                return textureSample(canvas, canvas_sampler, input.uv);
            }
        "#
            .into(),
        ),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("zui-render canvas blit pipeline"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

pub(crate) fn create_blit_bind_group(
    device: &wgpu::Device,
    pipeline: &wgpu::RenderPipeline,
    canvas_view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    let layout = pipeline.get_bind_group_layout(0);
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("zui-render canvas sampler"),
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("zui-render canvas bind group"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(canvas_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    })
}

pub(crate) fn create_stencil_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    create_stencil_pipeline_with_state(device, format, stencil_reset_state(), None)
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuTransform {
    pub(crate) matrix: [[f32; 4]; 4],
}

pub(crate) fn gpu_transform(transform: Transform, size: PhysicalSize) -> GpuTransform {
    let [a, b, c, d, tx, ty] = transform.matrix;
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    GpuTransform {
        // WGSL matrices are column-major. This maps logical DIP coordinates
        // directly to NDC, including arbitrary affine node transforms.
        matrix: [
            [2.0 * a / width, -2.0 * b / height, 0.0, 0.0],
            [2.0 * c / width, -2.0 * d / height, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [2.0 * tx / width - 1.0, 1.0 - 2.0 * ty / height, 0.0, 1.0],
        ],
    }
}

pub(crate) fn create_transform_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("zui-render transform bind group layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<GpuTransform>() as u64),
            },
            count: None,
        }],
    })
}

pub(crate) fn transform_key(transform: Transform) -> [u32; 6] {
    transform.matrix.map(f32::to_bits)
}

pub(crate) fn transform_bind_group_for(
    state: &mut SurfaceState,
    device: &wgpu::Device,
    transform: Transform,
    size: PhysicalSize,
) -> wgpu::BindGroup {
    let key = transform_key(transform);
    if let Some(binding) = state.transform_bindings.get(&key) {
        return binding.bind_group.clone();
    }
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("zui-render transform uniform"),
        contents: bytemuck::bytes_of(&gpu_transform(transform, size)),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("zui-render transform bind group"),
        layout: &state.transform_bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });
    state.transform_bindings.insert(
        key,
        TransformBinding {
            _buffer: buffer,
            bind_group: bind_group.clone(),
        },
    );
    bind_group
}

pub(crate) fn create_stencil_mask_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    transform_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    create_stencil_pipeline_with_state(device, format, stencil_mask_state(), Some(transform_layout))
}

fn create_stencil_pipeline_with_state(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    stencil_state: wgpu::DepthStencilState,
    transform_layout: Option<&wgpu::BindGroupLayout>,
) -> wgpu::RenderPipeline {
    let transformed = transform_layout.is_some();
    let layout = transform_layout.map(|layout| {
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("zui-render stencil mask pipeline layout"),
            bind_group_layouts: &[Some(layout)],
            immediate_size: 0,
        })
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("zui-render stencil mask shader"),
        source: wgpu::ShaderSource::Wgsl(
            if transformed { r#"
            struct Transform { matrix: mat4x4<f32>, };
            @group(0) @binding(0) var<uniform> transform: Transform;
            @vertex
            fn vs(@location(0) position: vec2<f32>, @location(1) color: vec4<f32>) -> @builtin(position) vec4<f32> {
                return transform.matrix * vec4<f32>(position, 0.0, 1.0);
            }

            @fragment
            fn fs() -> @location(0) vec4<f32> {
                return vec4<f32>(0.0);
            }
        "# } else { r#"
            @vertex
            fn vs(@location(0) position: vec2<f32>, @location(1) color: vec4<f32>) -> @builtin(position) vec4<f32> {
                return vec4<f32>(position, 0.0, 1.0);
            }
            @fragment
            fn fs() -> @location(0) vec4<f32> { return vec4<f32>(0.0); }
        "# }
            .into(),
        ),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("zui-render stencil mask pipeline"),
        layout: layout.as_ref(),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<RectVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::empty(),
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(stencil_state),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

pub(crate) fn create_rect_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    transform_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("zui-render rectangle shader"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
            struct Transform { matrix: mat4x4<f32>, };
            @group(0) @binding(0) var<uniform> transform: Transform;
            struct VertexOutput {
                @builtin(position) position: vec4<f32>,
                @location(0) color: vec4<f32>,
            };
            @vertex
            fn vs(@location(0) position: vec2<f32>, @location(1) color: vec4<f32>) -> VertexOutput {
                var output: VertexOutput;
                output.position = transform.matrix * vec4<f32>(position, 0.0, 1.0);
                output.color = color;
                return output;
            }
            @fragment
            fn fs(@location(0) color: vec4<f32>) -> @location(0) vec4<f32> { return color; }
        "#
            .into(),
        ),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("zui-render rectangle pipeline layout"),
        bind_group_layouts: &[Some(transform_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("zui-render rectangle pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<RectVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(stencil_draw_state()),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

pub(crate) fn create_image_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    transform_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("zui-render image shader"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
            struct Transform { matrix: mat4x4<f32>, };
            @group(0) @binding(0) var<uniform> transform: Transform;
            @group(1) @binding(0) var image: texture_2d<f32>;
            @group(1) @binding(1) var image_sampler: sampler;
            @group(1) @binding(2) var<uniform> image_uv: vec4<f32>;

            struct VertexOutput {
                @builtin(position) position: vec4<f32>,
                @location(0) uv: vec2<f32>,
                @location(1) opacity: f32,
                @location(2) color: vec4<f32>,
            };

            @vertex
            fn vs(
                @location(0) position: vec2<f32>,
                @location(1) uv: vec2<f32>,
                @location(2) opacity: f32,
                @location(3) color: vec4<f32>,
            ) -> VertexOutput {
                var output: VertexOutput;
                output.position = transform.matrix * vec4<f32>(position, 0.0, 1.0);
                output.uv = mix(image_uv.xy, image_uv.zw, uv);
                output.opacity = opacity;
                output.color = color;
                return output;
            }

            @fragment
            fn fs(input: VertexOutput) -> @location(0) vec4<f32> {
                let color = textureSample(image, image_sampler, input.uv);
                return vec4<f32>(color.rgb * input.color.rgb, color.a * input.color.a * input.opacity);
            }
        "#
            .into(),
        ),
    });
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("zui-render image bind group layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(16),
                },
                count: None,
            },
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("zui-render image pipeline layout"),
        bind_group_layouts: &[Some(transform_layout), Some(&bind_group_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("zui-render image pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<ImageVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32, 3 => Float32x4],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(stencil_draw_state()),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

pub(crate) fn create_rounded_rect_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    transform_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("zui-render rounded rectangle SDF shader"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
            struct Transform { matrix: mat4x4<f32>, };
            @group(0) @binding(0) var<uniform> transform: Transform;
            struct VertexOutput {
                @builtin(position) position: vec4<f32>,
                @location(0) local: vec2<f32>,
                @location(1) size: vec2<f32>,
                @location(2) radius: f32,
                @location(3) color: vec4<f32>,
            };

            @vertex
            fn vs(
                @location(0) position: vec2<f32>,
                @location(1) local: vec2<f32>,
                @location(2) size: vec2<f32>,
                @location(3) radius: f32,
                @location(4) color: vec4<f32>,
            ) -> VertexOutput {
                var output: VertexOutput;
                output.position = transform.matrix * vec4<f32>(position, 0.0, 1.0);
                output.local = local;
                output.size = size;
                output.radius = radius;
                output.color = color;
                return output;
            }

            @fragment
            fn fs(input: VertexOutput) -> @location(0) vec4<f32> {
                let half_size = input.size * 0.5;
                let point = input.local - half_size;
                let rounded = half_size - vec2<f32>(input.radius, input.radius);
                let q = abs(point) - rounded;
                let distance = length(max(q, vec2<f32>(0.0, 0.0)))
                    + min(max(q.x, q.y), 0.0)
                    - input.radius;
                let antialias = max(fwidth(distance), 0.0001);
                let alpha = 1.0 - smoothstep(-antialias, antialias, distance);
                return vec4<f32>(input.color.rgb, input.color.a * alpha);
            }
        "#
            .into(),
        ),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("zui-render rounded rectangle pipeline layout"),
        bind_group_layouts: &[Some(transform_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("zui-render rounded rectangle SDF pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<RoundedRectVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x2,
                    1 => Float32x2,
                    2 => Float32x2,
                    3 => Float32,
                    4 => Float32x4,
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(stencil_draw_state()),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

pub(crate) fn create_stencil_rounded_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    transform_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("zui-render rounded stencil mask shader"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
            struct Transform { matrix: mat4x4<f32>, };
            @group(0) @binding(0) var<uniform> transform: Transform;
            struct VertexOutput {
                @builtin(position) position: vec4<f32>,
                @location(0) local: vec2<f32>,
                @location(1) size: vec2<f32>,
                @location(2) radius: f32,
            };
            @vertex
            fn vs(@location(0) position: vec2<f32>, @location(1) local: vec2<f32>,
                  @location(2) size: vec2<f32>, @location(3) radius: f32,
                  @location(4) color: vec4<f32>) -> VertexOutput {
                var output: VertexOutput;
                output.position = transform.matrix * vec4<f32>(position, 0.0, 1.0);
                output.local = local;
                output.size = size;
                output.radius = radius;
                return output;
            }
            @fragment
            fn fs(input: VertexOutput) -> @location(0) vec4<f32> {
                let half_size = input.size * 0.5;
                let point = input.local - half_size;
                let rounded = half_size - vec2<f32>(input.radius, input.radius);
                let q = abs(point) - rounded;
                let distance = length(max(q, vec2<f32>(0.0, 0.0)))
                    + min(max(q.x, q.y), 0.0) - input.radius;
                if distance > 0.0 { discard; }
                return vec4<f32>(0.0);
            }
        "#
            .into(),
        ),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("zui-render rounded stencil mask pipeline layout"),
        bind_group_layouts: &[Some(transform_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("zui-render rounded stencil mask pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<RoundedRectVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32, 4 => Float32x4,
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::empty(),
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(stencil_mask_state()),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

pub(crate) fn create_line_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    transform_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("zui-render antialiased line shader"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
            struct Transform { matrix: mat4x4<f32>, };
            @group(0) @binding(0) var<uniform> transform: Transform;
            struct VertexOutput {
                @builtin(position) position: vec4<f32>,
                @location(0) point: vec2<f32>,
                @location(1) start: vec2<f32>,
                @location(2) end: vec2<f32>,
                @location(3) width: f32,
                @location(4) color: vec4<f32>,
            };

            @vertex
            fn vs(
                @location(0) position: vec2<f32>,
                @location(1) point: vec2<f32>,
                @location(2) start: vec2<f32>,
                @location(3) end: vec2<f32>,
                @location(4) width: f32,
                @location(5) color: vec4<f32>,
            ) -> VertexOutput {
                var output: VertexOutput;
                output.position = transform.matrix * vec4<f32>(position, 0.0, 1.0);
                output.point = point;
                output.start = start;
                output.end = end;
                output.width = width;
                output.color = color;
                return output;
            }

            @fragment
            fn fs(input: VertexOutput) -> @location(0) vec4<f32> {
                let segment = input.end - input.start;
                let segment_length = max(dot(segment, segment), 0.0001);
                let projection = clamp(
                    dot(input.point - input.start, segment) / segment_length,
                    0.0,
                    1.0,
                );
                let nearest = input.start + segment * projection;
                let line_distance = distance(input.point, nearest) - input.width * 0.5;
                let antialias = max(fwidth(line_distance), 0.0001);
                let alpha = 1.0 - smoothstep(-antialias, antialias, line_distance);
                return vec4<f32>(input.color.rgb, input.color.a * alpha);
            }
        "#
            .into(),
        ),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("zui-render line pipeline layout"),
        bind_group_layouts: &[Some(transform_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("zui-render antialiased line pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<LineVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x2,
                    1 => Float32x2,
                    2 => Float32x2,
                    3 => Float32x2,
                    4 => Float32,
                    5 => Float32x4,
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(stencil_draw_state()),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}
