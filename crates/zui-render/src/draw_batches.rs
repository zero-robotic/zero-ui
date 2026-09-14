//! CPU-side tessellation and GPU batch construction.

use super::*;

pub(crate) fn build_gpu_batches(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    arenas: &mut VertexArenas,
    index_arena: &mut IndexArena,
    batches: Vec<RenderBatch>,
) -> Vec<GpuBatch> {
    batches
        .into_iter()
        .filter_map(|batch| match batch {
            RenderBatch::Rect(vertices, indices, transform) if !vertices.is_empty() => {
                Some(GpuBatch::Draw(
                    BatchKind::Rect,
                    arenas.upload(
                        device,
                        queue,
                        VertexArenaKind::Rect,
                        bytemuck::cast_slice(&vertices),
                        vertices.len() as u32,
                    ),
                    index_arena.upload(device, queue, &indices),
                    transform,
                ))
            }
            RenderBatch::IndexedRect(vertices, indices, transform)
                if !vertices.is_empty() && !indices.is_empty() =>
            {
                Some(GpuBatch::Draw(
                    BatchKind::Rect,
                    arenas.upload(
                        device,
                        queue,
                        VertexArenaKind::Rect,
                        bytemuck::cast_slice(&vertices),
                        vertices.len() as u32,
                    ),
                    index_arena.upload(device, queue, &indices),
                    transform,
                ))
            }
            RenderBatch::Rounded(vertices, indices, transform) if !vertices.is_empty() => {
                Some(GpuBatch::Draw(
                    BatchKind::Rounded,
                    arenas.upload(
                        device,
                        queue,
                        VertexArenaKind::Rounded,
                        bytemuck::cast_slice(&vertices),
                        vertices.len() as u32,
                    ),
                    index_arena.upload(device, queue, &indices),
                    transform,
                ))
            }
            RenderBatch::Line(vertices, indices, transform) if !vertices.is_empty() => {
                Some(GpuBatch::Draw(
                    BatchKind::Line,
                    arenas.upload(
                        device,
                        queue,
                        VertexArenaKind::Line,
                        bytemuck::cast_slice(&vertices),
                        vertices.len() as u32,
                    ),
                    index_arena.upload(device, queue, &indices),
                    transform,
                ))
            }
            RenderBatch::Image {
                page,
                sampling,
                images,
                vertices,
                indices,
                transform,
            } if !vertices.is_empty() => Some(GpuBatch::Draw(
                BatchKind::Image {
                    page,
                    sampling,
                    images: Arc::new(images),
                },
                arenas.upload(
                    device,
                    queue,
                    VertexArenaKind::Image,
                    bytemuck::cast_slice(&vertices),
                    vertices.len() as u32,
                ),
                index_arena.upload(device, queue, &indices),
                transform,
            )),
            RenderBatch::Clip(geometry, clip_transform) => {
                let mask = match geometry {
                    ClipGeometry::Reset => None,
                    // Rect clips are represented by the render-pass
                    // scissor, so they require neither a stencil mask nor
                    // vertex/index arena allocations.
                    ClipGeometry::Rect(_) => None,
                    ClipGeometry::Rounded { rect, radius } => {
                        let mut vertices = Vec::new();
                        let mut indices = Vec::new();
                        append_rounded_rect(
                            &mut vertices,
                            &mut indices,
                            rect,
                            radius,
                            Color::WHITE,
                        );
                        Some(ClipGpuGeometry {
                            vertices: arenas.upload(
                                device,
                                queue,
                                VertexArenaKind::Rounded,
                                bytemuck::cast_slice(&vertices),
                                vertices.len() as u32,
                            ),
                            indices: Some(index_arena.upload(device, queue, &indices)),
                        })
                    }
                    ClipGeometry::Path {
                        ref vertices,
                        ref indices,
                        ..
                    } => Some(ClipGpuGeometry {
                        vertices: arenas.upload(
                            device,
                            queue,
                            VertexArenaKind::Rect,
                            bytemuck::cast_slice(vertices),
                            vertices.len() as u32,
                        ),
                        indices: Some(index_arena.upload(device, queue, indices)),
                    }),
                };
                Some(GpuBatch::Clip(geometry, mask, clip_transform))
            }
            _ => None,
        })
        .collect()
}

pub(crate) struct TextDraw<'a> {
    pub text: &'a str,
    pub origin: Point,
    pub color: Color,
    pub scale: u32,
    pub pixels_per_dip: f32,
    pub transform: Transform,
}

pub(crate) fn append_text(
    batches: &mut Vec<RenderBatch>,
    resources: &mut ResourceManager,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    draw: TextDraw<'_>,
) {
    let scale_factor = draw.pixels_per_dip.max(1.0);
    let [a, b, c, d, _, _] = draw.transform.matrix;
    let translation_only = a == 1.0 && b == 0.0 && c == 0.0 && d == 1.0;
    let transform_scale = (a.hypot(b)).max(c.hypot(d)).max(f32::EPSILON);
    let (text_origin, raster_scale, vertex_transform) = if translation_only {
        (
            draw.transform.point(draw.origin),
            scale_factor,
            Transform::IDENTITY,
        )
    } else {
        (draw.origin, scale_factor * transform_scale, draw.transform)
    };
    let mut system = text_system().lock().expect("text system poisoned");
    let TextSystem { fonts, rasterizer } = &mut *system;
    let buffer = text_buffer(fonts, draw.text, text_font_size(draw.scale));
    let first_baseline = buffer
        .layout_runs()
        .next()
        .map(|run| run.line_y)
        .unwrap_or(0.0);

    for run in buffer.layout_runs() {
        let baseline = text_origin.y.0 + run.line_y - first_baseline;
        for layout_glyph in run.glyphs {
            // Cosmic Text bakes the final font size and the subpixel phase of
            // the glyph into the cache key. The returned x/y are integer
            // physical pixels, so the atlas bitmap is never enlarged a
            // second time by the image pipeline.
            let physical = layout_glyph.physical(
                (text_origin.x.0 * raster_scale, baseline * raster_scale),
                raster_scale,
            );
            let key = physical.cache_key;
            if !resources.glyph_cache.contains_key(&key) {
                let Some(glyph_image) = rasterizer.get_image_uncached(fonts, key) else {
                    continue;
                };
                let image = resources.register_glyph(device, queue, &glyph_image);
                resources.glyph_cache.insert(
                    key,
                    CachedGlyph {
                        image,
                        left: glyph_image.placement.left,
                        top: glyph_image.placement.top,
                        width: glyph_image.placement.width,
                        height: glyph_image.placement.height,
                        is_color: glyph_image.content == SwashContent::Color,
                    },
                );
            }
            resources.touch_glyph(key);
            let glyph = resources
                .glyph_cache
                .get(&key)
                .copied()
                .expect("glyph was inserted into the cache");
            if glyph.width == 0 || glyph.height == 0 {
                continue;
            }
            let image = glyph.image;
            let Some((page, uv)) = resources.ensure_image_atlas_slot(device, queue, image) else {
                continue;
            };
            let left = physical.x + glyph.left;
            let top = physical.y - glyph.top;
            let glyph_color = if glyph.is_color {
                Color::WHITE
            } else {
                draw.color
            };
            let (vertices, indices) =
                image_batch(batches, page, image, ImageSampling::Glyph, vertex_transform);
            append_image(
                vertices,
                indices,
                Rect {
                    origin: Point {
                        x: Dip(left as f32 / raster_scale),
                        y: Dip(top as f32 / raster_scale),
                    },
                    size: zui_core::Size {
                        width: Dip(glyph.width as f32 / raster_scale),
                        height: Dip(glyph.height as f32 / raster_scale),
                    },
                },
                if glyph.is_color { draw.color.a } else { 1.0 },
                glyph_color,
                uv,
            );
        }
    }
}

pub(crate) fn append_rect(
    vertices: &mut Vec<RectVertex>,
    indices: &mut Vec<u32>,
    rect: Rect,
    color: Color,
) {
    let left = rect.origin.x.0;
    let right = rect.origin.x.0 + rect.size.width.0;
    let top = rect.origin.y.0;
    let bottom = rect.origin.y.0 + rect.size.height.0;
    let color = [color.r, color.g, color.b, color.a];
    let base = vertices.len() as u32;
    vertices.extend([
        RectVertex {
            position: [left, top],
            color,
        },
        RectVertex {
            position: [right, top],
            color,
        },
        RectVertex {
            position: [right, bottom],
            color,
        },
        RectVertex {
            position: [left, bottom],
            color,
        },
    ]);
    indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

pub(crate) fn path_bounds(path: &IconPath) -> Rect {
    ClipShape::Path { path: path.clone() }.bounds()
}

pub(crate) fn path_fill_mesh(path: &IconPath, color: Color) -> PathMesh {
    let Some(lyon_path) = path.to_lyon() else {
        return PathMesh::default();
    };
    let options = FillOptions::tolerance(0.1).with_fill_rule(match path.fill_rule {
        FillRule::EvenOdd => LyonFillRule::EvenOdd,
        FillRule::NonZero => LyonFillRule::NonZero,
    });
    let mut geometry: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    if FillTessellator::new()
        .tessellate_path(
            &lyon_path,
            &options,
            &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
                let point = vertex.position();
                [point.x, point.y]
            }),
        )
        .is_err()
    {
        return PathMesh::default();
    }
    PathMesh {
        vertices: geometry
            .vertices
            .into_iter()
            .map(|position| RectVertex {
                position,
                color: [color.r, color.g, color.b, color.a],
            })
            .collect(),
        indices: geometry.indices,
    }
}

pub(crate) fn path_stroke_mesh(path: &IconPath, width: Dip, color: Color) -> PathMesh {
    let Some(lyon_path) = path.to_lyon() else {
        return PathMesh::default();
    };
    let mut geometry: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    if StrokeTessellator::new()
        .tessellate_path(
            &lyon_path,
            &StrokeOptions::tolerance(0.1).with_line_width(width.0),
            &mut BuffersBuilder::new(&mut geometry, |vertex: StrokeVertex| {
                let point = vertex.position();
                [point.x, point.y]
            }),
        )
        .is_err()
    {
        return PathMesh::default();
    }
    PathMesh {
        vertices: geometry
            .vertices
            .into_iter()
            .map(|position| RectVertex {
                position,
                color: [color.r, color.g, color.b, color.a],
            })
            .collect(),
        indices: geometry.indices,
    }
}

pub(crate) fn append_line(
    vertices: &mut Vec<LineVertex>,
    indices: &mut Vec<u32>,
    start: Point,
    end: Point,
    width: Dip,
    color: Color,
) {
    let dx = end.x.0 - start.x.0;
    let dy = end.y.0 - start.y.0;
    let length = (dx * dx + dy * dy).sqrt();
    if length <= f32::EPSILON || width.0 <= 0.0 {
        return;
    }
    let padding = width.0 / 2.0 + 1.0;
    let nx = -dy / length * padding;
    let ny = dx / length * padding;
    let points = [
        Point {
            x: Dip(start.x.0 + nx),
            y: Dip(start.y.0 + ny),
        },
        Point {
            x: Dip(end.x.0 + nx),
            y: Dip(end.y.0 + ny),
        },
        Point {
            x: Dip(end.x.0 - nx),
            y: Dip(end.y.0 - ny),
        },
        Point {
            x: Dip(start.x.0 - nx),
            y: Dip(start.y.0 - ny),
        },
    ];
    let to_vertex = |point: Point| LineVertex {
        position: [point.x.0, point.y.0],
        point: [point.x.0, point.y.0],
        start: [start.x.0, start.y.0],
        end: [end.x.0, end.y.0],
        width: width.0,
        color: [color.r, color.g, color.b, color.a],
    };
    let base = vertices.len() as u32;
    vertices.extend([
        to_vertex(points[0]),
        to_vertex(points[1]),
        to_vertex(points[2]),
        to_vertex(points[3]),
    ]);
    indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

pub(crate) fn append_image(
    vertices: &mut Vec<ImageVertex>,
    indices: &mut Vec<u32>,
    rect: Rect,
    opacity: f32,
    color: Color,
    uv: [f32; 4],
) {
    let left = rect.origin.x.0;
    let right = rect.origin.x.0 + rect.size.width.0;
    let top = rect.origin.y.0;
    let bottom = rect.origin.y.0 + rect.size.height.0;
    let base = vertices.len() as u32;
    vertices.extend([
        ImageVertex {
            position: [left, top],
            uv: [uv[0], uv[1]],
            opacity,
            color: [color.r, color.g, color.b, color.a],
        },
        ImageVertex {
            position: [right, top],
            uv: [uv[2], uv[1]],
            opacity,
            color: [color.r, color.g, color.b, color.a],
        },
        ImageVertex {
            position: [right, bottom],
            uv: [uv[2], uv[3]],
            opacity,
            color: [color.r, color.g, color.b, color.a],
        },
        ImageVertex {
            position: [left, bottom],
            uv: [uv[0], uv[3]],
            opacity,
            color: [color.r, color.g, color.b, color.a],
        },
    ]);
    indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

pub(crate) fn set_scissor(
    pass: &mut wgpu::RenderPass<'_>,
    rect: Option<Rect>,
    size: PhysicalSize,
    scale_factor: ScaleFactor,
) {
    let rect = rect.unwrap_or(Rect {
        origin: Point::default(),
        size: zui_core::Size {
            width: Dip(size.width as f32 / scale_factor.0 as f32),
            height: Dip(size.height as f32 / scale_factor.0 as f32),
        },
    });
    let scale = scale_factor.0.max(1.0) as f32;
    let x = (rect.origin.x.0 * scale).max(0.0).min(size.width as f32) as u32;
    let y = (rect.origin.y.0 * scale).max(0.0).min(size.height as f32) as u32;
    let right = ((rect.origin.x.0 + rect.size.width.0) * scale)
        .min(size.width as f32)
        .max(x as f32) as u32;
    let bottom = ((rect.origin.y.0 + rect.size.height.0) * scale)
        .min(size.height as f32)
        .max(y as f32) as u32;
    pass.set_scissor_rect(x, y, right.saturating_sub(x), bottom.saturating_sub(y));
}

pub(crate) fn append_rounded_rect(
    vertices: &mut Vec<RoundedRectVertex>,
    indices: &mut Vec<u32>,
    rect: Rect,
    radius: Dip,
    color: Color,
) {
    let radius = radius
        .0
        .max(0.0)
        .min(rect.size.width.0 / 2.0)
        .min(rect.size.height.0 / 2.0);
    let left = rect.origin.x.0;
    let right = rect.origin.x.0 + rect.size.width.0;
    let top = rect.origin.y.0;
    let bottom = rect.origin.y.0 + rect.size.height.0;
    let color = [color.r, color.g, color.b, color.a];
    let make_vertex = |position: [f32; 2], local: [f32; 2]| RoundedRectVertex {
        position,
        local,
        size: [rect.size.width.0, rect.size.height.0],
        radius,
        color,
    };
    let base = vertices.len() as u32;
    vertices.extend([
        make_vertex([left, top], [0.0, 0.0]),
        make_vertex([right, top], [rect.size.width.0, 0.0]),
        make_vertex([right, bottom], [rect.size.width.0, rect.size.height.0]),
        make_vertex([left, bottom], [0.0, rect.size.height.0]),
    ]);
    indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

#[allow(dead_code)]
fn glyph_rows(character: char) -> [u8; 7] {
    match character.to_ascii_uppercase() {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01111, 0b10000, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111,
        ],
        'J' => [
            0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001,
        ],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110,
        ],
        '6' => [
            0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ],
        _ => [0; 7],
    }
}
