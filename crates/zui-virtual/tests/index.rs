use zui_virtual::{Alignment, Error, ExtentIndex, Limits};
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-7, "{a} != {b}");
}
#[test]
fn fixed_million_rows_query_only_viewport_and_overscan() {
    let index = ExtentIndex::fixed(
        1_000_000,
        20.0,
        2.0,
        Limits {
            max_index_bytes: 0,
            ..Default::default()
        },
    )
    .unwrap();
    let window = index.window(2_200_000.0, 100.0, 2).unwrap();
    assert_eq!(window.items, 99_998..100_007);
    assert_eq!(window.leading_extent, 2_199_956.0);
    close(window.total_extent, 21_999_998.0);
    let last = index.item(window.items.end - 1).unwrap();
    close(window.trailing_extent, window.total_extent - last.end);
    assert_eq!(
        index.window(f64::MAX, 100.0, 0).unwrap().items,
        999_995..1_000_000
    );
}
#[test]
fn half_open_edges_gaps_empty_and_zero_viewport_are_explicit() {
    for index in [
        ExtentIndex::fixed(3, 10.0, 5.0, Limits::default()).unwrap(),
        ExtentIndex::variable(&[10.0; 3], 5.0, Limits::default()).unwrap(),
    ] {
        assert_eq!(index.window(0.0, 10.0, 0).unwrap().items, 0..1);
        assert_eq!(index.window(10.0, 5.0, 0).unwrap().items, 1..1);
        assert_eq!(index.window(10.0, 5.1, 0).unwrap().items, 1..2);
        assert_eq!(index.window(10.0, 5.0, 1).unwrap().items, 0..2);
        let zero = index.window(40.0, 0.0, usize::MAX).unwrap();
        assert_eq!(zero.items, 3..3);
        assert_eq!(zero.leading_extent, 40.0);
        assert_eq!(zero.trailing_extent, 0.0);
        assert_eq!(index.window(5.0, 100.0, usize::MAX).unwrap().items, 0..3);
    }
    for index in [
        ExtentIndex::fixed(0, 20.0, 4.0, Limits::default()).unwrap(),
        ExtentIndex::variable(&[], 4.0, Limits::default()).unwrap(),
    ] {
        assert!(index.is_empty());
        assert_eq!(index.total_extent(), 0.0);
        assert_eq!(index.window(100.0, 50.0, 10).unwrap().items, 0..0);
        assert_eq!(index.item(0), Err(Error::InvalidIndex));
    }
}
#[test]
fn variable_ranges_and_updates_match_a_linear_reference() {
    let mut heights: Vec<_> = (0..257).map(|i| 5.25 + (i % 11) as f64 * 0.5).collect();
    let gap = 1.25;
    let mut index = ExtentIndex::variable(&heights, gap, Limits::default()).unwrap();
    for update in 0..80 {
        let item = update * 37 % heights.len();
        let height = 3.5 + (update % 17) as f64;
        assert_eq!(index.update(item, height).unwrap(), heights[item] != height);
        heights[item] = height;
        let total = heights.iter().sum::<f64>() + gap * (heights.len() - 1) as f64;
        close(index.total_extent(), total);
        for query in 0..29 {
            let offset = (query * 97) as f64;
            let viewport = 23.75;
            let actual = index.window(offset, viewport, 0).unwrap();
            let current = offset.min((total - viewport).max(0.0));
            let mut start = 0.0;
            let mut visible = Vec::new();
            for (i, &height) in heights.iter().enumerate() {
                close(index.item(i).unwrap().start, start);
                if start < current + viewport && start + height > current {
                    visible.push(i);
                }
                start += height + gap;
            }
            let expected = visible.first().copied().unwrap()..visible.last().copied().unwrap() + 1;
            assert_eq!(actual.items, expected);
            let end = index.item(actual.items.end - 1).unwrap().end;
            close(
                actual.leading_extent,
                index.item(actual.items.start).unwrap().start,
            );
            close(actual.trailing_extent, total - end);
        }
    }
    // Replacing the same leaf repeatedly must not accumulate floating delta drift.
    let baseline = index.total_extent();
    let original = heights[3];
    for _ in 0..10_000 {
        index.update(3, 0.125).unwrap();
        index.update(3, original).unwrap();
    }
    assert_eq!(index.total_extent(), baseline);
}
#[test]
fn index_alignment_clamps_and_nearest_is_stable_for_large_rows() {
    let index = ExtentIndex::variable(&[10.0, 100.0, 20.0], 5.0, Limits::default()).unwrap();
    assert_eq!(
        index.offset_for(1, 0.0, 30.0, Alignment::Start).unwrap(),
        15.0
    );
    assert_eq!(
        index.offset_for(1, 0.0, 30.0, Alignment::Center).unwrap(),
        50.0
    );
    assert_eq!(
        index.offset_for(1, 0.0, 30.0, Alignment::End).unwrap(),
        85.0
    );
    assert_eq!(
        index.offset_for(1, 0.0, 30.0, Alignment::Nearest).unwrap(),
        15.0
    );
    assert_eq!(
        index.offset_for(1, 40.0, 30.0, Alignment::Nearest).unwrap(),
        40.0
    );
    assert_eq!(
        index
            .offset_for(1, 110.0, 30.0, Alignment::Nearest)
            .unwrap(),
        85.0
    );
    assert_eq!(
        index.offset_for(2, 0.0, 30.0, Alignment::Start).unwrap(),
        110.0
    );
    assert_eq!(
        index.offset_for(0, 100.0, 30.0, Alignment::End).unwrap(),
        0.0
    );
    assert_eq!(
        index.offset_for(1, 0.0, 0.0, Alignment::Center).unwrap(),
        15.0
    );
}
#[test]
fn invalid_input_precision_and_resource_budgets_do_not_corrupt_updates() {
    assert!(matches!(
        ExtentIndex::fixed(2, 10.0, -1.0, Limits::default()),
        Err(Error::InvalidGap)
    ));
    for height in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            ExtentIndex::variable(&[height], 0.0, Limits::default()),
            Err(Error::InvalidExtent)
        ));
    }
    assert!(matches!(
        ExtentIndex::variable(
            &[10.0; 10],
            0.0,
            Limits {
                max_index_bytes: 1,
                ..Default::default()
            }
        ),
        Err(Error::BudgetExceeded)
    ));
    assert!(matches!(
        ExtentIndex::fixed(
            11,
            10.0,
            0.0,
            Limits {
                max_items: 10,
                ..Default::default()
            }
        ),
        Err(Error::BudgetExceeded)
    ));
    assert!(matches!(
        ExtentIndex::variable(&[1e-12, 1e12], 0.0, Limits::default()),
        Err(Error::InsufficientPrecision)
    ));
    let mut index = ExtentIndex::variable(
        &[10.0, 20.0],
        2.0,
        Limits {
            max_extent: 40.0,
            ..Default::default()
        },
    )
    .unwrap();
    let before = index.window(0.0, 15.0, 0).unwrap();
    assert_eq!(index.update(0, 100.0), Err(Error::BudgetExceeded));
    assert_eq!(index.update(0, 1e-20), Err(Error::InsufficientPrecision));
    assert_eq!(index.update(0, f64::NAN), Err(Error::InvalidExtent));
    assert_eq!(index.window(0.0, 15.0, 0).unwrap(), before);
    for (offset, viewport) in [
        (-1.0, 5.0),
        (0.0, -1.0),
        (f64::NAN, 5.0),
        (0.0, f64::INFINITY),
    ] {
        assert_eq!(
            index.window(offset, viewport, 0),
            Err(Error::InvalidViewport)
        );
    }
    assert_eq!(index.update(2, 20.0), Err(Error::InvalidIndex));
    let mut fixed = ExtentIndex::fixed(1, 20.0, 0.0, Limits::default()).unwrap();
    assert_eq!(fixed.update(0, 30.0), Err(Error::FixedExtents));
}

#[test]
fn anchors_preserve_visible_item_when_preceding_heights_change() {
    let mut index = ExtentIndex::variable(&[20.0; 10], 4.0, Limits::default()).unwrap();
    let anchor = index.anchor(53.0, 40.0).unwrap().unwrap();
    assert_eq!(
        anchor,
        zui_virtual::Anchor {
            index: 2,
            within: 5.0
        }
    );
    index.update(0, 50.0).unwrap();
    assert_eq!(index.offset_for_anchor(anchor, 40.0).unwrap(), 83.0);
    index.update(2, 2.0).unwrap();
    assert_eq!(index.offset_for_anchor(anchor, 40.0).unwrap(), 80.0);
    let gap = index.anchor(77.0, 40.0).unwrap().unwrap();
    assert_eq!(
        gap,
        zui_virtual::Anchor {
            index: 2,
            within: -1.0
        }
    );
    index.update(1, 30.0).unwrap();
    assert_eq!(index.offset_for_anchor(gap, 40.0).unwrap(), 87.0);
    let empty = ExtentIndex::fixed(0, 20.0, 0.0, Limits::default()).unwrap();
    assert_eq!(empty.anchor(0.0, 100.0).unwrap(), None);
}
