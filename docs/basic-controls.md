# 基础交互与数值控件

[文档索引](README.md) · [模块边界](module-boundaries.md) · [未完成事项](implementation-plan.md)

按架构职责归并的专题参考。设计目标与接口约束分开理解；待办统一维护在实施计划中，各节的验证说明仅覆盖所述范围。

## 章节导航

- [按钮基础](#button)
- [ToggleButton 切换按钮](#toggle)
- [复选框与受控状态](#checkbox)
- [单选组与受控选择](#radio)
- [受控开关 Switch](#switch)
- [分段选择 SegmentedControl](#segmented)
- [水平滑块与数值范围](#slider)
- [Stepper 数值步进器](#stepper)
- [进度条](#progress)

<a id="button"></a>


<a id="button--按钮基础"></a>

## 按钮基础

`zui-widgets::ButtonView` 构造持久 Button，内部复用 Text 的测量、排版和字形绘制。默认、悬停、按下、禁用及焦点样式由控件层的 `ButtonStyle` 决定；核心不引用按钮类型。

```rust
use zui_api::{action::ActionId, layout::{Dimension, Edges, LayoutStyle}, view::View};
use zui_widgets::ButtonView;

// theme 与 families 由应用提供；字体仍需显式注册。
let button = View::new(ButtonView::themed(&theme, "保存", families, ActionId(1)))
    .keyed("save")
    .styled(LayoutStyle {
        min_height: Dimension::Length(44.0),
        padding: Edges { left: 16.0, right: 16.0, top: 8.0, bottom: 8.0 },
        ..Default::default()
    });
```

尺寸与 padding 使用通用布局属性，没有隐藏的控件布局算法。主题构造的文字默认水平居中；按钮在最终内容区域中垂直居中文字，并裁剪越界字形。当前支持文字换行和裁剪，尚无省略号、图标或任意子控件插槽。


<a id="button--交互语义"></a>

### 交互语义

- 主鼠标按钮按下请求焦点和鼠标捕获，在按钮命中区域内松开激活；拖出后松开取消，拖回后松开可激活。
- Tab 可聚焦启用的按钮。Space 首次按下进入按下状态，松开激活；Enter 首次按下立即激活。键盘 repeat 不产生重复动作。
- Control/Alt/Super 修饰的 Space/Enter 不执行默认激活。Escape 取消尚未完成的按下手势，并释放鼠标捕获。
- 失焦或输入取消清除手势状态。按下期间禁用或更换 ActionId 会取消旧手势，不能将旧手势用来激活新绑定。
- 禁用按钮不获取焦点/捕获，也不输出激活动作；仍参与命中并处理主按钮输入，避免点击穿透到下层控件。

`Widget::enabled` 是通用契约。核心据此禁止焦点/捕获请求，并在属性协调后清理已禁用节点的所有权；控件自身负责禁用绘制及事件语义。禁用不改变按钮测量结果。


<a id="button--样式与消息"></a>

### 样式与消息

`ButtonStyle` 提供各状态的背景/前景颜色和焦点描边颜色，`from_theme` 从语义主题映射默认值。焦点描边为当前实现固定的 2 个逻辑单位；状态切换只重绘，不重新排版。主题颜色变化保持状态，标签或文字样式变化才重新布局。

成功激活输出 `WidgetAction`，ID 为应用提供的 ActionId，载荷为公开的 `ButtonActivated`。应用在 `map_action` 中检查 ID 与 `payload::<ButtonActivated>()`，映射成自有 Message，在 `message` 中修改业务状态。示例的“切换主题”按钮已使用这条路径。

```sh
cargo run -p zui-window-check --no-default-features --features cpu --locked -- cpu --widgets
cargo run -p zui-render-check --locked -- cpu artifacts/button-cpu.png button
```

窗口示例可点击按钮，或 Tab 聚焦后使用 Space/Enter；不再用全局 Space 快捷键代替按钮动作。定时冒烟模式仍通过模型定时事件切换主题，不能代替真实按钮点击验证。


<a id="button--验证和剩余范围"></a>

### 验证和剩余范围

测试覆盖鼠标点击/拖出/拖回、键盘重复与取消、禁用期间的焦点/捕获清理、按下期间动作绑定变化，以及 CPU/GPU 上五种状态在 1×/2× 下的真实像素。预览使用真实按钮树，并通过事件路由设置焦点和按下状态。

本轮原生桌面自动化连接启动失败，实际鼠标点击和键盘激活尚未由桌面自动化验证。触摸、多指手势、无障碍角色/动作、焦点可见性策略、圆角、图标按钮和可访问性对比度验收仍待实现；这是一批可用的基础行为，不代表完整按钮验收或全部传统控件已完成。

<a id="toggle"></a>


<a id="toggle--togglebutton-切换按钮"></a>

## ToggleButton 切换按钮

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

<a id="checkbox"></a>


<a id="checkbox--复选框与受控状态"></a>

## 复选框与受控状态

`zui-widgets::CheckBoxView` 提供未选中、选中和混合状态，持久控件类型为 `CheckBox`。它复用 Text 排版，以及控件层内部的按压/取消状态机；核心没有复选框分支。

```rust
use zui_api::{action::ActionId, layout::{Dimension, Edges, LayoutStyle}, view::View};
use zui_widgets::{CheckBoxView, CheckState};

let check = View::new(CheckBoxView::themed(
    &theme, "显示详细说明", families, CheckState::Checked, ActionId(2),
))
.keyed("show-details")
.styled(LayoutStyle {
    min_height: Dimension::Length(40.0),
    padding: Edges::all(8.0),
    ..Default::default()
});
```


<a id="checkbox--状态归属"></a>

### 状态归属

选中值是应用持有的受控属性，控件不自行保存第二份业务值。激活时输出 `CheckChanged { state }` 请求；应用在 `map_action` 中识别 ActionId 和载荷，将请求映射为 Message，并在 `message` 中修改自己的状态，随后重建 View。

| 当前值 | 激活请求 |
| --- | --- |
| Unchecked | Checked |
| Checked | Unchecked |
| Mixed | Checked |

Mixed 用于应用表达部分选中，不在三种值之间自动循环。应用可以接受、拒绝或校验请求；若不回写 View，视觉值保持不变，下一次激活仍基于当前值请求。仅选择值变化产生 Paint 失效，不重新测量标签。

外部选择值或动作 ID 在按下期间改变，会取消尚未完成的手势，避免使用旧状态生成新请求。


<a id="checkbox--输入和布局"></a>

### 输入和布局

指示框和标签所在的整块控件区域均可点击；拖出松开取消，拖回松开激活。Tab 聚焦，Space 按下后松开切换，重复键不重复激活；Enter 不作为复选框默认激活键，保留给上层。Escape、失焦、禁用和输入取消清理按压状态。禁用时不能取得焦点/捕获，也不输出值变更请求，但仍阻止主按钮点击穿透。

测量时先扣除指示框宽度与间距，再将剩余宽度交给文字服务；自然高度取指示框与文字高度的较大值。最终准备使用最终内容宽度重新排版，指示框和文字垂直居中，绘制限制在内容区域内。标签可换行，当前指示框位于整个标签块的垂直中心。

`CheckBoxStyle` 在控件层提供默认、悬停、按下、禁用颜色，以及焦点颜色、指示框尺寸与标签间距。默认尺寸为 20 个逻辑单位，间距来自主题 Sm。尺寸必须有限且不小于 4，间距必须有限且非负；非法样式通过布局错误返回，不能发布场景。修改尺寸或间距会重新布局。


<a id="checkbox--验证与运行"></a>

### 验证与运行

```sh
cargo run -p zui-render-check --locked -- cpu artifacts/checkbox-cpu.png checkbox
cargo run -p zui-window-check --no-default-features --features cpu --locked -- cpu --widgets
```

窗口示例已加入“显示详细说明”复选框，应用接收 `CheckChanged` 后更新说明文字。离屏预览展示三态和禁用外观。

测试覆盖标签点击、请求不自动回写、应用回写跳过测量、Mixed 转换、键盘/取消/禁用、外部状态变化、剩余宽度排版、非法样式及 CPU/GPU 的 1×/2× 实际像素；共享交互逻辑提取后，按钮回归继续通过。

触摸、无障碍选中/混合语义、任意标签插槽、RTL 指示框位置及真实桌面点击验收尚未完成。原生桌面自动化连接此前失败，本轮未将窗口冒烟等同于复选框交互实测。

<a id="radio"></a>


<a id="radio--单选组与受控选择"></a>

## 单选组与受控选择

状态：纵向与横向等宽 `RadioGroup` 已实现，整个组是一个持久控件节点和一个 Tab 焦点位置。支持 CPU/GPU 绘制；触摸、无障碍语义和独立单选按钮的跨节点分组尚未实现。


<a id="radio--公共入口"></a>

### 公共入口

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


<a id="radio--输入与状态"></a>

### 输入与状态

- Tab/Shift+Tab 进入或离开整个组。进入时优先定位已选中的可用项，否则定位第一个可用项；不会因此自动提交选择。
- 上/左方向键前移，下/右方向键后移，跳过禁用项并循环。Home/End 定位首/末可用项。按下即请求选择，允许方向键重复；Control/Alt/Super 组合不被组消费。
- Space 按下进入按压状态，松开请求活动项；Enter 留给上层。方向键会取消尚未完成的按压手势，再移动活动项。
- 鼠标可点击圆形指示器或所在行的标签区域。只有同一选项内按下并松开才请求选择；按住移到别项再松开不会误选，移出后回到原选项仍可激活。
- 外层 padding、行间距、禁用项和内容裁剪区域之外不触发选择。鼠标捕获使用公共事件接口，命中依赖 EventGeometry 和准备好的行区域，不读取核心树。
- Escape、失焦、捕获丢失、选项或选中值更新、动作绑定变化及禁用会取消相关未完成手势。全组禁用或全部选项禁用时，组不可聚焦。

选项变化保留仍存在且可用的活动 ID，选中值变化则重新定位活动项。外部选项更新会取消旧手势并重新准备标签和行布局，避免重排后的坐标复用导致误选。


<a id="radio--布局与绘制"></a>

### 布局与绘制

外层仍使用公共 Flex/Grid 布局；组内默认采用纵向排列。标签复用 Text 与注入的文字服务，测量和最终准备都预留指示器宽度与标签间距，每行高度取标签高度与指示器尺寸的最大值。长标签自动换行，空组的自然内容尺寸为零。

样式提供正常、悬停、按压、禁用颜色，焦点色、指示器尺寸、标签间距和行间距。指示器尺寸至少 4 个逻辑单位，间距必须有限且非负。绘制使用公共路径、字形和裁剪描述；CPU/GPU 后端都不认识单选组。内容超过固定高度时裁剪，当前不会自动滚动。


<a id="radio--横向等宽排列"></a>

#### 横向等宽排列

设置 `group.orientation = RadioOrientation::Horizontal` 启用横向模式。可用内容宽度先扣除选项间距，再等分给全部选项（包括禁用项）；每个槽位预留圆点和标签间距，文字在余下宽度内换行。所有槽位使用最长标签所需的共同高度，垂直居中绘制圆点与标签。无约束自然宽度按最大选项宽度等分排列计算。

style.row_gap 在横向模式表示相邻选项的水平间距。组不自动拆成多行，也不自动水平滚动；外层不足时按内容边界裁剪。宽度小于间距总和时槽位宽度为零，不产生可点击选项区域。padding、间隙和禁用项仍不会选中。

切换方向会触发布局并取消未完成点击/空格手势，保留组节点身份和有效的选项 ID。键盘仍接受上/左为上一项、下/右为下一项，与默认纵向语义一致；未增加 RTL 视觉顺序。横向布局并不等同于 SegmentedControl 或 Tab 页签，这些控件仍需独立的视觉和内容语义。

新增行为测试覆盖共同高度、约束变化换行、水平命中、间隙/内边距/禁用、零宽槽位、方向切换取消、稳定身份、重排和空组。新增 CPU/GPU 像素测试检查第三项圆点、首项未选中背景、间隙与外部边界，覆盖 1×/2×。

```sh
cargo run -p zui-render-check --locked --offline -- cpu artifacts/radio-horizontal-cpu.png radio-horizontal
```


<a id="radio--验证与示例"></a>

### 验证与示例

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


<a id="radio--内部交互复用"></a>

### 内部交互复用

RadioGroup 的单选交互现由控件库内部 choice 模块维护：它只接收稳定 ID、禁用状态、选中值和调用方计算的命中选项，返回公共 EventResponse 与可选选择请求。选项文字、布局、绘制和 RadioChanged 载荷仍由 RadioGroup 负责。该模块不依赖核心、文字服务或渲染后端，也不公开为应用需要操作的状态。

方向键直接扫描选项切片，不为每次按键分配临时可用项数组，时间仍为 O(N)。测试枚举四项选择的全部禁用组合以及无效/空活动身份，核对方向、首尾与环绕；重排/禁用测试核对活动身份保留和按压取消。此抽取用于后续分段选择复用，不表示 SegmentedControl 或页签已完成。

<a id="switch"></a>


<a id="switch--受控开关-switch"></a>

## 受控开关 Switch

SwitchView/Switch 提供独立布尔开关，使用公共 Widget、Painter 和主题契约，不依赖核心、具体布局引擎或 Vello。标签通过 row 和 TextView 等普通控件组合；当前没有标签点击联动或无障碍名称关联。

```rust
let mut toggle = SwitchView::themed(&theme, model.enabled, ActionId(9));
toggle.transition = None; // 关闭运动，适用于减少动效或低资源配置
let view = View::new(toggle);
```


<a id="switch--值与交互"></a>

### 值与交互

checked 是应用控制的值。鼠标主键在控件内按下、仍在控件内松开时输出 SwitchChanged { checked: !checked }；不会提前修改内部值。应用处理动作并重建 View 后才确认变化。未回写时重复激活仍请求相同目标值。

复用现有 Interaction 状态机：鼠标按下请求焦点与捕获，拖出松开取消；聚焦后 Space 按下显示按压状态，释放激活，重复按下不重复触发。Escape、失焦、捕获丢失以及禁用/动作或值变化取消未完成激活。Enter 不切换，Control/Alt/Super 修饰的 Space 不激活。禁用开关仍可命中以阻止穿透，但不能聚焦或激活。触摸手势及原生设备输入验收仍待完成。


<a id="switch--视觉布局与动画"></a>

### 视觉、布局与动画

SwitchStyle 分别提供普通、悬停、按下、禁用配色，以及焦点环、轨道宽高和滑块内缩。每套颜色包含开/关轨道与开/关滑块颜色、边框。默认宽 48、高 26、内缩 3 逻辑像素，主题映射保证深色主题下关闭滑块可辨识。

轨道采用胶囊路径，滑块采用圆形路径，绘制裁剪到内容区域。轨道在可用内容区域居中；小于自然尺寸的区域会裁剪，不自动压扁形状。布局仍支持外层 padding、宽高和容器组合。宽高必须有限、宽不小于高、高至少 4；内缩至少 1 且两倍小于高度。非法样式在测量/准备阶段返回 InvalidStyle，固定尺寸不能绕过校验。

默认确认值变化后，滑块在 120 毫秒内平滑移动，轨道/滑块颜色立即采用确认值。首次构建直接显示当前值。动画使用注入的 elapsed 时钟，仅产生 Paint 失效和按需下一帧；途中反向从当前显示位置继续。transition=None 或禁用时立即归位并停止调度；Some(Duration::ZERO) 被拒绝。这里的开关过渡不是通用动画服务或所有控件动效已完成。


<a id="switch--验证"></a>

### 验证

三项行为测试覆盖受控请求、鼠标捕获与取消、Space/Enter/Escape、禁用、动画反向、减少动效和非法样式恢复。CPU/GPU 像素用例分别检查开关滑块位置、轨道颜色、圆角和内容边界，在 1×/2× 下验证。

```sh
cargo test -p zui-render-check --test switch --all-features --locked --offline
cargo test -p zui-render-check --test switch --all-features --locked --offline switch_track_and_thumb_render_gpu -- --ignored
cargo run -p zui-render-check --locked --offline -- cpu artifacts/switch-cpu.png switch
```

原生点击、触摸、屏幕阅读器语义与无 GPU 嵌入式性能仍需验收。预览是实际 Tree/Taffy/Parley/Vello CPU 组合生成的静态图，不代表这些设备验收已经完成。

<a id="segmented"></a>


<a id="segmented--分段选择-segmentedcontrol"></a>

## 分段选择 SegmentedControl

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


<a id="segmented--数据与交互"></a>

### 数据与交互

SegmentId 在组内稳定且唯一，不能用重排后的索引代替业务身份。selected 为应用持有的 Option<SegmentId>，None 表示未选择。选中值必须存在，可以是禁用项；重复 ID、缺失选中项和非法尺寸在测量/准备阶段拒绝。

点击或键盘操作输出 SegmentChanged { selected } 请求，应用处理后重建 View 才改变选中背景。重复激活当前选中项不输出请求；活动焦点位置与确认值分开，应用拒绝请求时仍可继续导航。

整个组只有一个节点和一个 Tab 停靠点。上/左方向键前移，下/右后移，跳过禁用项并循环；Home/End 到首尾。Space 按下、释放激活当前活动项，Enter 不由组消费。Control/Alt/Super 修饰键不触发导航或激活。聚焦时优先使用已选中的可用项，否则使用首个可用项。

指针必须在同一可用分段内按下并松开才产生选择请求；跨段释放、间隙、外部 padding 和禁用段不选择。按下请求捕获，Escape、失焦、捕获丢失、选项/几何或选中值更新、动作变化与禁用取消手势。重排保留仍有效的活动 SegmentId，全禁用或空组不可聚焦。


<a id="segmented--布局与视觉"></a>

### 布局与视觉

可用内容宽度扣除 gap 后等分，禁用项同样占位。每段扣除 padding_x 后排版标签；标签默认居中，可通过 text_style.align 配置，长文字在槽位内换行。自然高度取所有标签所需高度加上下 padding_y，与 min_height 的最大值；无约束自然宽度按最大段宽等分计算。空组自然内容大小为零。

最终槽位铺满分配的内容高度，标签垂直居中并在自己的内边距区域裁剪。分段不自动换成多行，也不自动滚动或压缩成弹出菜单；过小的固定尺寸按内容区域裁剪。宽度不足以容纳间隙时槽位宽为零，不存在有效点击区域。

SegmentedStyle 提供普通、悬停、按下、禁用四套背景/文字/边框颜色，每套包含选中背景和选中文字；另有活动焦点环。默认使用直角矩形分段、2 逻辑像素间隙、水平内边距 12、垂直内边距 8、最小高度 32。样式尺寸必须有限非负。颜色更新只请求 Paint，几何和文字变化请求 Layout。

本控件是单选筛选器/模式切换器，不包含页签内容管理；尚未实现多选分段、图标、自动溢出、RTL、过渡动画、触摸手势和选项级无障碍语义。


<a id="segmented--验证与预览"></a>

### 验证与预览

三项行为测试覆盖受控请求、禁用/间隙/padding 命中、捕获取消、键盘与单一焦点、重排、长标签、空组、零宽槽位和非法配置。窗口侧集成测试验证动作到模型回写、重新协调后焦点身份保持以及禁用清理。CPU/GPU 像素测试在 1×/2× 下检查选中/普通/禁用背景、间隙和外部裁剪。

```sh
cargo test -p zui-render-check --test segmented --all-features --locked --offline
cargo test -p zui-render-check --test segmented --all-features --locked --offline segmented_selection_disabled_and_spacing_render_gpu -- --ignored
cargo test -p zui-window-check --test segmented --locked --offline
cargo run -p zui-render-check --locked --offline -- cpu artifacts/segmented-cpu.png segmented
```

预览使用真实 Tree/Taffy/Parley/Vello CPU 链路。合成输入和离屏像素测试不代表原生桌面点击、屏幕阅读器或嵌入式设备性能已经验收。

<a id="slider"></a>


<a id="slider--水平滑块与数值范围"></a>

## 水平滑块与数值范围

状态：首期水平 Slider 已实现受控数值、鼠标拖动、步进键盘操作与 CPU/GPU 绘制。垂直方向、触摸、无障碍、刻度、值提示和双端 RangeSlider 尚未实现。


<a id="slider--范围与状态所有权"></a>

### 范围与状态所有权

`NumericRange::new(min, max, step)` 返回经过验证的范围或 `InvalidNumericRange`。范围端点必须有限，min 小于 max，跨度有限；步长有限、为正且不超过跨度。跨度与步长之比不得超过 2^52，端点处必须能表示一个步长的变化，避免浮点精度不足造成无法移动的控件。字段私有，通过 min/max/step 只读方法访问。

```rust
let slider = SliderView::themed(
    &theme,
    NumericRange::new(0.0, 100.0, 5.0)?,
    50.0,
    ActionId(4),
);
let view = View::new(slider);
```

`SliderView.value` 由应用持有，必须有限且位于范围内；不要求恰好落在步进网格上。非法值在测量或最终准备时返回布局错误。控件不乐观修改显示值，只发出 `SliderChanged { value }`，应用按 ActionId 映射消息并回写 View。

步进网格从 min 开始，max 始终可达，即使最后一个区间不足一个步长。拖动按最近网格值或末端点吸附，键盘从当前值移到相应方向的网格点。小数使用 f64 近似表示，不用于需要十进制定点精度的业务计算。


<a id="slider--连续拖动协议"></a>

### 连续拖动协议

按下滑块时记录指针相对滑块中心的偏移，不立即吸附或跳动；点击轨道直接请求对应值。捕获期间可以拖到控件外，数值限制在端点范围。未移动就松开滑块不会把应用传入的非网格值改成吸附值。

拖动记录上次请求，抑制连续相同请求。应用回写这个值属于确认，保留拖动；应用回写其他值，或改变范围、动作绑定、几何样式、禁用状态，则取消旧拖动。应用不接受请求时显示仍保持原值，指针移动到其他档位仍能提交新请求。该确认协议面向当前同步 Host 消息回写，不是异步消息确认协议。

Escape、失焦和捕获丢失取消尚未完成的拖动，不回滚应用已经接受的值。松开主键释放捕获；外部属性更新取消拖动后，不再发出移动请求，核心仍按自身规则在按钮释放或输入取消时回收捕获。

外层 padding 不启动拖动。轨道与滑块使用同一局部几何计算；极窄内容导致轨道长度为零时，不启动鼠标捕获，仍可通过键盘改变数值。


<a id="slider--键盘与样式"></a>

### 键盘与样式

Tab 进入滑块。右/上方向键增加一个档位，左/下方向键减少一个档位；PageUp/PageDown 移动十个档位；Home/End 请求端点，允许键盘重复。Control/Alt/Super 组合、Space 与 Enter 留给上层。键盘调整会取消现有鼠标拖动并释放捕获。

`SliderStyle` 提供正常、悬停、按压、禁用的轨道/填充/滑块颜色，以及焦点色、滑块尺寸、轨道厚度和自然宽度。自然宽度服从布局可用空间；固定尺寸由外层布局决定。首期使用矩形滑块，尺寸随过小的内容区域缩小并裁剪。样式数值必须有限：滑块至少 4 个逻辑单位、轨道厚度大于零且不超过滑块尺寸、自然宽度不小于滑块尺寸。

范围计算位于控件库内部的独立 numeric 模块；Slider 管理自己的拖动和视觉状态，通过公共事件、动作和 Painter 契约协作。核心、布局引擎和后端均没有 Slider 专用分支。


<a id="slider--验证与示例"></a>

### 验证与示例

```sh
cargo run -p zui-render-check --locked -- cpu artifacts/slider-cpu.png slider
cargo test -p zui-render-check --all-features --locked --test slider
# 需要真实 GPU。
cargo test -p zui-render-check --all-features --locked --test slider -- --ignored
cargo run -p zui-window-check --all-features --locked -- cpu --widgets
```

控件测试覆盖连续拖动确认、捕获到边界外、抓取偏移、未移动松开、外部覆盖、取消、键盘、小数步长、非完整末区间、非法输入、极窄布局以及 CPU/GPU 1×/2× 像素。

窗口示例提供“设定值”滑块与数值文本。离线 Host 测试验证实际应用模型在连续鼠标输入之间回写 View 后仍保留捕获，并验证完整 Tab 顺序。窗口定时冒烟验证呈现与生命周期，不等价于真实桌面拖动自动化。

<a id="stepper"></a>


<a id="stepper--stepper-数值步进器"></a>

## Stepper 数值步进器

StepperView 在控件层组合减号按钮、数值 Text 和加号按钮。底层 Button 不感知 Stepper；内部包装仅把 ButtonActivated 转成 StepperChanged。数值计算复用 NumericRange，与 Slider 使用同一套步长和端点规则。

```rust
use zui_api::action::ActionId;
use zui_widgets::{NumericRange, StepperView};

let range = NumericRange::new(0.0, 10.0, 3.0)?;
let mut stepper = StepperView::themed(&theme, families, 3.0, range, ActionId(19));
stepper.display = Some("3 台设备".into());
let view = stepper.build()?;
```

`StepperChanged { value }` 是请求，应用确认后更新 value 并重建。拒绝请求不会改变数值或累加下一步。value 必须有限且在范围内，build 在创建子节点前拒绝非法值；允许非步长网格上的值，向上/向下到对应方向的相邻网格。端点始终可选，例如 0..10、step=3 时 9→10、10→9。

display 是可选的业务格式化字符串，不参与计算，也不作为可编辑文本解析；未设置时用 value 的标准字符串表示。单位、本地化及小数位由应用决定。按钮与数值文字样式、三部分布局、根 key 均可配置。根固定采用 Row，子节点使用 decrement/value/increment 稳定 key。

减号和加号分别是普通按钮焦点，支持鼠标、Space 和 Enter。到达端点时对应按钮禁用；disabled 禁用两个按钮，数值继续显示。确认数值或范围变化时取消尚未释放的手势，即便新旧数值算出的下一步相同；动作绑定和禁用变化沿用 Button 的取消规则。更新保留子节点身份；焦点按钮变为禁用时由 core 清理焦点。

当前不是可编辑 SpinBox，也未提供单焦点方向键、长按连发、滚轮调值或控件级无障碍语义。


<a id="stepper--验证"></a>

### 验证

三项真实 Tree/Taffy/Parley 集成测试覆盖受控鼠标和键盘请求、非网格数值、短末区间、端点及整体禁用、外部改值/范围/绑定取消、格式化更新保留身份，以及 NaN/无穷/越界拒绝。Button 四项回归通过。widgets/render-check 严格 Clippy、格式、23 包依赖检查和 12 包 Cortex-M 裸机库构建通过。

复用既有 Button/Text 绘制，本阶段未新增 CPU/GPU 像素或原生设备专项验收。


<a id="stepper--应用示例"></a>

### 应用示例

`window-check --widgets` 中的 Stepper 与 Slider、ProgressBar 共用 `Demo.level`（0..100、步长 5）。Stepper 使用 ActionId(6) 映射为 SetLevel，应用确认后重建三者；百分比字符串由应用格式化。没有将业务模型或消息类型引入 widgets。

```sh
cargo run -p zui-window-check --features gpu --locked --offline -- cpu --widgets --smoke-ms 1600
cargo run -p zui-window-check --features gpu --locked --offline -- gpu --widgets --smoke-ms 1600
```

2026-10-28 当前 macOS 开发机，两条命令均退出码 0，均报告 events=12/redraws=3，逻辑窗口 800×480、scale=2，并完成自动主题切换。该记录验证窗口与呈现路径，不代表原生手动交互或截图验收。

扩展的 Host 集成测试验证 Slider 改值后 Stepper 请求回写到同一模型、零值时 Tab 跳过禁用减号、按钮激活重建后焦点保持、后续 TextInput 输入不受影响，以及主题切换后步进按钮身份保留。window-check 全目标全特性严格 Clippy、格式及依赖检查通过。

<a id="progress"></a>


<a id="progress--进度条"></a>

## 进度条

ProgressBar/View 提供确定进度、不确定进度、四个增长方向及主题样式。它是被动显示控件，不接收焦点、不发出修改值的动作；应用更新 View 中的进度值。

```rust
let view = ProgressBarView::themed(&theme, ProgressValue::Determinate(0.35));
let busy = ProgressBarView::themed(&theme, ProgressValue::Indeterminate);
```


<a id="progress--数值与布局"></a>

### 数值与布局

Determinate 接收有限的 0..=1 比例，0 仅绘制轨道，1 填满轨道。NaN、无穷和越界值作为非法样式报错，不静默截断。使用字节数或任务数量时，由应用按业务语义计算比例；进度条不依赖带步进的 Slider 数值范围。

ProgressDirection 支持 LeftToRight、RightToLeft、BottomToTop 和 TopToBottom。轨道使用扣除 padding 后的内容区域，并在横向进度条中垂直居中、纵向进度条中水平居中。thickness 默认 8，natural_length 默认 200；确定尺寸由布局容器决定，厚度受实际内容空间限制。空内容区域不绘制，也不请求动画帧。

普通数值、颜色和禁用状态更新只需要绘制失效。方向或自然尺寸变化触发布局，非法配置必须重新验证；同一 Key 下保留控件身份。控件由公共 Widget 契约实现，核心和 CPU/GPU 后端不增加进度条分支。


<a id="progress--不确定进度与动画"></a>

### 不确定进度与动画

不确定进度以固定长度片段在轨道内部往返，默认占轨道的 25%，完整往返周期 1500 ms，请求帧间隔 33 ms。时间从公共 animate 回调传入；直接计算当前相位，不创建定时器或补跑遗漏帧。帧请求仍由上层按窗口可见性和其他期限合并。

ProgressAnimation 可配置 cycle 和 frame_interval；两者必须非零，帧间隔不得超过周期。segment_fraction 必须有限且严格介于 0 和 1。主题颜色改变保留动画相位，模式、动画配置或禁用状态改变重置计时。确定进度、禁用、空内容区域不请求动画；animation 为 None 时，不确定进度显示居中的静止片段，供减少动态效果的界面使用。

在独立 Tree 中需推进 advance_animations；WidgetHost 已接通此流程。没有时间推进时仍显示初始片段。共享调度说明见 [按需动画](runtime.md#animation)。


<a id="progress--样式与验证"></a>

### 样式与验证

ProgressBarStyle 提供轨道、正常填充、禁用填充颜色以及自然长度、厚度、片段比例；默认值来自语义主题。文字标签与百分比通过 Text/布局容器组合，不让进度条承担文字排版或业务状态解释。

测试覆盖数值更新不重排、无焦点/动作、主题更新保留相位、停止条件、非法值恢复，以及四个方向的 0%、25%、100%、动画与禁用像素。CPU 和真实 GPU 均验证 1×/2× 的方向和端点；动画与禁用像素另在 1× 验证。已生成并目视检查 `artifacts/progress-cpu.png`，桌面示例的滑块下方同步显示当前比例。

```sh
cargo run -p zui-render-check --locked -- cpu artifacts/progress-cpu.png progress
cargo test -p zui-render-check --all-features --locked --test progress
# 真实 GPU 环境
cargo test -p zui-render-check --all-features --locked --test progress -- --ignored
```

当前为矩形轨道，不含圆角、缓冲进度、分段标记、圆环进度、自动数值缓动或无障碍语义；这些仍在完整控件目录中。原生窗口交互与嵌入式实机性能需继续验证。
