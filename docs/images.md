# 图片资源与显示实现进度

当前已完成可移植图片资源、图片命令、CPU/GPU 绘制、Image 控件和静态 PNG 解码；已提供可选后台解码，其他格式及文件/网络加载尚未实现。资源位于 zui-paint，不依赖文件系统、解码库、控件或 Vello；应用/资源加载层负责取得并解码像素。

## 像素约定

ImageResource 保存从上到下排列、紧密排列的预乘 RGBA8，颜色通道使用 sRGB 编码，与 PixelBuffer 的字节约定一致。宽高必须非零，长度严格为 width × height × 4，不接受行尾填充；stride 返回 width × 4。

- `from_rgba8(extent, Vec<u8>, limits)` 接收直通 alpha，逐通道按 `(channel * alpha + 127) / 255` 转为预乘格式。全透明像素的 RGB 清零，alpha 不变。
- `from_premultiplied_rgba8(extent, Arc<[u8]>, limits)` 验证 RGB 均不超过 alpha，复用传入像素存储，不复制字节。失败返回第一个非法像素的索引。

这是编码值上的预乘约定，不执行线性光混合或色彩配置文件转换。静态 PNG 解码见下节；方向元数据和颜色管理尚未接入。

## 身份、生命周期与限制

资源持有不可变共享字节，clone 保留 ImageId，已创建的资源不受调用者后续模型变化影响；重新创建相同像素会获得不同 ID。相等比较依据身份，避免每次 View 比较扫描整张图片。后续后端可以依据 ID 管理上传缓存，无需上层携带后端纹理句柄。Debug 仅输出 ID、尺寸和字节数。

ImageLimits 默认单边不超过 16,384、字节数不超过 64 MiB；可由应用收紧。构造先检查非零尺寸、限制、长度溢出和字节长度，再扫描或转换像素。ImageError 区分尺寸、超限、字节长度、非法预乘像素及身份耗尽。

限制仅适用于每个资源，不代表全部图片、解码峰值、旧帧引用或 GPU 上传总内存限制。传入字节已经由调用者分配，限制不能追溯限制先前的解码分配。资源/Arc 分配沿用全局分配器策略，不承诺返回可恢复的 OOM 错误。默认尺寸也不表示所有后端支持该尺寸。

## 验证及后续阶段

三项资源单元测试通过，覆盖全部 65,536 种通道/alpha 组合、转换舍入、全透明处理、共享字节与身份、生存期、限制边界、地址计算溢出、非法字节长度和预乘数据。paint 严格 Clippy、格式、23 包依赖检查及 12 包 Cortex-M 裸机库构建通过。

## 图片绘制协议与 CPU 后端

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

## GPU 图片验收

2026-10-28 当前 macOS 开发机，显式 GPU-only 配置测试通过：

```sh
cargo test -p zui-render-check --test images --no-default-features --features gpu --locked --offline -- --ignored
```

CPU/GPU 共用验证函数，覆盖 1×/2× 缩放/裁剪/透明合成、清屏、同尺寸资源替换、多个独立图片及共享资源同时出现、相同场景再次提交和双线性插值。GPU 另验证超出设备尺寸时报错及后续合法帧恢复。适配器初始化失败算测试失败，不跳过；未作所有像素完全一致或性能承诺。

CPU 三项图片/预算测试、四项已有绘制回归和一项显式 GPU 集成测试通过；相关严格 Clippy、格式和 23 包依赖检查通过。本轮没有修改公共资源的 no_std 边界，未重跑 MCU 构建。未进行图片原生窗口、长时显存或跨设备验收。

PNG 解码和可选后台解码已在独立资源适配模块实现；编码、其他格式和文件/网络加载尚待实现。

## Image 控件

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

### 通用首选宽高比

Widget::preferred_aspect_ratio 和 LayoutTree::preferred_aspect_ratio 默认返回 None。core 仅转发该可选值；Taffy 适配将它应用为首选布局盒宽/高比例，拒绝零、负数、NaN 和无穷。该能力可由任意自定义控件实现；值改变时须报告 Layout 失效。其他布局引擎需要自行实现对应约束语义。

三项 CPU/布局集成测试和一项显式真实 GPU 测试通过，覆盖固有/单轴/有限空间布局、资源更新失效、节点身份、非交互性、1×/2× 四种 fit、对齐/内边距/零尺寸，以及普通自定义控件的比例与非法值恢复。布局包回归、相关严格 Clippy、格式/依赖检查和 Cortex-M 库构建通过。未进行原生窗口图片交互或跨设备视觉验收。

## 静态 PNG 解码

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

## 有界后台 PNG 解码

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

## 后台解码窗口示例

`zui-window-check --images` 展示应用层后台解码流程。启动时在内存生成一张 64×32、含透明像素的 PNG 测试图，交给容量 2 的 worker；这不是文件/网络加载器。完成后同一资源在 Contain/Cover/Fill/None 四个 ImageView 中显示，窗口变窄时自动换行并通过 ScrollView 浏览。

“重新加载”和“模拟损坏图片”分别提交合法/非法字节。应用保存最新请求 ID，旧完成结果只取回释放名额而不发布；最新提交被拒绝时保持提交错误，不让更早的成功覆盖它。解码失败保留上一张成功图片，之后可重试。默认显示正在解码状态。

应用在有未取回请求时用 WindowAction.wake_at 安排约 16 ms 后轮询，与冒烟退出截止时间取最早值；完成或 worker 关闭后不再续订解码轮询。没有让解码线程访问 UI 或直接重建树。退出并销毁模型时，底层 worker 的既有 Drop 规则仍适用。

```sh
cargo run -p zui-window-check --features gpu --locked --offline -- cpu --images --smoke-ms 1600
cargo run -p zui-window-check --features gpu --locked --offline -- gpu --images --smoke-ms 1600
```

2026-10-29 当前 macOS 开发机，两次均记录 `image demo: decoded`，正常限时退出，各 events=10/redraws=2。该结果验证后台完成到原生呈现的流程，不代表手动按钮点击或截图视觉验收。

两项新增测试覆盖等待/成功/错误/重试、过期结果和满载拒绝、完成后停止解码轮询，以及真实 WidgetHost 中四个图片命令的发布、窄窗口布局、按钮焦点保持和独立退出截止时间。window-check 全目标全特性严格 Clippy、格式及 24 包依赖检查通过。
