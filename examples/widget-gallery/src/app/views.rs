use super::*;

impl Demo {
    pub(super) fn gallery(&self) -> View {
        let theme = self.theme();
        let scroll = zui_widgets::ScrollView::vertical(
            column([
                self.advanced.view(&theme, &self.families),
                self.catalog.view(&theme, &self.families),
                self.data_views.view(&theme, &self.families),
                self.basic_view(),
                self.extras
                    .view(&theme, &self.families, &self.input, self.level),
            ])
            .style(|s| s.gap = Size::new(0.0, 20.0))
            .build(),
        )
        .style(|s| {
            s.grow = 1.0;
            s.basis = Dimension::Length(0.0);
            s.min_height = Dimension::Length(0.0);
            s.width = Dimension::Percent(1.0);
        })
        .build();
        let base = column([
            self.overlays.controls(&theme, &self.families, self.level),
            scroll,
        ])
        .style(|s| {
            s.width = Dimension::Percent(1.0);
            s.height = Dimension::Percent(1.0);
        })
        .build();
        let mut layers = vec![
            zui_widgets::ContextMenuRegion {
                action: ActionId(106),
            }
            .build(base)
            .keyed("base"),
        ];
        if let Some(overlay) = self.overlays.overlay(&theme, &self.families, self.level) {
            layers.push(overlay.keyed("popup"));
        }
        if let Some(overlay) = self.advanced.overlay(&theme, &self.families) {
            layers.push(overlay.keyed("advanced-overlay"));
        }
        if let Some(overlay) = self.catalog.overlay(&theme, &self.families) {
            layers.push(overlay.keyed("catalog-overlay"));
        }
        zui_widgets::stack(layers)
            .style(|s| {
                s.width = Dimension::Percent(1.0);
                s.height = Dimension::Percent(1.0);
            })
            .build()
    }
    fn basic_view(&self) -> View {
        let theme = self.theme();
        let mut title = TextView::themed(
            &theme,
            "ZUI · 传统控件展厅",
            self.families.clone(),
            TypeRole::Heading,
        );
        title.color = theme.color(ColorRole::Accent);
        let body = TextView::themed(
            &theme,
            "Slider / Stepper 同步进度条；输入文字在下方页签回显。向下滚动查看全部控件，Tab 切换焦点，Escape 退出。",
            self.families.clone(),
            TypeRole::Body,
        );
        let caption = TextView::themed(
            &theme,
            if self.details == CheckState::Checked {
                if self.mode == RadioOptionId(1) {
                    "运行模式：自动 · View → 布局与排版 → CPU / GPU"
                } else {
                    "运行模式：手动 · View → 布局与排版 → CPU / GPU"
                }
            } else {
                "详细说明已关闭"
            },
            self.families.clone(),
            TypeRole::Caption,
        );
        let mut stepper = StepperView::themed(
            &theme,
            self.families.clone(),
            self.level,
            NumericRange::new(0.0, 100.0, 5.0).expect("constant range"),
            ActionId(6),
        );
        stepper.display = Some(format!("{:.0}%", self.level));
        stepper.key = Some("level-stepper".into());
        column([
            View::new(title).keyed("title"),
            Container::new(
                column([
                    View::new(body).keyed("body"),
                    View::new(caption).keyed("caption"),
                ])
                .style(|s| s.gap = Size::new(0.0, theme.space(Space::Md) as f64))
                .build(),
            )
            .background(theme.color(ColorRole::Surface))
            .keyed("panel")
            .style(|s| s.padding = Edges::all(theme.space(Space::Lg)))
            .build(),
            row([
                column([
                    View::new(ButtonView::themed(
                        &theme,
                        "切换主题",
                        self.families.clone(),
                        ActionId(1),
                    ))
                    .keyed("toggle-theme")
                    .styled(zui_api::layout::LayoutStyle {
                        width: Dimension::Length(200.0),
                        min_height: Dimension::Length(28.0),
                        ..Default::default()
                    }),
                    View::new(CheckBoxView::themed(
                        &theme,
                        "显示详细说明",
                        self.families.clone(),
                        self.details,
                        ActionId(2),
                    ))
                    .keyed("show-details")
                    .styled(zui_api::layout::LayoutStyle {
                        min_height: Dimension::Length(28.0),
                        ..Default::default()
                    }),
                ])
                .style(|s| s.gap = Size::new(0.0, 12.0))
                .build(),
                View::new(RadioGroupView::themed(
                    &theme,
                    vec![
                        RadioOption::new(RadioOptionId(1), "自动模式"),
                        RadioOption::new(RadioOptionId(2), "手动模式"),
                    ],
                    self.families.clone(),
                    Some(self.mode),
                    ActionId(3),
                ))
                .keyed("mode")
                .styled(zui_api::layout::LayoutStyle {
                    width: Dimension::Length(200.0),
                    ..Default::default()
                }),
            ])
            .style(|s| {
                s.gap = Size::new(24.0, 12.0);
                s.wrap = true;
            })
            .build(),
            row([
                column([
                    View::new(TextView::themed(
                        &theme,
                        format!("设定值：{:.0}%", self.level),
                        self.families.clone(),
                        TypeRole::Caption,
                    )),
                    View::new(SliderView::themed(
                        &theme,
                        NumericRange::new(0.0, 100.0, 5.0).expect("constant range"),
                        self.level,
                        ActionId(4),
                    ))
                    .keyed("level")
                    .styled(zui_api::layout::LayoutStyle {
                        height: Dimension::Length(36.0),
                        padding: Edges::all(8.0),
                        ..Default::default()
                    }),
                    stepper
                        .build()
                        .expect("model level remains within the shared range"),
                    View::new(ProgressBarView::themed(
                        &theme,
                        ProgressValue::Determinate(self.level / 100.0),
                    ))
                    .keyed("level-progress"),
                ])
                .style(|s| {
                    s.gap = Size::new(0.0, 8.0);
                    s.width = Dimension::Length(320.0);
                })
                .build(),
                View::new({
                    let mut input = TextInputView::themed(
                        &theme,
                        self.input.clone(),
                        self.families.clone(),
                        ActionId(5),
                    );
                    input.placeholder = "输入后同步到下方摘要".into();
                    input
                })
                .keyed("input")
                .styled(zui_api::layout::LayoutStyle {
                    width: Dimension::Length(320.0),
                    min_height: Dimension::Length(28.0),
                    ..Default::default()
                }),
            ])
            .style(|s| {
                s.gap = Size::new(24.0, 12.0);
                s.wrap = true;
            })
            .build(),
        ])
        .style(|s| {
            s.width = Dimension::Percent(1.0);
            s.padding = Edges::all(theme.space(Space::Lg));
            s.gap = Size::new(0.0, theme.space(Space::Lg) as f64);
        })
        .build()
    }
}
