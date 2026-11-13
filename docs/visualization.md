# 可选扩展：图表与大屏

[文档索引](README.md) · [模块边界](module-boundaries.md) · [未完成事项](implementation-plan.md)

按架构职责归并的专题参考。设计目标与接口约束分开理解；待办统一维护在实施计划中，各节的验证说明仅覆盖所述范围。

## 章节导航

- [可选扩展组件 zui-extras](#extras)
- [图表模块与 Sparkline](#charts)
- [大屏组合模块](#dashboard)

<a id="extras"></a>


<a id="extras--可选扩展组件-zui-extras"></a>

## 可选扩展组件 zui-extras

原 `zui-charts` 和 `zui-dashboard` 合并为一个 `zui-extras` crate；不保留旧 crate 的兼容包装。后续大屏相关图表、仪表、指标卡、面板、装饰和组合组件统一放入此 crate，不再按组件类别拆分 crate。


<a id="extras--按需启用"></a>

### 按需启用

默认 feature 为空，不编译组件，也不引入 ZUI 依赖。基础控件和运行时不依赖扩展组件。

```toml
[dependencies]
zui-extras = { path = "crates/zui-extras", default-features = false, features = ["dashboard"] }
```

路径按应用位置调整；工作区成员使用 `workspace = true` 替代 `path`。

| feature | 模块与依赖 |
| --- | --- |
| `charts` | `zui_extras::charts`：Sparkline、样本序列、值域和历史缓冲；仅依赖公共 API、类型、绘制与主题 |
| `dashboard` | `zui_extras::dashboard`：MetricCard、DashboardPanel；自动启用 `charts` 并引入标准控件 |

只使用图表时启用 `charts`，不会引入 `zui-widgets`。两种功能均保持 `no_std + alloc`，不依赖核心运行时、窗口、布局引擎或 CPU/GPU 渲染后端。

```rust
use zui_extras::charts::{SparklineHistory, SparklineSeries, SparklineView};
use zui_extras::dashboard::{DashboardPanelView, MetricCardView};
```


<a id="extras--模块边界"></a>

### 模块边界

大屏组合可以依赖图表；图表不得反向依赖大屏组合。各模块通过公开类型协作，不访问兄弟模块的私有实现。新增大屏能力优先增加内部模块，必要时增加 feature。采样、网络与业务调度仍由应用负责，不迁入组件库。

渲染验证工具、窗口验证工具及大屏示例显式启用 `dashboard`，因此保留原有展示与测试覆盖。传统控件示例无需启用此扩展。


<a id="extras--验证"></a>

### 验证

```sh
cargo check -p zui-extras --no-default-features --locked
cargo test -p zui-extras --no-default-features --features charts --locked
cargo test -p zui-extras --no-default-features --features dashboard --locked
python3 tools/check-dependencies.py
python3 tools/check-mcu.py
```

依赖检查覆盖空 feature、仅 charts、dashboard 和 CPU/GPU 隔离；MCU 脚本显式启用 dashboard，避免空 feature 编译造成假通过。

<a id="charts"></a>


<a id="charts--图表模块与-sparkline"></a>

## 图表模块与 Sparkline

zui-extras::charts 是 zui-extras 中由 charts feature 按需启用的 no_std + alloc 模块，依赖 zui-api、zui-types、zui-paint 和 zui-theme；不依赖核心、标准控件、窗口、文字引擎或 Vello。应用将图表作为普通 View 放入容器；CPU/GPU 使用相同 Painter 路径描述。no_std 编译不代表已经完成 MCU 内存、显示驱动或硬件验收。


<a id="charts--迷你趋势图"></a>

### 迷你趋势图

```rust,ignore
let series = SparklineSeries::new(vec![Some(12.0), Some(18.0), None, Some(15.0)])?;
let mut chart = SparklineView::themed(&theme, series);
chart.range = Some(ChartRange::new(0.0, 20.0)?);
chart.style.natural_size = Size::new(240.0, 80.0);
let view = View::new(chart);
```

SparklineSeries 拥有共享不可变样本与缓存的极值；Clone 不复制样本数组。样本等间隔排列，None 保留其位置并断开路径，不跨缺测区连线或填充。构造拒绝 NaN/无穷值，并返回出错索引。最多 65,536 个槽位，超出返回 TooManySamples；大数据应在应用或后续聚合层显式降采样，不静默丢点。

range=None 使用所有有效样本的极值。常量序列垂直居中；空序列或全缺测不绘图。单个样本水平居中，连续段中的孤立样本绘制线宽大小的方形标记。固定 ChartRange 要求有限且严格递增，超出范围的样本饱和到上下边界。归一化兼容跨度溢出的极大端点与可表示的极小跨度，不将非有限坐标交给渲染器。

SparklineStyle 配置线色、线宽、可选面积色和自然尺寸。默认 160×48、2 像素逻辑线宽及主题强调色；面积填至绘图区底部，并非数值零基线。折线各缺测段分别填充。所有绘制裁剪到内容框，正常线宽在四周留半线宽，过大线宽仍受裁剪。自然尺寸和线宽必须有限、为正且不大于 f32::MAX，非法样式在测量/准备阶段报错，修正后可重新布局恢复。

数据、范围和配色更新只请求 Paint；自然尺寸变化或非法样式进入/恢复请求 Layout。绘图时间与样本数线性相关，每次 paint 构建路径；尚未缓存几何或增量更新。图表为被动展示，不参与命中或键盘焦点。


<a id="charts--连续采样与有界历史"></a>

### 连续采样与有界历史

SparklineHistory 在应用层维护固定容量的等间隔历史，容量范围为 1..=65,536。构造时预留环形存储，后续 push/extend 在该存储内写入；满容量先淘汰最旧槽位，None 同样占一个采样位置。

```rust,ignore
let mut history = SparklineHistory::new(240)?;
// 数据到达时写入，不必每个样本都重建界面。
history.extend(&[Some(12.0), None, Some(15.0)])?;
// 应用决定刷新时机，然后更新 SparklineView.series。
let series = history.snapshot();
```

push 返回被淘汰槽位：None 表示尚未满，Some(None) 表示淘汰缺测，Some(Some(v)) 表示淘汰有效值。extend 先验证整个切片，NaN/无穷值返回批内索引并保持数据及缓存不变；返回淘汰数量，包含同一批中被后续样本挤出的槽位。不会自动补齐时间、排序或插值，真实缺测应由应用显式写入 None。

snapshot 首次或数据变更后复制当前有效历史并重算极值，时间/额外空间为 O(N)；未变更时返回缓存的共享快照，时间 O(1)。旧快照不可变，继续可用。clear 清空历史并保留环形容量，旧快照不受影响。

容量限制针对当前历史槽位，不是整个应用内存硬上限。应用保留的旧快照、当前缓存和构建期间的临时数组都需要额外内存；构造的 AllocationFailed 仅覆盖环形存储预留，快照分配仍使用全局分配器。每个样本 push 为 O(1)，批量写入为 O(M)，绘图仍是全量路径构建。模块不创建线程、定时器或帧循环；采样与显示刷新频率由应用控制。

新增三项历史单元测试覆盖不同容量下的重复环绕、槽位淘汰、存储容量不增长、批量错误原子性、快照缓存/不可变和极值淘汰重算。CPU 集成测试验证历史更新只触发 Paint、图像随新快照改变且旧快照可以重现原图。


<a id="charts--预览和验证"></a>

### 预览和验证

```sh
cargo run -p zui-render-check --locked --offline -- cpu artifacts/sparkline-cpu.png sparkline
cargo test -p zui-extras --no-default-features --features charts --locked --offline
cargo test -p zui-render-check --test sparkline --locked --offline
# 需要可用 GPU
cargo test -p zui-render-check --features gpu --test sparkline --locked --offline -- --ignored
```

两项数据单元测试覆盖极端值、极小跨度、常量、缺测和输入校验。CPU/GPU 像素测试均实际运行通过，验证 1×/2× 断线与填充、空数据清屏、单点、Paint 更新，以及非法样式修正。CPU 预览已目视确认三种趋势状态。图表依赖策略纳入 22 包检查，关闭默认特性编译通过。


<a id="charts--后续范围"></a>

### 后续范围

当前 Sparkline 是等间隔、单序列、无坐标轴的基础趋势控件。非等间隔横轴、多序列、坐标轴/刻度、图例、提示、选择/缩放、数据聚合、增量几何更新、动画与无障碍描述仍待实现；不将当前版本视为完整折线图或整个图表体系已完成。

<a id="dashboard"></a>


<a id="dashboard--大屏组合模块"></a>

## 大屏组合模块

zui-extras::dashboard 是 zui-extras 中由 dashboard feature 按需启用的 no_std + alloc 上层组合模块，依赖公共 API/类型/Painter/主题、zui-widgets 和 zui-extras::charts，不依赖核心、窗口、布局引擎或渲染实现。基础控件和图表不反向引用大屏组件。当前提供 MetricCardView 指标卡和 DashboardPanelView 切角面板，后续逐步加入更多装饰组合。


<a id="dashboard--指标卡"></a>

### 指标卡

```rust,ignore
let mut card = MetricCardView::themed(&theme, "平均延迟", "24", families);
card.unit = Some("ms".into());
card.change = Some(MetricChange::new("延迟下降 8 ms", MetricTone::Positive));
card.trend = Some(SparklineView::themed(&theme, history.snapshot()));
card.layout.width = Dimension::Length(220.0);
card.key = Some("latency".into());
let view = card.build();
```

标题、数值、单位和变化说明都是应用已经格式化的字符串。组件不解析数字，不计算同比/环比，也不假定正负号、上涨/下跌与业务好坏之间的关系。MetricTone 提供 Neutral/Positive/Warning/Negative，分别映射为主题的普通说明/成功/警告/危险颜色。建议应用同时提供明确说明，不单靠颜色表达状态；缺测可由应用提供“—”等占位字符串。

卡片依次组合强调色条、标题、数值/单位行、可选变化说明与可选 Sparkline。单位使用较小文字并与数值底部对齐；空间不足时允许换行，不承诺文字基线精确对齐。标题和说明沿用 Text 的换行规则，不自动省略。未设置 change/trend 时不生成相应节点。

MetricCardStyle 配置背景、色条、各文字颜色、语义状态色和三组文字样式。themed 在构建时解析主题，主题改变后应重新创建描述。layout 配置根 Column 的宽高、内边距和间距，默认内边距 16、垂直间距 12；固定高度由应用确保足够，默认自然高度。trend 保留独立样式，卡片不会根据变化说明擅自改变曲线颜色。

内部节点都有稳定 key：可选说明插入/删除和主题更新不会替换数值、单位或趋势节点。整个卡片在列表中使用应用提供的稳定 key。它是被动展示组合，不提供点击动作、刷新调度、数字滚动动画或加载骨架。


<a id="dashboard--预览与验证"></a>

### 预览与验证

```sh
cargo run -p zui-render-check --locked --offline -- cpu artifacts/metric-cpu.png metric
cargo test -p zui-render-check --test metric --locked --offline
```

预览展示在线设备、平均延迟和待处理告警三种指标。真实 Taffy/Parley 集成测试验证可选说明增删、主题切换后的节点身份，以及窄宽度下标题/单位换行。CPU 图片已目视检查；严格 Clippy、no-default-features 编译、格式和 23 包依赖/后端隔离检查通过。本阶段复用已有绘制实现，未重跑 GPU；不代表 MCU 硬件或完整大屏方案已验收。

可点击指标卡、精确基线对齐、数字动画、自动阈值规则、大屏装饰面板和无障碍分组语义仍待后续实现。


<a id="dashboard--切角装饰面板"></a>

### 切角装饰面板

```rust,ignore
let mut panel = DashboardPanelView::themed(&theme, [heading, metrics_row, trend]);
panel.layout.width = Dimension::Percent(1.0);
panel.style.corner = 12.0;
panel.style.accent_length = 36.0;
let view = panel.build();
```

DashboardPanelView 使用固定 Column 根布局，接受普通 View 子节点；可在内部组合 Row/Grid、文本、按钮和图表。默认内边距 20、垂直间距 12。PanelStyle 配置背景、边框和强调线颜色，以及切角尺寸、边框宽度、强调线宽度与长度。尺寸值必须有限、非负且不超过 f32::MAX；零线宽/长度关闭相应装饰。

切角与线宽根据当前面板尺寸收敛，零尺寸不绘制。装饰绘制始终裁剪在自身矩形范围内；背景/边框为八边切角路径，顶部左侧与底部右侧绘制强调线。样式变化通常仅 Paint，非法样式及恢复触发 Layout 验证。此控件不自动改变内边距以适应新的切角值，应用应设置足够的内容留白。

clip 默认 true，仅对子内容进行**矩形裁剪**，不是切角多边形遮罩。clip=false 允许子内容溢出，自身装饰仍受矩形裁剪。装饰表面不参与命中，不阻挡空白区域下方的节点；子控件按普通核心规则命中。root key 和子 View key 由应用设置以保持身份。

```sh
cargo run -p zui-render-check --locked --offline -- cpu artifacts/panel-cpu.png panel
cargo test -p zui-render-check --test panel --locked --offline
# 需要可用 GPU
cargo test -p zui-render-check --features gpu --test panel --locked --offline -- --ignored
```

CPU/GPU 像素用例均实际执行通过，覆盖 1×/2× 切角外背景、填色、边框/强调线、子内容、子节点命中与空白穿透，以及非法切角恢复、极小和零尺寸。panel-cpu.png 已目视确认面板内组合指标卡及趋势图。没有实现发光/模糊、装饰动画或多边形子内容裁剪。


<a id="dashboard--实时窗口示例"></a>

### 实时窗口示例

```sh
cargo run -p zui-window-check --locked --offline -- cpu --dashboard
cargo run -p zui-window-check --features gpu --locked --offline -- gpu --dashboard
# 有限时长运行，适合在可用桌面环境中执行冒烟检查
cargo run -p zui-window-check --locked --offline -- cpu --dashboard --smoke-ms 2000
```

该入口使用明确标注的模拟数据，组合装饰面板、两张指标卡、趋势图、暂停/恢复按钮和垂直 Scroll。窗口变窄时指标卡换行，内容超出时滚动；--dashboard 与 --tabs/--widgets 互斥。Escape 或关闭窗口退出。

应用每 250 ms 生成一个模拟样本，历史最多 120 个槽位，显示范围固定为 0..100。采样数据由应用拥有；变化后显式更新快照和重建描述。暂停清除后续采样计划，可能已排队的一次旧 Wake 不产生样本；恢复先请求一次 Wake 获取当前单调时钟，再等待完整采样周期，不使用旧输入时间补采。Occluded(true) 停止采样计划，解除遮挡后重新计时。原生平台挂起由既有后端生命周期处理，恢复事件重新安排采样。

延迟 Wake 按遗漏周期写入缺测槽位，最多补 120 个 None，再写入一个新模拟样本，避免无界追赶和伪造过去的有效读数；下一截止时间从本次 Wake 计算。它是样本索引趋势演示，不是精确时间戳数据采集器。--smoke-ms 的退出截止时间独立于暂停状态。

三项测试使用注入时钟验证早到/到期/延迟唤醒、暂停/恢复、遮挡/恢复及暂停期间按时退出，并通过真实 WidgetHost/Taffy/Parley 验证按钮键盘动作、更新后的焦点身份和 320×260 窗口布局。示例编译及严格 Clippy 通过。随后在本机执行了 CPU/GPU 原生窗口的 2 秒限时冒烟检查，均正常退出，结果见下节；这不等同于手动交互验收。


<a id="dashboard--原生窗口冒烟记录"></a>

### 原生窗口冒烟记录

2026-10-27 在当前 macOS 开发机运行以下命令，均返回退出码 0：

```sh
cargo run -p zui-window-check --features gpu --locked --offline -- cpu --dashboard --smoke-ms 2000
cargo run -p zui-window-check --features gpu --locked --offline -- gpu --dashboard --smoke-ms 2000
```

| 运行后端 | 事件数 | 重绘数 | 结果 |
| --- | ---: | ---: | --- |
| CPU | 24 | 8 | 自动关闭，正常退出 |
| GPU | 26 | 9 | 自动关闭，正常退出 |

两次均使用启用 GPU 特性的同一应用构建，通过运行参数选择实际后端。结果证明当前机器上的窗口启动、选定渲染路径、事件循环和限时退出可运行，且发生多次重绘；不把重绘次数当作性能基准。采样/暂停/恢复逻辑另由注入时钟和 Host 测试覆盖。

本次没有原生窗口截图或逐帧视觉检查，没有手动点击、拖动缩放、输入法、长时间运行、跨平台及嵌入式硬件验收。CPU-only/GPU-only 依赖隔离继续由独立依赖检查覆盖，不能仅凭这两次同时启用特性的运行推断。
