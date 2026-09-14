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
    let _damage_clear_pipeline =
        create_damage_clear_pipeline(&device, wgpu::TextureFormat::Rgba8Unorm);
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
fn surface_metrics_keep_device_and_ui_scales_independent() {
    let metrics = SurfaceMetrics::new(
        PhysicalSize {
            width: 2400,
            height: 1800,
        },
        ScaleFactor(2.0),
        1.5,
    );
    assert_eq!(metrics.device_scale_factor, ScaleFactor(2.0));
    assert_eq!(metrics.ui_scale, 1.5);
    assert_eq!(metrics.pixels_per_content_dip(), ScaleFactor(3.0));
}

#[test]
fn renderer_keeps_the_adapters_native_surface_resolution_limit() {
    let mut adapter_limits = wgpu::Limits::downlevel_defaults();
    adapter_limits.max_texture_dimension_1d = 16_384;
    adapter_limits.max_texture_dimension_2d = 16_384;
    adapter_limits.max_texture_dimension_3d = 2_048;

    let requested = renderer_required_limits(adapter_limits);

    assert_eq!(requested.max_texture_dimension_1d, 16_384);
    assert_eq!(requested.max_texture_dimension_2d, 16_384);
    assert_eq!(requested.max_texture_dimension_3d, 2_048);
}

#[test]
fn maximized_high_dpi_surface_stays_at_native_pixel_size() {
    let physical_size = PhysicalSize {
        width: 2_922,
        height: 1_010,
    };
    let configured = constrain_surface_size(physical_size, 16_384);
    let metrics = SurfaceMetrics::new(physical_size, ScaleFactor(2.0), 1.0);

    assert_eq!(configured, physical_size);
    assert_eq!(
        Renderer::surface_scale_factor(metrics, configured),
        ScaleFactor(2.0)
    );
}

#[test]
fn surfaces_beyond_the_real_gpu_limit_keep_their_aspect_ratio() {
    let configured = constrain_surface_size(
        PhysicalSize {
            width: 32_768,
            height: 18_432,
        },
        16_384,
    );

    assert_eq!(
        configured,
        PhysicalSize {
            width: 16_384,
            height: 9_216,
        }
    );
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
    root.id = Some(1);
    let mut child = RenderNode::new(Rect::default());
    child.id = Some(2);
    let mut grandchild = RenderNode::new(Rect::default());
    grandchild.id = Some(3);
    child.add_child(grandchild);
    root.add_child(child);

    let index = root.build_index();
    assert_eq!(index.path_for(1), Some([].as_slice()));
    assert_eq!(index.path_for(3), Some([0, 0].as_slice()));
    assert_eq!(index.node(&root, 3).and_then(|node| node.id), Some(3));
}

#[test]
fn scene_update_merges_node_damage_without_losing_full_rebuild() {
    let region = |x| Rect {
        origin: Point {
            x: Dip(x),
            y: Dip(0.0),
        },
        size: zui_core::Size {
            width: Dip(10.0),
            height: Dip(10.0),
        },
    };
    let mut first = SceneUpdate::default();
    first.invalidate_node(7, region(0.0));
    let mut second = SceneUpdate::default();
    second.invalidate_node(7, region(20.0));
    second.invalidate_node(9, region(40.0));
    second.request_full_rebuild();

    first.merge(second);

    assert!(first.full_rebuild());
    assert_eq!(first.dirty_node_ids().collect::<Vec<_>>(), vec![7, 9]);
    assert_eq!(
        first
            .node_regions()
            .find(|(id, _)| *id == 7)
            .unwrap()
            .1
            .len(),
        2
    );
    assert_eq!(first.damage_regions().len(), 3);
}

#[test]
fn render_node_index_replaces_only_changed_subtree() {
    let mut root = RenderNode::new(Rect::default());
    let mut left = RenderNode::new(Rect::default());
    left.id = Some(2);
    let mut right = RenderNode::new(Rect::default());
    right.id = Some(3);
    root.add_child(left);
    root.add_child(right);
    let mut index = root.build_index();

    let mut replacement = RenderNode::new(Rect::default());
    replacement.id = Some(4);
    *root.child_mut(0).expect("left child exists") = replacement;
    index.update_subtrees(&root, &[vec![0]]);

    assert_eq!(index.path_for(2), None);
    assert_eq!(index.path_for(4), Some([0].as_slice()));
    assert_eq!(index.path_for(3), Some([1].as_slice()));
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
fn clean_render_node_subtrees_are_reused_by_node_id() {
    let mut previous = RenderNode::new(Rect::default());
    previous.id = Some(1);
    let mut previous_clean = RenderNode::new(Rect::default());
    previous_clean.id = Some(2);
    previous_clean.commands_mut().push(PaintCommand::Rect {
        rect: Rect::default(),
        color: Color::WHITE,
    });
    previous.add_child(previous_clean);
    let mut previous_dirty = RenderNode::new(Rect::default());
    previous_dirty.id = Some(3);
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
            node_id: None,
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
fn path_cache_key_tracks_commands_and_fill_rule_without_debug_formatting() {
    let square = |fill_rule| {
        IconPath::from_commands(
            vec![
                PathCommand::MoveTo(Point::default()),
                PathCommand::LineTo(Point {
                    x: Dip(10.0),
                    y: Dip(0.0),
                }),
                PathCommand::LineTo(Point {
                    x: Dip(10.0),
                    y: Dip(10.0),
                }),
                PathCommand::Close,
            ],
            fill_rule,
        )
    };
    let even_odd = square(FillRule::EvenOdd);
    let non_zero = square(FillRule::NonZero);
    assert_eq!(path_cache_key(&even_odd), path_cache_key(&even_odd));
    assert_ne!(path_cache_key(&even_odd), path_cache_key(&non_zero));
}

#[test]
fn resource_handles_track_references() {
    let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut resources = ResourceManager::new(&device);
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
fn text_measure_cache_evicts_least_recently_used_entry() {
    let mut cache = TextMeasureCache {
        capacity: 2,
        ..TextMeasureCache::default()
    };
    let measurement = |width| TextMeasurement {
        width: Dip(width),
        ink_top: Dip(-8.0),
        ink_bottom: Dip(2.0),
    };
    cache.insert(("first".into(), 1), measurement(10.0));
    cache.insert(("second".into(), 1), measurement(20.0));
    assert_eq!(cache.get(&("first".into(), 1)), Some(measurement(10.0)));
    cache.insert(("third".into(), 1), measurement(30.0));

    assert!(cache.get(&("second".into(), 1)).is_none());
    assert_eq!(cache.get(&("first".into(), 1)), Some(measurement(10.0)));
    assert_eq!(cache.get(&("third".into(), 1)), Some(measurement(30.0)));
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
fn render_node_localizes_world_transform_with_full_parent_inverse() {
    let parent_world = compose_transform(
        Transform::translate(Dip(10.0), Dip(20.0)),
        Transform::scale(2.0, 2.0),
    );
    let child_world = Transform::translate(Dip(30.0), Dip(50.0));
    let mut child = RenderNode::new(Rect::default());
    child.transform = child_world;
    child.localize_to_parent(parent_world);

    assert_eq!(child.transform.matrix[4], 10.0);
    assert_eq!(child.transform.matrix[5], 15.0);
    assert_eq!(
        compose_transform(parent_world, child.transform),
        child_world
    );
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
                sampling: ImageSampling::Linear,
                images: Arc::new(vec![ImageId(10)]),
            },
            vertex(0),
            index(0),
            Transform::IDENTITY,
        ),
        GpuBatch::Draw(
            BatchKind::Image {
                page: 2,
                sampling: ImageSampling::Linear,
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
fn glyph_and_image_sampling_never_share_a_material_group() {
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
                sampling: ImageSampling::Linear,
                images: Arc::new(vec![ImageId(10)]),
            },
            vertex(0),
            index(0),
            Transform::IDENTITY,
        ),
        GpuBatch::Draw(
            BatchKind::Image {
                page: 2,
                sampling: ImageSampling::Glyph,
                images: Arc::new(vec![ImageId(11)]),
            },
            vertex(80),
            index(24),
            Transform::IDENTITY,
        ),
    ]);
    assert_eq!(groups.len(), 2);
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
    let (vertices, indices) = image_batch(
        &mut batches,
        3,
        ImageId(1),
        ImageSampling::Linear,
        Transform::IDENTITY,
    );
    append_image(
        vertices,
        indices,
        rect,
        1.0,
        Color::WHITE,
        [0.0, 0.0, 0.5, 0.5],
    );
    let (vertices, indices) = image_batch(
        &mut batches,
        3,
        ImageId(2),
        ImageSampling::Linear,
        Transform::IDENTITY,
    );
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
fn submission_merges_compatible_indirect_groups_across_retained_nodes() {
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
    let first = GpuBatch::DrawGroup(
        BatchKind::Rect,
        Arc::new(vec![(vertex(0), index(0))]),
        Transform::IDENTITY,
        Some(IndirectDrawRange {
            offset: 0,
            count: 1,
        }),
    );
    let next = GpuBatch::DrawGroup(
        BatchKind::Rect,
        Arc::new(vec![(vertex(64), index(24))]),
        Transform::IDENTITY,
        Some(IndirectDrawRange {
            offset: stride,
            count: 1,
        }),
    );
    assert_eq!(
        merge_indirect_draw_ranges(
            &first,
            indirect_draw_range(&first).unwrap(),
            &next,
            indirect_draw_range(&next).unwrap(),
        )
        .map(|range| (range.offset, range.count)),
        Some((0, 2))
    );

    let other_page = GpuBatch::DrawGroup(
        BatchKind::Rect,
        Arc::new(vec![(
            VertexArenaAllocation {
                page: 1,
                ..vertex(128)
            },
            index(48),
        )]),
        Transform::IDENTITY,
        Some(IndirectDrawRange {
            offset: stride * 2,
            count: 1,
        }),
    );
    assert!(merge_indirect_draw_ranges(
        &next,
        indirect_draw_range(&next).unwrap(),
        &other_page,
        indirect_draw_range(&other_page).unwrap(),
    )
    .is_none());
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
        node_id: None,
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
fn tile_submission_index_preserves_paint_order_after_incremental_replace() {
    let item = |x| RenderNodeItem {
        node_id: None,
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
    let mut items = BTreeMap::from([(vec![0], item(0.0)), (vec![1], item(10.0))]);
    let mut index = TileSubmissionIndex::build(&items, ScaleFactor(1.0));
    items.insert(vec![0], item(20.0));
    index.update_subtrees(&items, &[vec![0]]);

    assert_eq!(
        index.query(Rect {
            origin: Point::default(),
            size: zui_core::Size {
                width: Dip(127.0),
                height: Dip(127.0),
            },
        }),
        vec![vec![0], vec![1]],
    );
}

#[test]
fn retained_submission_table_replaces_only_dirty_subtree_segments() {
    let item = || RetainedGpuItem {
        node_id: None,
        bounds: Rect::default(),
        batches: Vec::new(),
        vertex_allocations: Vec::new(),
        index_allocations: Vec::new(),
        indirect_allocations: Vec::new(),
        resources: Vec::new(),
    };
    let mut items = BTreeMap::new();
    items.insert(vec![0, 0], item());
    items.insert(vec![0, 1], item());
    items.insert(vec![1], item());
    let mut table = RetainedSubmissionTable::rebuild(&items);
    assert_eq!(table.segments.len(), 3);

    items.remove(&vec![0, 0]);
    items.insert(vec![0, 2], item());
    table.replace_subtrees(&items, &[vec![0]]);

    assert!(!table.segments.contains_key(&vec![0, 0]));
    assert!(table.segments.contains_key(&vec![0, 1]));
    assert!(table.segments.contains_key(&vec![0, 2]));
    // The sibling segment was retained instead of participating in the
    // dirty subtree replacement.
    assert!(table.segments.contains_key(&vec![1]));
}

#[test]
fn retained_submission_table_keeps_gpu_batches_node_owned() {
    let item = |node_id| RetainedGpuItem {
        node_id: Some(node_id),
        bounds: Rect::default(),
        batches: Vec::new(),
        vertex_allocations: Vec::new(),
        index_allocations: Vec::new(),
        indirect_allocations: Vec::new(),
        resources: Vec::new(),
    };
    let items = BTreeMap::from([(vec![0], item(11)), (vec![1], item(12))]);
    let table = RetainedSubmissionTable::rebuild(&items);
    let mut entries = Vec::new();
    table.for_each_entry(|path, entry| entries.push((path.to_vec(), entry.node_id)));
    assert_eq!(entries, vec![(vec![0], Some(11)), (vec![1], Some(12))]);
}

#[test]
fn repeated_scene_revision_is_a_presentation_retry_not_a_gap() {
    assert!(!scene_revision_has_gap(Some(7), 7));
    assert!(!scene_revision_has_gap(Some(7), 8));
    assert!(scene_revision_has_gap(Some(7), 9));
    assert!(scene_revision_has_gap(None, 7));
    assert!(!scene_revision_has_gap(None, 0));
}

#[test]
fn deferred_canvas_forces_full_replay_even_for_a_later_partial_update() {
    let damage = [Rect {
        origin: Point::default(),
        size: zui_core::Size {
            width: Dip(10.0),
            height: Dip(10.0),
        },
    }];

    assert!(full_replay_required(true, &damage));
    assert!(!full_replay_required(false, &damage));
    assert!(full_replay_required(false, &[]));
}

#[test]
fn resource_eviction_never_discards_a_retained_image() {
    let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut resources = ResourceManager::new(&device);
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
            padding: 0,
        },
    );
    resources.last_used.insert(image, 1);
    resources.retain(ResourceHandle::Image(image));
    assert!(!resources.evict_one_gpu_image());
    assert!(resources.gpu_images.contains_key(&image));
}

#[test]
fn text_materialization_keeps_every_glyph_atlas_slot_alive_until_retained() {
    let Ok(mut renderer) = Renderer::new_blocking() else {
        return;
    };
    // Force the failure mode without needing hundreds of glyphs. Before the
    // materialization lease existed, each new upload immediately recycled an
    // earlier glyph's slot while its vertices still referenced the old UVs.
    renderer.resources.set_gpu_image_capacity(1);
    let batches = renderer.build_render_batches(
        &[PaintCommand::Text {
            text: "TextEditor 支持换行；可继续输入更多内容。".into(),
            origin: Point {
                x: Dip(11.25),
                y: Dip(30.0),
            },
            color: Color::WHITE,
            scale: 3,
        }],
        PhysicalSize {
            width: 2048,
            height: 1200,
        },
        4.0,
        Transform::IDENTITY,
        1.0,
    );
    let images = batches
        .iter()
        .filter_map(|batch| match batch {
            RenderBatch::Image { images, .. } => Some(images.as_slice()),
            _ => None,
        })
        .flatten()
        .copied()
        .collect::<HashSet<_>>();

    assert!(images.len() > 1);
    assert!(
        images
            .iter()
            .all(|image| renderer.resources.gpu_images.contains_key(image)),
        "a glyph slot was recycled before the text batch could be retained"
    );

    let handles = images
        .iter()
        .copied()
        .map(ResourceHandle::Image)
        .collect::<Vec<_>>();
    renderer.resources.retain_materialized(&handles);
    assert!(
        images
            .iter()
            .all(|image| renderer.resources.gpu_images.contains_key(image)),
        "retained glyph slots must remain resident even above the soft cache capacity"
    );
}

#[test]
fn retained_image_batches_contribute_resource_references() {
    let image = ImageId(42);
    let handles = batch_resource_handles(&[GpuBatch::DrawGroup(
        BatchKind::Image {
            page: 0,
            sampling: ImageSampling::Linear,
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
fn text_line_metrics_enclose_the_glyph_baseline_box() {
    let metrics = text_metrics(3);
    assert!(metrics.ascent.0 > 0.0);
    assert!(metrics.line_height.0 >= metrics.ascent.0 + metrics.descent.0);
}

#[test]
fn shaped_glyph_cache_keys_track_final_physical_font_size() {
    let mut system = text_system().lock().expect("text system poisoned");
    let buffer = text_buffer(&mut system.fonts, "字体 Ab", 21.0);
    let glyph = buffer
        .layout_runs()
        .flat_map(|run| run.glyphs)
        .next()
        .expect("the test run contains a glyph");

    for pixels_per_dip in [1.0, 1.25, 1.5, 1.875, 2.0, 3.75] {
        let physical = glyph.physical((13.25, 9.5), pixels_per_dip);
        assert_eq!(
            f32::from_bits(physical.cache_key.font_size_bits),
            21.0 * pixels_per_dip
        );
    }
}

#[test]
fn text_quads_are_one_to_one_with_physical_glyph_pixels_at_all_scales() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut resources = ResourceManager::new(&device);

    for pixels_per_dip in [1.0, 1.25, 1.5, 1.625, 2.0, 3.75] {
        let mut batches = Vec::new();
        append_text(
            &mut batches,
            &mut resources,
            &device,
            &queue,
            TextDraw {
                text: "清晰 Text",
                origin: Point {
                    x: Dip(13.25),
                    y: Dip(31.5),
                },
                color: Color::WHITE,
                scale: 3,
                pixels_per_dip,
                transform: Transform::translate(Dip(7.0), Dip(5.0)),
            },
        );

        let glyph_batches = batches.iter().filter_map(|batch| match batch {
            RenderBatch::Image {
                sampling: ImageSampling::Glyph,
                vertices,
                transform,
                ..
            } => Some((vertices, transform)),
            _ => None,
        });
        let mut glyph_count = 0;
        for (vertices, transform) in glyph_batches {
            assert_eq!(*transform, Transform::IDENTITY);
            for quad in vertices.chunks_exact(4) {
                glyph_count += 1;
                let left = quad[0].position[0] * pixels_per_dip;
                let top = quad[0].position[1] * pixels_per_dip;
                let width = (quad[1].position[0] - quad[0].position[0]) * pixels_per_dip;
                let height = (quad[3].position[1] - quad[0].position[1]) * pixels_per_dip;
                for physical_value in [left, top, width, height] {
                    assert!((physical_value - physical_value.round()).abs() < 0.001);
                }
            }
        }
        assert!(glyph_count > 0);
    }
}

#[test]
fn transformed_text_rasterizes_for_the_final_uniform_scale() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut resources = ResourceManager::new(&device);
    let transform = Transform::scale(2.0, 2.0);
    let mut batches = Vec::new();
    append_text(
        &mut batches,
        &mut resources,
        &device,
        &queue,
        TextDraw {
            text: "缩放 Text",
            origin: Point {
                x: Dip(12.0),
                y: Dip(30.0),
            },
            color: Color::WHITE,
            scale: 3,
            pixels_per_dip: 1.5,
            transform,
        },
    );

    let mut glyph_count = 0;
    for batch in batches {
        let RenderBatch::Image {
            sampling: ImageSampling::Glyph,
            vertices,
            transform: glyph_transform,
            ..
        } = batch
        else {
            continue;
        };
        assert_eq!(glyph_transform, transform);
        for quad in vertices.chunks_exact(4) {
            glyph_count += 1;
            let physical_width = (quad[1].position[0] - quad[0].position[0]) * 1.5 * 2.0;
            let physical_height = (quad[3].position[1] - quad[0].position[1]) * 1.5 * 2.0;
            assert!((physical_width - physical_width.round()).abs() < 0.001);
            assert!((physical_height - physical_height.round()).abs() < 0.001);
        }
    }
    assert!(glyph_count > 0);
}

#[test]
fn text_damage_bounds_cover_ink_above_and_below_the_baseline() {
    let origin = Point {
        x: Dip(12.0),
        y: Dip(40.0),
    };
    let command = PaintCommand::Text {
        text: "字体 glyph".into(),
        origin,
        color: Color::WHITE,
        scale: 3,
    };
    let bounds = command.bounds().expect("text has paint bounds");
    let metrics = text_run_metrics("字体 glyph", 3);

    assert!(bounds.origin.y.0 <= origin.y.0 + metrics.ink_top.0);
    assert!(bounds.size.height.0 >= metrics.ink_height().0);
    assert!(bounds.origin.y.0 < origin.y.0);
    assert!(bounds.origin.y.0 + bounds.size.height.0 > origin.y.0);
}

#[test]
fn atlas_padding_is_transparent_and_uvs_exclude_it() {
    let image = ImageResource::new(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 255]).unwrap();
    let padded = padded_rgba8(&image, 1);
    assert_eq!(padded.len(), 4 * 3 * 4);
    assert!(padded[..20].iter().all(|channel| *channel == 0));
    assert_eq!(&padded[20..28], image.rgba8.as_slice());
    assert!(padded[28..].iter().all(|channel| *channel == 0));

    let uv = atlas_content_uv(
        AtlasSlot {
            page: 0,
            origin: wgpu::Origin3d { x: 8, y: 4, z: 0 },
            width: 4,
            height: 3,
            atlas_width: 16,
            atlas_height: 16,
        },
        1,
    );
    assert_eq!(uv, [9.0 / 16.0, 5.0 / 16.0, 11.0 / 16.0, 6.0 / 16.0]);
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
