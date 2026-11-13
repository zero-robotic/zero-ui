# ZUI

图表与大屏组件统一位于可选扩展包 `zui-extras`，按 `charts` / `dashboard` feature 启用，迁移方式与后续扩展边界见 [可选扩展组件](docs/extras.md)。

使用 Rust 实现的 GUI 框架，面向桌面 GPU、无 GPU 嵌入式 Linux，并预留 MCU 扩展边界。

架构约束：上层通过公开契约调用下层，下层不感知上层具体实现；模块按职责和数据所有权划分，保持高内聚、低耦合。依赖白名单和边界审查规则见 [模块分层与依赖约束](docs/module-boundaries.md)。

当前推进第三阶段。已有后端无关的基础值类型、绘制命令、Vello CPU/GPU 离屏适配、桌面窗口呈现、基础 Flex/Grid 布局、持久控件树、基础声明式 View/Key 协调，以及 Parley 排版、字形绘制、只读文本控件、标准布局容器和语义主题基础。编辑控件和完整事件运行时仍按计划建设。渲染预览是基础能力验证，不代表大屏组件已完成。

## 构建与运行

使用 Rust 1.89 或更新版本；当前开发验证环境为 1.98.1。依赖使用提交的 `Cargo.lock`。

```sh
# CPU 路径不依赖 wgpu，也不要求 GPU 或窗口系统。
cargo run -p zui-render-check --locked -- cpu artifacts/cpu.png

# GPU 预览使用真实 GPU，失败时报告错误，不静默改用 CPU。
cargo run -p zui-render-check --no-default-features --features gpu --locked -- gpu artifacts/gpu.png

# 使用实际 Flex/Grid 计算结果绘制嵌套布局。
cargo run -p zui-render-check --locked -- cpu artifacts/layout-cpu.png layout

# 使用固定中文字体进行排版与字形绘制。
cargo run -p zui-render-check --locked -- cpu artifacts/text-cpu.png text

# 通过声明式文本控件、控件树和布局服务绘制。
cargo run -p zui-render-check --locked -- cpu artifacts/text-widget-cpu.png text-widget
```

GPU 离屏接口为测试和图片导出执行像素回读。桌面 GPU 窗口直接呈现到表面；CPU 窗口使用软件像素缓冲。

```sh
# 单后端桌面窗口；点击或按空格切换颜色，Escape 关闭。
cargo run -p zui-window-check --no-default-features --features cpu --locked -- cpu
cargo run -p zui-window-check --no-default-features --features gpu --locked -- gpu

# 自动关闭的窗口冒烟检查，需要桌面会话。
cargo run -p zui-window-check --all-features --locked -- gpu --smoke-ms 1500

# 控件、文字和主题窗口；自动切换主题后退出。
cargo run -p zui-window-check --all-features --locked -- cpu --widgets --smoke-ms 1600
```

后台图片解码示例：`cargo run -p zui-window-check --features gpu --locked --offline -- cpu --images`（将 cpu 改为 gpu 可切换后端）。显示四种图片适配，支持重新加载及模拟解码错误。

## 验证

```sh
cargo fmt --all --check
python3 tools/check-dependencies.py
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

# 默认测试不启动 GPU；有真实 GPU 时显式执行，初始化失败算失败。
cargo test -p zui-render-check --all-features --locked -- --ignored --nocapture
```

裸机库编译检查：安装目标后执行 `python3 tools/check-mcu.py`，范围和验证边界见 [MCU 扩展验证](docs/mcu-portability.md)。

基础类型与绘制契约分别采用 `no_std` 和 `no_std + alloc`，不代表完整框架已经可在 MCU 运行。

## 设计与进度

- [分阶段实现与验证记录](docs/implementation-plan.md)
- [Stepper 数值步进器](docs/stepper.md)
- [图片资源与显示进度](docs/images.md)
- [Canvas 便捷自定义绘制](docs/canvas.md)
- [总体架构](docs/rust-gui-framework-design.md)
- [模块分层与依赖约束](docs/module-boundaries.md)
- [布局方案](docs/layout-design.md)
- [标准布局容器](docs/containers.md)
- [Stack 叠层布局](docs/stack.md)
- [子内容视口与坐标一致性](docs/child-viewport.md)
- [ScrollView 单轴与双轴滚动](docs/scroll.md)
- [焦点与区域自动滚入](docs/reveal.md)
- [虚拟化范围索引](docs/virtualization.md)
- [VirtualList：固定、显式与自动行高](docs/virtual-list.md)
- [主题变量与更新](docs/theme.md)
- [控件窗口应用装配](docs/widget-application.md)
- [控件事件、焦点与鼠标捕获](docs/events.md)
- [按钮基础](docs/button.md)
- [单选列表 ListBox](docs/list-box.md)：受控选择、换行标签和内部滚动。
- [切换按钮 ToggleButton](docs/toggle.md)：受控选中状态与两套按钮样式。
- [大屏指标卡](docs/dashboard.md)：标题、读数、变化说明与可选趋势组合。
- [图表与 Sparkline](docs/charts.md)：独立图表模块、缺测断线与趋势面积。
- [分组折叠 Accordion](docs/accordion.md)：单项/多项展开策略和稳定项身份。
- [可折叠面板 Disclosure](docs/disclosure.md)：受控展开与按需内容构建。
- [复选框与受控状态](docs/checkbox.md)
- [单选组与受控选择](docs/radio.md)
- [水平滑块与数值范围](docs/slider.md)
- [进度条](docs/progress.md)
- [单行输入框基础](docs/text-input.md)
- [输入法会话与候选位置桥接](docs/ime.md)
- [文本剪贴板](docs/clipboard.md)
- [按需动画与光标闪烁](docs/animation.md)
- [声明式 View 与 Key 协调](docs/view-reconciliation.md)
- [文字服务与字形绘制](docs/text-design.md)
- [Unicode 编辑状态与撤销历史](docs/editing.md)
- [控件与大屏](docs/widgets-and-dashboard-design.md)
- [独立自定义控件示例](examples/custom-widget/README.md)
- [渲染选型与版本策略](docs/rendering-library-evaluation.md)
- [受控 Switch 开关](docs/switch.md)：布尔请求、主题状态和可关闭过渡。
- [分段选择 SegmentedControl](docs/segmented.md)：受控选择、等宽标签和键盘导航。
- [页签 Tabs](docs/tabs.md)：受控页签、惰性页面构建和稳定页面身份。

## 可运行组件展厅

- [传统控件展厅](examples/controls/README.md)：全部现有传统控件、Canvas 和多组状态联动。`cargo run -p zui-controls-example -- cpu`
- [大屏组件展示](examples/dashboard/README.md)：指标卡、趋势图、切角面板与可控制的模拟实时采样。`cargo run -p zui-dashboard-example -- cpu`

两个示例均支持 `--no-default-features --features gpu -- gpu`，并可追加 `--smoke-ms 1600` 自动退出。

- [现有控件完善与 P0 控件](docs/p0-controls.md)：弹出层、下拉框、菜单、弹窗、Tooltip、数字输入及密码/多选能力。

统一矢量图标、主题配置与自定义路径：[图标说明](docs/icons.md)。

树形选择、表格、分栏与多行编辑：[P1 控件](docs/p1-controls.md)。

P1/P2 完整清单、可编辑表格与专用选择器：[控件清单](docs/controls-catalog.md)。

已有控件增强的接口、示例与剩余缺项见 [控件增强](docs/advanced-controls.md)。

原生 OpenGL 控件：`cargo run -p zui-opengl-example -- cpu`；GPU 合成使用 `--features gpu -- gpu`。业务可直接编写 shader 和 GL draw calls，接入方式与生命周期约定见 [OpenGlView](docs/opengl-view.md)。
