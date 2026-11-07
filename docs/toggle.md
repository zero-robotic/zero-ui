# ToggleButton 切换按钮

ToggleButtonView 提供受控布尔 selected 状态，在控件层复用 Button 的文字、测量、绘制和输入处理。成功激活发出 ToggleChanged { selected: !当前确认值 }，应用选择是否更新模型并重建。未确认时按钮保持现状，反复激活会继续请求同一目标值。

```rust,ignore
let view = View::new(ToggleButtonView::themed(
    &theme, "显示网格", families, model.show_grid, ActionId(12),
)).styled(LayoutStyle {
    padding: Edges::all(10.0),
    ..Default::default()
});
// map_action 读取 ToggleChanged.selected，message 更新 show_grid。
```

ToggleButtonStyle 包含 selected/unselected 两套 ButtonStyle，分别配置正常、悬停、按下、禁用颜色与焦点环。默认未选中使用 Surface/普通文本，选中使用 Accent/OnAccent；禁用配色仍按两套样式区分。文字通过 text_style 配置，尺寸、内边距和裁剪沿用普通 View 布局。

鼠标完整点击、Space 松开、Enter 按下可激活；重复键与取消规则继承 Button。只有一个焦点位置，外部 selected 改变保持节点/焦点但取消未完成按压；动作绑定变化或禁用也取消旧手势。禁用后不再可聚焦，焦点清理由核心处理。该控件不包含单选组约束或三态，互斥选择可使用 SegmentedControl/RadioGroup。

三项 ToggleButton 测试覆盖受控请求、选中状态切换、Space/Enter、外部改值/绑定/禁用取消、越界释放和输入取消，以及 1×/2× 两种选中状态与禁用配色。四项 Button 回归通过，相关严格 Clippy、no-default-features 编译、格式及依赖检查通过。没有新增绘制算法，未重跑 GPU；触摸、图标、按下状态无障碍语义及原生输入验收仍待完成。
