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
                RenderBatch::Rect(vertices, transform) if !vertices.is_empty() => {
                    Some(GpuBatch::Draw(
                        BatchKind::Rect,
                        arenas.upload(device, queue, VertexArenaKind::Rect, bytemuck::cast_slice(&vertices), vertices.len() as u32),
                        index_arena.upload(device, queue, &sequential_indices(vertices.len())),
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
                RenderBatch::Rounded(vertices, transform) if !vertices.is_empty() => {
                    Some(GpuBatch::Draw(
                        BatchKind::Rounded,
                        arenas.upload(device, queue, VertexArenaKind::Rounded, bytemuck::cast_slice(&vertices), vertices.len() as u32),
                        index_arena.upload(device, queue, &sequential_indices(vertices.len())),
                        transform,
                    ))
                }
                RenderBatch::Line(vertices, transform) if !vertices.is_empty() => {
                    Some(GpuBatch::Draw(
                        BatchKind::Line,
                        arenas.upload(device, queue, VertexArenaKind::Line, bytemuck::cast_slice(&vertices), vertices.len() as u32),
                        index_arena.upload(device, queue, &sequential_indices(vertices.len())),
                        transform,
                    ))
                }
                RenderBatch::Image {
                    image,
                    vertices,
                    transform,
                } if !vertices.is_empty() => Some(GpuBatch::Draw(
                    BatchKind::Image(image),
                    arenas.upload(device, queue, VertexArenaKind::Image, bytemuck::cast_slice(&vertices), vertices.len() as u32),
                    index_arena.upload(device, queue, &sequential_indices(vertices.len())),
                    transform,
                )),
                RenderBatch::Clip(geometry, clip_transform) => {
                    let mask = match geometry {
                        ClipGeometry::Reset => None,
                        ClipGeometry::Rect(rect) => {
                            let mut vertices = Vec::new();
                            append_rect(&mut vertices, rect, Color::WHITE);
                            Some(ClipGpuGeometry {
                                vertices: arenas.upload(device, queue, VertexArenaKind::Rect, bytemuck::cast_slice(&vertices), vertices.len() as u32),
                                indices: None,
                            })
                        }
                        ClipGeometry::Rounded { rect, radius } => {
                            let mut vertices = Vec::new();
                            append_rounded_rect(&mut vertices, rect, radius, Color::WHITE);
                            Some(ClipGpuGeometry {
                                vertices: arenas.upload(device, queue, VertexArenaKind::Rounded, bytemuck::cast_slice(&vertices), vertices.len() as u32),
                                indices: None,
                            })
                        }
                        ClipGeometry::Path { ref vertices, ref indices, .. } => {
                            Some(ClipGpuGeometry {
                                vertices: arenas.upload(device, queue, VertexArenaKind::Rect, bytemuck::cast_slice(vertices), vertices.len() as u32),
                                indices: Some(index_arena.upload(device, queue, indices)),
                            })
                        }
                    };
                    Some(GpuBatch::Clip(geometry, mask, clip_transform))
                }
                _ => None,
            })
            .collect()
}

fn sequential_indices(vertex_count: usize) -> Vec<u32> {
    (0..vertex_count as u32).collect()
}

pub(crate) fn cached_system_fonts() -> &'static [fontdue::Font] {
    SYSTEM_FONTS.get_or_init(load_system_fonts).as_slice()
}

fn load_system_fonts() -> Vec<fontdue::Font> {
    let candidates = [
        std::env::var("ZUI_FONT_PATH").ok(),
        std::env::var("ZUI_LATIN_FONT_PATH").ok(),
        Some("/System/Library/Fonts/SFNS.ttf".into()),
        Some("/System/Library/Fonts/SFNSRounded.ttf".into()),
        Some("/System/Library/Fonts/Hiragino Sans GB.ttc".into()),
        Some("/System/Library/Fonts/Supplemental/Verdana.ttf".into()),
        Some("/System/Library/Fonts/Supplemental/Tahoma.ttf".into()),
        Some("/System/Library/Fonts/Supplemental/Arial.ttf".into()),
        Some("/System/Library/Fonts/Supplemental/Arial Unicode.ttf".into()),
        Some("/System/Library/Fonts/Supplemental/NISC18030.ttf".into()),
        Some("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".into()),
        std::env::var("ZUI_CJK_FONT_PATH").ok(),
        Some("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc".into()),
        Some("/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc".into()),
        Some("C:\\Windows\\Fonts\\segoeui.ttf".into()),
        Some("C:\\Windows\\Fonts\\msyh.ttc".into()),
    ];
    candidates
        .into_iter()
        .flatten()
        .filter_map(|path| std::fs::read(path).ok())
        .filter_map(|bytes| fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).ok())
        .collect()
}

pub(crate) fn append_text(
    batches: &mut Vec<RenderBatch>,
    resources: &mut ResourceManager,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    text: &str,
    origin: Point,
    color: Color,
    scale: u32,
    scale_factor: f32,
    transform: Transform,
) {
    let fonts = resources.fonts;
    if !fonts.is_empty() {
        let scale_factor = scale_factor.max(1.0);
        let logical_font_size = (scale.max(1) * 7) as f32;
        let font_size = logical_font_size * scale_factor;
        let baseline = (origin.y.0 + logical_font_size * 0.8) * scale_factor;
        let mut x = origin.x.0;
        for character in text.chars() {
            let font_id = *resources.font_cache.entry(character).or_insert_with(|| {
                fonts
                    .iter()
                    .enumerate()
                    .find(|(_, font)| font.lookup_glyph_index(character) != 0)
                    .map(|(font_id, _)| font_id)
            });
            let Some(font_id) = font_id else {
                x += font_size;
                continue;
            };
            let font = &fonts[font_id];
            let key = (
                font_id,
                character,
                scale.max(1),
                (scale_factor * 100.0).round() as u32,
            );
            if !resources.glyph_cache.contains_key(&key) {
                let (metrics, bitmap) = font.rasterize(character, font_size);
                let image = resources.register_glyph(
                    device,
                    queue,
                    metrics.width as u32,
                    metrics.height as u32,
                    &bitmap,
                );
                resources.glyph_cache.insert(key, CachedGlyph { metrics, image });
            }
            let glyph = resources
                .glyph_cache
                .get(&key)
                .expect("glyph was inserted into the cache");
            let metrics = glyph.metrics;
            // Fontdue reports glyph bounds relative to the baseline. Keeping
            // one baseline for the complete run prevents punctuation and
            // lowercase glyphs from drifting vertically.
            let top = baseline - metrics.height as f32 - metrics.ymin as f32;
            append_image(
                image_batch(batches, glyph.image, transform),
                Rect {
                    origin: Point {
                        x: Dip(x),
                        y: Dip(top / scale_factor),
                    },
                    size: zui_core::Size {
                        width: Dip(metrics.width as f32 / scale_factor),
                        height: Dip(metrics.height as f32 / scale_factor),
                    },
                },
                1.0,
                color,
            );
            x += metrics.advance_width / scale_factor;
        }
        return;
    }

    let scale = scale.max(1) as f32;
    let mut x = origin.x.0;
    for character in text.chars() {
        for (row, bits) in glyph_rows(character).iter().enumerate() {
            for column in 0..5 {
                if bits & (1 << (4 - column)) != 0 {
                    append_rect(
                        rect_batch(batches, transform),
                        Rect {
                            origin: Point {
                                x: Dip(x + column as f32 * scale as f32),
                                y: Dip(origin.y.0 + row as f32 * scale as f32),
                            },
                            size: zui_core::Size {
                                width: Dip(scale as f32),
                                height: Dip(scale as f32),
                            },
                        },
                        color,
                    );
                }
            }
        }
        x += 6.0 * scale;
    }
}

pub(crate) fn append_rect(vertices: &mut Vec<RectVertex>, rect: Rect, color: Color) {
    let left = rect.origin.x.0;
    let right = rect.origin.x.0 + rect.size.width.0;
    let top = rect.origin.y.0;
    let bottom = rect.origin.y.0 + rect.size.height.0;
    let color = [color.r, color.g, color.b, color.a];
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
            position: [left, top],
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

pub(crate) fn path_stroke_mesh(
    path: &IconPath,
    width: Dip,
    color: Color,
) -> PathMesh {
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

pub(crate) fn append_line(vertices: &mut Vec<LineVertex>, start: Point, end: Point, width: Dip, color: Color) {
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
    vertices.extend([
        to_vertex(points[0]),
        to_vertex(points[1]),
        to_vertex(points[2]),
        to_vertex(points[0]),
        to_vertex(points[2]),
        to_vertex(points[3]),
    ]);
}

pub(crate) fn append_image(vertices: &mut Vec<ImageVertex>, rect: Rect, opacity: f32, color: Color) {
    let left = rect.origin.x.0;
    let right = rect.origin.x.0 + rect.size.width.0;
    let top = rect.origin.y.0;
    let bottom = rect.origin.y.0 + rect.size.height.0;
    vertices.extend([
        ImageVertex {
            position: [left, top],
            uv: [0.0, 0.0],
            opacity,
            color: [color.r, color.g, color.b, color.a],
        },
        ImageVertex {
            position: [right, top],
            uv: [1.0, 0.0],
            opacity,
            color: [color.r, color.g, color.b, color.a],
        },
        ImageVertex {
            position: [right, bottom],
            uv: [1.0, 1.0],
            opacity,
            color: [color.r, color.g, color.b, color.a],
        },
        ImageVertex {
            position: [left, top],
            uv: [0.0, 0.0],
            opacity,
            color: [color.r, color.g, color.b, color.a],
        },
        ImageVertex {
            position: [right, bottom],
            uv: [1.0, 1.0],
            opacity,
            color: [color.r, color.g, color.b, color.a],
        },
        ImageVertex {
            position: [left, bottom],
            uv: [0.0, 1.0],
            opacity,
            color: [color.r, color.g, color.b, color.a],
        },
    ]);
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

pub(crate) fn command_list_hash(commands: &[PaintCommand]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{commands:?}").hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn append_rounded_rect(
    vertices: &mut Vec<RoundedRectVertex>,
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
    vertices.extend([
        make_vertex([left, top], [0.0, 0.0]),
        make_vertex([right, top], [rect.size.width.0, 0.0]),
        make_vertex([right, bottom], [rect.size.width.0, rect.size.height.0]),
        make_vertex([left, top], [0.0, 0.0]),
        make_vertex([right, bottom], [rect.size.width.0, rect.size.height.0]),
        make_vertex([left, bottom], [0.0, rect.size.height.0]),
    ]);
}

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
