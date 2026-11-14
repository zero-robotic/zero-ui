# 绘制契约、资源与渲染适配

[文档索引](README.md) · [模块边界](module-boundaries.md) · [未完成事项](implementation-plan.md)

按架构职责归并的专题参考。设计目标与接口约束分开理解；待办统一维护在实施计划中，各节的验证说明仅覆盖所述范围。

## 章节导航

- [ZUI 2D 渲染库选型评估](#rendering-library-evaluation)
- [子树绘制缓存与区域重绘](#region-rendering)
- [Canvas 便捷自定义绘制](#canvas)
- [图片资源与显示实现进度](#images)
- [OpenGlView](#opengl-view)

<a id="rendering-library-evaluation"></a>


<a id="rendering-library-evaluation--zui-2d-渲染库选型评估"></a>

## ZUI 2D 渲染库选型评估

采用 Vello CPU/GPU 双后端。本文保留选型依据与约束，不作为特定版本的验证报告；依赖版本以 Cargo 配置为准，目标硬件仍需独立验证。


<a id="rendering-library-evaluation--1-结论与前提"></a>

### 1. 结论与前提

ZUI 首期同时支持 GPU 和无 GPU 嵌入式 Linux，未来允许扩展到 MCU。渲染库应按 GPU/CPU 组合评估，关注绘制语义、文字、资源管理和维护成本，不能只分别挑选两端的性能领先者。

已确定的首期方案：

- GPU 采用 Vello GPU，CPU 采用 Vello CPU，共享框架定义的绘制语义，通过各自适配器接入。
- 原始 compute-centric `vello` 不作为本方案默认 GPU 实现；集成时核对 Vello GPU 的实际发布包名与可用版本。
- Skia、tiny-skia 和 FemtoVG 保留为评估备选，不列入首期并行实现范围。
- MCU 将来可以采用独立的受限渲染实现，复用 ZUI 的基础绘制契约；不要求首期渲染库直接覆盖所有 MCU。

采用该方案是为了集中维护 Rust 渲染技术栈，并接受通过适配层管理 Vello 的版本演进与功能限制。这是明确的工程决策，不是实测性能排名；未来升级不能替代当前版本的功能与硬件验收。


<a id="rendering-library-evaluation--2-候选组合"></a>

### 2. 候选组合

| 组合 | 优势 | 主要成本或限制 | 定位 |
| --- | --- | --- | --- |
| Skia GPU + Skia Raster，经 skia-safe 接入 | CPU/GPU 共用 SkCanvas 绘制 API，减少两套绘制语义的适配工作 | C++ 工具链、交叉编译、产物与运行时资源需验证；不能视为 MCU 方案 | 保留为评估备选 |
| Vello GPU + Vello CPU | Rust 实现，共享 Sparse Strips 架构及部分基础设施 | API 与功能仍在演进；部分功能有明确限制；发布版本与主分支可能不同 | 首期已选定 |
| FemtoVG + tiny-skia 或 Vello CPU | 可独立选择 GPU 与 CPU 路径，适合特定 GPU 接口要求 | 两套库的裁剪、混合、字体和缓存行为需要协调 | 硬件或功能匹配时采用 |
| 自建 wgpu 绘制器 + CPU 绘制器 | 可针对有限控件和绘制原语优化 | 路径、抗锯齿、文字、裁剪和资源管理均增加自研成本 | 当前不优先 |
| MCU 专用受限后端 | 可针对固定资源和显示协议优化 | 通用矢量、文字和特效能力需要裁剪 | 后续阶段 |

`wgpu` 属于 GPU 抽象层，单独选择它并未完成 GUI 的 2D 光栅化方案。渲染库与屏幕呈现同样是不同职责，CPU 后端还需验证显示输出链路不依赖 GPU。


<a id="rendering-library-evaluation--3-skia-路线"></a>

### 3. Skia 路线

Skia 的 Raster 与 GPU 后端接受 SkCanvas 绘制命令。Raster 可以绘制到由调用方管理的像素内存；GPU 上下文与显示集成仍需由应用或适配层建立。[Skia 官方说明](https://skia.org/docs/user/api/skcanvas_creation/)

对于 ZUI，这意味着可以共享一套绘制描述到 SkCanvas 的转换逻辑，再分别管理 CPU 与 GPU 输出，减少同时维护两套库的语义差异。这不保证两端像素完全相同，仍需验证抗锯齿、颜色处理、文字和缓存行为。

`skia-safe` 是 Skia C++ API 的 Rust 绑定。官方提供部分目标的预编译产物；目标或功能组合不匹配时，需要源码构建，涉及 LLVM、Python 和 Ninja。嵌入式 Linux 还需要核对架构、sysroot、C/C++ 运行库和链接方式。[rust-skia 构建说明](https://github.com/rust-skia/rust-skia)

Skia 保留为备选的理由是可能降低双后端绘制集成成本，并非未经测量地认定其体积最小或速度最快。若未来重新评估并引入，Skia 类型仍应仅存在于适配器内部。


<a id="rendering-library-evaluation--4-vello-路线"></a>

### 4. Vello 路线

Vello 项目包含不同实现，选型时不能只写一个笼统的 Vello：

- 原始 compute-centric `vello`：主要使用 GPU compute shaders，当前仓库将其归于实验研究实现。
- Vello GPU：CPU 预处理路径，GPU 光栅化和合成。
- Vello CPU：在 CPU 上绘制，适用于没有 GPU 或 GPU 能力不足的设备。

选定的 CPU/GPU 组合共享 Sparse Strips 架构；共享架构并不代表完全相同的 API 或功能覆盖，仍需通过适配和回归测试保证 ZUI 的绘制语义。[Vello 项目说明](https://github.com/linebender/vello)

当前 GPU 主分支文档列出限制：mask layers、复杂滤镜图以及部分非隔离混合模式尚不支持且可能 panic；字形缓存等部分 API 仍为实验性。该包从 `vello_hybrid` 改名为 `vello_gpu`，文档明确提醒新名称的发布可能落后于仓库改名。实现前必须核对实际发布版本，不能直接把主分支说明视为可用版本承诺。[Vello GPU 说明](https://github.com/linebender/vello/tree/main/vello_gpu)

CPU 文档也列出复杂滤镜、多线程滤镜和实验性字形缓存等限制。它提供可配置的 std/libm、文字和渲染流水线选项，但仍需检查完整依赖与内存需求，不能从配置选项直接推导出目标 MCU 可用。[Vello CPU 文档](https://docs.rs/vello_cpu/latest/vello_cpu/)

选择这一组合时，应定义明确的支持功能集合，在调用渲染库前检查能力，对不支持的效果提供明确替代或错误，避免运行过程中触发已知不支持路径。


<a id="rendering-library-evaluation--5-tiny-skia-与-femtovg-的位置"></a>

### 5. tiny-skia 与 FemtoVG 的位置

tiny-skia 是 Rust CPU 2D 绘图库，包含路径填充、描边、裁剪和图像混合等能力，但不包含文字渲染。若作为 ZUI 的 CPU 后端，需要补充字形栅格化、字体处理和缓存；应将这些成本一起纳入对比。[tiny-skia 官方说明](https://github.com/linebender/tiny-skia)

FemtoVG 当前发布文档列出 OpenGL 与可选 WGPU 渲染器，因此不应将其概括为只能使用 OpenGL。它值得在 GPU 接口需求明确时评估，但没有直接解决本项目所需的纯 CPU 后端，仍需组合另一套实现。[FemtoVG 渲染器文档](https://docs.rs/femtovg/latest/femtovg/renderer/index.html)

暂不默认选择混合库组合，原因是它会增加裁剪、透明合成、图片采样及文字效果的一致性维护工作。具体硬件兼容性或测量收益足以抵消这些成本时，再采用该路线。


<a id="rendering-library-evaluation--6-文字与-mcu-的共同边界"></a>

### 6. 文字与 MCU 的共同边界

双后端应共享字体选择、塑形和布局结果，通过框架定义的定位字形序列传给绘制适配器，避免分别排版导致换行和光标位置不同。字体数据、字形 ID、字号、变化轴和位置等信息需要保持一致；彩色字形和字体格式要单独验证。

首期仍可评估 Parley 作为共享排版实现，但与 Vello CPU/GPU 的字形接口需要做实际原型，不能视为自动兼容。

未来 MCU 可以替换文字和渲染实现，采用静态资源与分块输出。核心契约不能直接暴露 SkCanvas、Vello Scene 或 GPU 纹理。若使用复杂特效，需要通过能力配置决定是否可在 MCU 上使用。


<a id="rendering-library-evaluation--7-首期集成与验收门槛"></a>

### 7. 首期集成与验收门槛

围绕已选定的 Vello CPU/GPU 实现和验证，明确两端支持的功能集合。若出现无法满足的硬件或功能约束，记录问题并重新评估选型，不自动引入另一套渲染库。

| 验证项 | 场景与记录 |
| --- | --- |
| 构建与部署 | 桌面目标与实际嵌入式 sysroot；release 产物及依赖体积；干净环境可重复构建 |
| 基础功能 | 圆角、路径、嵌套裁剪、图片缩放、渐变、透明层和必要阴影 |
| 文字 | 中文长列表、混合字体、缩放、选择、光标、必要的彩色字形 |
| 性能 | 首帧、滚动、局部更新；区分布局、命令生成、光栅化、GPU 执行及显示提交 |
| 内存 | 峰值与稳定占用、字形和图片缓存、临时层、帧缓冲和颜色转换开销 |
| 无 GPU | 设备无需 GPU API 初始化即可绘制并呈现；验证实际显示驱动 |
| 一致性 | 相同字体和资源、相同场景；容许抗锯齿差异，不能容许布局和交互差异 |
| 稳定性 | 长时间更新、窗口缩放、资源释放与错误处理 |

帧率和内存阈值在明确芯片、RAM、分辨率与刷新需求后确定。必须计入显示传输，不能用桌面离屏性能代替嵌入式端到端表现。

目前已运行本机基础图元 CPU/GPU 回归，尚未完成上述完整基准或目标嵌入式硬件验证。集成与验收记录应包含确切版本、功能开关、测试设备、结果及失败原因。


<a id="rendering-library-evaluation--8-版本锁定与升级策略"></a>

### 8. 版本锁定与升级策略

1. 首次集成核对实际发布包名、版本和功能覆盖，锁定经过验证的 CPU/GPU 依赖组合，提交工作区 `Cargo.lock`。发布的库还需声明正确的依赖约束，不能依赖消费者沿用本项目的锁文件。
2. 优先使用已发布版本；确需使用未发布能力时，固定 Git revision 并记录原因，不使用浮动 main 作为交付依赖。
3. 将 Vello 的场景、上下文、资源对象和错误处理限制在 `zui-render-cpu` / `zui-render-gpu` 适配层。控件和公共绘制契约不暴露这些类型。
4. 升级前检查发布说明、API 变化、功能限制及共享依赖兼容性。CPU/GPU 按可验证的兼容组合升级，不要求版本号相同。
5. 在独立变更中完成升级与适配，运行仅 CPU、仅 GPU 和双后端构建检查，以及绘制、文字、局部刷新和资源生命周期回归。
6. 在真实无 GPU 嵌入式设备上复核峰值内存、帧耗时与显示传输，桌面 GPU 上复核输出及资源使用。未通过时保留原有可用版本，并记录回退原因。

首期以当前锁定版本实际支持的能力交付。不支持的效果通过能力检查明确替代、拒绝或暂缓；不能以未来 Vello 更新一定实现某项功能作为交付承诺。

<a id="region-rendering"></a>


<a id="region-rendering--子树绘制缓存与区域重绘"></a>

## 子树绘制缓存与区域重绘

设计参考 [BUI 的理念](https://github.com/ppmpreetham/BUI#2-why-bui-exists)：只处理变化，空闲时等待输入或定时器。本实现保留 ZUI 的 Widget → Scene → CPU/GPU 分层，不引入 BUI 或 Blender 的运行时依赖。


<a id="region-rendering--两层复用"></a>

### 两层复用

1. `Tree::scene()` 缓存子树的普通绘制命令。事件、焦点、动画、属性更新、子内容显露和托管子节点回调返回失效时，节点及其祖先的缓存失效。其他子树跳过绘制回调。布局提交只使几何变化或显式失效的节点失效；字体 epoch 变化和显式全局失效清空缓存。增删、重排、换父节点和 clip 变化均参与失效。
2. `RetainedCpuRenderer` 与 GPU 原生呈现比较前后绘制描述，计算变化区域。保留未变化像素，在变化区域重新绘制背景及所有与之相交的内容，再替换该区域。不会把新的半透明图形再次叠加到旧像素上。

`Scene` 的命令存储采用不可变共享、写时复制。外部保留的旧 Scene 不会被后续更新修改。渲染器仍然不持有 Widget 或节点引用。


<a id="region-rendering--区域计算与正确性"></a>

### 区域计算与正确性

- 比较实际命令与有效裁剪栈，不以哈希相等作为正确性依据。相同帧参数、相同命令直接复用。
- 同时包含旧位置与新位置，覆盖移动、删除、裁剪变化和重排；数量变化时保留公共前后缀，只计算中间变化部分。
- 曲线路径按保守边界处理；描边包含斜接扩展，文字使用字体字形边界并留出 hinting/抗锯齿余量，不能确定字形范围时退回整个目标范围。图像使用完整变换后的图像边界。
- 损伤区域按物理像素扩展到 64×32 网格，合并相交或相接区域；默认最多 24 个区域，超限或总面积超过整帧 60% 时回退整帧。
- scale、物理尺寸或背景变化会触发整帧更新。失败使缓存键失效，下一次有效调用重新绘制；输入校验和资源预算检查不会被缓存绕过。
- 无变化时不执行栅格化。不可见变化也可以得到空区域列表，但会更新比较基线，随后进入视口时仍能正确重绘。

区域绘制使用整数物理偏移，保留分数缩放和文字网格对齐。复杂透明叠加及分数坐标下，Vello 在不同目标尺寸/分块路径中可能产生单通道 1/255 的舍入差异；回归测试对这类场景逐通道验证误差不超过 1，简单整数矩形与最终 GPU 纹理复制要求完全相等。区域每次从背景重新合成，误差不会通过反复叠加累计。


<a id="region-rendering--cpu-与-gpu"></a>

### CPU 与 GPU

CPU 窗口默认使用 `RetainedCpuRenderer`。它保留输出 PixelBuffer，局部区域使用临时缓冲，逐行比较并替换变化的像素。`CpuRenderer` 保留完整绘制接口，作为无缓存路径及像素对照基准。CPU 系统呈现当前仍提交完整 PixelBuffer，不宣称只向系统传输脏区域。

GPU 的 `render_to_view()` 默认保留一张 GPU 纹理。局部区域在 GPU 临时纹理中绘制，并复制到保留纹理的对应位置；最后通过无混合的纹理复制绘制到本次获得的目标。该路径不回读 CPU，也不假设交换链目标保留上一帧。多个区域按队列顺序提交，避免 Vello 的共享工作缓冲在提交前被后续区域覆盖。

`zui-render-gpu/profiling` 是默认关闭的诊断特性；启用后可调用 `set_gpu_timing_enabled(true)`，在 `render_to_view` 后通过 `take_gpu_time()` 等待并读取提交区间总时长。每次最多累积 64 次渲染提交，须逐帧读取；读取后计数归零。不支持时间戳的设备返回错误，不伪造 CPU 计时为 GPU 时间。标记通道有额外开销，因此端到端计时必须关闭插桩，GPU 区间不能解释成纯着色器执行时间。

`GpuRenderer::render()` 是完整离屏绘制；`render_retained()` 使用与原生窗口相同的缓存更新路径，并仅为测试/截图回读。系统要求重新呈现时仍会把完整保留纹理复制到新目标；“没有重新栅格化”不等于“没有呈现”。

窗口循环沿用 `ControlFlow::Wait` / `WaitUntil`，缓存没有引入轮询、额外线程或固定帧率计时器。动态光标及业务动画仍按实际期限请求刷新。


<a id="region-rendering--预算与观测"></a>

### 预算与观测

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


<a id="region-rendering--验证"></a>

### 验证

```sh
cargo test -p zui-core -p zui-paint -p zui-render-check --locked
cargo test -p zui-app -p zui-window-check -p zui-backend-winit --locked
cargo test -p zui-render-check --features gpu --test damage --locked -- --ignored
cargo test -p zui-render-gpu --locked -- --ignored
python3 tools/check-dependencies.py
```

GPU 命令要求可用的真实图形适配器。验证覆盖子树绘制次数、旧 Scene 独立性、零预算、不可见变化、透明叠加、路径裁剪、描边、移动/删除、字形与图像、1/1.25/1.5/2 倍缩放，以及保留 GPU 纹理向全新呈现目标的复制。

<a id="canvas"></a>


<a id="canvas--canvas-便捷自定义绘制"></a>

## Canvas 便捷自定义绘制

CanvasView 用于网格、刻度、装饰背景等被动绘制。开发者提供一个绘制回调即可构建普通 View；需要事件、动画、生命周期或自定义命中时，仍使用完整 Widget 接口。

```rust
use zui_api::view::View;
use zui_types::{Color, Rect, Size};
use zui_widgets::CanvasView;

let canvas = CanvasView::new(Size::new(240.0, 80.0), |context, painter| {
    let bounds = context.content_bounds;
    painter.fill_rect(bounds, Color::rgb(12, 18, 30));
    painter.fill_rect(
        Rect::new(bounds.origin.x, bounds.origin.y, bounds.size.width, 2.0),
        Color::rgb(30, 210, 240),
    );
});
let view = View::new(canvas);
```

回调接收 PaintContext 和公共 Painter，绘制使用节点局部坐标，content_bounds 包含内边距产生的偏移。回调可使用图片、路径和已排版字形等既有能力，不接触 Vello/wgpu 类型。自然尺寸为 natural_size，两个轴分别受可用空间限制，不隐含宽高比；显式 LayoutStyle 尺寸可覆盖。尺寸必须有限、非负且可由布局 f32 表示，在测量和准备阶段都校验。

clip 默认为 true，只绘制内容盒内像素；设为 false 可在自身内容盒外绘制，但祖先裁剪仍生效。零内容尺寸不调用回调。Canvas 不参与指针命中或焦点，因此适合放在交互控件前后做装饰。


<a id="canvas--公共形状路径"></a>

### 公共形状路径

`zui-paint::Path` 提供 `circle(center, radius)`、`ellipse(rect)` 和 `rounded_rectangle(rect, radius)`，均返回 `Result<Path, RenderError>`。结果可用于 Painter 的填充、描边、裁剪和局部变换，CPU/GPU 共用同一份路径；自定义控件不需要引用渲染器。

```rust
use zui_paint::Path;
use zui_types::{Color, Rect};

// 在 Canvas 回调中绘制圆角面板；动态几何应在准备阶段验证并缓存。
let panel = Path::rounded_rectangle(Rect::new(8.0, 8.0, 200.0, 64.0), 12.0)
    .expect("constant valid geometry");
// painter.fill(panel, Color::rgb(12, 28, 44));
```

构造器拒绝非有限坐标、负尺寸/半径及边界溢出。零面积返回空路径；圆角半径自动限制到短边的一半，零半径等价于矩形。圆和椭圆由四段三次贝塞尔曲线近似，并非精确圆锥曲线；圆角使用同样的四分之一圆近似。圆要求其包围盒和直径可由 f64 表示。当前仅支持统一圆角半径，不提供独立四角半径或通用圆弧接口。

几何模块保持 no_std + alloc，不依赖控件、Vello 或平台。Radio 和 Switch 复用公共构造器，不再维护独立圆弧实现。


<a id="canvas--更新与状态"></a>

### 更新与状态

回调以 Rc 共享，clone 保留回调身份。新的回调或 clip/revision 变化触发 Paint；natural_size 变化触发 Layout；完全相同的描述无需更新。每次重新调用 new 都创建新回调，因此会触发重绘。

推荐捕获不可变绘制数据，数据变化后构建新 CanvasView。若选择共享内部可变数据，则修改数据后必须改变 revision 并重建 View；框架不追踪任意 Rc/Cell 的变化。修改 revision 不负责动画定时，回调也不应修改模型、发起 I/O 或请求调度。没有自动线程切换，回调运行在界面绘制过程中；其异常和阻塞由调用者负责。

该模块仍为 no_std + alloc，不引入新依赖，也不把应用类型传给 core。纯绘制以外的需求应直接实现 Widget，避免在闭包中隐藏输入和生命周期逻辑。


<a id="canvas--验证"></a>

### 验证

三项测试覆盖回调身份/显式 revision/尺寸失效、自然尺寸约束、非法值恢复、节点身份和非交互性，以及 1×/2× 的内容几何、默认裁剪/关闭裁剪和零尺寸。CPU 像素测试通过；同一验证函数提供显式 GPU 测试入口，本阶段未实际运行 GPU。相关严格 Clippy、格式、24 包依赖检查及 12 包 Cortex-M 裸机库构建通过。

公共形状另有三项几何测试及共用 CPU/GPU 像素测试，覆盖校验、零面积、圆角钳制、闭合、填充/描边/裁剪和 1×/2× 缩放；2026-10-31 已显式运行真实 GPU，并通过 Radio/Switch CPU/GPU 回归。12 包 Cortex-M 库构建通过；不等同于 MCU 固件或设备渲染验收。

<a id="images"></a>


<a id="images--图片资源与显示实现进度"></a>

## 图片资源与显示实现进度

当前已完成可移植图片资源、图片命令、CPU/GPU 绘制、Image 控件和静态 PNG 解码；已提供可选后台解码，其他格式及文件/网络加载尚未实现。资源位于 zui-paint，不依赖文件系统、解码库、控件或 Vello；应用/资源加载层负责取得并解码像素。


<a id="images--像素约定"></a>

### 像素约定

ImageResource 保存从上到下排列、紧密排列的预乘 RGBA8，颜色通道使用 sRGB 编码，与 PixelBuffer 的字节约定一致。宽高必须非零，长度严格为 width × height × 4，不接受行尾填充；stride 返回 width × 4。

- `from_rgba8(extent, Vec<u8>, limits)` 接收直通 alpha，逐通道按 `(channel * alpha + 127) / 255` 转为预乘格式。全透明像素的 RGB 清零，alpha 不变。
- `from_premultiplied_rgba8(extent, Arc<[u8]>, limits)` 验证 RGB 均不超过 alpha，复用传入像素存储，不复制字节。失败返回第一个非法像素的索引。

这是编码值上的预乘约定，不执行线性光混合或色彩配置文件转换。静态 PNG 解码见下节；方向元数据和颜色管理尚未接入。


<a id="images--身份生命周期与限制"></a>

### 身份、生命周期与限制

资源持有不可变共享字节，clone 保留 ImageId，已创建的资源不受调用者后续模型变化影响；重新创建相同像素会获得不同 ID。相等比较依据身份，避免每次 View 比较扫描整张图片。后续后端可以依据 ID 管理上传缓存，无需上层携带后端纹理句柄。Debug 仅输出 ID、尺寸和字节数。

ImageLimits 默认单边不超过 16,384、字节数不超过 64 MiB；可由应用收紧。构造先检查非零尺寸、限制、长度溢出和字节长度，再扫描或转换像素。ImageError 区分尺寸、超限、字节长度、非法预乘像素及身份耗尽。

限制仅适用于每个资源，不代表全部图片、解码峰值、旧帧引用或 GPU 上传总内存限制。传入字节已经由调用者分配，限制不能追溯限制先前的解码分配。资源/Arc 分配沿用全局分配器策略，不承诺返回可恢复的 OOM 错误。默认尺寸也不表示所有后端支持该尺寸。


<a id="images--验证及后续阶段"></a>

### 验证及后续阶段

三项资源单元测试通过，覆盖全部 65,536 种通道/alpha 组合、转换舍入、全透明处理、共享字节与身份、生存期、限制边界、地址计算溢出、非法字节长度和预乘数据。paint 严格 Clippy、格式、23 包依赖检查及 12 包 Cortex-M 裸机库构建通过。


<a id="images--图片绘制协议与-cpu-后端"></a>

### 图片绘制协议与 CPU 后端

`Painter::image(image, sampling)` 在局部 `[0,width] × [0,height]` 范围绘制整张图片，支持 `ImageSampling::Nearest` 和 `Bilinear`。位置和尺寸通过现有 `with_transform` 指定，裁剪复用 `with_clip`；帧 scale 在适配层统一应用，不进入资源数据。

```rust
painter.with_transform(
    Affine::translate(20.0, 30.0).compose(Affine::scale(2.0, 2.0)),
    |p| p.image(image.clone(), ImageSampling::Bilinear),
);
```

Scene 校验图片变换和按 ImageId 去重的资源预算。RenderLimits 新增 max_images（默认 1,024）和 max_image_bytes（默认 64 MiB）；重复引用同一资源只计一次，独立创建的相同像素资源分别计数。已有命令预算仍约束绘制次数。这里仍不计入应用保存的其他图片、跨帧引用或后端转换副本。

CPU 适配将预乘像素复制到 Vello Pixmap，按 ImageId 复用缓存；下一帧不再使用的缓存项释放。该缓存仅属于 CPU 后端，公共契约不引用 Pixmap。尺寸超出 Vello u16 表示范围时返回 TargetTooLarge。资源校验允许的尺寸不保证任一后端均能处理。

GPU 现使用适配层持有的 Rgba8Unorm 外部纹理，以 ImageId 缓存上传结果；普通帧重复引用只复用纹理，下一帧不再引用时释放缓存引用。Vello TextureId 和 wgpu TextureView 不进入公共资源接口。上传前检查全部图片尺寸，超出设备纹理维度或 u16 范围返回 TargetTooLarge；失败后可以继续提交合法帧。纹理内存实际回收时机由 GPU 提交和驱动引用决定。CPU/GPU 共用采样参数映射，不再返回阶段性的 UnsupportedImage。

新增三项场景/CPU 测试覆盖共享预算、非法变换、1×/2× 缩放/裁剪/透明合成、双线性插值、空帧清除、同尺寸资源替换以及预算失败保持目标缓冲不变；新增缓存测试确认复用与释放。四项已有 CPU 绘制回归通过。工作区全目标全特性编译、相关严格 Clippy、格式/依赖及 Cortex-M 库构建通过。该段为 CPU 阶段记录；后续 GPU 验证见下节。


<a id="images--gpu-图片验收"></a>

### GPU 图片验收

2026-10-28 当前 macOS 开发机，显式 GPU-only 配置测试通过：

```sh
cargo test -p zui-render-check --test images --no-default-features --features gpu --locked --offline -- --ignored
```

CPU/GPU 共用验证函数，覆盖 1×/2× 缩放/裁剪/透明合成、清屏、同尺寸资源替换、多个独立图片及共享资源同时出现、相同场景再次提交和双线性插值。GPU 另验证超出设备尺寸时报错及后续合法帧恢复。适配器初始化失败算测试失败，不跳过；未作所有像素完全一致或性能承诺。

CPU 三项图片/预算测试、四项已有绘制回归和一项显式 GPU 集成测试通过；相关严格 Clippy、格式和 23 包依赖检查通过。本轮没有修改公共资源的 no_std 边界，未重跑 MCU 构建。未进行图片原生窗口、长时显存或跨设备验收。

PNG 解码和可选后台解码已在独立资源适配模块实现；编码、其他格式和文件/网络加载尚待实现。


<a id="images--image-控件"></a>

### Image 控件

`ImageView::new(resource)` 默认 Contain、水平/垂直居中、Bilinear。资源像素尺寸作为固有逻辑尺寸，一像素对应一逻辑单位；高 DPI 的物理缩放由 Frame 统一处理。可在普通 View 的 LayoutStyle 中指定显示尺寸与 padding。

```rust
use zui_api::{layout::{Dimension, LayoutStyle}, view::View};
use zui_widgets::{ImageView, ImageFit};

let mut image_view = ImageView::new(resource);
image_view.fit = ImageFit::Cover;
let view = View::new(image_view).styled(LayoutStyle {
    width: Dimension::Length(240.0),
    height: Dimension::Length(160.0),
    ..Default::default()
});
```

四种绘制策略独立于盒子布局：

| ImageFit | 行为 |
| --- | --- |
| Contain | 等比缩放，完整显示，剩余区域透明 |
| Cover | 等比填满，多余部分裁剪 |
| Fill | 两轴分别缩放，可能改变比例 |
| None | 保持原始逻辑尺寸，溢出裁剪 |

horizontal/vertical 使用 ImageAlign::Start/Center/End。Cover 和 None 的对齐也决定裁剪哪一侧。所有绘制都裁剪到内容盒，不覆盖 padding；零内容尺寸不产生绘制命令。控件不参与命中或焦点，交互需由应用组合其他控件；当前没有图片无障碍语义。

自动尺寸默认不放大固有尺寸，并根据有限可用空间等比缩小。仅确定一个轴时，通过通用首选宽高比向布局引擎提供另一轴的建议；两个轴都确定时以明确尺寸为准，图片绘制按 fit 适配。宽高比是布局盒子的首选比例，约束可覆盖它；带 padding 时不要把该比例理解为内容区域的强制比例，需要严格内容比例时可使用外层容器提供内边距。

同尺寸新资源、fit、对齐或采样变化只使 Paint 失效，资源尺寸变化使 Layout 失效；clone 相同描述不失效，重建保持节点身份。图片控件通过 Painter::image 绘制，不引用任何具体渲染器。


<a id="images--通用首选宽高比"></a>

#### 通用首选宽高比

Widget::preferred_aspect_ratio 和 LayoutTree::preferred_aspect_ratio 默认返回 None。core 仅转发该可选值；Taffy 适配将它应用为首选布局盒宽/高比例，拒绝零、负数、NaN 和无穷。该能力可由任意自定义控件实现；值改变时须报告 Layout 失效。其他布局引擎需要自行实现对应约束语义。

三项 CPU/布局集成测试和一项显式真实 GPU 测试通过，覆盖固有/单轴/有限空间布局、资源更新失效、节点身份、非交互性、1×/2× 四种 fit、对齐/内边距/零尺寸，以及普通自定义控件的比例与非法值恢复。布局包回归、相关严格 Clippy、格式/依赖检查和 Cortex-M 库构建通过。未进行原生窗口图片交互或跨设备视觉验收。


<a id="images--静态-png-解码"></a>

### 静态 PNG 解码

`zui-image::decode_png(bytes, DecodeLimits)` 同步解码字节，返回公共 ImageResource；该 std 模块仅向下依赖 types/paint 和 png，不依赖 widgets、core、平台或渲染器。文件读取和网络由应用负责；可选后台解码见下节。MCU 可使用预解码像素，不需要引入本模块。

```rust
let resource = zui_image::decode_png(
    include_bytes!("logo.png"),
    zui_image::DecodeLimits::default(),
)?;
let view = zui_api::view::View::new(zui_widgets::ImageView::new(resource));
```

支持静态 PNG 的 RGB/RGBA、灰度/灰度透明、调色板及 tRNS 展开，低位深展开为 8 位；16 位样本取高字节，再转换为预乘 RGBA8。APNG 明确返回 AnimatedPng，不静默取首帧。会完成尾部读取和校验，截断或 CRC 错误不返回成功资源。

不执行 ICC、gamma 或方向变换，编码通道按 sRGB 解释；忽略文本及 ICC 数据块内容。不声称具备完整色彩管理能力。

DecodeLimits 分三部分：max_encoded_bytes 默认 16 MiB；max_decoder_bytes 默认 64 MiB，传给 PNG 解码器内部预算；image 沿用 ImageLimits，限制最终 RGBA 尺寸和字节。输出尺寸在分配像素输出前检查。DecodeError 区分编码超限、解码器预算、动画不支持、PNG 错误和资源错误，不暴露第三方错误类型。

这些限制不是统一进程内存上限：调用者输入、解码器内部数据、归一化输出、RGBA 转换和 Arc 资源在某些阶段会同时存在。分配沿用全局分配器策略，不承诺可恢复的 OOM 错误；需要按目标设备收紧各项预算。

四项解码测试覆盖颜色/透明/调色板/低位深、16 位转换、限制边界以及损坏/截断/动画拒绝。Image 控件测试改为先生成 PNG 字节再解码，三项 CPU/布局测试通过；本轮未重复 GPU 运行（此前 GPU 绘制及控件已单独验收）。image/render-check 严格 Clippy、格式和 24 包依赖检查通过。


<a id="images--有界后台-png-解码"></a>

### 有界后台 PNG 解码

启用 `zui-image` 的 `worker` 特性可使用 PngDecodeWorker；默认特性为空，同步解码用户不会创建后台线程。worker 只处理已取得的 PNG 字节，不读取文件、发起网络请求或依赖 UI 运行时。

```rust
use std::{num::NonZeroUsize, sync::Arc};
use zui_image::{DecodeLimits, PngDecodeWorker};

let mut worker = PngDecodeWorker::new(
    DecodeLimits::default(), NonZeroUsize::new(2).unwrap(),
)?;
let wanted = worker.try_submit(Arc::from(png_bytes))?;
// 在应用自己的 Wake/消息处理阶段轮询，不在 UI 线程等待或忙循环。
if let Some(completion) = worker.try_receive()? {
    if completion.id == wanted {
        let resource = completion.result?;
        // 保存到模型，然后请求重建 ImageView。
    }
}
```

每个 worker 一个线程，顺序解码。容量限制的是全部未取回请求：排队、执行中、已完成但尚未接收都占用一个名额；只有 try_receive 取回结果后释放名额。输入和输出通道也有界，满载返回 SubmitError::Full，不等待解码完成。提交前验证编码字节上限；DecodeLimits 在创建 worker 时固定，所有请求使用同一预算。

DecodeRequestId 在一个 worker 内单调唯一；不同 worker 的 ID 不保证不同。应用切换资源时保存最新请求 ID，丢弃不匹配的完成结果。这里提供的是过期结果识别，不是单请求取消：旧任务仍可能解码并占用容量直到取回。提交拒绝不占名额，调用者若要重试应保留 Arc 克隆；解码错误作为该请求的完成结果返回，不使后续任务失效。

try_receive 返回 None 表示暂时没有完成结果；线程意外结束且结果已取完时返回 DecodeWorkerClosed。模块不访问窗口事件循环，不发送唤醒回调；应用应自行安排有截止时间的轮询或上层任务通知，完成后停止轮询。

Drop 发送停止信号、关闭通道且不 join，避免在 UI 线程等待当前解码。正在进行的单次解码不能中断，可能在 owner 丢弃后继续到本次解码结束；之后丢弃排队请求，不调用 UI。没有保证同步销毁线程或立即回收正在解码的内存。

容量不是总内存预算：有界请求/结果及一次活动解码仍可能同时占用内存，应用已取回的资源和持有的输入 Arc 也不计入 outstanding。按设备同时收紧容量和 DecodeLimits。

三项 worker 测试覆盖容量/请求顺序和 ID、失败后继续工作、超限拒绝、丢弃 owner 后排队字节释放，四项同步 PNG 测试回归通过。全特性严格 Clippy、默认关闭 worker 的编译、格式及 24 包依赖检查通过。上述为后台解码模块的独立验证记录；窗口示例见下节，返回资源沿用已有绘制接口。


<a id="images--后台解码窗口示例"></a>

### 后台解码窗口示例

`zui-window-check --images` 展示应用层后台解码流程。启动时在内存生成一张 64×32、含透明像素的 PNG 测试图，交给容量 2 的 worker；这不是文件/网络加载器。完成后同一资源在 Contain/Cover/Fill/None 四个 ImageView 中显示，窗口变窄时自动换行并通过 ScrollView 浏览。

“重新加载”和“模拟损坏图片”分别提交合法/非法字节。应用保存最新请求 ID，旧完成结果只取回释放名额而不发布；最新提交被拒绝时保持提交错误，不让更早的成功覆盖它。解码失败保留上一张成功图片，之后可重试。默认显示正在解码状态。

应用在有未取回请求时用 WindowAction.wake_at 安排约 16 ms 后轮询，与冒烟退出截止时间取最早值；完成或 worker 关闭后不再续订解码轮询。没有让解码线程访问 UI 或直接重建树。退出并销毁模型时，底层 worker 的既有 Drop 规则仍适用。

```sh
cargo run -p zui-window-check --features gpu --locked --offline -- cpu --images --smoke-ms 1600
cargo run -p zui-window-check --features gpu --locked --offline -- gpu --images --smoke-ms 1600
```

2026-10-29 当前 macOS 开发机，两次均记录 `image demo: decoded`，正常限时退出，各 events=10/redraws=2。该结果验证后台完成到原生呈现的流程，不代表手动按钮点击或截图视觉验收。

两项新增测试覆盖等待/成功/错误/重试、过期结果和满载拒绝、完成后停止解码轮询，以及真实 WidgetHost 中四个图片命令的发布、窄窗口布局、按钮焦点保持和独立退出截止时间。window-check 全目标全特性严格 Clippy、格式及 24 包依赖检查通过。

<a id="opengl-view"></a>


<a id="opengl-view--openglview"></a>

## OpenGlView

`zui-opengl` 提供独立的桌面 OpenGL 控件。应用依赖此 crate 后使用 `zui_opengl::OpenGlView`，普通 `zui-widgets`、核心与绘制契约不增加原生 GL 依赖。

实现采用 Windows WGL 隐藏宿主窗口 / macOS CGL 无表面上下文，申请桌面 OpenGL 3.3+。每个已挂载控件独立持有上下文、RGBA8 颜色附件和 DEPTH24_STENCIL8 深度/模板附件。业务直接通过重导出的 `glow` 调用 GL，结果经同步回读转换为不可变 `ImageResource`，可由现有 CPU/GPU 后端绘制，正常参与父级裁剪、滚动和弹层覆盖。


<a id="opengl-view--接入"></a>

### 接入

```toml
[dependencies]
zui-opengl = { path = "../crates/zui-opengl" }
```

```rust,ignore
use zui_opengl::{glow::{self, HasContext}, OpenGlError, OpenGlFrame, OpenGlRenderer, OpenGlView};
use zui_types::Size;

struct MyRenderer;
impl OpenGlRenderer for MyRenderer {
    fn render(&mut self, gl: &glow::Context, frame: OpenGlFrame) -> Result<(), OpenGlError> {
        // frame.framebuffer 已绑定，viewport 已设置为 frame.extent 的物理像素尺寸。
        unsafe {
            gl.clear_color(0.1, 0.2, 0.3, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            // 自行使用 program、VAO、纹理和 draw calls。
        }
        Ok(())
    }
}
let mut gl_view = OpenGlView::new(Size::new(640.0, 360.0), || MyRenderer);
gl_view.scale_factor = window_metrics.scale;
let view = zui_api::view::View::new(gl_view.clone());
```

将 `OpenGlView` 保存在应用模型中，`view()` 里克隆它，使用稳定 Key。每次重建都重新创建工厂会销毁并重建 GL 资源。工厂每次挂载创建独立业务 renderer，不共享 GL 对象名。

`scale_factor` 必须在初次显示及窗口 `ScaleChanged` 时从 `WindowMetrics.scale` 更新，并请求应用重建；控件不隐式读取窗口或系统全局状态。物理尺寸为 `ceil(逻辑内容尺寸 × scale_factor)`，会先校验有限值、64 MiB 图像上限及驱动的 renderbuffer 限制。布局 padding 不属于 GL 图像区域。


<a id="opengl-view--生命周期与输入"></a>

### 生命周期与输入

- `initialize(gl)`：创建程序、缓冲、VAO、纹理。首次有效尺寸绘制前调用。
- `resize(gl, extent, scale_factor)`：目标尺寸变化后通知。
- `render(gl, frame)`：目标 FBO 已绑定、viewport 已设置，并清除颜色/深度/模板附件；返回前应将最终结果绘制到指定 FBO。
- `input(context, event)`：处理目标/直接事件；这是 CPU 状态回调，此时不保证 GL 上下文 current。修改业务状态并返回 `Invalidation::Paint`，在下一帧应用 GL 更新。`context.local_position` 为控件边框盒内逻辑坐标，减去 `geometry.content_bounds.origin` 可获得画布内坐标，再乘缩放得到像素坐标。可返回标准焦点/指针捕获/动作请求。
- `destroy(gl)`：在所属上下文 current 时释放业务资源；初始化部分失败也会调用。必须容忍资源尚未完整创建，不能 panic。

GL 回调固定在 WidgetHost 所在线程；macOS 必须是主线程，否则报告错误。不要从回调切换或销毁上下文，不要删除框架的 FBO/附件，也不要在其他线程使用 GL 对象。框架每次回调完成后解除自己的 current 上下文，不恢复应用先前绑定的其他 GL 上下文。业务负责自行配置 program、VAO、纹理、混合、深度和剔除等绘制状态。

卸载释放上下文和所有资源；同一工厂的普通视图更新保留资源。零尺寸不创建/绘制，`visible = false` 暂停绘制、输入和连续帧请求。父级滚动裁剪本身不会自动更改此标记，大型不可见面板应由应用显式暂停。窗口不可绘制时沿用 WidgetHost 的暂停逻辑。


<a id="opengl-view--刷新与错误"></a>

### 刷新与错误

默认按需渲染：首次布局、尺寸/缩放变化、业务 `revision` 变化或输入返回失效时才重新执行 GL。`request_redraw()` 更新 revision；调用后仍须让应用返回 `ViewAction::rebuild()`。这不是跨线程唤醒句柄。

设置 `frame_interval = Some(Duration::from_millis(16))` 可请求连续帧；这是最小调度间隔，不保证 60 FPS。设为 `None` 或零时按需刷新。时间来自宿主的 monotonic elapsed，而非控件自己读取系统时钟。

GL 工作在 `Widget::animate` 阶段进行，`paint` 只提交缓存图像。手动使用 `Tree` 时，先完成 layout，再调用 `advance_animations(elapsed)`，最后生成 scene；WidgetHost 已按此顺序执行。

`on_error` 接收上下文、初始化、回调、分配或回读错误。失败后显示 `fallback_color`，停止该控件的帧循环并释放会话，避免无限重试；修改 revision（`request_redraw`）可以重试。默认错误处理打印诊断，应用可用回调保存错误并显示 UI。普通布局校验失败仍走原有 LayoutError。


<a id="opengl-view--像素约定与限制"></a>

### 像素约定与限制

- 回读自动将 OpenGL 自下而上的行顺序翻转为 ZUI 的自上而下。
- RGBA8 附件按 sRGB 编码字节解释，框架关闭 FRAMEBUFFER_SRGB；业务 shader 应输出最终显示编码颜色，线性工作流需自行转换。
- 默认 `OpenGlAlphaMode::Opaque` 强制 alpha=255。`Straight` 会转成 ZUI 所需的预乘像素；`Premultiplied` 要求输出 RGB 不超过 alpha，否则报告错误。业务自行设置与此模式匹配的混合参数。
- 框架在回读前重设 READ_FRAMEBUFFER、读缓冲、PIXEL_PACK_BUFFER 与像素打包步长，避免业务状态破坏回读。
- 当前为单采样目标。业务可自行使用 MSAA FBO 并 resolve 到框架 FBO。
- 当前是同步 GPU→CPU 回读，GPU 后端还需重新上传。适合先接入已有 GL 代码，不承诺高分辨率高帧率，不包含跨 API 零拷贝或跨控件资源共享。
- 原生适配仅实现 Windows/macOS；其他平台返回明确错误。macOS 的 OpenGL 已被 Apple 弃用，要求新 GL 特性的应用应另行评估。


<a id="opengl-view--示例与验证"></a>

### 示例与验证

```sh
cargo run -p opengl -- cpu
cargo run -p opengl --features gpu -- gpu
cargo run -p opengl -- cpu --snapshot artifacts/opengl.png
cargo run -p opengl --features gpu -- gpu --smoke-ms 1000
cargo test -p zui-opengl
# 真实驱动验证必须在有图形驱动的桌面环境运行，使用独立主线程测试入口。
cargo test -p zui-opengl --test native -- --native
```

示例演示独立 vertex/fragment shader、VAO、旋转三角形、鼠标/方向键输入、暂停和按需重绘。像素预览使用 CPU 合成；窗口可选 CPU/GPU 合成。
