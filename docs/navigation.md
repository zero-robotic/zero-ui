# 列表、页签与折叠容器

[文档索引](README.md) · [模块边界](module-boundaries.md) · [未完成事项](implementation-plan.md)

按架构职责归并的专题参考。设计目标与接口约束分开理解；待办统一维护在实施计划中，各节的验证说明仅覆盖所述范围。

## 章节导航

- [ListBox 单选列表](#list-box)
- [Tabs 页签容器](#tabs)
- [Disclosure 可折叠面板](#disclosure)
- [Accordion 分组折叠容器](#accordion)

<a id="list-box"></a>


<a id="list-box--listbox-单选列表"></a>

## ListBox 单选列表

ListBoxView 提供普通规模的受控单选列表，复用控件内部 ChoiceState 与 Text，不依赖核心、Taffy/Parley 或具体渲染后端。

```rust,ignore
let list = ListBoxView::themed(
    &theme,
    vec![ListItem::new(ListItemId(1), "设备概览"), ListItem::new(ListItemId(2), "运行记录")],
    families,
    model.selected,
    ActionId(10),
);
let view = View::new(list).styled(LayoutStyle {
    width: Dimension::Length(240.0),
    height: Dimension::Length(160.0),
    ..Default::default()
});
```

选择请求为 ListSelectionChanged { selected }。应用确认后更新 selected 并重建；控件不先行修改选中状态。ListItemId 应保持业务身份并在列表内唯一；重复 ID、未知选择和非法几何样式在测量/准备阶段返回 InvalidStyle。禁用项允许保持选中，但不能交互；空列表与全禁用列表不参与焦点。


<a id="list-box--交互与滚动"></a>

### 交互与滚动

列表占一个 Tab 焦点。方向键、Home/End、Space 复用单选状态机，跳过禁用项并循环；Left/Right 也按前后项处理。活动项与模型确认的选中项可以不同：例如应用拒绝选择请求时，键盘活动仍可移动，背景继续显示模型值。

PageDown/PageUp 从活动行顶部沿对应方向移动一个视口高度，选择达到该位置的首个可用行；不足一页时定位末项/首项，不循环。若单行高于视口，也至少尝试移动到相邻可用项。分页只处理标题列表自身焦点下的无 Ctrl/Alt/Super 按键，支持重复按下，释放不发选择请求；复用受控动作和按压取消规则。

字符定位处理无 Ctrl/Alt/Super 的 Character 按键，允许 Shift 和重复按下；优先使用该事件的 text，没有 text 时使用 Character 的字符串。每次以本次事件的完整文本作为标签前缀，从活动项之后开始循环查找，跳过禁用项，最后考虑当前项。匹配成功复用分页导航的受控选择、滚入和手势取消流程；应用未确认选择时，后续搜索仍从新的活动项继续。释放不触发动作，空串、纯空白、控制字符和无匹配输入不改变状态。

匹配按 Unicode 小写字符序列比较，支持中文等多字符前缀；不执行完整 case folding、Unicode 规范化、拼音或地区排序。每个事件独立搜索，尚未累积连续输入或设置搜索超时，也不启用 IME。查找为按标签前缀比较的线性扫描，不分配每项的小写副本，不依赖系统时钟或平台后端。

活动项改变或获得焦点时自动滚入内部视口；过高的单行以顶部为目标。外部选择更新只需 Paint 即可调整已有行的滚动位置。重排保留活动项 ID，布局变化重新计算行几何并收敛滚动范围。鼠标命中使用相同偏移和内容裁剪，间隙、禁用项及视口外不能选中。

滚轮支持像素与行单位，方向与 Scroll 一致。实际移动时停止向祖先传播；到达边界且本次未移动时允许外层滚动容器接管。滚动、尺寸变化、选项/选择/绑定变化及禁用会取消未完成的选择手势，避免释放旧按压激活另一行。默认显示右侧覆盖式滚动条，scrollbar=None 可关闭。滑块可拖动，轨道点击按一个视口翻页，不改变模型选择；滑块优先于背后条目命中。滚动条取得列表焦点时不会将刚完成的轨道滚动重置到选中项。拖动捕获支持指针移出视口，释放或捕获丢失会结束拖动。


<a id="list-box--布局和样式"></a>

### 布局和样式

标签在可用宽度内换行，每行高度为标签高度加上下内边距与 min_row_height 的较大值。行间距可配置。自然高度最多 natural_max_height（默认 240），显式 height 可覆盖；所有行仍参与测量，视口只裁剪绘制并跳过不可见行。

ScrollbarStyle 与 Scroll 共用，可配置宽度、最短滑块和颜色；滚动条不占布局空间，标签右侧应保留足够内边距，避免被覆盖。宽度/最短滑块必须为有限正数。两种控件共享内部轴/滚动条几何与拖动偏移算法，控件各自拥有输入与选择状态。

ListBoxStyle 配置正常/悬停/按下/禁用状态的背景、文字和选中配色，以及焦点环、行内边距、间距和尺寸。text_style 配置文字。ListBox::scroll_offset 提供当前内部偏移只读值，当前没有公开的滚动定位请求。

所有选项与文字排版均驻留，构建/布局/查找开销随项目数增加；当前不是虚拟列表。海量数据需要后续将选择状态与 VirtualList 组合，不应直接传入百万项。


<a id="list-box--验证与后续"></a>

### 验证与后续

十项测试覆盖受控选择、禁用跳过、键盘滚入、无需重新布局的外部选择滚动、重排身份、滚轮取消按压、非法 ID/空列表恢复、宽度换行与尺寸变化取消，以及 1×/2× 选中行 CPU 像素。新增分页用例验证视口步长、禁用跳过、端点不循环、超高行、修饰键/释放排除和 Space 取消。RadioGroup 九项、SegmentedControl 四项及控件六项单元测试回归通过。新增滚动条用例验证轨道翻页、视口外拖动/释放、捕获取消、不误选及 1×/2× 端点像素，Scroll 双轴四项与滚动条三项回归通过。相关严格 Clippy、no-default-features 编译、格式与 23 包依赖检查通过。

新增两项字符定位测试覆盖循环/禁用/受控请求、中文及大小写、重排、无匹配、修饰键与空白过滤、重复/释放及 Space 取消。

多选/范围选择、超时累积搜索、IME 搜索、虚拟化选择、触摸、项目级无障碍和原生设备验收仍待实现。2026-10-28 已完成 ListBox 专项真实 GPU 离屏像素验收，见下节。


<a id="list-box--cpugpu-离屏验收"></a>

### CPU/GPU 离屏验收

选中行与滚动条测试共享 `OffscreenRenderer` 验证函数。CPU 默认执行，GPU 用显式 ignored 测试调用真实适配器，初始化失败会使测试失败，不静默跳过。

```sh
cargo test -p zui-render-check --test list_box --locked --offline
cargo test -p zui-render-check --test list_box --no-default-features --features gpu --locked --offline -- --ignored
```

2026-10-28 当前 macOS 开发机：CPU 配置 10 项通过；GPU-only 配置显式执行 1 项通过。两种后端均检查 1×/2× 的选中背景、外部选择滚入后的像素、顶部/底部滚动条滑块，以及清空条目后整帧恢复背景色。后端共用语义和像素采样断言，不要求文字抗锯齿逐像素一致，也未完成字体/裁剪的全面视觉验收。此结果不代表原生窗口交互、跨平台或嵌入式设备验收。

<a id="tabs"></a>


<a id="tabs--tabs-页签容器"></a>

## Tabs 页签容器

TabsView 在控件层组合页签栏和当前页面。页签栏复用 SegmentedControl 的布局、绘制和选择交互；通过 TabChanged 提交选择请求，由应用修改 selected 后重建。核心、布局、渲染模块无需认识 Tabs。

```rust,ignore
let mut tabs = TabsView::themed(
    &theme,
    vec![TabOption::new(TabId(1), "概览"), TabOption::new(TabId(2), "趋势")],
    families,
    Some(model.selected),
    ActionId(42),
);
tabs.layout.width = Dimension::Percent(1.0);
let view = tabs.build(|id| model.page(id))?;
// map_action 读取 TabChanged，message 更新 model.selected。
```


<a id="tabs--构建与身份"></a>

### 构建与身份

- TabId 在同一容器内唯一且保持业务身份，不以位置作为 ID。
- build 在调用内容工厂前校验重复 ID 和不存在的 selected，返回 TabsError；无选择时不调用工厂，合法选择时恰好调用一次。空列表允许无选择。
- 选中页可以处于 disabled 状态，仍显示内容；disabled 仅阻止页签交互，不禁用页面内的控件。不会自动改选第一项。
- 根节点为 Column；页签栏使用固定文本 key，页面包装容器使用数字 TabId key，二者不会冲突。同一选择的重建、页签重排保留页面身份；切换选择卸载旧页面，即使两页内部使用相同 key 也不会误复用。
- 仅挂载选中页，不缓存未选中页的控件树。持久业务数据放在应用模型；编辑状态可配合 TextInputDraft 保存。再次打开旧页会创建新节点。


<a id="tabs--布局外观与输入"></a>

### 布局、外观与输入

layout 配置整个容器，header_layout 配置页签栏尺寸，panel_layout 配置页面包装容器。根与页面包装容器的布局类型固定为 Column；页签栏固定为 Leaf。默认页签栏 shrink 为 0；固定高度容器的剩余空间分配可由应用设置 panel_layout.grow。内容滚动由页面显式组合 Scroll。

header_style 使用 SegmentedStyle，text_style 配置文字；标签栏共用一个外边框，相邻标签只绘制一条分隔线，间隙固定为零（忽略 header_style.gap）；默认 `compact=true`，页签按文字宽度和内边距紧凑靠左排列，不填满剩余空间；`compact=false` 恢复等宽布局。空间不足时按比例收缩并允许标签换行，正常/选中/禁用和焦点视觉复用分段控件。

页签栏占一个 Tab 焦点位置。方向键、Home/End 选择并跳过禁用项，Space 请求活动项，指针沿用捕获与取消规则。模型确认选择后，栏内焦点保持；Tab 按树顺序进入当前页。外部切换若卸载了焦点所在页面，核心清理焦点，不自动跳到新页面。


<a id="tabs--验证与范围"></a>

### 验证与范围

窗口侧集成测试使用真实 Taffy/Parley 和 WidgetHost，验证动作模型回写、禁用跳过、页面工厂/构建/销毁次数、稳定节点、进入内容区、切换清理焦点及无效选择。另回归 SegmentedControl 模型回写。本阶段没有新增绘制原语，未重跑 GPU 像素测试。

当前支持横向紧凑/等宽页签、关闭、拖动重排和单页挂载。关闭与拖动命中使用实际标签边界。页签溢出滚动、竖向/RTL、Ctrl+Tab、保留非活动控件树和页签级无障碍语义尚未实现。


<a id="tabs--可运行示例"></a>

### 可运行示例

```sh
# CPU 原生窗口：概览、可编辑巡检备注、禁用归档页签
cargo run -p zui-window-check --locked --offline -- cpu --tabs
# GPU 原生窗口（需可用 GPU 和桌面环境）
cargo run -p zui-window-check --features gpu --locked --offline -- gpu --tabs
# 有限时长运行，在中途自动切换到巡检页
cargo run -p zui-window-check --locked --offline -- cpu --tabs --smoke-ms 2000
# 静态 CPU 预览
cargo run -p zui-render-check --locked --offline -- cpu artifacts/tabs-cpu.png tabs
```

窗口示例在应用层拥有 selected、备注字符串和 TextInputDraft，通过 TabChanged/TextInputEvent 更新模型；控件不引用示例业务类型。--tabs 与 --widgets 互斥。巡检页面卸载后释放草稿的活动绑定，重新进入时恢复绑定；示例集成测试实际经过 WidgetHost 和文本布局验证这一过程。撤销/选区保存机制沿用 TextInputDraft，详见 [文本输入](text.md#text-input)。

静态预览位于 artifacts/tabs-cpu.png，已目视检查文字、选中/禁用状态和页面布局。窗口示例已编译、完成模型集成测试及严格 Clippy；原生窗口手动交互、实际 IME 和 GPU 运行尚未在本阶段验收。

新增增强交互及能力边界见 [控件增强](advanced-widgets.md#advanced-controls)。

<a id="disclosure"></a>


<a id="disclosure--disclosure-可折叠面板"></a>

## Disclosure 可折叠面板

DisclosureView 由标题按钮与可选内容容器组成。标题复用 Button 的测量、绘制、指针捕获、Space/Enter 激活和取消机制；在 widgets 层转换为 DisclosureChanged，不向核心或渲染模块引入具体控件类型。

```rust,ignore
let mut panel = DisclosureView::themed(
    &theme, "高级设置", families, model.expanded, ActionId(10),
);
panel.key = Some("advanced-settings".into());
panel.layout.gap = Size::new(0.0, 12.0);
let view = panel.build(|| settings_view(&model.settings));
// map_action 读取 DisclosureChanged.expanded；message 更新模型并重建。
```


<a id="disclosure--状态和生命周期"></a>

### 状态和生命周期

expanded 是应用确认的状态。标题激活只发出请求，不立即修改展开状态；如果应用拒绝，界面保持原状。标题通过 + / − 前缀显示收起/展开状态。

build 仅在 expanded=true 时调用一次内容工厂。收起时不生成内容 View；从展开切为收起会卸载整个内容子树。再次展开创建新节点，持久业务数据应放在模型，文本编辑状态可通过 TextInputDraft 保存。

根节点为 Column，标题为 Leaf，内容包装容器为 Column。header_layout 默认有 12 逻辑像素内边距且不收缩；layout、header_layout、panel_layout 可配置尺寸和间距，布局类型由组件固定。header_style 与 text_style 分别配置 Button 样式和文字，默认左对齐。key 用于在父容器中保持整个面板身份；内部标题和内容使用独立稳定 key。

标题占一个 Tab 焦点位置，展开后内容按普通树顺序参与焦点遍历。标题操作引起的展开/收起保留标题焦点；外部收起时，如果焦点位于被卸载内容，核心清理焦点，不自动迁移。外部展开状态变化会取消尚未完成的标题按压，防止释放旧手势触发过期操作。

disabled 只禁止标题交互，不改变展开状态，也不禁用已经展开的内容。


<a id="disclosure--验证与后续"></a>

### 验证与后续

三项真实 WidgetHost/Taffy/Parley 集成测试覆盖受控请求、Space/Enter、外部变化取消按压、惰性构建次数、标题身份保留、内容卸载与重建、焦点清理，以及禁用标题下内容仍可聚焦。绘制复用 Button，本阶段未增加渲染原语或重跑 GPU 像素测试。

当前提供单面板基础。[Accordion](navigation.md#accordion) 已提供稳定项身份与单项/多项展开策略，并支持组内方向键导航；高度过渡动画、保留收起内容树、无障碍展开语义及原生设备验收也未完成。

<a id="accordion"></a>


<a id="accordion--accordion-分组折叠容器"></a>

## Accordion 分组折叠容器

AccordionView 在 widgets 层组合 DisclosureView；核心不感知组、项身份或展开策略。每项以稳定 AccordionId 作为子树 key，重排不改变同一项的节点身份。标题与内容的布局、交互及收起卸载规则继承 [Disclosure](navigation.md#disclosure)。

```rust,ignore
let view = AccordionView::themed(
    &theme,
    vec![
        AccordionItem::new(AccordionId(1), "设备参数"),
        AccordionItem::new(AccordionId(2), "高级设置"),
    ],
    families,
    AccordionExpansion::Single(model.expanded),
    ActionId(20),
).build(|id| model.settings_page(id))?;
```


<a id="accordion--展开策略和动作"></a>

### 展开策略和动作

AccordionExpansion::Single(Option<AccordionId>) 最多展开一项；打开其他项时，请求状态只保留新项。激活当前展开项请求全部收起。

AccordionExpansion::Multiple(BTreeSet<AccordionId>) 允许多项同时展开；激活某项只切换该项，保留其他项。两种模式均允许零项展开；切换模式由应用显式替换 expansion。

标题发出 AccordionChanged { item, expansion }，包含操作项及完整的请求状态，ActionId 使用组的 action。应用读取动作、确认新的 expansion 后重建；也可以拒绝请求。控件不先行修改状态。该状态是基于生成标题时的模型快照，异步业务应自行校验或合并请求。

所有重复 ID 与未知展开 ID 在调用任何内容工厂前返回 AccordionError。合法时按 items 显示顺序，仅为展开项调用一次 FnMut 内容工厂。禁用项可以保持展开；组 disabled 或项 disabled 仅禁止标题操作，不禁用其内容。


<a id="accordion--布局与生命周期"></a>

### 布局与生命周期

layout 配置整个组，item_layout 配置每个面板，header_layout 与 panel_layout 配置标题和内容包装；Column/Leaf 类型由组件固定。header_style/text_style 统一设置标题外观；key 保持组在父节点中的身份。

收起或移除项会卸载相应内容，焦点在被删除内容中时由核心清理。重排保留稳定项、标题和仍展开的内容身份。业务数据放在应用模型，编辑状态可使用 TextInputDraft。

每个可用标题是一个 Tab 停靠点，展开内容按树顺序参与遍历；Space/Enter 激活标题。组状态变化使标题待发送请求改变时，会取消其未完成的按压，避免发送过期状态。

标题获得焦点时，ArrowDown/ArrowUp 在可用标题间按显示顺序循环，Home/End 定位首尾可用标题；只移动焦点，不修改展开状态，不调用内容工厂。忽略按键释放和 Ctrl/Alt/Super 组合，允许重复按键。内容区按键不参与标题导航；Tab/Shift+Tab 仍按普通树顺序进入内容。导航会取消尚未完成的标题激活手势。

每次描述的标题共享可用顺序和节点登记表，随挂载/更新/卸载维护。重排保留节点身份并更新导航顺序，删除项移除登记；没有目标时不发焦点请求。核心通过 FocusRequest::Target 校验实际节点资格，复用焦点通知与自动滚入。标题列表查找为 O(N)，登记表为 O(N) 存储，不向核心增加 Accordion 特例。

本实现适合设置面板等有限项数的分组。所有标题共享一份不可变展开状态快照，仅在激活标题时生成完整请求集合；状态存储为 O(N + M)，其中 N 为标题数、M 为展开项数。多项模式每次激活复制集合的成本为 O(M)；普通重建比较快照内容仍可能需要 O(N × M) 时间。所有标题均构建，不用于海量虚拟树。


<a id="accordion--验证与后续"></a>

### 验证与后续

三项集成测试覆盖单项/多项模型回写、收起、替换展开、重排后内容/焦点身份保留、非法 ID 提前拒绝及禁用展开项的内容工厂顺序；同时运行三项 Disclosure 回归。另有两项单元测试验证多个请求共享快照、激活结果不修改原快照，以及相同状态的新快照不会误判为变化；集成测试验证按住 Space 重排后仍能正常释放激活。

导航测试另外覆盖禁用跳过、首尾循环、Home/End、修饰键/释放排除、内容区隔离、重排/删除登记更新与 Space 手势取消。

当前未实现至少保留一项展开策略、展开动画、收起子树保留与无障碍组/展开语义。本阶段复用已有绘制，没有进行新的 GPU 或原生设备验收。


<a id="accordion--渲染预览"></a>

### 渲染预览

```sh
cargo run -p zui-render-check --locked --offline -- cpu artifacts/accordion-cpu.png accordion
```

640×360 预览并排展示单项与多项展开、禁用标题，以及内容区文字和进度条。该入口只生成静态图片，不处理交互。最终 CPU 图片已目视检查，示例间距调整后两列完整显示。

CPU 像素测试在 1×/2× 下依次切换多项展开、单项展开和全部收起，检查各内容面板的实际像素面积、位置及收起后像素消失，包含禁用项保持展开的情况。没有新增绘制原语，本阶段未重跑 GPU。
