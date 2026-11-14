# 示例导航

按学习目标选择示例；每个目录是独立 Cargo 包，从仓库根目录运行。

| 示例 | 适合学习 | 启动方式 |
| --- | --- | --- |
| [minimal](minimal/README.md) | 最小窗口、View、Action 和状态更新 | `cargo run -p minimal --locked` |
| [widget-gallery](widget-gallery/README.md) | 控件交互、主题、表单和页面组合 | `cargo run -p widget-gallery --locked` |
| [dashboard](dashboard/README.md) | 定时采样、业务状态和图表联动 | `cargo run -p dashboard --locked` |
| [custom-widget](custom-widget/README.md) | 用公共 Widget/Painter 契约扩展控件 | 库示例，按 README 运行集成测试 |
| [opengl](opengl/README.md) | 接入原生 GL 内容 | `cargo run -p opengl --locked` |

建议先读 `minimal/src/main.rs`，再按需求阅读控件展厅或数据大屏，最后阅读扩展示例。离屏渲染检查和性能基准位于 `tools/`。

## 运行约定

窗口示例默认构建 CPU/GPU 两种后端，运行时省略参数使用 GPU。仅构建并运行 CPU 后端：

```sh
cargo run -p minimal --no-default-features --features cpu --locked -- cpu
```

替换包名即可运行其他窗口示例。OpenGL 示例即使使用 CPU 合成也需要原生 GL 环境。窗口运行需要桌面会话和已安装的字体。桌面示例通过 `zui-text/system-fonts` 发现系统字体，并按文字脚本回退，不嵌入仓库字体；中文内容需要系统有中文字体。Linux 构建需要 Fontconfig 开发包（如 `libfontconfig1-dev`）。

控件展厅、数据大屏和 OpenGL 支持 `--smoke-ms 1600` 自动退出；最小示例只接受可选的 `cpu` / `gpu` 参数，点击窗口关闭按钮退出。

## 组织方式

最小示例将应用流程保留在一个文件，便于直接阅读和复制。复杂的 widget-gallery/dashboard 分为：

- `main.rs`：参数与窗口配置。
- `app.rs`：应用状态、消息和生命周期。
- `app/views.rs`：页面构建。
- `app/tests.rs`：应用回归测试。

控件展厅的功能分组也位于 `app/`，按功能命名，不按开发阶段编号。各示例保持独立，不依赖另一个示例的启动器。
