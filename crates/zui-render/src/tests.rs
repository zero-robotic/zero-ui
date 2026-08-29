//! Unit tests for render commands, retained nodes, and GPU helpers.

use super::*;

#[test]
fn noop_device_can_create_gpu_objects() {
    let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let _encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let transform_layout = create_transform_bind_group_layout(&device);
    let _pipeline =
        create_rounded_rect_pipeline(&device, wgpu::TextureFormat::Rgba8Unorm, &transform_layout);
    let _stencil_pipeline = create_stencil_pipeline(&device, wgpu::TextureFormat::Rgba8Unorm);
    let _rounded_stencil_pipeline = create_stencil_rounded_pipeline(
        &device,
        wgpu::TextureFormat::Rgba8Unorm,
        &transform_layout,
    );
    let _line_pipeline =
        create_line_pipeline(&device, wgpu::TextureFormat::Rgba8Unorm, &transform_layout);
    let _image_pipeline =
        create_image_pipeline(&device, wgpu::TextureFormat::Rgba8Unorm, &transform_layout);
}

#[test]
fn gpu_transform_maps_local_dips_to_clip_space() {
    let transform = compose_transform(
        Transform::translate(Dip(10.0), Dip(20.0)),
        Transform::scale(2.0, 3.0),
    );
    let matrix = gpu_transform(
        transform,
        PhysicalSize {
            width: 100,
            height: 200,
        },
    )
    .matrix;
    // Column-major affine matrix: x = 2 * (2x + 10) / 100 - 1;
    // y = 1 - 2 * (3y + 20) / 200.
    assert_eq!(matrix[0][0], 0.04);
    assert_eq!(matrix[1][1], -0.03);
    assert_eq!(matrix[3][0], -0.8);
    assert_eq!(matrix[3][1], 0.8);
}

#[test]
fn render_node_keeps_commands_before_children() {
    let mut node = RenderNode::new(Rect::default());
    node.commands_mut().push(PaintCommand::Rect {
        rect: Rect::default(),
        color: Color::WHITE,
    });
    let mut child = RenderNode::new(Rect::default());
    child.commands_mut().push(PaintCommand::Clear(Color::BLACK));
    node.add_child(child);
    assert!(matches!(node.commands[0], PaintCommand::Rect { .. }));
    assert!(matches!(
        node.children[0].commands[0],
        PaintCommand::Clear(_)
    ));
}

#[test]
fn render_node_index_resolves_nested_widget_paths() {
    let mut root = RenderNode::new(Rect::default());
    root.source_id = Some(1);
    let mut child = RenderNode::new(Rect::default());
    child.source_id = Some(2);
    let mut grandchild = RenderNode::new(Rect::default());
    grandchild.source_id = Some(3);
    child.add_child(grandchild);
    root.add_child(child);

    let index = root.build_index();
    assert_eq!(index.path_for(1), Some([].as_slice()));
    assert_eq!(index.path_for(3), Some([0, 0].as_slice()));
    assert_eq!(
        index.node(&root, 3).and_then(|node| node.source_id),
        Some(3)
    );
}

#[test]
fn dirty_path_propagates_flags_and_region_to_ancestors() {
    let mut root = RenderNode::new(Rect::default());
    let mut child = RenderNode::new(Rect::default());
    let leaf = RenderNode::new(Rect::default());
    root.add_child(child.clone());
    child.add_child(leaf);
    *root.child_mut(0).expect("child was added") = child;
    root.clear_dirty();

    let region = Rect {
        origin: Point {
            x: Dip(4.0),
            y: Dip(8.0),
        },
        size: Size {
            width: Dip(12.0),
            height: Dip(16.0),
        },
    };
    root.mark_dirty_path(&[0, 0], DirtyFlags::PAINT, Some(region));

    assert!(root.dirty.flags.contains(DirtyFlags::CHILDREN));
    assert!(root.dirty.regions.as_slice().contains(&region));
    assert!(root.children[0].dirty.flags.contains(DirtyFlags::CHILDREN));
    assert!(root.children[0].dirty.regions.as_slice().contains(&region));
    assert!(root.children[0].children[0]
        .dirty
        .flags
        .contains(DirtyFlags::PAINT));
    assert!(root.children[0].children[0]
        .dirty
        .regions
        .as_slice()
        .contains(&region));
}

#[test]
fn clean_render_node_subtrees_are_reused_by_source_id() {
    let mut previous = RenderNode::new(Rect::default());
    previous.source_id = Some(1);
    let mut previous_clean = RenderNode::new(Rect::default());
    previous_clean.source_id = Some(2);
    previous_clean.commands_mut().push(PaintCommand::Rect {
        rect: Rect::default(),
        color: Color::WHITE,
    });
    previous.add_child(previous_clean);
    let mut previous_dirty = RenderNode::new(Rect::default());
    previous_dirty.source_id = Some(3);
    previous.add_child(previous_dirty);

    let mut current = previous.clone();
    current.child_mut(0).expect("first child").commands_mut()[0] = PaintCommand::Rect {
        rect: Rect::default(),
        color: Color::BLACK,
    };
    current
        .child_mut(1)
        .expect("second child")
        .commands_mut()
        .push(PaintCommand::Clear(Color::BLACK));
    let dirty = vec![vec![1]];
    let merged = current.reuse_clean_subtrees(Some(&previous), &dirty, false);

    assert_eq!(
        merged.children[0].commands[0],
        previous.children[0].commands[0]
    );
    assert_eq!(merged.children[1].commands.len(), 1);
}

#[test]
fn clean_render_node_clone_shares_storage_until_dirty_mutation() {
    let mut previous = RenderNode::new(Rect::default());
    previous
        .commands_mut()
        .push(PaintCommand::Clear(Color::BLACK));
    previous.add_child(RenderNode::new(Rect::default()));

    let mut current = previous.clone();
    assert!(Arc::ptr_eq(&previous.commands, &current.commands));
    assert!(Arc::ptr_eq(&previous.children, &current.children));

    current
        .commands_mut()
        .push(PaintCommand::Clear(Color::WHITE));
    current.child_mut(0).expect("child exists").set_opacity(0.5);

    assert!(!Arc::ptr_eq(&previous.commands, &current.commands));
    assert!(!Arc::ptr_eq(&previous.children, &current.children));
    assert_eq!(previous.commands.len(), 1);
    assert_eq!(previous.children[0].opacity, 1.0);
}

#[test]
fn shared_item_damage_is_coalesced_once() {
    let items = std::collections::BTreeMap::from([(
        vec![0],
        RenderNodeItem {
            bounds: Rect {
                origin: Point {
                    x: Dip(4.0),
                    y: Dip(4.0),
                },
                size: zui_core::Size {
                    width: Dip(4.0),
                    height: Dip(4.0),
                },
            },
            transform: Transform::IDENTITY,
            opacity: 1.0,
            clips: Vec::new(),
            commands: Arc::new(Vec::new()),
        },
    )]);
    let regions = [
        Rect {
            origin: Point::default(),
            size: zui_core::Size {
                width: Dip(6.0),
                height: Dip(10.0),
            },
        },
        Rect {
            origin: Point {
                x: Dip(6.0),
                y: Dip(0.0),
            },
            size: zui_core::Size {
                width: Dip(6.0),
                height: Dip(10.0),
            },
        },
    ];
    let index = SpatialIndex::build(&items);
    let merged = coalesce_damage_for_spatial_index(&regions, Some(&index));
    assert_eq!(merged.len(), 1);
}

#[test]
fn render_node_applies_transform_and_opacity() {
    let mut node = RenderNode::new(Rect::default());
    node.set_transform(Transform::translate(Dip(10.0), Dip(20.0)));
    node.set_opacity(0.5);
    node.commands_mut().push(PaintCommand::Rect {
        rect: Rect {
            origin: Point::default(),
            size: zui_core::Size {
                width: Dip(4.0),
                height: Dip(5.0),
            },
        },
        color: Color::WHITE,
    });
    assert_eq!(node.transform, Transform::translate(Dip(10.0), Dip(20.0)));
    assert_eq!(node.opacity, 0.5);
    assert_eq!(node.commands.len(), 1);
    assert!(matches!(node.commands[0], PaintCommand::Rect { .. }));
}

#[test]
fn render_node_clips_commands_and_propagates_dirty_state() {
    let mut node = RenderNode::new(Rect::default());
    node.set_clip(Some(ClipShape::Rect(Rect {
        origin: Point::default(),
        size: zui_core::Size {
            width: Dip(5.0),
            height: Dip(5.0),
        },
    })));
    node.commands_mut().push(PaintCommand::Rect {
        rect: Rect {
            origin: Point {
                x: Dip(20.0),
                y: Dip(20.0),
            },
            size: zui_core::Size {
                width: Dip(2.0),
                height: Dip(2.0),
            },
        },
        color: Color::WHITE,
    });
    assert!(node.dirty.flags.contains(DirtyFlags::PAINT));
    assert_eq!(
        node.clips.as_slice(),
        [ClipShape::Rect(Rect {
            origin: Point::default(),
            size: zui_core::Size {
                width: Dip(5.0),
                height: Dip(5.0)
            },
        })]
    );

    node.clear_dirty();
    assert!(!node.dirty.is_dirty());
    assert!(node.dirty.flags.is_empty());
    node.mark_dirty_region(Rect::default());
    assert!(node.dirty.is_dirty());
    assert_eq!(node.dirty.regions.union(), Some(Rect::default()));
}

#[test]
fn paint_commands_validate_geometry_and_report_bounds() {
    let command = PaintCommand::RoundedRect {
        rect: Rect {
            origin: Point {
                x: Dip(2.0),
                y: Dip(3.0),
            },
            size: zui_core::Size {
                width: Dip(10.0),
                height: Dip(8.0),
            },
        },
        radius: Dip(2.0),
        color: Color::WHITE,
    };
    assert!(command.validate().is_ok());
    assert_eq!(command.bounds().unwrap().size.width, Dip(10.0));

    let invalid = PaintCommand::Line {
        start: Point::default(),
        end: Point {
            x: Dip(1.0),
            y: Dip(1.0),
        },
        width: Dip(0.0),
        color: Color::WHITE,
    };
    assert!(invalid.validate().is_err());
}

#[test]
fn image_resource_requires_matching_rgba_data() {
    assert!(ImageResource::new(2, 2, vec![0; 16]).is_ok());
    assert!(ImageResource::new(2, 2, vec![0; 15]).is_err());
}

#[test]
fn scoped_command_transforms_are_preserved_for_gpu_batches() {
    let mut commands = Vec::new();
    commands.push(PaintCommand::PushTransform(Transform::translate(
        Dip(3.0),
        Dip(4.0),
    )));
    commands.push(PaintCommand::PushClip(ClipShape::Rect(Rect {
        origin: Point {
            x: Dip(1.0),
            y: Dip(2.0),
        },
        size: zui_core::Size {
            width: Dip(10.0),
            height: Dip(11.0),
        },
    })));
    commands.push(PaintCommand::Rect {
        rect: Rect {
            origin: Point {
                x: Dip(2.0),
                y: Dip(3.0),
            },
            size: zui_core::Size {
                width: Dip(4.0),
                height: Dip(5.0),
            },
        },
        color: Color::WHITE,
    });
    commands.push(PaintCommand::PopClip);
    commands.push(PaintCommand::PopTransform);
    let Ok(mut renderer) = Renderer::new_blocking() else {
        return;
    };
    let batches = renderer.build_render_batches(
        &commands,
        PhysicalSize {
            width: 100,
            height: 100,
        },
        1.0,
        Transform::IDENTITY,
        1.0,
    );
    assert!(
        matches!(batches.first(), Some(RenderBatch::Clip(_, transform)) if *transform == Transform::translate(Dip(3.0), Dip(4.0)))
    );
    assert!(
        matches!(batches.get(1), Some(RenderBatch::Rect(_, _, transform)) if *transform == Transform::translate(Dip(3.0), Dip(4.0)))
    );
}

#[test]
fn path_mesh_preserves_tessellator_indices() {
    let path = IconPath::from_commands(
        vec![
            PathCommand::MoveTo(Point {
                x: Dip(0.0),
                y: Dip(0.0),
            }),
            PathCommand::LineTo(Point {
                x: Dip(20.0),
                y: Dip(0.0),
            }),
            PathCommand::LineTo(Point {
                x: Dip(20.0),
                y: Dip(20.0),
            }),
            PathCommand::LineTo(Point {
                x: Dip(0.0),
                y: Dip(20.0),
            }),
            PathCommand::Close,
        ],
        FillRule::NonZero,
    );
    let mesh = path_fill_mesh(&path, Color::WHITE);
    assert!(!mesh.vertices.is_empty());
    assert_eq!(mesh.indices.len() % 3, 0);
    assert!(mesh
        .indices
        .iter()
        .all(|index| (*index as usize) < mesh.vertices.len()));
    assert!(mesh.indices.len() > mesh.vertices.len());
}

#[test]
fn resource_handles_track_references() {
    let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut resources = ResourceManager::new(&device, &[]);
    let handle = ResourceHandle::Image(ImageId(7));
    resources.retain(handle);
    resources.retain(handle);
    assert_eq!(resources.reference_count(handle), 2);
    resources.release(handle);
    assert_eq!(resources.reference_count(handle), 1);
    resources.release(handle);
    assert_eq!(resources.reference_count(handle), 0);
}

#[test]
fn render_node_composes_multiple_clips() {
    let mut node = RenderNode::for_widget(Rect {
        origin: Point::default(),
        size: zui_core::Size {
            width: Dip(40.0),
            height: Dip(40.0),
        },
    });
    node.push_clip(ClipShape::Rect(Rect {
        origin: Point::default(),
        size: zui_core::Size {
            width: Dip(30.0),
            height: Dip(30.0),
        },
    }));
    node.push_clip(ClipShape::Rect(Rect {
        origin: Point {
            x: Dip(10.0),
            y: Dip(10.0),
        },
        size: zui_core::Size {
            width: Dip(30.0),
            height: Dip(30.0),
        },
    }));
    let (item, _, _, _, _) = build_render_item(&node, Transform::IDENTITY, None, 1.0, &[]);
    assert_eq!(item.clips.len(), 2);
    assert_eq!(
        item.bounds.origin,
        Point {
            x: Dip(10.0),
            y: Dip(10.0)
        }
    );
    assert_eq!(item.bounds.size.width, Dip(20.0));
}

#[test]
fn normalizing_nested_cached_nodes_is_idempotent() {
    let mut root = RenderNode::for_widget(Rect {
        origin: Point {
            x: Dip(10.0),
            y: Dip(20.0),
        },
        size: zui_core::Size {
            width: Dip(100.0),
            height: Dip(80.0),
        },
    });
    root.add_child(RenderNode::for_widget(Rect {
        origin: Point {
            x: Dip(30.0),
            y: Dip(50.0),
        },
        size: zui_core::Size {
            width: Dip(20.0),
            height: Dip(10.0),
        },
    }));
    root.normalize_local_coordinates();
    assert_eq!(root.transform.matrix[4], 10.0);
    assert_eq!(root.children[0].transform.matrix[4], 20.0);
    let snapshot = root.clone();
    root.normalize_local_coordinates();
    assert_eq!(root, snapshot);
}

#[test]
fn dirty_regions_keep_separate_damage_areas() {
    let mut regions = DirtyRegionSet::new();
    regions.add(Rect {
        origin: Point {
            x: Dip(1.0),
            y: Dip(1.0),
        },
        size: zui_core::Size {
            width: Dip(4.0),
            height: Dip(4.0),
        },
    });
    regions.add(Rect {
        origin: Point {
            x: Dip(20.0),
            y: Dip(20.0),
        },
        size: zui_core::Size {
            width: Dip(4.0),
            height: Dip(4.0),
        },
    });
    assert_eq!(regions.as_slice().len(), 2);
    regions.add(Rect {
        origin: Point {
            x: Dip(3.0),
            y: Dip(3.0),
        },
        size: zui_core::Size {
            width: Dip(4.0),
            height: Dip(4.0),
        },
    });
    assert_eq!(regions.as_slice().len(), 2);
}

#[test]
fn render_node_gpu_key_changes_with_subtree() {
    let mut parent = RenderNode::new(Rect::default());
    let first = parent.gpu_cache_key();
    parent.add_child(RenderNode::new(Rect {
        origin: Point::default(),
        size: zui_core::Size {
            width: Dip(4.0),
            height: Dip(4.0),
        },
    }));
    assert_ne!(first, parent.gpu_cache_key());
}

#[test]
fn ordered_indirect_groups_do_not_cross_arena_pages() {
    let vertex = |page| VertexArenaAllocation {
        kind: VertexArenaKind::Rect,
        page,
        range: 0..64,
        vertex_count: 4,
    };
    let index = |page| IndexArenaAllocation {
        page,
        range: 0..24,
        index_count: 6,
    };
    let groups = group_ordered_draws(vec![
        GpuBatch::Draw(BatchKind::Rect, vertex(0), index(0), Transform::IDENTITY),
        GpuBatch::Draw(BatchKind::Rect, vertex(0), index(0), Transform::IDENTITY),
        GpuBatch::Draw(BatchKind::Rect, vertex(1), index(0), Transform::IDENTITY),
    ]);
    assert!(matches!(groups.as_slice(), [
            GpuBatch::DrawGroup(_, first, _, _),
            GpuBatch::DrawGroup(_, second, _, _),
        ] if first.len() == 2 && second.len() == 1));
}

#[test]
fn retained_indirect_commands_are_uploaded_once() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut arena = IndirectArena::new(&device);
    let mut groups = group_ordered_draws(vec![GpuBatch::Draw(
        BatchKind::Rect,
        VertexArenaAllocation {
            kind: VertexArenaKind::Rect,
            page: 0,
            range: 0..64,
            vertex_count: 4,
        },
        IndexArenaAllocation {
            page: 0,
            range: 0..24,
            index_count: 6,
        },
        Transform::IDENTITY,
    )]);
    assert_eq!(
        prepare_indirect_draws(&mut groups, &mut arena, &device, &queue, false).len(),
        1
    );
    assert!(prepare_indirect_draws(&mut groups, &mut arena, &device, &queue, false).is_empty());
}

#[test]
fn composition_tiles_coalesce_damage_to_physical_tile_bounds() {
    let tiles = CompositionTiles::new(PhysicalSize {
        width: 320,
        height: 240,
    });
    let regions = tiles.regions(
        &[Rect {
            origin: Point {
                x: Dip(126.0),
                y: Dip(4.0),
            },
            size: zui_core::Size {
                width: Dip(8.0),
                height: Dip(8.0),
            },
        }],
        ScaleFactor(1.0),
        false,
    );
    assert_eq!(regions.len(), 2);
    assert_eq!(regions[0].origin.x, Dip(0.0));
    assert_eq!(regions[1].origin.x, Dip(128.0));
}

#[test]
fn adjacent_images_on_one_atlas_page_share_a_material_group() {
    let vertex = |offset| VertexArenaAllocation {
        kind: VertexArenaKind::Image,
        page: 0,
        range: offset..offset + 80,
        vertex_count: 4,
    };
    let index = |offset| IndexArenaAllocation {
        page: 0,
        range: offset..offset + 24,
        index_count: 6,
    };
    let groups = group_ordered_draws(vec![
        GpuBatch::Draw(
            BatchKind::Image {
                page: 2,
                images: Arc::new(vec![ImageId(10)]),
            },
            vertex(0),
            index(0),
            Transform::IDENTITY,
        ),
        GpuBatch::Draw(
            BatchKind::Image {
                page: 2,
                images: Arc::new(vec![ImageId(11)]),
            },
            vertex(80),
            index(24),
            Transform::IDENTITY,
        ),
    ]);
    assert!(
        matches!(groups.as_slice(), [GpuBatch::DrawGroup(BatchKind::Image { page: 2, .. }, draws, _, _)] if draws.len() == 2)
    );
    assert_eq!(
        batch_resource_handles(&groups),
        vec![
            ResourceHandle::Image(ImageId(10)),
            ResourceHandle::Image(ImageId(11))
        ]
    );
}

#[test]
fn image_vertices_are_built_once_for_an_atlas_page_run() {
    let mut batches = Vec::new();
    let rect = Rect {
        origin: Point::default(),
        size: zui_core::Size {
            width: Dip(10.0),
            height: Dip(10.0),
        },
    };
    let (vertices, indices) = image_batch(&mut batches, 3, ImageId(1), Transform::IDENTITY);
    append_image(
        vertices,
        indices,
        rect,
        1.0,
        Color::WHITE,
        [0.0, 0.0, 0.5, 0.5],
    );
    let (vertices, indices) = image_batch(&mut batches, 3, ImageId(2), Transform::IDENTITY);
    append_image(
        vertices,
        indices,
        rect,
        1.0,
        Color::WHITE,
        [0.5, 0.0, 1.0, 0.5],
    );
    assert!(
        matches!(batches.as_slice(), [RenderBatch::Image { page: 3, images, vertices, indices, .. }]
        if images.as_slice() == [ImageId(1), ImageId(2)] && vertices.len() == 8 && indices.len() == 12)
    );
}

#[test]
fn adjacent_retained_groups_share_one_contiguous_indirect_submission() {
    let vertex = |offset| VertexArenaAllocation {
        kind: VertexArenaKind::Rect,
        page: 0,
        range: offset..offset + 64,
        vertex_count: 4,
    };
    let index = |offset| IndexArenaAllocation {
        page: 0,
        range: offset..offset + 24,
        index_count: 6,
    };
    let stride = std::mem::size_of::<wgpu::util::DrawIndexedIndirectArgs>() as u64;
    let groups = group_ordered_draws(vec![
        GpuBatch::DrawGroup(
            BatchKind::Rect,
            Arc::new(vec![(vertex(0), index(0))]),
            Transform::IDENTITY,
            Some(IndirectDrawRange {
                offset: 0,
                count: 1,
            }),
        ),
        GpuBatch::DrawGroup(
            BatchKind::Rect,
            Arc::new(vec![(vertex(64), index(24))]),
            Transform::IDENTITY,
            Some(IndirectDrawRange {
                offset: stride,
                count: 1,
            }),
        ),
    ]);
    assert!(
        matches!(groups.as_slice(), [GpuBatch::DrawGroup(_, draws, _, Some(range))]
        if draws.len() == 2 && range.count == 2)
    );
}

#[test]
fn grouped_batches_keep_all_arena_allocations_owned_by_the_node() {
    let allocations = batch_vertex_allocations(&[GpuBatch::DrawGroup(
        BatchKind::Rect,
        Arc::new(vec![
            (
                VertexArenaAllocation {
                    kind: VertexArenaKind::Rect,
                    page: 0,
                    range: 0..64,
                    vertex_count: 4,
                },
                IndexArenaAllocation {
                    page: 0,
                    range: 0..24,
                    index_count: 6,
                },
            ),
            (
                VertexArenaAllocation {
                    kind: VertexArenaKind::Rect,
                    page: 0,
                    range: 64..128,
                    vertex_count: 4,
                },
                IndexArenaAllocation {
                    page: 0,
                    range: 24..48,
                    index_count: 6,
                },
            ),
        ]),
        Transform::IDENTITY,
        None,
    )]);
    assert_eq!(allocations.len(), 2);
}

#[test]
fn arena_free_ranges_are_coalesced_for_reuse() {
    let mut free = vec![0..16, 48..64];
    release_arena_range(&mut free, 16..48);
    assert_eq!(free, vec![0..64]);
}

#[test]
fn tile_submission_index_updates_only_changed_subtree_paths() {
    let item = |x| RenderNodeItem {
        bounds: Rect {
            origin: Point {
                x: Dip(x),
                y: Dip(0.0),
            },
            size: zui_core::Size {
                width: Dip(20.0),
                height: Dip(20.0),
            },
        },
        transform: Transform::IDENTITY,
        opacity: 1.0,
        clips: Vec::new(),
        commands: Arc::new(Vec::new()),
    };
    let mut items = BTreeMap::new();
    items.insert(vec![0], item(0.0));
    items.insert(vec![1], item(200.0));
    let mut index = TileSubmissionIndex::build(&items, ScaleFactor(1.0));
    assert_eq!(
        index.query(Rect {
            origin: Point::default(),
            size: zui_core::Size {
                width: Dip(127.0),
                height: Dip(127.0)
            },
        }),
        vec![vec![0]],
    );
    items.insert(vec![0], item(260.0));
    index.update_subtrees(&items, &[vec![0]]);
    assert!(index
        .query(Rect {
            origin: Point::default(),
            size: zui_core::Size {
                width: Dip(127.0),
                height: Dip(127.0)
            },
        })
        .is_empty());
    assert_eq!(
        index.query(Rect {
            origin: Point {
                x: Dip(256.0),
                y: Dip(0.0)
            },
            size: zui_core::Size {
                width: Dip(63.0),
                height: Dip(127.0)
            },
        }),
        vec![vec![0]],
    );
}

#[test]
fn resource_eviction_never_discards_a_retained_image() {
    let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut resources = ResourceManager::new(&device, &[]);
    let image = ImageId(7);
    resources.gpu_images.insert(
        image,
        GpuImage {
            bytes: 4,
            slot: AtlasSlot {
                page: 0,
                origin: wgpu::Origin3d::ZERO,
                width: 1,
                height: 1,
                atlas_width: 1,
                atlas_height: 1,
            },
        },
    );
    resources.last_used.insert(image, 1);
    resources.retain(ResourceHandle::Image(image));
    assert!(!resources.evict_one_gpu_image());
    assert!(resources.gpu_images.contains_key(&image));
}

#[test]
fn retained_image_batches_contribute_resource_references() {
    let image = ImageId(42);
    let handles = batch_resource_handles(&[GpuBatch::DrawGroup(
        BatchKind::Image {
            page: 0,
            images: Arc::new(vec![image]),
        },
        Arc::new(Vec::new()),
        Transform::IDENTITY,
        None,
    )]);
    assert_eq!(handles, vec![ResourceHandle::Image(image)]);
}

#[test]
fn rectangular_clip_uses_scissor_without_gpu_mask_geometry() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut vertices = VertexArenas::new(&device);
    let mut indices = IndexArena::new(&device);
    let batches = build_gpu_batches(
        &device,
        &queue,
        &mut vertices,
        &mut indices,
        vec![RenderBatch::Clip(
            ClipGeometry::Rect(Rect {
                origin: Point::default(),
                size: zui_core::Size {
                    width: Dip(10.0),
                    height: Dip(10.0),
                },
            }),
            Transform::IDENTITY,
        )],
    );
    assert!(matches!(
        batches.as_slice(),
        [GpuBatch::Clip(ClipGeometry::Rect(_), None, _)]
    ));
}

#[test]
fn atlas_free_slots_coalesce_after_resource_release() {
    let slot = |x| AtlasSlot {
        page: 0,
        origin: wgpu::Origin3d { x, y: 0, z: 0 },
        width: 16,
        height: 16,
        atlas_width: 64,
        atlas_height: 64,
    };
    let mut free = vec![slot(0), slot(16)];
    coalesce_atlas_slots(&mut free);
    assert_eq!(free.len(), 1);
    assert_eq!(free[0].width, 32);
}
