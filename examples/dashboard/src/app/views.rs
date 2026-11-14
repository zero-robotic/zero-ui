use super::*;
impl Demo {
    pub(super) fn dashboard_view(&self) -> View {
        let theme = Theme::dashboard();
        let mut load = MetricCardView::themed(
            &theme,
            "模拟设备负载",
            format!("{:.0}", self.value),
            self.families.clone(),
        );
        load.unit = Some("%".into());
        load.change = Some(MetricChange::new(
            if self.running {
                "采样中 · 250 ms"
            } else {
                "已暂停"
            },
            MetricTone::Neutral,
        ));
        load.layout.width = Dimension::Length(220.0);
        load.key = Some("load".into());
        let mut count = MetricCardView::themed(
            &theme,
            "保留样本槽位",
            self.history.len().to_string(),
            self.families.clone(),
        );
        count.unit = Some(format!("/ {CAPACITY}"));
        count.change = Some(MetricChange::new(
            "有界历史 · 缺测保留断线",
            MetricTone::Neutral,
        ));
        count.layout.width = Dimension::Length(220.0);
        count.key = Some("history".into());
        let mut trend = SparklineView::themed(&theme, self.series.clone());
        trend.range = Some(ChartRange::new(0.0, 100.0).expect("fixed increasing range"));
        trend.style.natural_size.height = 96.0;
        let panel = DashboardPanelView::themed(
            &theme,
            [
                View::new(TextView::themed(
                    &theme,
                    "ZUI · 实时监测演示（模拟数据）",
                    self.families.clone(),
                    TypeRole::Heading,
                ))
                .keyed("heading"),
                row([load.build(), count.build()])
                    .keyed("metrics")
                    .style(|s| {
                        s.wrap = true;
                        s.gap = Size::new(16.0, 16.0);
                    })
                    .build(),
                View::new(trend).keyed("trend"),
                View::new(ProgressBarView::themed(
                    &theme,
                    ProgressValue::Determinate(self.value / 100.0),
                )),
                View::new(TextView::themed(
                    &theme,
                    format!(
                        "负载偏移 {:+.0}% · 拖动滑块同步指标、进度条与趋势；暂停后仍可手动调整。",
                        self.bias
                    ),
                    self.families.clone(),
                    TypeRole::Body,
                )),
                View::new(SliderView::themed(
                    &theme,
                    NumericRange::new(-20.0, 20.0, 1.0).unwrap(),
                    self.bias,
                    ActionId(2),
                ))
                .keyed("bias")
                .styled(LayoutStyle {
                    height: Dimension::Length(36.0),
                    ..Default::default()
                }),
                View::new(SwitchView::themed(&theme, self.running, ActionId(3))).keyed("running"),
                View::new(ButtonView::themed(
                    &theme,
                    if self.running {
                        "暂停采样"
                    } else {
                        "恢复采样"
                    },
                    self.families.clone(),
                    ActionId(1),
                ))
                .keyed("pause")
                .styled(LayoutStyle {
                    width: Dimension::Length(140.0),
                    padding: Edges::all(10.0),
                    ..Default::default()
                }),
            ],
        )
        .build();
        ScrollView::vertical(
            column([panel])
                .style(|s| s.padding = Edges::all(20.0))
                .build(),
        )
        .style(|s| {
            s.width = Dimension::Percent(1.0);
            s.height = Dimension::Percent(1.0);
        })
        .build()
    }
}
