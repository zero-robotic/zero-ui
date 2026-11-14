# 主题、统一外观与图标

[文档索引](README.md) · [模块边界](module-boundaries.md) · [未完成事项](implementation-plan.md)

按架构职责归并的专题参考。设计目标与接口约束分开理解；待办统一维护在实施计划中，各节的验证说明仅覆盖所述范围。

## 章节导航

- [主题变量与更新](#theme)
- [控件公共外观层](#widget-appearance)
- [控件焦点反馈](#focus-feedback)
- [桌面控件呈现](#desktop-presentation)
- [矢量图标](#icons)

<a id="theme"></a>


<a id="theme--主题变量与更新"></a>

## 主题变量与更新

`zui-theme` 是 `no_std`、无堆分配的通用值模块，仅依赖 `zui-types`。它不认识控件、布局引擎、字体注册器、核心或渲染器；控件状态到主题值的映射属于控件层。

当前提供：

- `ColorRole`：页面背景、面板、抬升面板、正文、次要文字、主色、主色上的文字、边界及成功/警告/危险颜色。
- `TypeRole`：Caption、Body、Heading、Display，对应字号和相对行高。
- `Space`：None、Xs、Sm、Md、Lg、Xl，对应逻辑间距。
- `Theme::dark()`、`light()` 和 `dashboard()`：三套配色，共享默认字号与间距。
- `Theme::desktop_dark()`、`desktop_light()`：参考 Blender 控件的中性灰桌面主题，蓝色强调色、4px 圆角、26px 控件最小高度与紧凑间距；控件展厅默认使用这组主题。输入框使用页面底色，与面板和按钮区分；选中背景使用不透明混色，主操作与勾选标记使用白色前景。

主题是应用拥有的普通值，无全局主题管理器。构建 View 时读取当前主题，将语义变量转为普通属性，再通过现有 `Tree::reconcile` 更新。主题改变不会自动修改已经构建的 View；应用必须重建描述并提交协调。

```rust
use zui_api::view::View;
use zui_theme::{ColorRole, Space, Theme, TypeRole};
use zui_widgets::{Container, TextView};

// families 来自调用方显式注册的字体族。
let theme = Theme::dashboard();
let text = TextView::themed(&theme, "设备状态", families, TypeRole::Body);
let view = Container::new(View::new(text))
    .background(theme.color(ColorRole::Surface))
    .style(|s| s.padding = zui_api::layout::Edges::all(theme.space(Space::Md)))
    .build();
```

`TextView::themed` 默认使用正文颜色，调用方可进一步覆盖其 color、align 等属性。字体族仍由应用提供；主题不会扫描字体、读取文件或改变文字服务。字号与行高必须为正且有限，间距必须非负且有限。主题变量验证不替代文字服务的资源预算，例如超出服务最大字号仍会返回排版错误。

局部覆盖通过克隆主题并调用 `set_color`、`set_typography`、`set_space` 完成，再用该副本构建指定子树；原主题不变。当前没有隐式的树级继承、样式选择器或优先级规则。

主题本身不发出“全树重排”指令。协调按最终使用的属性决定失效：

| 实际变化 | 当前行为 |
| --- | --- |
| 背景或文字颜色 | 仅绘制失效，保留已排版字形和节点身份 |
| 字号、行高、字体族 | 文本布局失效，重新测量和准备 |
| padding、gap 等布局值 | 布局失效 |
| 没有被本次 View 使用的变量 | 不改变控件属性，不产生额外失效 |

预览与测试由顶层验证工程装配：

```sh
cargo run -p zui-render-check --locked -- cpu artifacts/theme-light.png theme-light
cargo run -p zui-render-check --locked -- cpu artifacts/theme-dashboard.png theme-dashboard
cargo run -p zui-render-check --locked -- cpu artifacts/desktop-dark.png desktop-dark
cargo run -p zui-render-check --locked -- cpu artifacts/desktop-light.png desktop-light
```

测试验证配色切换后像素改变、字形和节点身份保留、无额外排版调用；字号和间距修改触发重新布局。CPU/GPU 执行同一验证路径。

`ControlMetrics` 提供控件/面板圆角、最小高度与水平内边距，可通过 `set_controls` 局部覆盖。控件层负责悬停、按下、禁用与焦点状态映射；桌面主题已有文字和选中态对比度、分数缩放及 Retina 渲染检查。系统主题跟随、主题过渡动画和完整可访问性验收仍待实现。Dashboard 配色不意味着已实现辉光、滤镜或完整大屏组件。

<a id="widget-appearance"></a>


<a id="widget-appearance--控件公共外观层"></a>

## 控件公共外观层

`zui-widgets/src/appearance.rs` 是控件内部的视觉层，不持有值、命中测试、焦点所有权或指针捕获。按钮、复选框、开关、单选组、分段按钮、输入框复用基础表面绘制；滑块复用状态解析和焦点环。Toggle、菜单按钮、表头等组合控件通过基础控件复用这些规则。


<a id="widget-appearance--状态规则"></a>

### 状态规则

- 交互调色板优先级：禁用 > 按下 > 悬停 > 正常。鼠标按下只有仍位于原目标时显示，键盘 Space 按下按活动项显示；滑块拖动期间保持按下态。
- 选中是调色板内的独立维度，不覆盖禁用、按下或悬停：先解析交互调色板，再选择其中的选中填充、文字或标记。Toggle 继续在组合层选择选中按钮样式。
- 焦点提示独立于填充：仅启用且拥有可见键盘焦点的控件绘制。公共焦点环为内缩 1px、宽 2px；选择组仅活动项绘制。鼠标获取焦点不新增外框。
- 输入框使用编辑语义而非普通按钮焦点环：校验错误边框优先，其次为启用且实际聚焦时的编辑边框，最后为普通边框。禁用的错误输入仍可显示错误；不改变光标、选择区和 IME 行为。


<a id="widget-appearance--几何与分组"></a>

### 几何与分组

`Surface` 统一填充、1px 内置边框、焦点环与圆角；先按外框尺寸限制半径，再随描边中心线内缩量减小半径，避免填充和轮廓圆角不一致。透明填充/边框不生成路径；退化尺寸不绘制边框。`padded` 将内边距限制在半宽/半高内，供按钮、输入框、分段文字使用。

`SegmentedStyle.radius` 控制圆角，默认取主题控件半径，必须有限且非负。零间距分段组（以及 Tabs）使用一个圆角裁剪、一个外框和每个分界一条 1px 线：首项仅左侧外角、末项仅右侧外角，中间项保持直角；单项自然保留四角。边线在所有项填充后绘制，不叠加两侧描边。非零间距的普通分段按钮仍各自绘制圆角边框。默认间距不变；需要连续分段时设置 `style.gap = 0.0`。

现有完全透明的 DataGrid 只读单元格保持透明，由行容器决定背景，不由公共层额外添加选中底色。


<a id="widget-appearance--渲染边界与验证"></a>

### 渲染边界与验证

公共层仅输出已有的 Path / Fill / Stroke / Clip 命令，CPU/Vello 后端与绘制协议没有改动，也没有新增专用 shader 或批处理接口。此重构不宣称 GPU 性能收益；进一步优化应先分别测量路径构建、命令转换、GPU 时间和端到端帧时间。

单元测试覆盖交互状态组合、选择组活动项、透明表面、内缩圆角、过小尺寸与共享线数量。分段像素测试覆盖三种主题、启用/禁用以及 1×/2× 缩放；现有焦点、DataGrid、排序表头和控件渲染测试继续约束行为。

<a id="focus-feedback"></a>


<a id="focus-feedback--控件焦点反馈"></a>

## 控件焦点反馈

参考 BUI 的 Blender 控件路径，而不是其 HTML 外观路径：[`state_of` / `text_draw`](https://github.com/ppmpreetham/BUI/blob/main/kit/intern/kit_widgets.c) 分别构造普通控件状态和文本编辑状态；[`bk_widget_state`](https://github.com/ppmpreetham/BUI/blob/main/kit/intern/kit_draw.c) 使用填充、现有轮廓和文字颜色表达选中、悬停及默认激活状态。普通控件不是只要持有焦点就额外叠加一圈统一描边。本项目借鉴状态分离的思路，独立实现，不引入 BUI 源码或运行时依赖。


<a id="focus-feedback--显示规则"></a>

### 显示规则

- 鼠标按下后隐藏普通控件的额外焦点框；逻辑焦点、键盘操作、指针捕获和应用的选中值不变。按钮按下色、复选标记、开关位置、列表选择色等反馈仍保留。
- Tab、Shift+Tab、方向键及其他不带 Control/Alt/Super 的有效按键按下后，显示键盘焦点提示。即使焦点仍在同一个节点，也必须重绘。
- 鼠标移动、悬停、滚轮、按键释放及单纯修饰键变化不切换提示模式，避免键盘导航时鼠标轻微移动导致提示消失。带 Control/Alt/Super 的快捷键不额外开启提示。
- 输入框保留编辑态边框、插入光标、选择区与 IME 目标，不受普通控件的鼠标隐藏规则影响。数值输入和文本区域沿用其输入框编辑逻辑。
- 初始没有键盘导航时不主动显示焦点框；程序化焦点请求、弹窗范围与虚拟列表的延后焦点请求继承最近的输入模式。失焦或禁用后不显示焦点框。

保留键盘焦点提示是 ZUI 的可访问性选择，不宣称逐像素复制 BUI 的所有控件表现。

DataGrid 的默认只读单元格不再沿用按钮的底板和圆角边框：普通、悬停、按下、禁用状态均使用透明单元格背景，由整行统一绘制普通或选中底色，避免“选中容器套一层按钮”的双层效果。点击与键盘选择继续保留；表头、可编辑字段和自定义单元格仍保留各自的外观。

Table 和 DataGrid 的可排序表头始终预留排序箭头位置。`ButtonView::reserve_trailing_icon` 默认关闭，表头显式开启后，箭头显示、隐藏或切换方向只更新绘制，不改变文字可用宽度与表头高度。窄列的长标题仍允许换行，但不会等点击排序时才突然换行；普通按钮、步进按钮和折叠标题保持原有布局。


<a id="focus-feedback--契约与缓存"></a>

### 契约与缓存

核心维护通用输入模式，通过 `WidgetEvent::FocusVisibleChanged(bool)` 向当前焦点节点直接通知。获得焦点时，先发送 `FocusChanged(true)`，再发送当前可见性；失焦仍用 `FocusChanged(false)` 清除控件本地焦点状态。可见性通知本身不触发焦点转移、取消手势或滚动揭示。

普通控件将“持有焦点”和“显示提示”分开：只有两者均为真且可用时才绘制额外描边。共享 `Interaction` 覆盖按钮、复选框、开关、单选、分段选择及列表；滑条、滚动容器、HSV 选择器也遵守同一通知。原来单选组独有的鼠标焦点追踪已移除，统一由核心传播，避免跨控件或延后焦点转移时不一致。

可见性变化返回 Paint 失效，正常失效相关子树缓存，不强制完整布局或整帧重绘。文本编辑类控件可以忽略可见性通知，继续根据真实焦点表达编辑状态。新增公共枚举分支后，自定义控件若穷尽匹配 `WidgetEvent`，需补充处理或忽略该通知。


<a id="focus-feedback--验证"></a>

### 验证

```sh
cargo test -p zui-core --test events --locked
cargo test -p zui-render-check --test focus_visible --test button --test radio --locked
```

覆盖鼠标点击、键盘恢复、同节点模式切换、悬停和修饰键不干扰、双向 Tab、程序化焦点继承、协调后状态保留、绘制缓存失效、失焦清除，以及文本输入边框与 IME 保留。既有控件测试继续验证点击激活、键盘操作、禁用状态、选择值和拖动捕获。

<a id="desktop-presentation"></a>


<a id="desktop-presentation--桌面控件呈现"></a>

## 桌面控件呈现

控件展厅默认使用 `Theme::desktop_dark()`，切换主题后使用 `Theme::desktop_light()`。原 `dark/light/dashboard` 主题保留原有字号和密度，已有应用可自行选择迁移。

桌面主题使用随应用注册的字体，正文 14、辅助文字 12、标题 20、展示文字 28 个逻辑像素。间距为 4/8/12/16/24；`ControlMetrics` 统一提供控件圆角 6、面板圆角 8、最小内容高度 28、按钮横向留白 8。布局外加 padding 会增加最终高度；显式尺寸优先，紧凑布局应设置最终高度并为文字预留足够空间。`Theme::set_controls` 支持应用级调整。

桌面主题的文本输入框默认内部留白为左右 8、上下 4 个逻辑像素，通过 `TextInputStyle::padding_x/padding_y` 调整；它会叠加布局 padding，文字、占位提示、选区、鼠标定位和输入法候选区域共享同一内容边界。

桌面主题的 `ButtonView::themed` 默认使用次按钮样式。重要操作显式指定 `ButtonStyle::primary(theme)`，辅助操作可指定 `ButtonStyle::text(theme)`。列表、树和分段控件使用不透明的柔和选中背景；`selection_background/selection_foreground` 成对使用。下拉弹层有完整背景、内边距、细边框和圆角；未增加模糊阴影或持续动画。

`Container::radius` 和 `border` 绘制表面，不裁剪子节点为圆角。需要给内容保留内边距，防止其覆盖边框和圆角。

树节点 `double_click_interval` 默认 500 毫秒，平台层可传入系统偏好，设为零可关闭文字双击展开。当前没有自动读取 Windows/macOS 双击设置。


<a id="desktop-presentation--验证与预览"></a>

### 验证与预览

```sh
cargo run -p zui-render-check -- cpu artifacts/desktop-dark.png desktop-dark
cargo run -p zui-render-check -- cpu artifacts/desktop-light.png desktop-light
cargo test -p zui-render-check --test desktop
cargo test -p zui-render-check --features gpu --test desktop
cargo test -p widget-gallery
```

桌面样张覆盖按钮层级、输入框、下拉按钮、分组面板和树节点；测试覆盖深浅主题正文/辅助文字/选中态对比度，以及 1/1.25/1.5/2 倍离屏渲染的圆角和背景。沿用同一字体资源与场景绘制路径，不依赖系统默认字体。离屏测试不等同于 Windows/macOS 的窗口、系统缩放、输入法或显示器色彩实机验收。

弹窗标题/关闭按钮、系统窗口装饰和自绘标题栏接入方式见 [弹窗与窗口装饰](advanced-widgets.md#dialog-window-chrome)。

桌面列表和 Tab 标签采用上下 4 的留白；单选、复选指示器为 16。分页按钮不再叠加额外布局留白。正文仍为 14，避免靠缩小文字压缩界面。CPU/GPU 文字绘制均开启字体 hinting，帮助小字号笔画对齐设备像素；实际效果取决于字体 hint 数据与显示缩放，不保证不同平台逐像素一致。

<a id="icons"></a>


<a id="icons--矢量图标"></a>

## 矢量图标

ZUI 的图标统一由 `zui-widgets::Icon` 表示，与字体、操作系统及 CPU/GPU 后端无关。实现位于 widgets 的独立 icon 模块，仅调用下层公共 Path/Painter 接口；core、paint 和渲染后端不识别具体图标语义。保持 no_std + alloc，无新增依赖。


<a id="icons--使用与扩展"></a>

### 使用与扩展

```rust
use zui_api::view::View;
use zui_widgets::{Icon, IconKind, IconView};

let arrow = Icon::builtin(IconKind::ChevronDown);
let view = View::new(IconView::themed(&theme, arrow));
```

内置 ChevronDown/Right/Up/Left、Check、Close、Plus、Minus。所有内置图标采用 24×24 设计坐标和 2 单位描边，居中等比缩放并裁剪到目标区域。独立 IconView 的默认尺寸取主题 Body 字号、颜色取 Text，可通过 size/color 覆盖；它不获取焦点或拦截命中。

`Icon::custom(path, stroke_width)` 接受相同坐标系的公共 Path；None 表示填充，Some 表示描边。非法路径或非有限、非正、超过 24 的描边宽度返回 None。几何通过 Arc 共享，应用可在自己的资源表中按名称注册和复用 Icon，无需全局可变注册表或修改框架枚举。未来 SVG 解析应由独立适配模块转换为公共几何，不进入基础控件和绘制层。目前为单色图标，不包含 SVG 解析、多色分层或动画图标。


<a id="icons--控件集成"></a>

### 控件集成

- ButtonView.trailing_icon：在文字右侧预留图标及间距，图标颜色随普通、悬停、按下、禁用状态变化。
- Select：内置 ChevronDown，替换原来追加的 Unicode 三角字符。
- ListItem.leading_icon/trailing_icon：支持自定义图标；同一 ListBox 按整列是否存在图标预留空间，保持文字对齐。
- Menu：勾选使用 Check，子菜单使用 ChevronRight，不再把图标拼进标签文本。

文字继续由文本引擎布局，图标不需要字体回退。结构体字面量构造 ButtonView/ListItem 的调用方需补充新增字段；使用 themed/new 构造器的调用方无需修改。


<a id="icons--验证"></a>

### 验证

icons 集成测试覆盖无字体环境、内置/自定义图标、1×/2× 像素、居中等比显示、被动命中、失效级别和非法参数。P0 回归新增 Reject 缺字策略下的中文下拉按钮、勾选与子菜单布局，防止重新引入字体符号依赖。原有菜单、下拉和列表交互测试继续保留。
