# 弹窗标题与系统窗口装饰

应用内弹窗和独立系统窗口分别配置。`DialogView` / `MessageBoxView` 是同一个窗口里的模态弹层，不创建独立操作系统窗口，也没有最小化/最大化语义。

## 应用内弹窗

```rust
let mut dialog = DialogView::themed(theme, "设置", families, dismiss_action);
dialog.show_title = true;
dialog.show_close_button = true;
let view = dialog.build(body, footer);
```

`show_title` 和 `show_close_button` 独立控制，可组成“标题+关闭”“仅标题”“仅关闭”“都隐藏”四种布局。关闭按钮发送 `PopupDismissed { reason: DismissReason::CloseButton }` 到 `dismiss`，不会发送确认结果。Escape 仍沿用原来的取消事件，外部点击由 `close_on_outside` 决定。隐藏关闭按钮不表示禁止取消。

为兼容原应用的初始焦点，默认仍为显示标题、不显示关闭按钮。启用关闭按钮后，该按钮参与 Tab 导航。正文在独立区域滚动，标题与操作区保持固定；正文滚动容器自身不占 Tab 停靠点，内部输入控件仍可获取焦点。`MessageBoxView` 可以通过 `message_box.dialog` 设置同样的选项。

## 独立系统窗口

```rust
let config = WindowConfig {
    decorations: true,
    close_button: true,
    minimize_button: false,
    maximize_button: false,
    resizable: false,
    ..Default::default()
};
```

默认保留系统标题栏和三个按钮。`decorations=false` 关闭系统窗口装饰，可用于无标题栏窗口或应用绘制自己的标题栏。后端不自动生成自绘标题栏；应用用普通 View 组合标题和按钮，并在动作处理时调用 `NativeWindow::drag_window`、`set_minimized`、`set_maximized`。拖动必须由标题区域的鼠标按下触发。关闭自绘窗口走现有 `WindowAction::exit()` / `ViewAction::exit()`。不支持这些能力的后端返回 `PlatformError::Unsupported`。

`resizable=false` 时后端同时禁用最大化按钮。按钮设置是原生能力请求：Windows/macOS 的按钮可能表现为隐藏或禁用，不保证完全相同；其他平台可能不支持。按钮开关不拦截系统关闭请求，不作为退出权限控制。

控件展厅可直接预览系统装饰设置：

```sh
cargo run -p zui-controls-example -- cpu --no-minimize --no-maximize
cargo run -p zui-controls-example -- cpu --no-titlebar
```

还支持 `--no-close`。无系统标题栏的展厅可按 Escape 退出；该参数用于验证无边框窗口，不会额外创建自绘标题栏。

验证：`cargo test -p zui-backend-winit`、`cargo test -p zui-controls-example`、`cargo test -p zui-render-check --test p0 --test scroll`。Windows 的实际窗口按钮外观需在 Windows 实机验收。
