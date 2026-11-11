use std::sync::Arc;
use zui_api::{
    layout::{Dimension, Edges},
    view::View,
};
use zui_paint::Scene;
use zui_theme::{ColorRole, Space, Theme, TypeRole};
use zui_types::Size;
use zui_widgets::{Container, TextView, column};

pub fn theme_view(theme: &Theme, families: Vec<String>) -> View {
    let mut title = TextView::themed(theme, "ZUI · 语义主题", families.clone(), TypeRole::Heading);
    title.color = theme.color(ColorRole::Accent);
    let body = TextView::themed(
        theme,
        "相同控件使用不同主题。颜色切换只更新绘制，字号与间距变化重新布局。",
        families.clone(),
        TypeRole::Body,
    );
    let mut caption = TextView::themed(
        theme,
        "CPU / GPU · 显式字体 · 无全局主题状态",
        families,
        TypeRole::Caption,
    );
    caption.color = theme.color(ColorRole::TextMuted);
    let panel = Container::new(
        column([
            View::new(body).keyed("body"),
            View::new(caption).keyed("caption"),
        ])
        .style(|s| s.gap = Size::new(0.0, theme.space(Space::Md) as f64))
        .build(),
    )
    .keyed("panel")
    .background(theme.color(ColorRole::Surface))
    .style(|s| s.padding = Edges::all(theme.space(Space::Lg)))
    .build();
    column([View::new(title).keyed("title"), panel])
        .background(theme.color(ColorRole::Background))
        .style(|s| {
            s.width = Dimension::Percent(1.0);
            s.height = Dimension::Percent(1.0);
            s.padding = Edges::all(theme.space(Space::Lg));
            s.gap = Size::new(0.0, theme.space(Space::Lg) as f64);
        })
        .build()
}

pub fn theme_preview(theme: &Theme) -> Result<Scene, Box<dyn std::error::Error>> {
    let mut text = zui_text::ParleyText::default();
    let families = text.register_font(Arc::from(
        include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
    ))?;
    let mut tree = zui_core::Tree::from_view(theme_view(theme, families), Default::default())?;
    tree.layout_with_text(
        &mut zui_layout::TaffyLayout::default(),
        &mut text,
        Size::new(640.0, 360.0),
    )?;
    Ok(tree.scene()?)
}

/// Desktop control specimen used for visual and scale-factor checks.
pub fn desktop_preview(theme: &Theme) -> Result<Scene, Box<dyn std::error::Error>> {
    use zui_api::action::ActionId;
    use zui_widgets::*;
    let mut text = zui_text::ParleyText::default();
    let families = text.register_font(Arc::from(
        include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
    ))?;
    let button = |label: &str, style: ButtonStyle| {
        let mut b = ButtonView::themed(theme, label, families.clone(), ActionId(1));
        b.style = style;
        View::new(b)
    };
    let mut editor = TextInputView::themed(theme, "采集设备 A", families.clone(), ActionId(2));
    editor.placeholder = "输入设备名称".into();
    let mut combo = ComboBoxView::themed(
        theme,
        vec![ListItem::new(ListItemId(1), "设备 1")],
        Some(ListItemId(1)),
        families.clone(),
        ActionId(3),
    );
    combo.searchable = true;
    let mut parent = TreeItem::new(TreeItemId(1), "采集设备");
    parent.children = vec![
        TreeItem::new(TreeItemId(2), "设备 1 · 在线"),
        TreeItem::new(TreeItemId(3), "设备 2 · 待连接"),
    ];
    let mut nodes = TreeView::themed(
        theme,
        vec![parent],
        families.clone(),
        Some(TreeItemId(2)),
        ActionId(4),
    );
    nodes.expanded.insert(TreeItemId(1));
    let mut disabled = ButtonView::themed(theme, "不可用", families.clone(), ActionId(5));
    disabled.disabled = true;
    let left = GroupBoxView::themed(theme, "设备设置", families.clone())
        .build(
            column([
                View::new(TextView::themed(
                    theme,
                    "设备名称",
                    families.clone(),
                    TypeRole::Caption,
                )),
                View::new(editor),
                combo.build(),
                row([
                    button("确认表单", ButtonStyle::primary(theme)),
                    button("取消", ButtonStyle::secondary(theme)),
                    button("帮助", ButtonStyle::text(theme)),
                ])
                .style(|s| s.gap = Size::new(8.0, 0.0))
                .build(),
                View::new(disabled),
            ])
            .style(|s| s.gap = Size::new(0.0, 12.0))
            .build(),
        )
        .styled(zui_api::layout::LayoutStyle {
            kind: zui_api::layout::LayoutKind::Column,
            width: Dimension::Length(304.0),
            ..Default::default()
        });
    let right = GroupBoxView::themed(theme, "设备列表", families.clone())
        .build(
            column([
                nodes.build()?,
                View::new(CheckBoxView::themed(
                    theme,
                    "启用自动采集",
                    families.clone(),
                    CheckState::Checked,
                    ActionId(6),
                )),
                row([
                    View::new(SliderView::themed(
                        theme,
                        NumericRange::new(0.0, 100.0, 1.0)?,
                        65.0,
                        ActionId(7),
                    )),
                    View::new(SwitchView::themed(theme, true, ActionId(8))),
                ])
                .style(|s| s.gap = Size::new(12.0, 0.0))
                .build(),
                View::new(SegmentedControlView::themed(
                    theme,
                    vec![
                        SegmentOption::new(SegmentId(1), "实时"),
                        SegmentOption::new(SegmentId(2), "历史"),
                    ],
                    families.clone(),
                    Some(SegmentId(1)),
                    ActionId(9),
                )),
            ])
            .style(|s| s.gap = Size::new(0.0, 12.0))
            .build(),
        )
        .styled(zui_api::layout::LayoutStyle {
            kind: zui_api::layout::LayoutKind::Column,
            grow: 1.0,
            min_width: Dimension::Length(0.0),
            ..Default::default()
        });
    let root = column([
        View::new(TextView::themed(
            theme,
            "ZUI · 桌面控件",
            families.clone(),
            TypeRole::Heading,
        )),
        View::new(TextView::themed(
            theme,
            "Blender 风格 · 中性灰阶 · 紧凑控件",
            families,
            TypeRole::Caption,
        )),
        row([left, right])
            .style(|s| s.gap = Size::new(16.0, 0.0))
            .build(),
    ])
    .background(theme.color(ColorRole::Background))
    .style(|s| {
        s.width = Dimension::Percent(1.0);
        s.height = Dimension::Percent(1.0);
        s.padding = Edges::all(16.0);
        s.gap = Size::new(0.0, 12.0);
    })
    .build();
    let mut tree = zui_core::Tree::from_view(root, Default::default())?;
    tree.layout_with_text(
        &mut zui_layout::TaffyLayout::default(),
        &mut text,
        Size::new(640.0, 360.0),
    )?;
    Ok(tree.scene()?)
}

pub fn dialog_preview(header: bool) -> Result<Scene, Box<dyn std::error::Error>> {
    use zui_api::action::ActionId;
    let theme = Theme::desktop_dark();
    let mut text = zui_text::ParleyText::default();
    let families = text.register_font(Arc::from(
        include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
    ))?;
    let mut dialog = zui_widgets::MessageBoxView::themed(
        &theme,
        "确认设备设置",
        "应用当前配置后，设备将使用新的采集参数。你也可以取消并继续编辑。",
        families,
        ActionId(1),
        ActionId(2),
    );
    dialog.dialog.show_title = header;
    dialog.dialog.show_close_button = header;
    dialog.dialog.size = Size::new(420.0, 220.0);
    let mut tree = zui_core::Tree::from_view(dialog.build(), Default::default())?;
    tree.layout_with_text(
        &mut zui_layout::TaffyLayout::default(),
        &mut text,
        Size::new(640.0, 360.0),
    )?;
    Ok(tree.scene()?)
}
