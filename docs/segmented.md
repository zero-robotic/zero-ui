# 分段选择 SegmentedControl

SegmentedControlView/SegmentedControl 提供横向等宽、单选的分段按钮。它与 RadioGroup 共用内部 ChoiceState 交互，但使用独立的分段视觉、属性与 SegmentChanged 动作。核心和渲染后端不认识分段控件。

```rust
let control = SegmentedControlView::themed(
    &theme,
    vec![
        SegmentOption::new(SegmentId(1), "实时监测"),
        SegmentOption::new(SegmentId(2), "历史趋势"),
    ],
    families,
    Some(SegmentId(1)),
    ActionId(8),
);
let view = View::new(control);
```

## 数据与交互

SegmentId 在组内稳定且唯一，不能用重排后的索引代替业务身份。selected 为应用持有的 Option<SegmentId>，None 表示未选择。选中值必须存在，可以是禁用项；重复 ID、缺失选中项和非法尺寸在测量/准备阶段拒绝。

点击或键盘操作输出 SegmentChanged { selected } 请求，应用处理后重建 View 才改变选中背景。重复激活当前选中项不输出请求；活动焦点位置与确认值分开，应用拒绝请求时仍可继续导航。

整个组只有一个节点和一个 Tab 停靠点。上/左方向键前移，下/右后移，跳过禁用项并循环；Home/End 到首尾。Space 按下、释放激活当前活动项，Enter 不由组消费。Control/Alt/Super 修饰键不触发导航或激活。聚焦时优先使用已选中的可用项，否则使用首个可用项。

指针必须在同一可用分段内按下并松开才产生选择请求；跨段释放、间隙、外部 padding 和禁用段不选择。按下请求捕获，Escape、失焦、捕获丢失、选项/几何或选中值更新、动作变化与禁用取消手势。重排保留仍有效的活动 SegmentId，全禁用或空组不可聚焦。

## 布局与视觉

可用内容宽度扣除 gap 后等分，禁用项同样占位。每段扣除 padding_x 后排版标签；标签默认居中，可通过 text_style.align 配置，长文字在槽位内换行。自然高度取所有标签所需高度加上下 padding_y，与 min_height 的最大值；无约束自然宽度按最大段宽等分计算。空组自然内容大小为零。

最终槽位铺满分配的内容高度，标签垂直居中并在自己的内边距区域裁剪。分段不自动换成多行，也不自动滚动或压缩成弹出菜单；过小的固定尺寸按内容区域裁剪。宽度不足以容纳间隙时槽位宽为零，不存在有效点击区域。

SegmentedStyle 提供普通、悬停、按下、禁用四套背景/文字/边框颜色，每套包含选中背景和选中文字；另有活动焦点环。默认使用直角矩形分段、2 逻辑像素间隙、水平内边距 12、垂直内边距 8、最小高度 32。样式尺寸必须有限非负。颜色更新只请求 Paint，几何和文字变化请求 Layout。

本控件是单选筛选器/模式切换器，不包含页签内容管理；尚未实现多选分段、图标、自动溢出、RTL、过渡动画、触摸手势和选项级无障碍语义。

## 验证与预览

三项行为测试覆盖受控请求、禁用/间隙/padding 命中、捕获取消、键盘与单一焦点、重排、长标签、空组、零宽槽位和非法配置。窗口侧集成测试验证动作到模型回写、重新协调后焦点身份保持以及禁用清理。CPU/GPU 像素测试在 1×/2× 下检查选中/普通/禁用背景、间隙和外部裁剪。

```sh
cargo test -p zui-render-check --test segmented --all-features --locked --offline
cargo test -p zui-render-check --test segmented --all-features --locked --offline segmented_selection_disabled_and_spacing_render_gpu -- --ignored
cargo test -p zui-window-check --test segmented --locked --offline
cargo run -p zui-render-check --locked --offline -- cpu artifacts/segmented-cpu.png segmented
```

预览使用真实 Tree/Taffy/Parley/Vello CPU 链路。合成输入和离屏像素测试不代表原生桌面点击、屏幕阅读器或嵌入式设备性能已经验收。
