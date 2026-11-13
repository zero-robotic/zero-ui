# 布局、滚动与虚拟化

[文档索引](README.md) · [模块边界](module-boundaries.md) · [未完成事项](implementation-plan.md)

按架构职责归并的专题参考。设计目标与接口约束分开理解；待办统一维护在实施计划中，各节的验证说明仅覆盖所述范围。

## 章节导航

- [ZUI 控件布局方案](#layout-design)
- [标准布局容器](#containers)
- [Stack 叠层布局](#stack)
- [ScrollView 单轴与双轴滚动](#scroll)
- [虚拟化范围索引](#virtualization)
- [VirtualList：固定、显式与自动行高](#virtual-list)

<a id="layout-design"></a>


<a id="layout-design--zui-控件布局方案"></a>

## ZUI 控件布局方案

状态：基础布局契约及 Taffy Flex/Grid 适配已实现；声明式控件容器和其余能力按本文继续建设。当前实现范围见第 10 节。

采用 **声明式布局容器 + 统一尺寸规则 + Taffy 布局引擎 + 自定义布局扩展**。布局在 CPU 上统一计算，生成控件位置、尺寸和相关几何信息，再交给 Vello CPU 或 GPU 绘制，两端共享布局结果。


<a id="layout-design--1-布局容器"></a>

### 1. 布局容器

| 容器 | 用途 | 实现方式 |
| --- | --- | --- |
| `Row` | 水平工具栏、按钮组 | Flex 横向布局 |
| `Column` | 页面、表单、纵向内容 | Flex 纵向布局 |
| `Flex` | 换行、弹性分配和复杂对齐 | 提供更多 Flex 配置 |
| `Grid` | 表单、仪表盘、多列内容 | Grid 布局 |
| `Stack` | 图片上叠文字、角标、遮罩 | ZUI 定义叠放、对齐和定位规则 |
| `Container` | 内边距、尺寸约束、单子节点对齐 | 基础盒模型 |
| `ScrollView` | 滚动视口 | 布局结合滚动、裁剪与输入处理 |
| `VirtualList` | 大量列表项 | 按可见区域创建和布局 |
| 自定义布局 | 仪表盘、特殊排列 | 测量和放置子节点的扩展接口 |

Taffy 提供 Flexbox、Grid 等算法，并通过测量接口接入文字和图片。滚动行为、虚拟列表、叠放语义与控件生命周期由 ZUI 负责，不将 Taffy 视为完整控件系统。[Taffy 官方文档](https://docs.rs/taffy/latest/taffy/)


<a id="layout-design--2-尺寸与间距规则"></a>

### 2. 尺寸与间距规则

| 属性 | 语义 |
| --- | --- |
| `Auto` | 由内容和父容器规则决定 |
| `Length(120.0)` | 固定逻辑尺寸，不直接等同于物理像素 |
| `Percent(0.5)` | 相对包含块对应尺寸的比例，遵循明确的百分比解析规则 |
| `grow(1.0)` / `shrink(...)` | 在 Flex 主轴上分配剩余空间或收缩空间 |
| `min_width` / `max_width` 等 | 尺寸上下限 |
| `padding` | 容器内部间距 |
| `gap` | 子项之间的间距 |
| `margin` | 节点外部间距 |
| Grid 轨道 | 独立提供固定长度、`auto`、`fr` 等概念 |

填满父容器和分配剩余空间应分别表达，不用含义模糊的 `fill()` 隐式承担两种行为。`grow` 本身也不保证子项等宽，最终尺寸还取决于基础尺寸、内容和约束。

百分比在包含块对应尺寸不确定时不能简单乘以屏幕尺寸，应遵循所采用布局算法的解析规则，并在公共 API 文档与测试中明确。根布局由平台提供确定的视口约束。

支持主轴对齐、交叉轴对齐、文字基线对齐和按需换行。对齐与尺寸属性均使用 ZUI 自有类型，不直接暴露 `taffy::Style`。


<a id="layout-design--3-声明式-api-示例"></a>

### 3. 声明式 API 示例

以下代码仅用于表达建议的 API 形式，尚不是可编译的已实现接口。外层布局由确定尺寸的窗口视口约束。

```rust
row((
    column((
        text("笔记"),
        search_input(),
        virtual_list(items),
    ))
    .width(Length(240.0))
    .height(Percent(1.0)),

    column((
        toolbar(),
        editor().grow(1.0).min_height(0.0),
        status_bar(),
    ))
    .grow(1.0)
    .min_width(0.0)
    .height(Percent(1.0)),
))
.gap(12.0)
.padding(16.0)
.width(Percent(1.0))
.height(Percent(1.0))
```

`min_width(0)` 等规则允许内容收缩，避免长文本的固有尺寸将窗口撑开。滚动和编辑控件还需定义自己的视口及溢出策略。


<a id="layout-design--4-布局树与控件树"></a>

### 4. 布局树与控件树

正式实现优先通过 Taffy 低层接口接入 ZUI 自己的节点树，以复用稳定的 `NodeId`、节点存储和生命周期，避免长期维护两套独立节点身份。ZUI 负责布局缓存、失效传播和算法分发。

布局模块只通过 `zui-api` 中的树访问和测量契约工作，核心提供适配实现。布局不得导入核心节点存储类型、具体控件或文字引擎；需要测量文字时调用注入的测量接口。依赖方向遵守 [模块分层与依赖约束](module-boundaries.md)。

早期原型可使用 `TaffyTree` 快速验证算法与测量接口；若采用该路径，需要管理控件节点到布局节点的映射，并在删除、重排时保持一致。不得将原型内部 ID 暴露给应用。

Taffy 官方将高层树 API 用于快速集成，低层接口用于已有树结构的框架集成。[接口说明](https://docs.rs/taffy/latest/taffy/)

自定义布局扩展通过上下文读取约束、测量子节点和提交子节点位置与自身尺寸；不直接访问 GPU 或绘图上下文，也不在测量期间修改业务状态。布局需允许同一节点在不同约束下被重复测量。


<a id="layout-design--5-文字和图片测量"></a>

### 5. 文字和图片测量

布局引擎传入已知尺寸和可用空间，文字系统根据这些约束返回尺寸、基线等信息。可用宽度变化可能改变换行，不能先独立计算所有文字尺寸，再假定一次布局就能完成。

测量缓存应考虑内容版本、字体与排版属性、约束及字体资源变化。最终绘制使用与最终布局一致的排版结果，避免测量、光标定位和显示使用不同换行结果。

图片通过固有尺寸、宽高比和显式约束参与测量。资源加载完成后，只有影响尺寸或比例的变化才需要重新布局；仅图像像素变化通常只触发重绘。


<a id="layout-design--6-叠放滚动与虚拟化"></a>

### 6. 叠放、滚动与虚拟化

`Stack` 明确子项的叠放顺序、对齐、定位以及是否参与容器尺寸计算。覆盖层的命中测试顺序与绘制层级保持一致。弹出层可能需要脱离普通容器裁剪，具体由覆盖层机制处理。

`ScrollView` 区分视口尺寸和内容尺寸，在滚动轴上按内容测量规则布局，在交叉轴上保留明确约束。内容溢出用于计算滚动范围；滚动偏移参与绘制和命中测试，并被视口裁剪限制。

普通滚动尽量只改变偏移，不触发完整布局。视口或内容尺寸变化时重新检查滚动范围；虚拟列表在可见范围改变时允许局部创建、测量和回收节点。

`VirtualList` 首期支持固定行高，仅布局可见项和少量预加载项，通过稳定 Key 维护数据身份。需要保留的编辑或业务状态不能仅存于随时会回收的行节点中。后续再支持可变行高、测量估算和滚动锚点补偿。


<a id="layout-design--7-坐标缩放与失效"></a>

### 7. 坐标、缩放与失效

布局使用逻辑坐标，由统一步骤处理缩放和必要的像素对齐，绘制与命中测试共用坐标变换。CPU/GPU 后端不能分别修改布局尺寸。

文字、尺寸、约束或子节点结构变化时标记布局失效，并沿尺寸依赖关系传播。颜色等不改变几何的属性仅触发重绘。布局结果变化后，应更新绘制脏区域和相关无障碍几何信息。

保留未取整的布局数据，像素对齐时协调共享边界，避免相邻节点独立取整造成间隙。缩放变化时复核几何与文字缓存，确保显示、输入和无障碍位置一致。


<a id="layout-design--8-响应式布局与-mcu-扩展"></a>

### 8. 响应式布局与 MCU 扩展

响应式布局根据容器可用宽度选择配置，使用少量明确断点。例如窄屏将侧栏变为抽屉，宽屏恢复左右排列。布局决策依赖父级约束，避免根据自身布局结果反复切换导致振荡。

固定分辨率的嵌入式设备可以直接选用固定布局配置，不要求引入完整 CSS 规则、选择器或样式级联系统。

科技感大屏额外提供设计画布配置：以明确逻辑尺寸组合 Grid、Flex 和 Stack，整体呈现选择等比适配、留边或裁剪，输入通过逆变换映射。默认避免非等比拉伸文字和图形。自适应布局仍作为另一种可选配置，详见 [控件、自定义扩展与大屏设计](architecture.md#widgets-and-dashboard-design)。

未来 MCU 保留 `Row`、`Column`、基础尺寸和测量接口的可裁剪性，允许按需关闭 Grid 等能力。当前不实现第二套布局算法；公共布局 API 与 Taffy 隔离，未来需要时可接入受限实现。依赖裁剪和 `no_std` 编译成功仍不等于满足某个 MCU 的内存与性能预算。


<a id="layout-design--9-实施顺序与验证"></a>

### 9. 实施顺序与验证

1. `Container`、`Row`、`Column` 与基本尺寸、对齐、文字测量。
2. `Stack`、`ScrollView`，验证裁剪、滚动和命中测试。
3. `Grid`，覆盖表单、多列和轨道约束。
4. 固定行高 `VirtualList`，验证稳定 Key 与节点回收。
5. 自定义布局扩展，后续增加可变行高虚拟列表。

核心布局测试在无窗口环境中注入约束，检查几何结果，覆盖长文本、收缩、百分比、嵌套容器、字体变化、缩放及列表重排。另验证滚动后命中位置、裁剪范围和布局失效行为。

使用同一布局结果在 Vello CPU/GPU 上绘制，检查文字位置、换行和控件边界一致性；真实嵌入式设备上测量布局耗时与缓存占用。


<a id="layout-design--10-当前实现与边界"></a>

### 10. 当前实现与边界

- `zui-types::NodeId` 是调用方分配的稳定身份，布局适配保持其完整 64 位值，不另外分配 TaffyTree 身份。
- `zui-api::layout` 提供自有尺寸、间距、对齐、轨道、测量输入及 `LayoutTree`/`LayoutEngine` 契约，采用 `no_std + alloc`。
- `zui-layout::TaffyLayout` 锁定 Taffy 0.15.0，通过其低层算法接口接入调用方节点树；第三方类型只存在于适配模块。参见 [Taffy 0.15.0 文档](https://docs.rs/taffy/0.15.0/taffy/)。
- 当前支持叶节点测量、Row/Column、Flex 换行和伸缩、固定/百分比/自动尺寸、最小最大尺寸、逻辑间距及 Grid 自动放置与固定/百分比/自动/fr 轨道。尺寸为 border-box；根节点的可用空间来自视口，填满视口需显式设置百分比宽高。
- 几何结果保留未取整的逻辑坐标，相对父节点输出，绘制时累积父节点变换。当前算法使用 f32 运算，公开几何结果转为 ZUI f64 值；超出范围或非有限输入明确返回错误，不承诺 f64 计算精度。
- 计算前验证可达节点的唯一性、样式及节点数/深度/轨道预算；计算后校验全部边框几何，再统一写回。测量失败不发布部分布局。测量期间调用方不得改变拓扑或样式。
- Taffy 扣除内边距后可能得到负的剩余空间；适配器在公共测量边界将有限负数归零，保证零宽或内边距超出可用尺寸时，各控件使用一致的内容空间语义。非有限值不按零吞掉，仍由错误检查处理。
- 目前缓存仅在一次计算内复用；跨帧缓存将在核心失效协议明确后接入，避免内容或字体变化造成旧结果复用。

布局单元测试使用可控测量替身检查约束变化；顶层集成测试另通过 Parley 和只读 Text 控件验证真实文字换行、自动高度及字体版本失效。持久节点生命周期及通用 View/Key 协调已由 `zui-core::Tree` 接入，布局通过注入的契约计算；粗粒度失效允许纯绘制变化跳过布局。最终几何发布前调用控件准备接口，避免使用布局算法最后一次试测结果绘制文字，详见 [文字服务与字形绘制](text.md#text-design)。标准声明式 Container/Row/Column/Grid/Stack 入口已接入，详见 [标准布局容器](layout.md#containers)。尚待实现：布局基线对齐、触摸惯性滚动、虚拟列表编辑行保活、自定义布局协议、像素边界对齐及跨帧子树缓存。不得用当前叶节点测量接口替代完整自定义布局扩展。

可运行的布局绘制预览：

```sh
cargo run -p zui-render-check --locked -- cpu artifacts/layout-cpu.png layout
cargo run -p zui-render-check --no-default-features --features gpu --locked -- gpu artifacts/layout-gpu.png layout
```

该预览通过公开标准容器构造嵌套 Flex/Grid，验证到 Scene 再到渲染器的实际路径，不代表完整交互控件或大屏组件已经实现。

Stack 已实现共享内容区域、独立水平/垂直对齐、自然尺寸及顺序叠放，复用核心绘制与命中逻辑；详细行为和未实现的弹层能力见 [Stack 叠层布局](layout.md#stack)。

纵向 ScrollView 已接入公共布局溢出范围与子视口契约，支持自动内容高度、滚轮/键盘与边界冒泡。覆盖式滚动条和横向/双向随后已接入，惯性仍未完成，详见 [ScrollView](layout.md#scroll)；固定行高虚拟化由独立 VirtualList 提供。

虚拟化的尺寸索引基础已由zui-widgets 内部的 zui_widgets::virtualization 模块 提供，包含固定/可变尺寸范围查询、索引定位与更新后的锚点恢复，详见 [虚拟化范围索引](layout.md#virtualization)。它不创建或布局节点；控件层已组合该索引与通用子项协调实现 [VirtualList：固定、显式与自动行高](layout.md#virtual-list)，只布局可见/预加载片段，并转换到局部坐标。大范围索引使用 f64 不代表布局算法能够准确处理同等绝对范围；显式可变行高与自动行高测量现已接入，虚拟焦点导航仍未接入。


尺寸契约新增 Dimension::MinContent / MaxContent，可用于 width、height、basis。MinContent 请求尽可能采用软换行机会的自然尺寸，MaxContent 请求不采用软换行的自然尺寸；其他轴仍遵循所属 Flex/Grid 算法，不保证收缩到绘制边缘。当前 min_width/max_width/min_height/max_height 不接受这两个值，适配器返回 InvalidStyle，不能静默当作 Auto。滚动内容使用单列弹性网格配合 MaxContent，先确定自然宽度再计算内容高度，避免 Flex 初步基准测量保留额外纵向空间。横向/双向滚动已接入，详见 ScrollView 文档。

<a id="containers"></a>


<a id="containers--标准布局容器"></a>

## 标准布局容器

`zui-widgets` 提供 `Container` 和 `row`、`column`、`grid`、`stack` 声明式构造入口。构造器输出普通 `View`，持久节点、子节点管理、Key 协调和布局仍由既有公共契约完成；控件库不依赖核心或 Taffy。

| 入口 | 子节点 | 布局行为 |
| --- | --- | --- |
| `Container::new(child)` | 一个 | 单列盒子；支持内边距、尺寸与对齐 |
| `Container::empty()` | 无 | 可用于背景面板或固定尺寸占位 |
| `row(children)` | 多个 | 横向 Flex，可通过样式开启换行 |
| `column(children)` | 多个 | 纵向 Flex |
| `grid(columns, children)` | 多个 | 指定列轨道，按既有 Grid 自动放置规则排列 |
| `stack(children)` | 多个 | 共享内容区域，按子节点顺序叠放；见 [Stack](layout.md#stack) |

所有构造器共享 `style`、`background`、`keyed`、`clipped` 和 `build` 方法，也可以用 `Into<View>` 转换。`style` 接收修改闭包，保留未修改的默认值；构造器决定布局 kind，闭包对 kind 的修改会被忽略。`build` 后得到普通 View，调用方应保持其布局类型与容器意图一致。

```rust
use zui_api::layout::{Dimension, Edges, Track};
use zui_types::{Color, Size};
use zui_widgets::{Container, grid, row};

let sidebar = Container::empty()
    .keyed("sidebar")
    .background(Color::rgb(25, 50, 80))
    .style(|s| {
        s.width = Dimension::Length(144.0);
        s.shrink = 0.0;
    })
    .build();
let cards = grid([Track::Fraction(1.0); 2], [])
    .style(|s| {
        s.grow = 1.0;
        s.min_width = Dimension::Length(0.0);
        s.gap = Size::new(12.0, 12.0);
    })
    .build();
let page = row([sidebar, cards])
    .style(|s| {
        s.width = Dimension::Percent(1.0);
        s.height = Dimension::Percent(1.0);
        s.padding = Edges::all(16.0);
        s.gap = Size::new(16.0, 0.0);
    })
    .build();
```

尺寸使用 border-box；以下主轴/交叉轴说明适用于 Flex 容器。`align` 控制交叉轴，`justify` 控制主轴。对于单子节点 Container，交叉轴为水平方向、主轴为垂直方向。默认 align 为 Stretch；要让有固定宽度的子节点居中，应显式设为 Center。

背景覆盖整个边框盒。默认不裁剪，`clipped(true)` 同时约束自身/后代绘制和命中。容器表面本身不参与命中，空白区域不会吞掉输入；后代仍按核心的逆绘制顺序命中。当前容器不是可点击面板，也未实现滚动、圆角或边框样式。

背景变化仅产生绘制失效。尺寸、间距、方向或子节点顺序改变需要重新布局；Row 与 Column 共享表面控件实现，切换方向可保留同一父节点下具有稳定 Key 的子节点身份。标准容器不会为子节点自动生成 Key；动态列表应由应用提供稳定 Key。

现有布局预览已使用这些公开入口：

```sh
cargo run -p zui-render-check --locked -- cpu artifacts/layout-cpu.png layout
```

验证覆盖单子节点对齐、内边距、背景、溢出裁剪与命中、1×/2× 像素、Key 重排和仅背景变化跳过布局；嵌套 Flex/Grid 预览另验证轨道与间隙。Stack 已验证独立对齐、叠放和重排，详见 [叠层布局](layout.md#stack)。[ScrollView 纵向基础](layout.md#scroll) 已接入。虚拟化与滚动接口见本专题对应章节，剩余交互能力见[实施计划](implementation-plan.md)。

<a id="stack"></a>


<a id="stack--stack-叠层布局"></a>

## Stack 叠层布局

`zui_widgets::stack(children)` 将直接子节点放入同一个内容区域，适用于背景上的文字、角标、装饰和组合面板。它输出普通 View；核心沿用既有绘制和命中遍历，不增加 Stack 或具体大屏控件分支。


<a id="stack--尺寸与对齐"></a>

### 尺寸与对齐

容器 padding 从边框盒中扣除，所有直接子节点共享剩余区域。自动尺寸按重叠层的最大尺寸贡献计算，而不是累加；子节点的 margin 和容器 padding 参与尺寸计算。百分比依据包含块解析，不能把不确定的百分比直接当作屏幕比例。

| 样式 | Stack 中的含义 |
| --- | --- |
| 父级 `align` | 子层默认垂直对齐 |
| 父级 `justify_items` | 子层默认水平对齐 |
| 子级 `align_self` | 覆盖垂直对齐；None 继承父级 |
| 子级 `justify_self` | 覆盖水平对齐；None 继承父级 |
| 子级 width/height/min/max | 子层自身尺寸约束 |

两个方向默认 Stretch，自动尺寸的子层填充可用区域；显式尺寸仍受自身约束。Start/Center/End 分别用于起始、居中和末端对齐。子层内部继续执行其自身的 Row、Column、Grid 或 Stack 布局。上述 self 对齐也适用于 Grid；Flex 使用 align_self 覆盖交叉轴对齐，justify_self 不控制 Flex 主轴。

```rust
use zui_api::layout::{Align, Dimension, Edges};
use zui_widgets::{Container, stack};
use zui_types::Color;

let background = Container::empty()
    .background(Color::rgb(12, 32, 50))
    .build();
let badge = Container::empty()
    .keyed("badge")
    .background(Color::rgb(20, 100, 100))
    .style(|s| {
        s.width = Dimension::Length(48.0);
        s.height = Dimension::Length(24.0);
        s.margin = Edges::all(12.0);
        s.align_self = Some(Align::Start);
        s.justify_self = Some(Align::End);
    })
    .build();
let panel = stack([background, badge])
    .style(|s| {
        s.width = Dimension::Length(320.0);
        s.height = Dimension::Length(180.0);
    })
    .build();
```

Stack 不接受非零 gap、wrap 或显式 rows/columns；这些配置返回 InvalidStyle，避免悄悄忽略调用意图。布局适配器内部将直接子层放在同一网格单元，并使用零最小值的弹性轨道：固定容器内超大的子层可以溢出，但不会扩大共享轨道并推移其他层的对齐位置。该策略不向公共 API 暴露 Taffy 类型；每轴仍占用一个轨道预算。


<a id="stack--绘制与输入"></a>

### 绘制与输入

子节点列表从后景到前景排列，后面的子层后绘制，命中按相反顺序查找。动态重排应提供稳定 Key，以保留控件局部状态。绘制顺序不等于键盘焦点隔离，叠加内容也不会自动捕获或阻止下层输入。

容器表面本身不命中，后代仍正常参与命中；透明颜色不自动让一个可命中的控件穿透。默认允许溢出，`clipped(true)` 复用边框盒裁剪，同时限制绘制和命中。需要阻止下层交互的遮罩必须提供明确的命中与事件行为。

当前实现不包含 z-index、绝对定位、忽略指针策略、脱离祖先裁剪的 Portal、模态焦点圈定或弹层关闭策略。这些是后续覆盖层和交互能力，不能仅以 Stack 代替。


<a id="stack--验证与示例"></a>

### 验证与示例

布局测试覆盖共享区域、padding/margin、独立对齐、自然尺寸、百分比、嵌套 Row、不合法配置和轨道预算。顶层集成测试验证稳定 Key 重排、事件目标、溢出命中以及 CPU/GPU 的 1×/2× 像素顺序。

```sh
cargo run -p zui-render-check --locked -- cpu artifacts/stack-cpu.png stack
cargo test -p zui-render-check --test stack --all-features --locked
# 需要真实 GPU。
cargo test -p zui-render-check --test stack --all-features --locked -- --ignored
```

<a id="scroll"></a>


<a id="scroll--scrollview-单轴与双轴滚动"></a>

## ScrollView 单轴与双轴滚动

`ScrollView::vertical(content)`、`horizontal(content)` 和 `both(content)` 接收一个普通 View，使用实际布局结果计算启用轴的滚动范围。内容可以是 Column、Grid、文字或自定义组合控件，不需要预先给出内容高度。三个入口共享 Scroll 持久状态、滚动条和区域滚入实现；触摸惯性和虚拟化仍在后续范围内。


<a id="scroll--构造与布局"></a>

### 构造与布局

```rust
use zui_api::{action::ActionId, layout::{Dimension, Edges}};
use zui_widgets::{ScrollView, column};

let content = column(items).build(); // items 是应用创建的子 View 集合。
let list = ScrollView::vertical(content)
    .keyed("devices")
    .on_scroll(ActionId(10))
    .line_step(40.0)
    .style(|s| {
        s.width = Dimension::Percent(1.0);
        s.height = Dimension::Length(240.0);
        s.padding = Edges::all(12.0);
    })
    .build();
```

视口在需要滚动的轴上必须由自身或父级约束得到有限尺寸。自动尺寸且没有外部限制时，容器可以随内容增大，因此不会凭空出现滚动范围。上例固定的是视口高度，内容高度仍由布局计算。

构造器内部使用不收缩的单列弹性网格包装内容，在实际列宽确定后测量高度，不覆写调用方内容 View 的样式。纵向模式的宽度等于视口内容宽度、高度自动，文字按视口宽度换行；横向/双向模式的内容宽度使用 MaxContent，自然宽度可以超过视口，横向模式的高度约束到视口，双向模式高度自动。应用显式设置的内容宽高与换行规则仍然有效。

`style` 可设置尺寸、min/max、padding、margin 及参与父级 Flex 的属性。外层构造器拥有 Column、Start 对齐、不换行、零 gap、无轨道以及溢出策略，build 时统一设置，避免与滚动语义冲突。每层动态内容仍应提供稳定 Key。


<a id="scroll--范围与状态"></a>

### 范围与状态

公共 `LayoutBox::scroll_extent` 表达从本地边框盒原点出发可到达的内容范围，包含 padding，至少等于边框盒尺寸。布局适配器从 Taffy 的溢出结果转换为 ZUI 的 Size；核心验证数值并通过 PrepareContext.layout 传给控件。其他布局实现可以用 `LayoutBox::new(bounds)` 表示没有已知溢出的盒子，也可以填写真实范围。负向起始侧溢出和呈现阶段平移不计入可滚动范围。

`Overflow::Visible/Clip/Scroll` 是布局策略，控制后代溢出贡献及自动最小尺寸，不自动实施绘制裁剪。ScrollView 在启用的轴上使用 Scroll 策略，让视口可以小于内容；通过 ChildViewport 单独负责内容平移和裁剪。子滚动容器的内部溢出不会被重复算入父滚动容器。

每个启用轴的最大偏移为该轴的 `scroll_extent - bounds.size`，未启用轴为零。持久控件 Scroll 用 Point 保存二维偏移，内容缩短或视口变大后自动夹到有效范围；普通 View 重建保留偏移。滚动本身只请求 Paint，不重新执行布局。裁剪固定在 content_bounds，padding 区域不随内容滚动。

`scroll_to(Point::new(x, y), revision)` 是一次显式跳转请求，两轴数值必须有限且非负，超出范围时夹到末尾，未启用轴归零。offset 或 revision 变化会重新应用请求；重建时保持二者不变，不会覆盖用户随后进行的滚动。需要重复跳到同一位置时增加 revision。移除请求只恢复普通状态保留语义，不自动归零。

`on_scroll` 可接收用户交互产生的 `ScrollChanged { offset: Point, max_offset: Point }`，由应用解释。程序跳转和布局后的范围收缩不发送交互动作。核心只搬运不透明动作载荷。


<a id="scroll--输入与嵌套"></a>

### 输入与嵌套

- 逻辑像素滚轮直接使用平台转换后的数值；行滚轮乘以 line_step，默认每行 40 逻辑像素。两轴均为正增量减少偏移、负增量增加偏移；未启用轴不消费。不将 y 滚轮隐式改为横向滚动，以便嵌套纵向容器继续处理；Shift+滚轮映射尚未提供。
- 滚轮在目标或冒泡阶段处理，捕获阶段不抢占子容器。有实际位移才停止传播；已在该方向边界时继续冒泡。
- 单次滚轮超出边界时，本次在子容器夹到边界并消费，剩余距离暂不在同次事件中分配给父级；下一次同方向事件可交给父级。
- 容器可以通过 Tab 或点击自身空白/padding 区域获得焦点。自身作为键盘目标时，方向键按对应轴处理；PageUp/PageDown 在纯横向模式处理 x，其他模式处理 y；Home/End 将全部启用轴移到起点/末尾。有修饰键时不处理。子输入框等控件的键盘目标不被父滚动容器接管。
- `disabled(true)` 停止容器的用户滚动及自身焦点能力，但不递归禁用内容控件，也不阻止显式程序跳转。零内容视口不消费滚轮。

内部偏移、范围和输入规则归 ScrollView 所有；核心只负责通用布局结果、事件路由和子视口坐标。渲染器收到普通绘制命令，不认识滚动控件。


<a id="scroll--验证与当前边界"></a>

### 验证与当前边界

已验证内容范围、padding、滚轮行/像素单位、Key 状态保留、版本跳转、内容缩短、视口变大、嵌套边界冒泡、键盘、禁用、零视口和非法配置恢复。真实 Parley 字体测试验证文字换行改变高度与范围；CPU/GPU 检查顶部、中间、末尾在 1×/2× 下的裁剪像素。核心另验证不合法的滚动范围不能发布布局。

```sh
cargo run -p zui-render-check --locked -- cpu artifacts/scroll-cpu.png scroll
cargo test -p zui-render-check --test scroll --all-features --locked
# 需要真实 GPU。
cargo test -p zui-render-check --test scroll --all-features --locked -- --ignored
```

目前所有内容节点仍会创建并布局，裁剪不等于虚拟化。输入光标/选区自动滚入、滚动惯性、触摸手势、同次滚轮余量分配、无障碍滚动语义及原生设备输入验收尚未完成。现有数据大屏或大型表格不能据此宣称具备完整滚动性能和交互能力。



<a id="scroll--前景滚动条"></a>

### 前景滚动条

默认在内容区域右侧/底部按启用轴显示覆盖式滚动条，仅当有可滚动范围且视口非空时显示。它不占用布局宽度，因此显示与隐藏不会使文字反复换行；右侧/底部内容会被滚动条覆盖，应用可在内容内部留出空白。`.scrollbar(None)` 隐藏滚动条，但保留滚轮、键盘和程序跳转。

`ScrollbarStyle` 配置宽度、最小滑块长度及轨道、滑块、拖动和禁用颜色。宽度与最小长度必须有限且大于零；实际尺寸不超过视口。滑块长度按可见内容比例计算，并受最小长度约束。轨道太短而没有可移动空间时不启动拖动，仍可使用滚轮和键盘。

按下滑块时获取焦点和指针捕获，按下点在滑块内的比例用于保持拖动锚点；指针移到视口外仍可拖动，松开会应用最终位置并释放捕获。拖动期间视口改变时使用新的滑块几何。点击滑块前后轨道每次移动一页，暂未实现长按连续翻页、自动隐藏或悬停动画。

窗口取消输入或失去捕获会终止拖动；禁用控件由核心通用规则清除捕获。拖动时隐藏滚动条或内容不再可滚动，下一次指针移动/松开会结束该手势并释放捕获；节点移除也沿用核心清理机制。普通样式更新不重置偏移。

绘制顺序为容器背景、子内容、前景滚动条和焦点框。命中顺序相反：滚动条区域先于子控件，其他位置继续命中内容。`Widget::paint_foreground` 和 `hit_test_foreground` 是通用公共能力，核心不引用 ScrollView 或 ScrollbarStyle。前景不受该节点自身的子内容平移/裁剪影响，仍受祖先裁剪与节点裁剪约束；只绘制的装饰应保持前景命中为 false。

新增测试覆盖覆盖区域命中优先级、轨道翻页、越界拖动、松开、缩放期间锚点、取消、禁用、隐藏、短轨道、非法样式恢复，以及 CPU/GPU 在三种位置的 1×/2× 前景像素。原有不显示滚动条的像素测试显式使用 scrollbar(None)，继续验证纯内容滚动。

焦点切换后的控件边框盒滚入和显式区域请求已接入，采用最近边界并从内到外处理嵌套容器；不会在普通重建时反复覆盖用户滚动，详见 [焦点与区域自动滚入](runtime.md#reveal)。



<a id="scroll--二维状态与接口更新"></a>

### 二维状态与接口更新

```rust
let matrix = ScrollView::both(content)
    .scroll_to(zui_types::Point::new(100.0, 50.0), 1)
    .style(|s| {
        s.width = Dimension::Length(600.0);
        s.height = Dimension::Length(300.0);
    })
    .build();
```

当前开发版将 VerticalScroll 更名为 Scroll，offset()/max_offset() 和 ScrollChanged 的对应字段改为 Point，scroll_to 的标量参数改为 Point。旧纵向调用迁移为 Point::new(0.0, y)，读取 offset().y。本次以统一二维契约替代旧的纵向标量接口，调用方需要同步迁移。

三个构造入口使用同一 Widget 类型，稳定 Key 下切换轴向保留仍有效的偏移；新禁用轴的偏移归零，内容重新布局并按新范围夹取。两条滚动条同时存在时分别缩短轨道，右下角保留为普通内容区域，避免两个拖动命中重叠。拖动只修改所抓取轴，二维滚轮、Home/End 与显式跳转可以同时修改两轴。

焦点和区域滚入对所有启用轴使用最近边界，继续复用核心从内到外的通用契约。双向模式并不自动虚拟化或冻结表头；大型表格和数据画布的专项能力仍需后续实现。

```sh
cargo run -p zui-render-check --locked -- cpu artifacts/scroll-both-cpu.png scroll-both
cargo test -p zui-render-check --test scroll_axes --all-features --locked
```

<a id="virtualization"></a>


<a id="virtualization--虚拟化范围索引"></a>

## 虚拟化范围索引

`zui_widgets::virtualization` 提供与轴向无关的 ExtentIndex，用于列表行、表格行或列的尺寸索引。它是 `no_std + alloc` 的独立模块，不依赖控件、NodeId、布局引擎、渲染器或平台。该模块只交付范围与定位算法；控件层已组合它实现 [VirtualList：固定、显式与自动行高](layout.md#virtual-list)，索引本身仍不创建节点。


<a id="virtualization--数据与查询"></a>

### 数据与查询

原 `zui-virtual` crate 已合入 `zui-widgets`，公开入口改为 `zui_widgets::virtualization`，无需额外 feature。`ScrollAlignment` 与 `VirtualIndexLimits` 的既有重导出保持不变；原有测试迁至 `crates/zui-widgets/tests/virtualization_index.rs` 和 `virtualization_sparse.rs`。

- `ExtentIndex::fixed(count, extent, gap, limits)` 表达固定尺寸项目，不分配逐项索引。
- `ExtentIndex::variable(&extents, gap, limits)` 构建可变尺寸的聚合树，建立成本与存储均为 O(N)。`update(index, extent)` 在 O(log N) 内更新路径，不累积重复加减浮点差值。
- `ExtentIndex::estimated(count, estimate, gap, limits)` 建立隐式默认尺寸区间，不分配逐项缓冲；后续 update 只为修改路径分配节点。
- `item(index)` 返回该项的逻辑起止位置。项目尺寸必须有限且大于零，gap 必须有限且非负；折叠或隐藏项目应从参与布局的序列中移除。
- `window(offset, viewport, overscan)` 在 O(log N) 内返回包含两侧预加载项的半开索引范围、夹取后的偏移、总尺寸以及前后占位尺寸。overscan 使用项数；零视口返回空范围，即使指定预加载也不创建项目。

固定尺寸索引的单项定位为 O(1)，可变尺寸为 O(log N)。三种存储使用相同公共行为；固定尺寸实例不允许单项更新，需要改变统一尺寸时重新构造，或显式改为可变尺寸实例。

```rust
use zui_widgets::virtualization::{Alignment, ExtentIndex, Limits};

let index = ExtentIndex::fixed(10_000, 30.0, 2.0, Limits::default())?;
let window = index.window(0.0, 240.0, 2)?;
assert_eq!(window.items, 0..10);
let offset = index.offset_for(5_000, 0.0, 240.0, Alignment::Start)?;
assert_eq!(offset, 160_000.0);
```

首尾不增加额外 gap，总尺寸为项目尺寸之和加 N−1 个 gap。窗口边界按半开区间处理：刚好在视口右侧/下侧开始的项目不算可见；视口完全位于间隙中时可见范围为空，非零 overscan 可以包含相邻项目。leading_extent 和 trailing_extent 包含范围外的间隙，列表装配不能再次在占位区与首尾项目之间重复增加 gap。


<a id="virtualization--稀疏估值存储"></a>

### 稀疏估值存储

estimated 模式的未修改区间用“估值 × 项目数”计算，不创建节点。更新前先沿路径计算候选聚合结果，验证总范围和精度，再按预算申请缺少的节点，最后提交。失败不会留下部分尺寸修改。祖先通过左右区间重新聚合，避免反复累加浮点差值；前缀、范围和单项查询沿区间树遍历，保持 O(log N)。更新路径计算为 O(log N)，偶发缓冲扩容的复制成本另计。

初始化没有逐项扫描或分配。存储随实际分配路径增长，共享祖先；最多完整展开为 2N−1 个节点。相同估值更新不分配，已分配路径会保留用于后续更新，即使该项恢复为默认估值也不立即释放。缓冲按有界倍增增长，保留容量计入 max_index_bytes；重建/丢弃索引释放旧缓冲。目前不自动做离屏淘汰或路径压缩，长时间遍历大量不同尺寸行仍会消耗预算，不能将“稀疏”理解成始终仅保存当前视口。可按下面的显式接口整理已经恢复默认值的路径。

`allocated_index_bytes()` 返回节点缓冲已分配容量字节数，固定模式为零。该值不包含索引对象、应用输入、分配器元数据或布局/绘制资源，不是进程总内存统计。预算不足返回 BudgetExceeded，已有路径上的尺寸更新仍可在不新增分配时成功。


<a id="virtualization--显式压缩"></a>

#### 显式压缩

`compact(max_scratch_bytes)` 移除完全恢复默认估值的稀疏路径，并收紧节点缓冲容量，返回释放的节点缓冲字节数。非默认测量值全部保留，不做离屏淘汰；固定模式与显式稠密模式返回零并保持不变。全部路径都恢复默认时，不申请临时缓冲即可释放整个节点数组。已经紧凑的索引也不重复分配。

部分压缩先统计保留节点，再根据预算申请临时标记数组和新节点缓冲，构建并验证候选索引后才替换旧索引。临时预算包含一字节/旧节点的标记以及新节点缓冲申请，不包含调用栈、分配器元数据和仍然存活的旧缓冲；峰值内存应按“旧缓冲 + 临时预算”另加其他资源规划。预算或分配失败不改变原索引和已分配容量。

压缩复杂度为 O(P)，P 是已分配节点数，不扫描全部 N 行。新索引重新聚合区间，浮点加法结合顺序可能变化，因此不承诺任意小数数据的聚合结果位级一致。调用方应在空闲或明确维护时执行，不能把它放进每次滚动查询中。压缩不会丢弃非默认测量；所有历史测量都仍有用时，能释放的可能只有多余容量，甚至为零。


<a id="virtualization--索引定位与锚点"></a>

### 索引定位与锚点

`offset_for(index, current, viewport, alignment)` 支持 Start、Center、End、Nearest，结果夹到可滚动范围。Nearest 与 ScrollView 的最近边界规则一致，大于视口的项目已覆盖视口时保持当前位置。返回的只是偏移，调用方通过滚动契约应用；索引模块不控制焦点、滚动控件或应用状态。

`anchor(offset, viewport)` 记录当前项目索引和相对其起点的位置。前方项目从估计高度更新为测量高度后，`offset_for_anchor` 可恢复该项目的位置；锚点所在项目缩小时，相对位置会夹到其当前尺寸。间隙内的锚点用负相对位置表示，空列表没有锚点。

锚点使用逻辑索引，不包含业务 Key。插入、删除、过滤或重排后，数据所有者必须先依据稳定 Key 重映射索引；本模块不会把旧索引错误地解释成同一个业务项目。当前结构变化通过重建索引完成，尚未提供增删树或自动 Key 映射。


<a id="virtualization--预算与数值边界"></a>

### 预算与数值边界

Limits 控制项目数、索引节点缓冲申请字节数和总逻辑尺寸。固定尺寸模式不使用节点缓冲预算；稠密与稀疏模式均在分配前检查容量与字节数，并使用可失败分配。默认最多一百万项、64 MiB 节点缓冲和 10¹² 总逻辑尺寸；这不代表所有嵌入式设备都适合该默认值，装配层应按实际资源配置。

非法尺寸、索引越界、预算超出和分配失败分别报告错误。若总范围与最小项目尺寸的比例超过 2⁴⁸，拒绝构造或更新，避免非空项目在浮点精度下消失。更新先验证候选根结果再修改节点，因此被拒绝的更新保留既有查询结果。

索引使用 f64。布局与渲染链路有不同精度约束，百万项测试只证明索引范围和无逐项分配，不证明巨大坐标能直接交给 Taffy/Vello 而保持像素精度。固定行高控件已采用可见片段局部原点和行数预算，其他尺寸模式仍需分别验证。


<a id="virtualization--验证与接入要求"></a>

### 验证与接入要求

六项测试覆盖百万固定项、预加载范围、半开边界、间隙、空列表、零视口、与线性参考的可变尺寸对照、反复更新无累计漂移、索引对齐、锚点恢复、非法输入、精度与预算拒绝，以及失败后的状态保持。

VirtualList 必须实际减少创建、布局和绘制的节点，行工厂只对可见项、预加载项及明确保留项调用。视口尺寸来自实际布局，滚动范围与索引保持一致；焦点、指针捕获、编辑状态和 Key 生命周期需要明确的保留或迁移策略。核心只提供通用的按需子项协调能力，范围选择和数据解释由上层列表负责，不能在核心增加 VirtualList 类型分支。

核心已增加 `Tree::reconcile_children`，可按稳定 Key 更新指定父节点的子项并复用重叠部分，详见 [局部子项协调](runtime.md#view-reconciliation--4-局部子项协调)。随后接入 `View::managed_children` 和 `Widget::child_views`，使容器在实际布局后生成子项、在根重建时保留生成子树，并通过有上限的迭代重新布局，详见 [控件管理的子项](runtime.md#view-reconciliation--5-控件管理的子项与布局收敛)。固定行高 VirtualList 已组合这些机制、ExtentIndex、共享 Scroll 状态与行工厂，使用局部原点呈现当前片段；离屏行目前回收并清理输入所有权，编辑行保活和虚拟导航仍待实现，详见 [VirtualList：固定、显式与自动行高](layout.md#virtual-list)。

显式可变行高已由 VirtualListView::variable 接入，重用同一共享高度数组时缓存索引，提交新数组时重建并可选择按逻辑索引保持可见锚点。自动行高测量随后已通过布局反馈接入，未测量行采用估值，自动模式现已使用 estimated 稀疏索引；显式路径压缩已接入，业务 Key 重排锚点和缓存淘汰仍未实现。

稀疏索引新增四项测试，覆盖百万估值零初始缓冲、1024 字节更新预算及失败不修改、相同估值不分配、恢复估值复用容量、与稠密参考在不同项目数和连续更新下的范围/对齐/锚点对照，以及空列表、间隙、精度和非法值拒绝。

显式压缩新增三项测试，覆盖零临时预算释放全部默认路径并恢复后续更新空间、保留非默认数据与查询/后续更新一致性、临时预算不足时状态不变，以及空/固定/稠密模式无操作。

<a id="virtual-list"></a>


<a id="virtual-list--virtuallist固定显式与自动行高"></a>

## VirtualList：固定、显式与自动行高

`zui-widgets::VirtualListView` 已接入实际视口、ExtentIndex、控件管理子项和共享 Scroll 交互。它只为可见项及两侧预加载项调用行工厂、创建节点并参与布局/绘制，不先创建全部数据行再裁剪。当前提供纵向固定行高、应用提供各项行高和自动测量三种模式。


<a id="virtual-list--构造与身份"></a>

### 构造与身份

`VirtualListView::new(count, row_height, factory)` 接收项目数、固定逻辑行高及 `Fn(usize) -> View` 行工厂。通过 `style` 设置视口尺寸或 Flex/Grid 约束；无约束的 Auto 高度不代表自动展示全部数据，应为列表提供实际有界视口。

行工厂返回的 View 必须有稳定 Key；缺少 Key 报 InvalidStyle，同一生成集合中重复 Key 报 DuplicateKey。重叠行在滚动或根重建后保留节点身份和有效本地状态；已回收行再次出现时创建新实例。数据排序、过滤或插入时必须用业务标识作 Key，只有身份与顺序固定的数据才能用索引。应用修改行数据后应重建 View；工厂可多次调用，不得消耗一次性数据或执行外部副作用。

列表在每一行外增加槽位，原始行 View 保留自身样式。槽位宽度随视口内容区变化；固定/显式模式按指定高度裁剪，自动模式使用自然高度。列表构造器管理 Column、间隙、溢出策略等结构字段；应用可设置宽高、padding、Flex 占比等外部布局属性。

常用配置：

- `gap(value)`：行间距，不在总内容首尾增加间隙。
- `overscan(count)`：两侧额外创建的行数，默认各 2 行；零高度内容区不创建行。
- `index_limits(VirtualIndexLimits)`：设置索引的项目数、缓冲字节数和总逻辑范围预算，与 UI 节点数/行数预算独立；改变配置会重建索引。
- `max_realized(count)`：单次生成行数上限，默认 512，超限在调用行工厂前返回 BudgetExceeded。
- `scrollbar(Some(style))`：默认覆盖式滚动条；None 隐藏滚动条并保留其他滚动方式。
- `background`、`disabled`、`line_step`、`on_scroll`：沿用 Scroll 的配置语义。


<a id="virtual-list--显式可变行高与尺寸缓存"></a>

### 显式可变行高与尺寸缓存

`VirtualListView::variable(row_heights: Rc<[f64]>, factory)` 使用应用提供的逐项高度，项目数取数组长度。所有行高必须为有限正数，归一到 f32 后也必须为正；零、负值、NaN、无穷大及下溢为零的尺寸均拒绝。其他样式、工厂、滚动和回收接口与固定模式一致，每个槽位采用对应行高。

高度数组不可变；修改尺寸时提交新的 Rc 数组。普通模型重建应 clone 原 Rc，核心通过共享存储身份判断是否需要重建尺寸索引。索引只在高度描述或 gap 改变时重建，不因滚动、视口缩放或布局收敛反复构建。当前新数组采用 O(N) 重建，不提供逐项增量尺寸更新接口；范围查询与定位继续使用 O(log N) 索引。

可变索引需要 O(N) 存储。默认节点缓冲预算仍为 64 MiB；原始高度数组、归一化临时数组、旧索引和候选新索引不都包含在这一缓冲预算中，因此重建峰值内存会更高。固定模式继续不分配逐项索引。嵌入式装配需要按实际内存限制数据量，不能将索引预算等同于进程内存上限。

`preserve_visible_anchor(true)` 可在尺寸或间隙变化时保持首个可见逻辑索引及其内部相对位置，默认 false（保留像素偏移后夹取）。锚点所在行缩小时夹取相对位置；尾部仍受总范围限制。新提交的像素或索引跳转优先于锚点恢复，普通重建中相同跳转请求不重复覆盖位置。

锚点按逻辑索引而非业务 Key 保存，开启此选项要求数据身份与顺序保持不变。排序、过滤、插入或删除时，应关闭它并由应用根据业务 Key 提交新定位。首个锚点索引已超出新列表时退回像素偏移夹取，不复用无效索引。失败的尺寸索引构建不替换旧索引，树保持布局失效，修正数据后可重试。

显式模式不会根据文字换行自动更新高度；需要该能力时使用下面的 measured 构造器。


<a id="virtual-list--自动测量估值与缓存失效"></a>

### 自动测量、估值与缓存失效

`VirtualListView::measured(count, estimated_height, data_revision, factory)` 为未测量行使用统一估计高度，为当前可见/预加载行采用实际布局高度。行槽使用 Auto 高度、最小 1 逻辑像素且不收缩，空内容不会形成零尺寸索引项。工厂应让内容在给定宽度下自然测量；依赖父级 Auto 高度的百分比高度或持续改变高度的控件可能无法收敛。

尺寸反馈来自通用 Widget::children_prepared。槽位携带不影响 Key 身份的 LayoutTag，标明内部测量代次和逻辑索引；缓存重建后忽略旧代次反馈。稳定业务 Key 继续决定节点复用，核心仅传递标签，不解释列表数据。测量模块逐项更新聚合索引，并更新逻辑范围、片段原点及可见锚点，再由通用协调生成需要的新行。

自动模式默认开启 preserve_visible_anchor，当前可见逻辑索引及项内位置在尺寸修正时尽量保持；目标索引的新跳转在目标行测量后再按实际尺寸修正 Start/Center/End/Nearest 对齐。关闭锚点选项则保留像素偏移并夹取。尚未测量的行仍是估值，完整像素范围和滚动条比例会随测量调整；不要把未测量区域的像素偏移当作预先精确计算的整表坐标。

有效内容宽度或文字服务 font_epoch 改变时，全部高度缓存失效，回到估值后重新测量当前片段。count、估计高度、gap 或 data_revision 改变也会重建。内容、字体大小、行样式、排序或数据身份改变时，应用必须递增 data_revision，不能仅替换工厂闭包而期望所有离屏缓存自动失效。替换同 epoch 的文字服务也应递增 data_revision。数据插入、删除或重排时，索引锚点不能替代业务身份，应关闭锚点并根据业务 Key 提交定位。

自动测量索引现使用 estimated 稀疏区间树，初始化不分配逐项索引，也不创建估值数组；只有偏离估值的更新路径需要节点。查询和更新路径计算为 O(log N)，缓冲扩容另计复制成本。已分配路径与容量会保留，恢复估值不会立即释放，当前没有离屏淘汰；长时间遍历大量不同高度行仍可能达到索引预算。显式高度数组模式继续采用稠密索引。

`VirtualList::compact_index(max_scratch_bytes)` 提供显式压缩：移除已恢复默认估值的路径并收紧容量，保留有效测量值，返回释放的缓冲字节数。通过 Tree::update_widget 调用时应声明 Layout 失效，让坐标与布局重新准备；不在滚动或每帧中自动执行。临时预算、峰值内存与失败行为见 [显式压缩](layout.md#virtualization--显式压缩)。顶层集成测试验证了压缩后节点身份、滚动位置与命中保持一致。

`VirtualList::allocated_index_bytes()` 返回索引节点缓冲容量，可与 index_limits 配合检查资源用量。它不包含行节点、应用数据、文字和渲染资源，不代表 MCU 整机内存要求已满足。预算耗尽会阻止当前布局，不静默丢弃测量或降低数据准确性；应用可调整预算或数据后重试。

估计高度应接近实际行高。严重高估可能需要多轮补齐视口，严重低估可能先触发 max_realized 限制；仍受核心 16 轮收敛上限约束，不会为了填满屏幕绕开预算或无限循环。测量超出索引约束时返回 InvalidMeasurement，阻止绘制并标记缓存待重建；修正数据后可重试。多项更新不是批量事务，错误后不会继续发布部分修正结果。


<a id="virtual-list--滚动和坐标"></a>

### 滚动和坐标

滚轮、上下方向键、PageUp/PageDown、Home/End、轨道翻页、滑块拖动、指针捕获和边界冒泡复用 Scroll 的状态与算法。`on_scroll` 输出现有 ScrollChanged，纵向偏移位于 offset.y，程序跳转不产生用户动作。

`scroll_to(offset, revision)` 请求逻辑像素偏移，`scroll_to_index(index, ScrollAlignment, revision)` 请求索引定位，支持 Start/Center/End/Nearest。后调用的请求覆盖另一种请求。只有请求内容或 revision 改变时重新应用，普通根重建不反复拉回用户位置。非法索引（包括空列表中的索引跳转）返回 InvalidStyle；项目数或视口变化会将已有像素偏移夹到有效范围。

公开持久状态 `VirtualList::offset()/max_offset()` 为纵向 f64。完整逻辑范围只保存在索引和滚动状态内；Taffy 仅布局当前片段，子内容视口通过“片段起点 − 滚动偏移”呈现。不使用高度为整个列表长度的占位控件，因此跳到较大索引时不会把巨大绝对坐标交给 f32 布局引擎。

行高与间隙先按布局链路的 f32 精度归一，再用于 f64 索引，避免索引与布局使用不同步长。当前尺寸索引采用默认 Limits：最多一百万项、总逻辑范围不超过 10¹²，并检查精度比例。局部行尺寸和布局本身仍受 f32 精度限制；不承诺任意巨大单行或任意小数间距下的像素精确性。

`LayoutBox::scroll_extent` 描述实际已生成片段，不能作为列表的完整逻辑范围使用；完整范围应读取 VirtualList 的状态。直接使用 Tree 时，滚动或属性更新后在 Scene 前调用布局入口，让生成集合收敛；WidgetHost 已统一处理。


<a id="virtual-list--程序化定位与焦点"></a>

### 程序化定位与焦点

直接管理 Tree 的应用可先更新 scroll_to_index 请求并完成布局，再按稳定业务 Key 解析已生成行中的可聚焦后代，调用 `tree.request_focus(Some(node_id))?`。槽位本身不一定可聚焦，应选择 enabled、focusable 的控件。请求复用通用焦点通知与滚入，不改变指针捕获。

定位与焦点请求是独立步骤，不提供原子事务；期间若数据变化，应重新确认业务 Key 和目标。回收后的旧 NodeId 会被拒绝，不能作为长期业务身份。顶层测试验证第 900 行生成后取得焦点、原焦点行回收清理和旧 ID 请求不影响新焦点。WidgetHost 应用可在 take_focus_request 的只读树回调中完成目标解析并返回 ViewFocusRequest，Host 会在索引定位对应的布局完成后应用请求。请求必须一次性消费；已可通过 on_navigate 与应用焦点请求组合完成跨行导航，边界和限制见下一节。


<a id="virtual-list--跨行-tab-导航动作"></a>

### 跨行 Tab 导航动作

`VirtualListView::on_navigate(ActionId)` 显式启用逻辑行导航。默认未启用，保持已有挂载树的 Tab 顺序。启用后，行内仍按普通 Tab/Shift+Tab 访问控件；即将跨出当前行时，列表产生 `VirtualListNavigate { from_index, index, reverse }`，并抑制本次默认跳转。普通按键已处理、Control/Alt/Super 修饰、禁用列表不会产生此动作。

应用需要完成以下链路（完整示例见 `tools/window-check/tests/virtual_navigation.rs`）：

1. map_action 校验 ActionId 和载荷，将目标索引映射为当前数据中的业务身份。
2. message 保存待聚焦意图，递增定位 revision，更新 scroll_to_index，并返回 ViewAction::rebuild。
3. take_focus_request 在布局完成后按业务 Key 查找目标行，正向选择首个、反向选择末个 enabled/focusable 后代，返回 ViewFocusRequest::Focus 并消费意图。

从列表自身的滚动焦点停靠点按 Tab，目标为第 0 行且 from_index 为 None；从外部按 Shift+Tab 进入列表时，由 enter_focus 请求逻辑末行，from_index 同样为 None，reverse 为 true；从行内跨出时 from_index 是当前逻辑索引，目标为相邻行。在第 0 行反向跨出或最后一行正向跨出时，列表不接管，按现有树顺序离开。方向决定进入目标行的哪一端，不反转行内控件顺序。

列表使用成功布局反馈中的槽位 ID 和 LayoutTag 建立当前生成行映射，不根据滚动后的期望窗口猜测旧节点索引。只缓存已生成行；未启用时不保留该缓存。核心仅提供直接子分支信息，不认识逻辑行或列表类型。

这是控件与应用共同完成的导航，不能只注册 on_navigate 而忽略动作，否则跨行 Tab 会停在原处。数据在动作处理期间重排、删除时，应按业务身份重新验证目标；不把索引当作长期身份。没有可聚焦后代的行可通过下面的 navigable_rows 声明跳过，不自动扫描任意数量的虚拟行。从列表外反向进入会先生成逻辑末行，再由应用选择其最后一个可聚焦后代；不会先把焦点提交到旧片段末项。正向进入继续保留列表自身的滚动焦点停靠点。文本编辑状态可通过 TextInputDraft 保存，离屏节点和 IME 会话保活仍未实现。

三项新增集成测试覆盖固定、显式和自动行高；单行视口中每行两个控件，验证正反向跨未生成行、行内顺序、首尾退出、空列表和默认关闭行为。测试不代表原生窗口或嵌入式键盘验收。


<a id="virtual-list--跳过不可聚焦行"></a>

### 跳过不可聚焦行

配合 on_navigate 使用 `navigable_rows(Rc<[usize]>)`，声明包含 enabled/focusable 目标的逻辑行索引。索引必须严格递增、无重复且小于项目数；空数组表示没有可导航行，未配置时仍假设每行都可导航。应用需保证声明与实际控件一致，这不会禁用行内控件，也不影响指针或程序化聚焦。

列表从声明中选择正向首项、反向末项以及当前索引两侧的相邻项。因此中间大量说明行、分组标题和禁用行无需为了导航而生成。查找采用二分，时间 O(log M)，M 为可导航行数；首尾查询 O(1)。无需扫描全部数据，也不调用被跳过行的工厂。视口正常显示的非导航行仍会生成并绘制。

元数据由应用拥有、列表共享 Rc，存储为 O(M)，不包含在尺寸索引缓冲预算中。复用同一 Rc 且项目数不变时复用校验结果；换数组或项目数变化时 O(M) 校验。重复、乱序或越界返回 InvalidStyle 并阻止布局，修正后可重试。数据重排、控件禁用状态改变时，应同步提交新的索引数组和一致的行描述，不能保留失效声明。

这是基于显式元数据的跳过机制，不会检查未生成的任意自定义控件来自动推断可聚焦性。目标行实际没有可聚焦后代时，应用仍需修正数据或取消待处理意图。首末有效行之外不接管 Tab，按现有默认树顺序退出；全部不可导航时不发送导航动作。

新增测试在三种行高模式的千行数据中仅声明第 100、900 行，验证正反向跳过和外部反向入口，工厂只访问初始视口及目标行。另一测试覆盖重复/乱序/越界、共享数组下数量缩减后的重新校验及修正恢复。


<a id="virtual-list--回收与当前限制"></a>

### 回收与当前限制

当前采用明确的回收策略：离开可见和预加载集合的行会卸载，若其后代持有焦点或指针捕获，先发送丢失通知再清理所有权。本地状态默认不缓存；TextInput 可绑定由应用按业务 Key 保存的 TextInputDraft，在卸载时移交提交文本、选区和撤销/重做记录，重建后恢复。工厂应复用句柄，不能在每次调用时新建。参见 [回收后的编辑草稿](text.md#text-input--回收后的编辑草稿)。这不保留离屏节点的焦点、捕获或 IME 预编辑。有限编辑工作集可使用 TextInputDraftCache 按业务 Key 复用句柄，并显式维护空闲载荷；条目数和空闲预算不等于全部活动编辑状态的硬内存上限。

尚未实现屏幕外编辑行保活、未声明导航元数据时对任意行可聚焦性的自动推断、虚拟无障碍节点、基于业务 Key 的数据重排锚点保持、测量缓存淘汰、触摸惯性和多列虚拟表格。现有焦点区域滚入仅适用于已经生成的节点；索引定位只改变滚动位置，不转移焦点。


<a id="virtual-list--验证与预览"></a>

### 验证与预览

顶层集成测试使用真实 Taffy 和核心协调，百万项测试场景只创建 11～15 个节点（列表、行槽及简单行），验证重叠身份、远索引定位、布局局部坐标与命中一致性、滚轮、滚动条捕获、键盘、缩短/空列表/零视口、工厂预算以及回收焦点/捕获。节点数量随实际行子树复杂度变化，该用例不代表帧率基准。

像素测试检查开头、第 90 万行附近的非整行偏移及末尾，在 1×/2× 下核对裁剪、间隙和行颜色。本阶段 CPU 与真实 GPU 用例均已通过；真实 GPU 用例默认忽略，需要显式运行。

```sh
cargo test -p zui-render-check --test virtual_list --all-features --locked --offline
cargo run -p zui-render-check --locked --offline -- cpu artifacts/virtual-list-cpu.png virtual-list
```

预览展示百万条遥测记录中的第 900001 条附近内容，实际只生成当前片段；它不是原生窗口输入或嵌入式硬件性能验收。

显式可变行高新增测试使用 4000 个混合高度项目，核对槽位尺寸、局部索引跳转、命中、根重建保留、锚点与请求优先级、非法尺寸恢复和空数据；像素测试覆盖 1×/2× 下的起点、较大索引附近非整行偏移及末尾。

自动测量测试覆盖混合自然尺寸、受限节点数、数据版本更新、宽度/字体版本变化后的真实文字换行、目标行对齐、最小空行及测量失败恢复。CPU 与真实 GPU 检查起点和较大索引位置、非整行滚动与 1×/2× 像素。自动行高预览：

```sh
cargo run -p zui-render-check --locked --offline -- cpu artifacts/virtual-list-measured-cpu.png virtual-list-measured
```

百万项自动测量预算测试确认：实际高度与估值相同时索引缓冲为零；出现尺寸偏差时零预算明确拒绝，增加到 16 KiB 后可在第 90 万行附近完成测量与正确命中，UI 节点保持少于 25 个。这是该测试场景的资源证据，不是全部数据访问模式的内存上限。

反向进入专项在三种行高模式下使用 1000 行、单行视口：初始仅生成第 0 行，从列表后方 Shift+Tab 后生成第 999 行并聚焦其最后一个控件，随后行内反向遍历不重复产生列表导航动作。
