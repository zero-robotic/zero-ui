# 最小计数器

一个窗口、一段文字、一个按钮。完整应用代码位于 `src/main.rs`，没有剪贴板、定时任务或展厅功能依赖。

```sh
cargo run -p minimal --locked
cargo run -p minimal --no-default-features --features cpu --locked -- cpu
```

默认使用 GPU。点击“加一”更新计数；Tab 聚焦按钮，Space/Enter 激活，点击窗口关闭按钮退出。

阅读顺序：

1. `main` 发现并选择系统字体，装配布局引擎、文本服务和 `WidgetHost`，启动窗口。
2. `Counter::create` 创建页面作用域中的计数属性，注册一次类型化按钮事件。
3. `Counter::view` 读取计数属性，构建文字和按钮。
4. 事件处理器修改属性；`ReactiveApplication` 追踪读取并自动通知宿主刷新，不需要手动消息映射或重建标记。
5. `Component` 默认处理窗口关闭；需要额外窗口行为时覆盖 `update`。

状态与事件只创建一次，视图可以重复构建。完整机制与当前实施边界见 [应用状态与绑定](../../docs/state.md)。

系统字体由 `zui-text/system-fonts` 可选能力提供；没有中文字体的系统需先安装中文字体。复制到独立项目时将 workspace 依赖改为对应路径或版本，并保留该 feature。

无需窗口的回归测试：

```sh
cargo test -p minimal --no-default-features --features cpu --locked
```
