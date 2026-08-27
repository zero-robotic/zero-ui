//! Unit tests for render commands, retained nodes, and GPU helpers.

use super::*;

    #[test]
    fn noop_device_can_create_gpu_objects() {
        let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let _encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let transform_layout = create_transform_bind_group_layout(&device);
        let _pipeline = create_rounded_rect_pipeline(
            &device,
            wgpu::TextureFormat::Rgba8Unorm,
            &transform_layout,
        );
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
        node.commands.push(PaintCommand::Rect {
            rect: Rect::default(),
            color: Color::WHITE,
        });
        let mut child = RenderNode::new(Rect::default());
        child.commands.push(PaintCommand::Clear(Color::BLACK));
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
        root.children[0] = child;
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
        previous_clean.commands.push(PaintCommand::Rect {
            rect: Rect::default(),
            color: Color::WHITE,
        });
        previous.add_child(previous_clean);
        let mut previous_dirty = RenderNode::new(Rect::default());
        previous_dirty.source_id = Some(3);
        previous.add_child(previous_dirty);

        let mut current = previous.clone();
        current.children[0].commands[0] = PaintCommand::Rect {
            rect: Rect::default(),
            color: Color::BLACK,
        };
        current.children[1]
            .commands
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
    fn shared_item_damage_is_coalesced_once() {
        let items = std::collections::BTreeMap::from([(vec![0], RenderNodeItem {
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
            commands: Vec::new(),
        })]);
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
        node.commands.push(PaintCommand::Rect {
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
        node.commands.push(PaintCommand::Rect {
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
            node.clip,
            Some(ClipShape::Rect(Rect {
                origin: Point::default(),
                size: zui_core::Size {
                    width: Dip(5.0),
                    height: Dip(5.0)
                },
            }))
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
    fn command_transforms_are_preserved_for_gpu_batches() {
        let mut commands = Vec::new();
        commands.push(PaintCommand::Transform(Transform::translate(
            Dip(3.0),
            Dip(4.0),
        )));
        commands.push(PaintCommand::Clip {
            shape: ClipShape::Rect(Rect {
                origin: Point {
                    x: Dip(1.0),
                    y: Dip(2.0),
                },
                size: zui_core::Size {
                    width: Dip(10.0),
                    height: Dip(11.0),
                },
            }),
        });
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
            matches!(batches.get(1), Some(RenderBatch::Rect(_, transform)) if *transform == Transform::translate(Dip(3.0), Dip(4.0)))
        );
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
