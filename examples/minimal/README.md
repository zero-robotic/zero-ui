# 最小计数器

一个窗口、一段文字、一个按钮。完整应用代码位于 `src/main.rs`，没有剪贴板、定时任务或展厅功能依赖。

```sh
cargo run -p minimal --locked
cargo run -p minimal --no-default-features --features cpu --locked -- cpu
```

默认使用 GPU。点击“加一”更新计数；Tab 聚焦按钮，Space/Enter 激活，点击窗口关闭按钮退出。

阅读顺序：

1. `main` 发现并选择系统字体，装配布局引擎、文本服务和 `WidgetHost`，启动窗口。
2. `Counter::view` 根据计数构建文字和按钮。
3. `map_action` 将按钮动作映射为消息。
4. `message` 修改计数并请求重建 View。
5. `update` 处理窗口关闭。

系统字体由 `zui-text/system-fonts` 可选能力提供；没有中文字体的系统需先安装中文字体。复制到独立项目时将 workspace 依赖改为对应路径或版本，并保留该 feature。

无需窗口的回归测试：

```sh
cargo test -p minimal --no-default-features --features cpu --locked
```
