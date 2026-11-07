# Canvas 便捷自定义绘制

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

## 公共形状路径

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

## 更新与状态

回调以 Rc 共享，clone 保留回调身份。新的回调或 clip/revision 变化触发 Paint；natural_size 变化触发 Layout；完全相同的描述无需更新。每次重新调用 new 都创建新回调，因此会触发重绘。

推荐捕获不可变绘制数据，数据变化后构建新 CanvasView。若选择共享内部可变数据，则修改数据后必须改变 revision 并重建 View；框架不追踪任意 Rc/Cell 的变化。修改 revision 不负责动画定时，回调也不应修改模型、发起 I/O 或请求调度。没有自动线程切换，回调运行在界面绘制过程中；其异常和阻塞由调用者负责。

该模块仍为 no_std + alloc，不引入新依赖，也不把应用类型传给 core。纯绘制以外的需求应直接实现 Widget，避免在闭包中隐藏输入和生命周期逻辑。

## 验证

三项测试覆盖回调身份/显式 revision/尺寸失效、自然尺寸约束、非法值恢复、节点身份和非交互性，以及 1×/2× 的内容几何、默认裁剪/关闭裁剪和零尺寸。CPU 像素测试通过；同一验证函数提供显式 GPU 测试入口，本阶段未实际运行 GPU。相关严格 Clippy、格式、24 包依赖检查及 12 包 Cortex-M 裸机库构建通过。

公共形状另有三项几何测试及共用 CPU/GPU 像素测试，覆盖校验、零面积、圆角钳制、闭合、填充/描边/裁剪和 1×/2× 缩放；2026-10-31 已显式运行真实 GPU，并通过 Radio/Switch CPU/GPU 回归。12 包 Cortex-M 库构建通过；不等同于 MCU 固件或设备渲染验收。
