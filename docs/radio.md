# 单选组与受控选择

状态：纵向与横向等宽 `RadioGroup` 已实现，整个组是一个持久控件节点和一个 Tab 焦点位置。支持 CPU/GPU 绘制；触摸、无障碍语义和独立单选按钮的跨节点分组尚未实现。

## 公共入口

`zui_widgets` 提供 `RadioGroupView`、`RadioGroup`、`RadioOption`、`RadioOptionId`、`RadioChanged`、`RadioGroupStyle` 、`RadioColors` 和 `RadioOrientation`。上层通过 `View::new(group)` 接入，核心只调用 Widget 契约，不识别选项或组类型。

```rust
let group = RadioGroupView::themed(
    &theme,
    vec![
        RadioOption::new(RadioOptionId(1), "自动模式"),
        RadioOption::new(RadioOptionId(2), "手动模式"),
    ],
    families,
    Some(RadioOptionId(1)),
    ActionId(3),
);
```

选项 ID 在组内唯一且不依赖排列索引。`selected: Option<RadioOptionId>` 属于应用；None 表示未选中。非空选中值必须指向现有选项，可以指向禁用项以保留业务状态。重复 ID、不存在的选中 ID、非法尺寸或间距在布局准备时返回 `InvalidStyle`，不会发布可用场景。

激活产生 `RadioChanged { selected }` 请求。应用在 `map_action` 中检查 ActionId 和载荷，将其映射成消息；更新模型后重建 View，圆点才改变。再次激活已经选中的项不产生变化请求。组内活动项是本地键盘导航状态，与应用持有的选中值分离；应用拒绝请求时仍可继续导航。

## 输入与状态

- Tab/Shift+Tab 进入或离开整个组。进入时优先定位已选中的可用项，否则定位第一个可用项；不会因此自动提交选择。
- 上/左方向键前移，下/右方向键后移，跳过禁用项并循环。Home/End 定位首/末可用项。按下即请求选择，允许方向键重复；Control/Alt/Super 组合不被组消费。
- Space 按下进入按压状态，松开请求活动项；Enter 留给上层。方向键会取消尚未完成的按压手势，再移动活动项。
- 鼠标可点击圆形指示器或所在行的标签区域。只有同一选项内按下并松开才请求选择；按住移到别项再松开不会误选，移出后回到原选项仍可激活。
- 外层 padding、行间距、禁用项和内容裁剪区域之外不触发选择。鼠标捕获使用公共事件接口，命中依赖 EventGeometry 和准备好的行区域，不读取核心树。
- Escape、失焦、捕获丢失、选项或选中值更新、动作绑定变化及禁用会取消相关未完成手势。全组禁用或全部选项禁用时，组不可聚焦。

选项变化保留仍存在且可用的活动 ID，选中值变化则重新定位活动项。外部选项更新会取消旧手势并重新准备标签和行布局，避免重排后的坐标复用导致误选。

## 布局与绘制

外层仍使用公共 Flex/Grid 布局；组内默认采用纵向排列。标签复用 Text 与注入的文字服务，测量和最终准备都预留指示器宽度与标签间距，每行高度取标签高度与指示器尺寸的最大值。长标签自动换行，空组的自然内容尺寸为零。

样式提供正常、悬停、按压、禁用颜色，焦点色、指示器尺寸、标签间距和行间距。指示器尺寸至少 4 个逻辑单位，间距必须有限且非负。绘制使用公共路径、字形和裁剪描述；CPU/GPU 后端都不认识单选组。内容超过固定高度时裁剪，当前不会自动滚动。

### 横向等宽排列

设置 `group.orientation = RadioOrientation::Horizontal` 启用横向模式。可用内容宽度先扣除选项间距，再等分给全部选项（包括禁用项）；每个槽位预留圆点和标签间距，文字在余下宽度内换行。所有槽位使用最长标签所需的共同高度，垂直居中绘制圆点与标签。无约束自然宽度按最大选项宽度等分排列计算。

style.row_gap 在横向模式表示相邻选项的水平间距。组不自动拆成多行，也不自动水平滚动；外层不足时按内容边界裁剪。宽度小于间距总和时槽位宽度为零，不产生可点击选项区域。padding、间隙和禁用项仍不会选中。

切换方向会触发布局并取消未完成点击/空格手势，保留组节点身份和有效的选项 ID。键盘仍接受上/左为上一项、下/右为下一项，与默认纵向语义一致；未增加 RTL 视觉顺序。横向布局并不等同于 SegmentedControl 或 Tab 页签，这些控件仍需独立的视觉和内容语义。

新增行为测试覆盖共同高度、约束变化换行、水平命中、间隙/内边距/禁用、零宽槽位、方向切换取消、稳定身份、重排和空组。新增 CPU/GPU 像素测试检查第三项圆点、首项未选中背景、间隙与外部边界，覆盖 1×/2×。

```sh
cargo run -p zui-render-check --locked --offline -- cpu artifacts/radio-horizontal-cpu.png radio-horizontal
```

## 验证与示例

`tools/render-check/tests/radio.rs` 覆盖受控回写、padding/标签命中、跨行拖动、禁用跳过、导航与取消、文本换行、空组、非法属性及两种后端的 1×/2× 选中像素。

```sh
cargo run -p zui-render-check --locked -- cpu artifacts/radio-cpu.png radio
cargo test -p zui-render-check --all-features --locked --test radio
# 需要真实 GPU；初始化失败算失败。
cargo test -p zui-render-check --all-features --locked --test radio -- --ignored
cargo run -p zui-window-check --all-features --locked -- cpu --widgets
```

窗口示例的“自动模式/手动模式”通过应用消息回写选中值并更新说明文字；其离线 Host 测试验证按钮、复选框、单选组的 Tab 顺序，以及方向键到模型再到 View 的链路。定时窗口冒烟仅验证呈现与生命周期，不代表已经完成真实桌面点击验证。

后续继续补充独立 RadioButton 与可组合分组、RTL、选项级无障碍语义、触摸和滚动。当前单个组节点不能替代这些能力。

## 内部交互复用

RadioGroup 的单选交互现由控件库内部 choice 模块维护：它只接收稳定 ID、禁用状态、选中值和调用方计算的命中选项，返回公共 EventResponse 与可选选择请求。选项文字、布局、绘制和 RadioChanged 载荷仍由 RadioGroup 负责。该模块不依赖核心、文字服务或渲染后端，也不公开为应用需要操作的状态。

方向键直接扫描选项切片，不为每次按键分配临时可用项数组，时间仍为 O(N)。测试枚举四项选择的全部禁用组合以及无效/空活动身份，核对方向、首尾与环绕；重排/禁用测试核对活动身份保留和按压取消。此抽取用于后续分段选择复用，不表示 SegmentedControl 或页签已完成。
