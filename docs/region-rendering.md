# 子树绘制缓存与区域重绘

设计参考 [BUI 的理念](https://github.com/ppmpreetham/BUI#2-why-bui-exists)：只处理变化，空闲时等待输入或定时器。本实现保留 ZUI 的 Widget → Scene → CPU/GPU 分层，不引入 BUI 或 Blender 的运行时依赖。

## 两层复用

1. `Tree::scene()` 缓存子树的普通绘制命令。事件、焦点、动画、属性更新、子内容显露和托管子节点回调返回失效时，节点及其祖先的缓存失效。其他子树跳过绘制回调。布局提交只使几何变化或显式失效的节点失效；字体 epoch 变化和显式全局失效清空缓存。增删、重排、换父节点和 clip 变化均参与失效。
2. `RetainedCpuRenderer` 与 GPU 原生呈现比较前后绘制描述，计算变化区域。保留未变化像素，在变化区域重新绘制背景及所有与之相交的内容，再替换该区域。不会把新的半透明图形再次叠加到旧像素上。

`Scene` 的命令存储采用不可变共享、写时复制。外部保留的旧 Scene 不会被后续更新修改。渲染器仍然不持有 Widget 或节点引用。

## 区域计算与正确性

- 比较实际命令与有效裁剪栈，不以哈希相等作为正确性依据。相同帧参数、相同命令直接复用。
- 同时包含旧位置与新位置，覆盖移动、删除、裁剪变化和重排；数量变化时保留公共前后缀，只计算中间变化部分。
- 曲线路径按保守边界处理；描边包含斜接扩展，文字使用字体字形边界并留出 hinting/抗锯齿余量，不能确定字形范围时退回整个目标范围。图像使用完整变换后的图像边界。
- 损伤区域按物理像素扩展到 64×32 网格，合并相交或相接区域；默认最多 24 个区域，超限或总面积超过整帧 60% 时回退整帧。
- scale、物理尺寸或背景变化会触发整帧更新。失败使缓存键失效，下一次有效调用重新绘制；输入校验和资源预算检查不会被缓存绕过。
- 无变化时不执行栅格化。不可见变化也可以得到空区域列表，但会更新比较基线，随后进入视口时仍能正确重绘。

区域绘制使用整数物理偏移，保留分数缩放和文字网格对齐。复杂透明叠加及分数坐标下，Vello 在不同目标尺寸/分块路径中可能产生单通道 1/255 的舍入差异；回归测试对这类场景逐通道验证误差不超过 1，简单整数矩形与最终 GPU 纹理复制要求完全相等。区域每次从背景重新合成，误差不会通过反复叠加累计。

## CPU 与 GPU

CPU 窗口默认使用 `RetainedCpuRenderer`。它保留输出 PixelBuffer，局部区域使用临时缓冲，逐行比较并替换变化的像素。`CpuRenderer` 保留完整绘制接口，作为无缓存路径及像素对照基准。CPU 系统呈现当前仍提交完整 PixelBuffer，不宣称只向系统传输脏区域。

GPU 的 `render_to_view()` 默认保留一张 GPU 纹理。局部区域在 GPU 临时纹理中绘制，并复制到保留纹理的对应位置；最后通过无混合的纹理复制绘制到本次获得的目标。该路径不回读 CPU，也不假设交换链目标保留上一帧。多个区域按队列顺序提交，避免 Vello 的共享工作缓冲在提交前被后续区域覆盖。

`zui-render-gpu/profiling` 是默认关闭的诊断特性；启用后可调用 `set_gpu_timing_enabled(true)`，在 `render_to_view` 后通过 `take_gpu_time()` 等待并读取提交区间总时长。每次最多累积 64 次渲染提交，须逐帧读取；读取后计数归零。不支持时间戳的设备返回错误，不伪造 CPU 计时为 GPU 时间。标记通道有额外开销，因此端到端计时必须关闭插桩，GPU 区间不能解释成纯着色器执行时间。

`GpuRenderer::render()` 是完整离屏绘制；`render_retained()` 使用与原生窗口相同的缓存更新路径，并仅为测试/截图回读。系统要求重新呈现时仍会把完整保留纹理复制到新目标；“没有重新栅格化”不等于“没有呈现”。

窗口循环沿用 `ControlFlow::Wait` / `WaitUntil`，缓存没有引入轮询、额外线程或固定帧率计时器。动态光标及业务动画仍按实际期限请求刷新。

## 预算与观测

| 接口 | 行为 |
| --- | --- |
| `Tree::set_paint_cache_budget(bytes)` | 子树描述缓存默认 16 MiB；0 禁用缓存。超预算的子树正常重建，不截断绘制。 |
| `Tree::paint_cache_stats()` | 最近一次 scene 的实际绘制节点数、复用子树数和保留描述的估算字节数。 |
| `DamageConfig` | 默认 24 区域、16 MiB 场景比较描述、128 MiB 像素缓存预算。 |
| `RetainedCpuRenderer::with_damage_config()` | 显式配置 CPU 缓存策略。 |
| `GpuRenderer::set_damage_config()` | 修改策略并释放旧 GPU 缓存。 |
| `damage_stats()` | 最近一次成功更新的区域数、栅格化像素数、整帧像素数、整帧复用状态和缓存估算字节数。 |

场景比较描述超过预算时整帧回退；完整目标和同尺寸临时目标无法同时容纳于像素预算时，关闭区域复用。CPU 的输出缓冲是返回结果所必需的，不能因为缓存预算为零而省略；GPU 则直接绘制到传入目标。绘制资源仍独立受 `RenderLimits` 限制。统计是缓存描述与像素存储的估算，不是进程 RSS，也不包含驱动、字体栅格器或 Vello 工作缓冲。

自定义控件必须遵守失效契约：影响绘制的变化返回 `Paint` 或 `Layout`，外部资源变更调用 `Tree::invalidate()`；`paint`/`paint_foreground` 仅描述绘制，不承担必须每帧发生的业务副作用。相同属性、布局及字体环境下的 `prepare` 应产生相同绘制结果。Canvas 等外部回调通过 revision 标记内容变化。

## 验证

```sh
cargo test -p zui-core -p zui-paint -p zui-render-check --locked
cargo test -p zui-app -p zui-window-check -p zui-backend-winit --locked
cargo test -p zui-render-check --features gpu --test damage --locked -- --ignored
cargo test -p zui-render-gpu --locked -- --ignored
python3 tools/check-dependencies.py
```

GPU 命令要求可用的真实图形适配器。验证覆盖子树绘制次数、旧 Scene 独立性、零预算、不可见变化、透明叠加、路径裁剪、描边、移动/删除、字形与图像、1/1.25/1.5/2 倍缩放，以及保留 GPU 纹理向全新呈现目标的复制。
