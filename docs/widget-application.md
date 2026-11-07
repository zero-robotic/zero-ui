# 控件窗口应用装配

`zui-app::WidgetHost<M, L, T>` 将应用状态、持久 Tree、布局服务和文字服务组合为窗口应用。它实现既有 `Application` 契约，可以直接交给 `zui_app::run`，选择 CPU 或 GPU 后端。

装配依赖方向保持单向：`zui-app` 使用 `zui-core` 与公共 API；核心不引用 app。Host 通过 `LayoutEngine` 和 `TextService` 调用服务，不引用 Taffy、Parley 或标准控件。示例在顶层注册字体、创建具体服务并传入，平台后端不知道应用状态或控件类型。

## 状态与帧流程

应用实现 `ViewApplication`：

- `view(&self)`：根据当前状态创建普通 View。
- `update(...)`：处理窗口生命周期和未被控件处理的输入，修改状态并返回 `ViewAction`。
- `Message`、`map_action` 和 `message`：定义应用消息类型，将控件动作映射为消息，再更新业务状态；无消息应用可使用 `Message = ()`。
- `background()`：提供帧背景；可从应用当前主题解析。
- `connected` / `disconnected`：可选生命周期回调。调用后 Host 标记描述需重建。

修改了影响 View 的状态时返回 `ViewAction::rebuild()`，需要退出时返回 `ViewAction::exit()`；定时唤醒通过 `ViewAction.window.wake_at` 请求。只返回窗口重绘不会自动重建 View。Host 已接入控件事件与类型化业务消息，完整异步命令执行仍待实现。

多个重建请求会合并到下一次需要几何的时点（绘制或控件输入）：先用当前状态构建描述并协调，随后按逻辑窗口尺寸布局和准备控件。输入在已提交的几何上派发，绘制再生成 Scene。这样不会在状态已变更后使用旧尺寸命中。纯配色变化保留布局；逻辑尺寸不变的 DPI 切换不重新排版，由渲染帧的 scale 负责物理像素缩放。隐藏或不可绘制时延后重建并丢弃输入，恢复可绘制后继续处理。

初始字体注册应在构造 Host 前完成。Host 拥有服务，当前不暴露可绕过失效流程的任意可变树访问；`model()` 与 `tree()` 提供只读检查。运行时字体注册、外部异步消息和完整应用命令接口仍待后续设计。

## 失败行为

`Application::draw` 返回 `Result<(), ApplicationError>`，`Application::update` 返回 `Result<WindowAction, ApplicationError>`；原有场景级应用需要分别返回 `Ok(())` 和 `Ok(action)`。输入前的协调/布局失败也通过 update 返回，不能在旧几何上继续派发。Host 只在协调、布局及准备全部成功后替换调用方 Scene。失败时保留调用方旧 Scene，但不代表内部所有控件缓存已经回滚；必须成功重试后才能继续使用新几何。

窗口驱动通过通用的 `PlatformError::Handler` 保留错误来源，不向平台层引入 Tree 或具体应用类型。失败帧不调用 presenter；目前窗口 runner 遇到该错误会结束运行并向调用方返回错误。离线调用 Host 时可以更新模型后重新绘制恢复，测试覆盖该路径。生产应用的错误页面与交互恢复策略尚未接入。

## 运行与验证

```sh
# 两条控件窗口路径；点击按钮或 Tab 聚焦后 Space/Enter 激活，Escape 退出。
cargo run -p zui-window-check --no-default-features --features cpu --locked -- cpu --widgets
cargo run -p zui-window-check --no-default-features --features gpu --locked -- gpu --widgets

# 自动在一半时间切换主题，到期关闭。
cargo run -p zui-window-check --all-features --locked -- cpu --widgets --smoke-ms 1600
```

示例使用真实 TextView、标准容器、主题、Taffy 和 Parley。窗口验证程序包含固定中文测试字体，不代表生产应用必须嵌入整份字体。

测试覆盖连续状态更新合并、主题切换保留节点并跳过排版、逻辑宽度变化重新换行、同逻辑尺寸的 DPI 变化、隐藏期间延迟工作、准备失败保留调用方场景及恢复，以及驱动拒绝提交失败帧。实际 CPU/GPU 窗口已验证打开、呈现、自动主题切换和退出；尚未通过自动化操作验证拖动缩放、真实按键、跨屏 DPI 和输入法交互。
