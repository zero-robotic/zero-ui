# `zui-platform` API 契约

> 对应实现：`crates/zui-platform`。本文描述 `0.1.x` 的 **core v0**，以及当前 capability / experimental 扩展点。

## 1. 分层与成熟度

`zui-platform` 的应用可见入口只有公开 API；操作系统与 winit 细节只能出现在 backend。

| 层级 | Rust 入口 | 兼容性承诺 |
|---|---|---|
| core v0 | `zui_platform::core`，同时在 crate 根重导出 | 应用窗闭环的必需面；破坏性修改需要迁移说明，并在 `0.1.x` 中明确记录 |
| capability | `zui_platform::capability` | 可选能力；允许增加方法，删除或改签名必须记录版本影响 |
| experimental | `zui_platform::experimental` | 尚未冻结，可在小版本中调整；成熟后再提升为 capability |
| backend SPI | `zui_platform::spi` | 仅 `zui-backend-*` 使用，不是应用 API，不承诺兼容 |

`CORE_API_VERSION` 当前为 `0`。在进入 core v1 前，`AppContext`、`AppLoop`、`Host`、事件与调度规则共同构成 core v0，不单独冻结某一个 trait。

### 从早期单 Host API 迁移

本次 core v0 是一次显式破坏性收敛：

- `AppLoop<H>` 改为对象安全的 `AppLoop`，删除 `host_ready`；
- `event(&H, event)` 改为 `event(&mut dyn AppContext, event)`；
- 在 `WindowCreated(id)` 中通过 `context.host(id)` attach surface；
- `LoopControl::RequestRedraw` 和 `WaitUntil` 增加目标 `WindowId`；
- 原 `WindowResized { size, scale_factor }` 拆为 `WindowResized` 与 `ScaleFactorChanged`。

后续 core v0 的破坏性修改必须在本节增加迁移项；提升到 core v1 后，按 roadmap 要求走 RFC 和主版本或明确迁移期。

## 2. Core v0

### 2.1 Backend、AppLoop 与 AppContext

```rust
pub trait Backend {
    fn run(self, initial_window: WindowOptions, app: &mut dyn AppLoop)
        -> Result<(), PlatformError>;
}

pub trait AppLoop {
    fn event(&mut self, context: &mut dyn AppContext, event: PlatformEvent)
        -> LoopControl;
}
```

backend 拥有原生事件循环和 Host。应用只在事件回调期间通过 `AppContext` 受控访问：

- `host(window)`：按稳定 `WindowId` 查询活跃 Host；
- `window_ids()`：取得当前所有活跃窗口的快照；
- `create_window(options)`：在事件循环线程创建窗口；
- `destroy_window(window)`：销毁指定窗口；未知 ID 返回 `PlatformError::UnknownWindow`；
- `paths()`、`outputs()`：读取平台路径与显示输出快照；
- `capabilities()`：类型化查询可选能力；
- `ui_task_poster()`：取得可跨线程克隆的 UI 任务投递器。

`WindowCreated(id)` 发出时，`context.host(id)` 必须已经可用。`WindowDestroyed(id)` 发出时 Host 已移除。收到 `CloseRequested(id)` 后，backend 在回调结束时销毁仍存活的该窗口；应用也可以在回调中显式销毁。

### 2.2 Host 与 surface target

`Host` 提供窗口 ID、逻辑尺寸、device scale、重绘请求和不透明 `SurfaceTarget`。应用及 widget 不接触 winit 或 OS handle；renderer 只消费 `SurfaceTarget`。原生 target 的构造函数位于 `spi`，仅 backend 可调用。

生命周期顺序为：

```text
create Host → WindowCreated → attach surface → resize/scale → redraw
            → CloseRequested → detach surface → destroy Host → WindowDestroyed
```

### 2.3 事件与坐标

布局、指针、Output 区域和窗口尺寸统一使用 dip。只有 renderer 根据 `ScaleFactor` 计算物理像素。

| 事件 | 含义 |
|---|---|
| `WindowResized { window, size }` | 逻辑尺寸改变，scale 未隐含在事件中 |
| `ScaleFactorChanged { window, size, scale_factor }` | device scale 改变，同时携带该 scale 下的最新逻辑尺寸 |
| `OutputsChanged(Vec<Output>)` | 完整输出快照替换旧快照 |
| `Input { window, event }` | 指针、滚轮、键盘、文本或 IME 输入 |
| `RedrawRequested(window)` | 指定窗口应提交一帧 |
| `WindowOccluded` | 可见性变化；恢复时可重试 retained frame |
| `DragDrop` / `DialogCompleted` | 可选能力的异步结果或输入 |
| `AboutToWait` | backend 即将休眠，可用于汇总调度 |

resize 与 scale 是两个独立事件，调用方不得通过尺寸变化猜测 scale。`zui-app` 对两者都会重新 layout，并以 `logical size × device scale` 更新 renderer surface。

### 2.4 调度与退出

`LoopControl` 的重绘与 deadline 都显式带 `WindowId`：

```rust
Continue
RequestRedraw(WindowId)
WaitUntil { window: WindowId, deadline: Instant }
Exit
```

这保证多窗口下不会把 animation deadline 错投给“当前窗口”。`LoopControl::from_redraw_deadline(window, deadline)` 是控件动画的标准转换入口。对已销毁窗口请求调度返回结构化错误或终止 backend，不允许静默改投其他窗口。

### 2.5 Paths

`Paths` 返回用户级基础目录：`config_dir`、`data_dir`、`cache_dir` 与可选 `runtime_dir`。它不自动拼接应用名称，也不自动创建目录。

- Windows：`APPDATA` / `LOCALAPPDATA`；
- macOS：`~/Library/Application Support` 与 `~/Library/Caches`；
- Unix/FreeBSD：XDG 变量，未设置时回退到 `$HOME/.config`、`$HOME/.local/share`、`$HOME/.cache`；runtime 仅在 `XDG_RUNTIME_DIR` 存在时返回；
- headless：默认使用 `/headless/*` 的确定性路径，可注入成功值或结构化错误。

缺少必需环境变量时返回 `PlatformError::PathUnavailable { variable, reason }`，不返回空路径。

### 2.6 Output

`Output` 至少包含：

- 稳定于当前快照的 `OutputId`；
- 可选名称；
- dip 表示的 `logical_bounds`；
- `scale_factor`。

`context.outputs()` 是当前权威快照；`OutputsChanged` 携带替换后的完整值。色域、刷新率和 HDR 不属于 core v0。

## 3. Capability

`Capabilities` 是类型化注入与查询容器。backend 可用 `with_clipboard`、`with_dialog`、`with_drag_drop`、`with_ime`、`with_accessibility` 注入实现；应用通过同名查询方法取得 `Option<&mut dyn Trait>`。

`None` 是“不支持”的唯一正常表达，不允许以 panic、空成功或虚假默认值代替。能力执行失败使用 `PlatformError::Capability`。

当前草案：

- `Clipboard`：读取/写入文本；
- `Dialog`：发起非阻塞请求，随后通过 `DialogCompleted` 返回结果；
- `DragDrop`：按窗口启停，数据通过 `DragDropEvent` 到达应用。

实现矩阵：

| backend | Clipboard | Dialog | DragDrop capability | IME | Accessibility |
|---|---:|---:|---:|---:|---:|
| headless 默认 | — | — | — | — | — |
| headless 注入 | 可测试实现 | 可测试实现 | 可测试实现 | 可测试实现 | 可测试实现 |
| winit | — | — | 原生文件事件始终转发，开关 trait 未实现 | 是 | — |

## 4. Experimental

### 4.1 IME

`Ime` capability 控制指定窗口的 enabled 状态和候选区 `cursor_area`。输入状态经 `InputEvent::Ime` 到达：

```text
Enabled → Preedit* → Commit
                   ↘ Cancelled
Disabled 会先结束仍活跃的组合态
```

`Preedit` 文本不是已提交内容。`TextInput` 保留并绘制 composition；`Commit` 才修改正式文本；`Cancelled` 或 `Disabled` 清除 composition。winit backend 不再把 commit 降级成普通 `Text`，并明确转发 enabled、preedit、commit 和 cancel。

### 4.2 Accessibility

`Accessibility::submit_tree` 接收平台无关的 `AccessibilityTree`，包含窗口、角色、标签、启用状态、逻辑 bounds 和子节点。`zui-app` 在绘制更新时从 `WidgetTree` 生成语义树；backend 可注入具体平台桥。当前节点 schema 与提交频率仍属 experimental。

### 4.3 UI 线程任务

`UiTaskPoster` 可安全克隆到后台线程，`post` 的闭包由 backend 事件循环线程执行。闭包必须短小；重计算仍应在后台完成。事件循环关闭后返回 `PlatformError::EventLoopClosed`。

## 5. Backend 一致性与验证

headless 与 winit 实现同一 `Backend` / `AppContext` 契约。headless 用于确定性验证多窗口、事件顺序、Output/scale、Paths、capability 缺失和任务投递；winit 负责原生窗口、monitor、IME 与文件拖放映射。

应用业务代码只依赖 `zui-app`、`zui-ui` 和公开 `zui-platform` 类型。切换 backend 只发生在 Cargo feature 和组装根，不允许业务代码依赖 `winit`、OS crate 或 `zui_platform::spi`。

主要验证命令：

```bash
cargo test -p zui-platform -p zui-backend-headless -p zui-backend-winit
cargo test -p zui-ui -p zui-app
cargo test --workspace
```
