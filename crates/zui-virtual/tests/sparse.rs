use zui_virtual::{Alignment, Error, ExtentIndex, Limits};

#[test]
fn million_estimates_need_no_initial_buffer_and_updates_obey_budget() {
    let limits = Limits {
        max_index_bytes: 1024,
        ..Default::default()
    };
    let mut index = ExtentIndex::estimated(1_000_000, 20.0, 2.0, limits).unwrap();
    assert_eq!(index.allocated_index_bytes(), 0);
    assert_eq!(
        index.window(2_200_000.0, 100.0, 2).unwrap().items,
        99_998..100_007
    );
    assert!(!index.update(999_999, 20.0).unwrap());
    assert_eq!(index.allocated_index_bytes(), 0);
    assert!(index.update(999_999, 35.0).unwrap());
    assert!(index.allocated_index_bytes() <= 1024);
    assert_eq!(index.total_extent(), 22_000_013.0);
    let old = index.window(f64::MAX, 100.0, 2).unwrap();
    let bytes = index.allocated_index_bytes();
    assert_eq!(index.update(0, 30.0), Err(Error::BudgetExceeded));
    assert_eq!(index.window(f64::MAX, 100.0, 2).unwrap(), old);
    assert_eq!(index.allocated_index_bytes(), bytes);
    // Existing paths can be updated under a full budget; restore the estimate.
    assert!(index.update(999_999, 20.0).unwrap());
    assert_eq!(index.total_extent(), 21_999_998.0);
    assert_eq!(index.allocated_index_bytes(), bytes);
}
#[test]
fn sparse_updates_queries_alignment_and_anchors_match_dense_reference() {
    for count in [1, 2, 3, 31, 257, 1001] {
        let heights = vec![20.0; count];
        let mut sparse = ExtentIndex::estimated(count, 20.0, 1.25, Limits::default()).unwrap();
        let mut dense = ExtentIndex::variable(&heights, 1.25, Limits::default()).unwrap();
        for step in 0..300 {
            let item = (step * 97 + 11) % count;
            let value = if step % 4 == 0 {
                20.0
            } else {
                1.25 + (step % 79) as f64 * 0.5
            };
            assert_eq!(sparse.update(item, value), dense.update(item, value));
            assert_eq!(sparse.total_extent(), dense.total_extent());
            for position in [0, item, count - 1] {
                assert_eq!(sparse.item(position), dense.item(position));
            }
            for viewport in [0.0, 5.0, 97.25, 100_000.0] {
                let offset = ((step * 43) % (count * 21)) as f64;
                for overscan in [0, 2, usize::MAX] {
                    assert_eq!(
                        sparse.window(offset, viewport, overscan),
                        dense.window(offset, viewport, overscan)
                    );
                }
                for alignment in [
                    Alignment::Start,
                    Alignment::Center,
                    Alignment::End,
                    Alignment::Nearest,
                ] {
                    assert_eq!(
                        sparse.offset_for(item, offset, viewport, alignment),
                        dense.offset_for(item, offset, viewport, alignment)
                    );
                }
                assert_eq!(
                    sparse.anchor(offset, viewport),
                    dense.anchor(offset, viewport)
                );
            }
        }
    }
}
#[test]
fn sparse_half_open_edges_gaps_and_empty_ranges_follow_existing_contract() {
    let index = ExtentIndex::estimated(3, 10.0, 5.0, Limits::default()).unwrap();
    assert_eq!(index.window(0.0, 10.0, 0).unwrap().items, 0..1);
    assert_eq!(index.window(10.0, 5.0, 0).unwrap().items, 1..1);
    assert_eq!(index.window(10.0, 5.1, 0).unwrap().items, 1..2);
    assert_eq!(index.window(40.0, 0.0, usize::MAX).unwrap().items, 3..3);
    let mut empty = ExtentIndex::estimated(0, 1.0, 0.0, Limits::default()).unwrap();
    assert_eq!(empty.window(f64::MAX, 100.0, 2).unwrap().items, 0..0);
    assert_eq!(empty.update(0, 3.0), Err(Error::InvalidIndex));
    assert_eq!(empty.allocated_index_bytes(), 0);
}
#[test]
fn rejected_values_and_precision_do_not_allocate_or_change_queries() {
    let mut index = ExtentIndex::estimated(
        100,
        20.0,
        0.0,
        Limits {
            max_extent: 3000.0,
            ..Default::default()
        },
    )
    .unwrap();
    let window = index.window(0.0, 100.0, 2).unwrap();
    for value in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(index.update(10, value), Err(Error::InvalidExtent));
    }
    assert_eq!(index.update(10, 2000.0), Err(Error::BudgetExceeded));
    assert_eq!(index.update(10, 1e-20), Err(Error::InsufficientPrecision));
    assert_eq!(index.window(0.0, 100.0, 2).unwrap(), window);
    assert_eq!(index.allocated_index_bytes(), 0);
    let mut no_buffer = ExtentIndex::estimated(
        100,
        20.0,
        0.0,
        Limits {
            max_index_bytes: 0,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!no_buffer.update(5, 20.0).unwrap());
    assert_eq!(no_buffer.update(5, 21.0), Err(Error::BudgetExceeded));
}

#[test]
fn compaction_releases_default_paths_and_reopens_a_small_update_budget() {
    let mut index = ExtentIndex::estimated(
        1_000_000,
        20.0,
        2.0,
        Limits {
            max_index_bytes: 1024,
            ..Default::default()
        },
    )
    .unwrap();
    index.update(0, 30.0).unwrap();
    index.update(0, 20.0).unwrap();
    let bytes = index.allocated_index_bytes();
    assert!(bytes > 0);
    assert_eq!(index.update(999_999, 35.0), Err(Error::BudgetExceeded));
    assert_eq!(index.compact(0).unwrap(), bytes);
    assert_eq!(index.allocated_index_bytes(), 0);
    assert_eq!(index.total_extent(), 21_999_998.0);
    index.update(999_999, 35.0).unwrap();
    assert_eq!(index.total_extent(), 22_000_013.0);
}
#[test]
fn compact_preserves_nondefault_values_queries_and_future_updates() {
    let mut index = ExtentIndex::estimated(257, 20.0, 1.25, Limits::default()).unwrap();
    let mut heights = vec![20.0; 257];
    for (i, height) in heights.iter_mut().enumerate() {
        *height = 30.0 + (i % 7) as f64;
        index.update(i, *height).unwrap();
    }
    for (i, height) in heights.iter_mut().enumerate() {
        if i % 61 != 0 {
            *height = 20.0;
            index.update(i, 20.0).unwrap();
        }
    }
    let mut dense = ExtentIndex::variable(&heights, 1.25, Limits::default()).unwrap();
    let bytes = index.allocated_index_bytes();
    let original = index.window(55.0, 100.0, 2).unwrap();
    assert_eq!(index.compact(0), Err(Error::BudgetExceeded));
    assert_eq!(index.allocated_index_bytes(), bytes);
    assert_eq!(index.window(55.0, 100.0, 2).unwrap(), original);
    let released = index.compact(usize::MAX).unwrap();
    assert!(released > 0);
    assert_eq!(index.allocated_index_bytes(), bytes - released);
    assert_eq!(index.compact(0), Ok(0));
    for i in 0..257 {
        assert_eq!(index.item(i), dense.item(i));
    }
    for i in 0..300 {
        assert_eq!(
            index.window(i as f64 * 19.0, 87.25, 2),
            dense.window(i as f64 * 19.0, 87.25, 2)
        );
        let row = i * 47 % 257;
        let extent = 2.25 + (i % 37) as f64;
        assert_eq!(index.update(row, extent), dense.update(row, extent));
        assert_eq!(index.total_extent(), dense.total_extent());
    }
}
#[test]
fn compaction_of_empty_fixed_and_dense_indices_is_a_noop() {
    for mut index in [
        ExtentIndex::estimated(0, 20.0, 0.0, Limits::default()).unwrap(),
        ExtentIndex::estimated(1000, 20.0, 0.0, Limits::default()).unwrap(),
        ExtentIndex::fixed(1000, 20.0, 0.0, Limits::default()).unwrap(),
        ExtentIndex::variable(&[20.0, 30.0], 0.0, Limits::default()).unwrap(),
    ] {
        let bytes = index.allocated_index_bytes();
        let window = index.window(0.0, 100.0, 2).unwrap();
        assert_eq!(index.compact(0).unwrap(), 0);
        assert_eq!(index.allocated_index_bytes(), bytes);
        assert_eq!(index.window(0.0, 100.0, 2).unwrap(), window);
    }
}
