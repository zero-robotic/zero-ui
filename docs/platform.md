# 应用装配、平台与可移植性

窗口示例默认选择 GPU，省略后端时也可直接传入窗口选项；传入 `cpu` 显式使用 CPU。默认构建启用两种后端，纯 CPU 构建使用 `--no-default-features --features cpu` 并显式传入 `cpu`。未编译的后端或 GPU 初始化失败会报告错误，不静默回退。

[文档索引](README.md) · [模块边界](module-boundaries.md) · [未完成事项](implementation-plan.md)

按架构职责归并的专题参考。设计目标与接口约束分开理解；待办统一维护在实施计划中，各节的验证说明仅覆盖所述范围。

## 章节导航

- [控件窗口应用装配](#widget-application)
- [MCU 扩展验证](#mcu-portability)

<a id="widget-application"></a>


<a id="widget-application--控件窗口应用装配"></a>

## 控件窗口应用装配

`zui-app::WidgetHost<M, L, T>` 将应用状态、持久 Tree、布局服务和文字服务组合为窗口应用。它实现既有 `Application` 契约，可以直接交给 `zui_app::run`，选择 CPU 或 GPU 后端。

装配依赖方向保持单向：`zui-app` 使用 `zui-core` 与公共 API；核心不引用 app。Host 通过 `LayoutEngine` 和 `TextService` 调用服务，不引用 Taffy、Parley 或标准控件。示例在顶层注册字体、创建具体服务并传入，平台后端不知道应用状态或控件类型。


<a id="widget-application--状态与帧流程"></a>

### 状态与帧流程

应用实现 `ViewApplication`：

- `view(&self)`：根据当前状态创建普通 View。
- `update(...)`：处理窗口生命周期和未被控件处理的输入，修改状态并返回 `ViewAction`。
- `Message`、`map_action` 和 `message`：定义应用消息类型，将控件动作映射为消息，再更新业务状态；无消息应用可使用 `Message = ()`。
- `background()`：提供帧背景；可从应用当前主题解析。
- `connected` / `disconnected`：可选生命周期回调。调用后 Host 标记描述需重建。

修改了影响 View 的状态时返回 `ViewAction::rebuild()`，需要退出时返回 `ViewAction::exit()`；定时唤醒通过 `ViewAction.window.wake_at` 请求。只返回窗口重绘不会自动重建 View。Host 已接入控件事件与类型化业务消息，完整异步命令执行仍待实现。

多个重建请求会合并到下一次需要几何的时点（绘制或控件输入）：先用当前状态构建描述并协调，随后按逻辑窗口尺寸布局和准备控件。输入在已提交的几何上派发，绘制再生成 Scene。这样不会在状态已变更后使用旧尺寸命中。纯配色变化保留布局；逻辑尺寸不变的 DPI 切换不重新排版，由渲染帧的 scale 负责物理像素缩放。隐藏或不可绘制时延后重建并丢弃输入，恢复可绘制后继续处理。

初始字体注册应在构造 Host 前完成。Host 拥有服务，当前不暴露可绕过失效流程的任意可变树访问；`model()` 与 `tree()` 提供只读检查。运行时字体注册、外部异步消息和完整应用命令接口仍待后续设计。


<a id="widget-application--失败行为"></a>

### 失败行为

`Application::draw` 返回 `Result<(), ApplicationError>`，`Application::update` 返回 `Result<WindowAction, ApplicationError>`；原有场景级应用需要分别返回 `Ok(())` 和 `Ok(action)`。输入前的协调/布局失败也通过 update 返回，不能在旧几何上继续派发。Host 只在协调、布局及准备全部成功后替换调用方 Scene。失败时保留调用方旧 Scene，但不代表内部所有控件缓存已经回滚；必须成功重试后才能继续使用新几何。

窗口驱动通过通用的 `PlatformError::Handler` 保留错误来源，不向平台层引入 Tree 或具体应用类型。失败帧不调用 presenter；目前窗口 runner 遇到该错误会结束运行并向调用方返回错误。离线调用 Host 时可以更新模型后重新绘制恢复，测试覆盖该路径。生产应用的错误页面与交互恢复策略尚未接入。


<a id="widget-application--运行与验证"></a>

### 运行与验证

```sh
# 两条控件窗口路径；点击按钮或 Tab 聚焦后 Space/Enter 激活，Escape 退出。
cargo run -p zui-window-check --no-default-features --features cpu --locked -- cpu --widgets
cargo run -p zui-window-check --no-default-features --features gpu --locked -- gpu --widgets

# 自动在一半时间切换主题，到期关闭。
cargo run -p zui-window-check --all-features --locked -- cpu --widgets --smoke-ms 1600
```

示例使用真实 TextView、标准容器、主题、Taffy 和 Parley。窗口验证程序包含固定中文测试字体，不代表生产应用必须嵌入整份字体。

测试覆盖连续状态更新合并、主题切换保留节点并跳过排版、逻辑宽度变化重新换行、同逻辑尺寸的 DPI 变化、隐藏期间延迟工作、准备失败保留调用方场景及恢复，以及驱动拒绝提交失败帧。实际 CPU/GPU 窗口已验证打开、呈现、自动主题切换和退出；尚未通过自动化操作验证拖动缩放、真实按键、跨屏 DPI 和输入法交互。


<a id="widget-application--刷新计时与批量更新"></a>

### 刷新计时与批量更新

`host.set_profiling(true)` 启用可选计时；`refresh_metrics()` 读取累计值，`take_refresh_metrics()` 取走并清零，保持启用状态。阶段包括 View、Reconcile、Layout、Input、Animation、Scene、Present，记录调用次数、累计耗时和单次最大耗时。默认关闭，不读取系统时钟。原生驱动自动记录 Present 的渲染与提交 CPU 墙钟耗时，不代表 GPU 完成时间。

`Application::update_batch(events, metrics, elapsed)` 按顺序执行批内事件，合并 redraw/exit 并保留最早 wake_at；退出后不再处理剩余事件。默认实现逐条调用 update，WidgetHost 对连续 Wake 只推进一次动画并更新一次 IME。所有模型 update 仍执行；遇到输入、焦点或窗口生命周期事件，先完成待处理工作，再走原有单事件路径。动画按 elapsed 时间计算，不应依赖 Wake 数量。

该 API 适用于已有队列的上层调用方；当前原生 Winit 驱动仍逐条传递事件，不会自动批处理系统输入。批内共用 metrics/elapsed，跨不同窗口尺寸或时间快照的事件应拆批。已有的 View 重建请求合并逻辑继续保留，新增优化针对重复的动画与 IME 收尾，不把已有功能计作新增收益。

CPU 窗口使用 `RetainedCpuRenderer` 保存像素和场景；命令及尺寸/缩放/背景完全不变时复用像素，仍向窗口提交画面。变化帧按损伤区域更新，超出预算或区域阈值时回退整帧绘制，详见 [区域绘制](rendering.md#region-rendering)。


<a id="widget-application--使用实际控件边界定位弹层"></a>

### 使用实际控件边界定位弹层

需要从动作来源读取几何时，实现 `ViewApplication::map_action_with_tree(action, tree)`；默认实现继续调用 `map_action`，既有应用无需迁移。该回调只获得只读 Tree，不允许绕过 Host 修改树。`Tree::presented_bounds(action.source)` 返回已经提交、包含祖先偏移及滚动变换的场景坐标边框盒；布局尚未提交时返回错误，结果不会按祖先裁剪区域截断。

Popup 的 Anchor 使用其覆盖层容器局部坐标，不能直接使用控件的父级布局坐标，也不能写死屏幕位置。示例中的普通 Select 和增强 ComboBox 在 Open 动作映射时捕获实际按钮边界，并转换为根覆盖层坐标。窗口 Resized/ScaleChanged 时关闭这些锚定弹层，下次打开重新读取边界；本方案不表示弹层会持续跟踪任意业务布局变化。

<a id="mcu-portability"></a>


<a id="mcu-portability--mcu-扩展验证"></a>

## MCU 扩展验证

首期产品仍使用 Vello CPU/GPU。MCU 扩展分开验证可移植库、固件装配与设备运行，避免把 `no_std` 声明或宿主机编译当作 MCU 支持完成。


<a id="mcu-portability--可重复的裸机库编译"></a>

### 可重复的裸机库编译

```sh
rustup target add thumbv7em-none-eabihf
python3 tools/check-mcu.py
```

脚本从仓库根目录调用 `cargo build --lib --target ... --no-default-features --locked --offline`，包含代码生成而不仅是类型检查；固定验证以下 12 个 crate：

- 基础契约：types、input、paint、theme、api、platform。
- 状态与运行时：edit、virtual、core。
- 控件与组合：widgets、charts、dashboard。

2026-10-28 在当前开发机上，以上库通过 `thumbv7em-none-eabihf`（Cortex-M4F/M7F）检查与构建。无需修改实现，也未引入反向依赖。除 types 等纯基础库外，相关代码依赖 `alloc`；固件仍须提供分配器。

脚本不自动安装目标、不联网解析依赖、不回退宿主目标、不跳过失败库。缺少目标时先安装目标；缺少缓存时先准备锁定版本的依赖。可通过 `--target` 选择脚本列出的其他裸机目标；目前仅上述 Cortex-M 硬浮点目标经过实际验证，其他目标选项不代表已通过。


<a id="mcu-portability--证据范围"></a>

### 证据范围

当前结果证明这组库及其实际依赖可为该无操作系统目标生成库产物。它不证明完整应用能链接、启动，或满足设备内存和实时性要求。泛型应用路径仍需固件实例化与最终链接验证。

Vello 渲染器、Parley 文本、Taffy 布局、桌面窗口及应用装配不在本检查集合内；不会为了通过检查将这些完整功能假装纳入 MCU 配置。无 GPU 嵌入式 Linux 的 Vello CPU 路径是另一条验证路线。

后续需要完成：

1. MCU 装配示例：启动代码、链接脚本、分配器和 panic 策略，形成可链接固件。
2. 在公共绘制、布局和文字契约下实现受限能力适配；由装配层注入，底层不引用具体应用或控件。
3. 显示刷新、输入、时钟等设备适配，以及硬件或模拟器运行验收。
4. 量化 RAM/Flash、峰值分配、帧缓冲策略、绘制与输入延迟，并明确哪些控件和效果可用。

这些工作完成前，项目只声明“保留 MCU 扩展能力，指定可移植库已通过裸机编译”。

## 跨显示器与渲染尺寸限制

窗口与控件使用逻辑尺寸；平台提供实际物理尺寸和缩放比例，渲染目标直接使用物理尺寸，绘制坐标只缩放一次。跨屏拖动时原生窗口的位置与 DPI 尺寸调整由 winit 负责，业务层不需要再次调用 `request_size` 补偿缩放。

当前精确锁定注册表版本 `winit = "=0.31.0-beta.4"`，使用已合入主线的 [PR #4341](https://github.com/rust-windowing/winit/pull/4341) 修复。该版本的 Windows 后端在 Windows 11 上直接采用 `WM_DPICHANGED` 提供的建议矩形，避免旧版跨屏位置修正引发连续 DPI 变化与窗口放大；Windows 10 的兼容处理仍由上游维护。不再依赖尚未合并的 0.30 补丁，也不跟随浮动的 master。此版本仍为预发布版，升级需固定版本并重新验证。

0.31 的 API 变化集中适配在 `zui-backend-winit`：

- 使用 `can_create_surfaces` / `destroy_surfaces` 管理窗口及呈现资源；桌面 `run_app_on_demand` 保留同步返回和借用 handler 的现有接口，返回后释放呈现资源，再释放原生窗口。
- `EventLoopProxy::wake_up` 对接原有 `ExternalWake`；允许合并唤醒，业务任务仍从队列取出。
- `surface_size` 提供物理渲染尺寸；统一指针事件转换为原有鼠标/触摸事件，点击使用事件自身坐标，触摸离开按释放或取消分别处理。
- 通过 `request_ime_update` 启用/关闭 IME 并更新逻辑坐标下的候选窗口区域；只声明现有接口支持的能力。空格键与 Meta 修饰键映射到原有 ZUI 键盘协议。

布局、控件、渲染与应用公共接口保持不变。ZUI 不增加重复的 DPI 尺寸状态机，也不通过跳帧或提高像素预算掩盖无限放大。

可设置环境变量 `ZUI_TRACE_WINDOW=1`，记录尺寸变化、DPI 变化与重绘时的物理/逻辑尺寸。该诊断默认关闭。Windows 手动验收应使用 3840×2160 / 150% 与 3072×1920 / 200% 双屏，分别从标题栏左侧和中间往返拖动普通窗口、最大化后拖动还原的窗口，确认无持续增长、闪退，且文字、鼠标命中与输入法位置正确；另外检查缩放设置改变和最大化/还原。自动化 Windows CI 只覆盖构建与单元测试，不代替双屏交互验收。

### 应用升级与验证

通过 Git 引用 ZUI 的应用，需要先将各个 ZUI 依赖的 `rev` 统一更新到包含此次升级的提交，再重新解析并提交应用自己的 `Cargo.lock`。只更新 ZUI 仓库内的锁文件不会更新其他应用；固定旧 `rev` 时重复运行 `cargo run --locked` 仍使用旧实现。应用无需自行添加 winit 补丁或改写业务接口。

在 ZUI 仓库运行以下命令可验证后端、应用适配与原生窗口：

```sh
cargo test -p zui-backend-winit -p zui-app -p minimal -p settings --all-features --locked
cargo run -p settings --no-default-features --features cpu --locked -- cpu --smoke
cargo run -p settings --no-default-features --features gpu --locked -- gpu --smoke
```

冒烟测试验证窗口启动、后台完成事件唤醒和自动退出，不验证真实双屏拖动或人工输入法交互。本次升级已通过上述 macOS 验证、Windows 后端及 GPU 应用交叉编译、依赖边界检查和工作区 Clippy；Windows 双屏交互仍待实机验收。

Windows PowerShell 中可开启日志后手动拖动示例窗口，关闭窗口后取消日志开关：

```powershell
$env:ZUI_TRACE_WINDOW = "1"
cargo run -p settings --locked -- gpu 2> window-trace.log
Remove-Item Env:ZUI_TRACE_WINDOW
```

## 可观察组件与后台结果

`zui-app::reactive::ReactiveApplication` 是既有 ViewApplication 的可选适配器，提供属性观察、类型化事件与区域更新，不替换 WidgetHost。新应用可从 [应用状态与绑定](state.md) 和 settings 示例开始；已有消息应用继续使用原接口。

NativeWindow 的可选 `event_waker` 返回线程安全 EventWaker。桌面实现产生 ExternalWake，驱动更新而不是直接绘制；应用任务完成队列在 UI 线程消费。无此能力的平台可以主动提交事件处理队列，不需要引入线程或具体任务库到核心。
