# zero-ui 架构与 surface 生命周期

> 当前契约基线：2026-09-15
> 产品与里程碑规划见 [roadmap.md](roadmap.md)，阶段任务见 [current-stage-tasks.md](current-stage-tasks.md)。

## 1. 分层和依赖方向

```text
application/example
       │
       ▼
    zui-app ──────── zui-ui
       │               │
       ├──────────── zui-render
       ├──────────── zui-render-runtime
       └──────────── zui-platform
                            ▲
                            │ implements public API / uses SPI
                  zui-backend-winit
                  zui-backend-headless
```

约束：

- 应用与 widgets 不得依赖 winit 或 `zui-platform::spi`；
- backend 可以依赖公开 platform API 和 SPI；
- renderer 可以依赖公开的 `SurfaceTarget`，但不得依赖 backend 类型或 platform SPI；
- wgpu device、queue、surface 和所有 GPU 资源由 renderer 持有；
- `zui-app` 只负责编排 platform 事件、UI 更新和 renderer 调用。

`zui-render-runtime` 当前保留为 renderer 选择和 app-facing adapter 层。它不得创建原生 target，也不得直接访问 platform SPI。

## 2. `SurfaceTarget` 契约

`zui-platform::SurfaceTarget` 是 backend 与 renderer 之间唯一的 surface attachment 契约：

- 类型公开、可克隆，不包含或暴露 winit 类型；
- native target 只能由 backend 通过 `zui-platform::spi::native_surface_target` 创建；
- native target 持有 `Arc<dyn NativeSurfaceSource>`，保证原生 handle source 至少与 renderer surface 同寿命；
- handle source 只实现标准 `raw-window-handle` 借用 trait；
- headless target 不包含原生 handle，可以通过 `SurfaceTarget::headless()` 创建；
- Host 通过 `Host::surface_target()` 把 target 借给 app；
- app 只能把 target 交给 renderer，不解析、不缓存原生句柄；
- renderer attach 后持有 target，并在 surface lost/outdated 时复用它完成恢复。

```text
winit Window ─Arc─┐
                  ├─ backend/SPI ─ SurfaceTarget ─ zui-app ─ renderer
headless marker ──┘                                  │
                                                    └─ retain for recover
```

renderer 使用 wgpu 的安全 surface 创建入口；不再接收裸句柄，也不再提供第二套 `attach_native_surface` API。

## 3. 所有权

| 对象/行为 | 唯一负责人 | 其他层职责 |
|---|---|---|
| 原生 Window 与事件循环 | backend | app 消费平台事件 |
| `SurfaceTarget` 创建 | backend/SPI | Host 返回克隆；app 透传 |
| 逻辑尺寸与 device scale 事件 | backend/platform | app 转换为 `SurfaceMetrics` |
| `wgpu::Surface` 创建和配置 | renderer | app 调用 attach/resize |
| surface acquire/present | renderer | app 根据 outcome 调度下一帧 |
| lost/outdated recovery | renderer | app 只调用 `recover_surface(metrics)` |
| retained scene 和 GPU cache | renderer | UI 提交 scene revision/update |
| detach 顺序 | app 调度、renderer 执行 | backend 随后销毁 Window |

app 可以保存 `pending_resize` 和 `pending_present` 作为事件调度状态，但不得复制 renderer 的 surface phase，也不得通过 detach+attach 实现恢复。

## 4. surface 状态机

每个已 attach 的 surface 都持有一个 `SurfaceLifecycle`。公开查询使用 `surface_phase(window)`；不存在的 window 返回 `Detached`。

```text
Detached
   │ attach(target, metrics)
   ▼
Attached
   │ resize(metrics)
   ▼
Resized ───────────────┐
   │ acquire/present   │ acquire timeout/occluded
   ▼                   ▼
Presented ◀──────── Deferred
   │                   │
   └──── lost/outdated ┘
             │
             ▼
            Lost
             │ recover_surface(metrics)
             ▼
          Recovered
             │ retry present
             ▼
          Presented

任意 attached phase ── detach_surface() ──▶ Detached
```

状态语义：

- `Detached`：renderer 没有该 window 的 surface；
- `Attached`：target 已接受，surface 和初始资源已创建；
- `Resized`：最新物理尺寸和 scale 已接受，下一帧必须完整呈现；
- `Deferred(reason)`：scene 已接受但未 present，必须保留 scene 等待重试；
- `Lost`：acquire 报告 lost/outdated，只允许 recover 或 detach；
- `Recovered`：renderer 已使用原 target 重建 surface，下一帧必须完整呈现；
- `Presented`：当前接受的 scene 已成功 present。

零物理尺寸也是一次有效 resize。renderer 将其转换为 `Deferred(Occluded)`，不会继续使用旧尺寸 present；恢复到非零尺寸后重新进入 `Resized`。

## 5. 调用顺序

### 首帧

1. backend 创建 Host；
2. app 从 `Host::surface_target()` 取得 target；
3. app 调用 `renderer.attach_surface(window, target, metrics)`；
4. UI layout/paint 并提交 scene；
5. renderer acquire、render、present，phase 进入 `Presented`。

### resize/scale

1. backend 更新 Host 的逻辑尺寸和 device scale；
2. 逻辑尺寸变化发出 `WindowResized`，device scale 变化单独发出 `ScaleFactorChanged`；
3. app 更新 layout，并调用 `renderer.resize`；
4. renderer 丢弃与旧物理尺寸相关的 cache，phase 进入 `Resized`；
5. 下一次 present 使用完整帧。

### deferred

1. renderer 接受最新 scene；
2. acquire 返回 timeout/occluded，phase 进入 `Deferred`；
3. UI scene 标记为 clean，但 app 保持 `pending_present`；
4. timeout 使用有限延迟重试；occluded 等待窗口恢复事件；
5. 重试复用同一 scene revision，成功后进入 `Presented`。

### lost/outdated

1. renderer 在 acquire 时把 phase 置为 `Lost` 并返回结构化错误；
2. app 调用 `renderer.recover_surface(window, current_metrics)`；
3. renderer 使用已持有的 target 重建 surface，phase 进入 `Recovered`；
4. app 请求 UI 完整帧；
5. 成功 present 后进入 `Presented`。

## 6. 契约测试

`zui-render` 的 `test-support` feature 提供唯一的状态机契约套件。winit 与 headless backend 的单元测试调用同一个套件，固定验证：

```text
Detached → Attached → Resized → Deferred
         → Lost → Recovered → Presented → Detached
```

除此之外：

- winit 测试验证生产 target 工厂产生 `Native` target，且不需要打开 GUI 窗口；
- headless 测试验证 `HeadlessHost` 产生 `Headless` target，并覆盖双 Host 创建/驱动/销毁、Paths、Output/scale、capability 缺失和 UI 线程任务；
- render-runtime 测试通过真实 `HeadlessRenderer` API 验证 attach、resize、零尺寸 deferred、recover、present 和 detach；
- app 测试验证 scale 变化触发 layout/surface resize、语义树提交、deferred scene revision 复用，以及 lost 后调用 recover 而不是 detach+attach。

平台 core v0、capability/experimental 分层、多窗口事件顺序与兼容性规则详见 [platform-api.md](platform-api.md)。

相关验证命令：

```bash
cargo test -p zui-backend-winit -p zui-backend-headless
cargo test -p zui-render-runtime -p zui-app
cargo test --workspace
```
