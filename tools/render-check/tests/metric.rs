use std::sync::Arc;
use zui_api::{layout::Dimension, view::Key};
use zui_charts::{SparklineSeries, SparklineView};
use zui_dashboard::{MetricCardView, MetricChange, MetricTone};
use zui_theme::Theme;
use zui_types::Size;
#[test]
fn optional_detail_reconciles_without_replacing_reading_or_trend_and_narrow_cards_wrap() {
    let mut text = zui_text::ParleyText::default();
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let make = |width, detail, light| {
        let theme = if light {
            Theme::light()
        } else {
            Theme::dashboard()
        };
        let mut card = MetricCardView::themed(
            &theme,
            "生产设备平均响应时间与服务状态",
            "123.45",
            families.clone(),
        );
        card.layout.width = Dimension::Length(width);
        card.unit = Some("毫秒".into());
        if detail {
            card.change = Some(MetricChange::new(
                "响应时间下降，服务改善",
                MetricTone::Positive,
            ));
        }
        card.trend = Some(SparklineView::themed(
            &theme,
            SparklineSeries::new(vec![Some(3.0), Some(1.0)]).unwrap(),
        ));
        card.build()
    };
    let mut tree =
        zui_core::Tree::from_view(make(360.0, false, false), Default::default()).unwrap();
    let layout = |tree: &mut zui_core::Tree, text: &mut zui_text::ParleyText| {
        tree.layout_with_text(
            &mut zui_layout::TaffyLayout::default(),
            text,
            Size::new(640.0, 480.0),
        )
        .unwrap();
    };
    layout(&mut tree, &mut text);
    let root = tree.root();
    let child = |tree: &zui_core::Tree, key: &str| {
        tree.children(tree.root())
            .unwrap()
            .iter()
            .copied()
            .find(|id| tree.key(*id).unwrap() == Some(&Key::from(key)))
            .unwrap()
    };
    let reading = child(&tree, "reading");
    let trend = child(&tree, "trend");
    let wide_height = tree.layout_box(root).unwrap().bounds.size.height;
    tree.reconcile(make(360.0, true, true)).unwrap();
    layout(&mut tree, &mut text);
    assert_eq!(child(&tree, "reading"), reading);
    assert_eq!(child(&tree, "trend"), trend);
    let change = child(&tree, "change");
    tree.reconcile(make(150.0, false, false)).unwrap();
    layout(&mut tree, &mut text);
    assert!(!tree.contains(change));
    assert_eq!(child(&tree, "trend"), trend);
    assert_eq!(child(&tree, "reading"), reading);
    assert!(tree.layout_box(root).unwrap().bounds.size.height > wide_height);
    let children = tree.children(reading).unwrap();
    let value = tree.layout_box(children[0]).unwrap().bounds;
    let unit = tree.layout_box(children[1]).unwrap().bounds;
    assert!(unit.origin.y >= value.origin.y + value.size.height);
    tree.scene().unwrap();
}
