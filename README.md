# ZUI

图表与大屏组件统一位于可选扩展包 `zui-extras`，按 `charts` / `dashboard` feature 启用，迁移方式与后续扩展边界见 [可选扩展组件](docs/visualization.md#extras)。

使用 Rust 实现的 GUI 框架，面向桌面 GPU、无 GPU 嵌入式 Linux，并预留 MCU 扩展边界。

架构约束：上层通过公开契约调用下层，下层不感知上层具体实现；模块按职责和数据所有权划分，保持高内聚、低耦合。依赖白名单和边界审查规则见 [模块分层与依赖约束](docs/module-boundaries.md)。

当前推进第三阶段。已有后端无关的基础值类型、绘制命令、Vello CPU/GPU 离屏适配、桌面窗口呈现、基础 Flex/Grid 布局、持久控件树、基础声明式 View/Key 协调，以及 Parley 排版、字形绘制、只读文本控件、标准布局容器和语义主题基础。编辑控件和完整事件运行时仍按计划建设。渲染预览是基础能力验证，不代表大屏组件已完成。

## 构建与运行

使用 Rust 1.95 或更新版本；当前开发验证环境为 1.98.1。依赖使用提交的 `Cargo.lock`。

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

裸机库编译检查：安装目标后执行 `python3 tools/check-mcu.py`，范围和验证边界见 [MCU 扩展验证](docs/platform.md#mcu-portability)。

基础类型与绘制契约分别采用 `no_std` 和 `no_std + alloc`，不代表完整框架已经可在 MCU 运行。

## 架构与文档

- [文档索引](docs/README.md)：按核心运行时、布局、控件、渲染及平台分层查阅。
- [总体架构](docs/architecture.md)与[模块边界](docs/module-boundaries.md)。
- [未完成事项与实施约束](docs/implementation-plan.md)。
- [独立自定义控件示例](examples/custom-widget/README.md)。

## 可运行组件展厅

- [传统控件展厅](examples/controls/README.md)：全部现有传统控件、Canvas 和多组状态联动。`cargo run -p zui-controls-example -- cpu`
- [大屏组件展示](examples/dashboard/README.md)：指标卡、趋势图、切角面板与可控制的模拟实时采样。`cargo run -p zui-dashboard-example -- cpu`

两个示例均支持 `--no-default-features --features gpu -- gpu`，并可追加 `--smoke-ms 1600` 自动退出。

控件接口集中见[基础控件](docs/basic-controls.md)、[导航与折叠容器](docs/navigation.md)和[复合控件](docs/advanced-widgets.md)；样式与图标见[主题与外观](docs/appearance.md)。

原生 OpenGL 控件：`cargo run -p zui-opengl-example -- cpu`；GPU 合成使用 `--features gpu -- gpu`。业务可直接编写 shader 和 GL draw calls，接入方式与生命周期约定见 [OpenGlView](docs/rendering.md#opengl-view)。
