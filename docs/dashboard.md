# 大屏组合模块

zui-extras::dashboard 是 zui-extras 中由 dashboard feature 按需启用的 no_std + alloc 上层组合模块，依赖公共 API/类型/Painter/主题、zui-widgets 和 zui-extras::charts，不依赖核心、窗口、布局引擎或渲染实现。基础控件和图表不反向引用大屏组件。当前提供 MetricCardView 指标卡和 DashboardPanelView 切角面板，后续逐步加入更多装饰组合。

## 指标卡

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

## 预览与验证

```sh
cargo run -p zui-render-check --locked --offline -- cpu artifacts/metric-cpu.png metric
cargo test -p zui-render-check --test metric --locked --offline
```

预览展示在线设备、平均延迟和待处理告警三种指标。真实 Taffy/Parley 集成测试验证可选说明增删、主题切换后的节点身份，以及窄宽度下标题/单位换行。CPU 图片已目视检查；严格 Clippy、no-default-features 编译、格式和 23 包依赖/后端隔离检查通过。本阶段复用已有绘制实现，未重跑 GPU；不代表 MCU 硬件或完整大屏方案已验收。

可点击指标卡、精确基线对齐、数字动画、自动阈值规则、大屏装饰面板和无障碍分组语义仍待后续实现。

## 切角装饰面板

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

## 实时窗口示例

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

## 原生窗口冒烟记录

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
