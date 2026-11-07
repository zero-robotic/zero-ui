# 桌面控件呈现

控件展厅默认使用 `Theme::desktop_dark()`，切换主题后使用 `Theme::desktop_light()`。原 `dark/light/dashboard` 主题保留原有字号和密度，已有应用可自行选择迁移。

桌面主题使用随应用注册的字体，正文 14、辅助文字 12、标题 20、展示文字 28 个逻辑像素。间距为 4/8/12/16/24；`ControlMetrics` 统一提供控件圆角 6、面板圆角 8、最小内容高度 28、按钮横向留白 8。布局外加 padding 会增加最终高度；显式尺寸优先，紧凑布局应设置最终高度并为文字预留足够空间。`Theme::set_controls` 支持应用级调整。

桌面主题的文本输入框默认内部留白为左右 8、上下 4 个逻辑像素，通过 `TextInputStyle::padding_x/padding_y` 调整；它会叠加布局 padding，文字、占位提示、选区、鼠标定位和输入法候选区域共享同一内容边界。

桌面主题的 `ButtonView::themed` 默认使用次按钮样式。重要操作显式指定 `ButtonStyle::primary(theme)`，辅助操作可指定 `ButtonStyle::text(theme)`。列表、树和分段控件使用不透明的柔和选中背景；`selection_background/selection_foreground` 成对使用。下拉弹层有完整背景、内边距、细边框和圆角；未增加模糊阴影或持续动画。

`Container::radius` 和 `border` 绘制表面，不裁剪子节点为圆角。需要给内容保留内边距，防止其覆盖边框和圆角。

树节点 `double_click_interval` 默认 500 毫秒，平台层可传入系统偏好，设为零可关闭文字双击展开。当前没有自动读取 Windows/macOS 双击设置。

## 验证与预览

```sh
cargo run -p zui-render-check -- cpu artifacts/desktop-dark.png desktop-dark
cargo run -p zui-render-check -- cpu artifacts/desktop-light.png desktop-light
cargo test -p zui-render-check --test desktop
cargo test -p zui-render-check --features gpu --test desktop
cargo test -p zui-controls-example
```

桌面样张覆盖按钮层级、输入框、下拉按钮、分组面板和树节点；测试覆盖深浅主题正文/辅助文字/选中态对比度，以及 1/1.25/1.5/2 倍离屏渲染的圆角和背景。沿用同一字体资源与场景绘制路径，不依赖系统默认字体。离屏测试不等同于 Windows/macOS 的窗口、系统缩放、输入法或显示器色彩实机验收。

弹窗标题/关闭按钮、系统窗口装饰和自绘标题栏接入方式见 [弹窗与窗口装饰](dialog-window-chrome.md)。

桌面列表和 Tab 标签采用上下 4 的留白；单选、复选指示器为 16。分页按钮不再叠加额外布局留白。正文仍为 14，避免靠缩小文字压缩界面。CPU/GPU 文字绘制均开启字体 hinting，帮助小字号笔画对齐设备像素；实际效果取决于字体 hint 数据与显示缩放，不保证不同平台逐像素一致。
