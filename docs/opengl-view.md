# OpenGlView

`zui-opengl` 提供独立的桌面 OpenGL 控件。应用依赖此 crate 后使用 `zui_opengl::OpenGlView`，普通 `zui-widgets`、核心与绘制契约不增加原生 GL 依赖。

实现采用 Windows WGL 隐藏宿主窗口 / macOS CGL 无表面上下文，申请桌面 OpenGL 3.3+。每个已挂载控件独立持有上下文、RGBA8 颜色附件和 DEPTH24_STENCIL8 深度/模板附件。业务直接通过重导出的 `glow` 调用 GL，结果经同步回读转换为不可变 `ImageResource`，可由现有 CPU/GPU 后端绘制，正常参与父级裁剪、滚动和弹层覆盖。

## 接入

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

## 生命周期与输入

- `initialize(gl)`：创建程序、缓冲、VAO、纹理。首次有效尺寸绘制前调用。
- `resize(gl, extent, scale_factor)`：目标尺寸变化后通知。
- `render(gl, frame)`：目标 FBO 已绑定、viewport 已设置，并清除颜色/深度/模板附件；返回前应将最终结果绘制到指定 FBO。
- `input(context, event)`：处理目标/直接事件；这是 CPU 状态回调，此时不保证 GL 上下文 current。修改业务状态并返回 `Invalidation::Paint`，在下一帧应用 GL 更新。`context.local_position` 为控件边框盒内逻辑坐标，减去 `geometry.content_bounds.origin` 可获得画布内坐标，再乘缩放得到像素坐标。可返回标准焦点/指针捕获/动作请求。
- `destroy(gl)`：在所属上下文 current 时释放业务资源；初始化部分失败也会调用。必须容忍资源尚未完整创建，不能 panic。

GL 回调固定在 WidgetHost 所在线程；macOS 必须是主线程，否则报告错误。不要从回调切换或销毁上下文，不要删除框架的 FBO/附件，也不要在其他线程使用 GL 对象。框架每次回调完成后解除自己的 current 上下文，不恢复应用先前绑定的其他 GL 上下文。业务负责自行配置 program、VAO、纹理、混合、深度和剔除等绘制状态。

卸载释放上下文和所有资源；同一工厂的普通视图更新保留资源。零尺寸不创建/绘制，`visible = false` 暂停绘制、输入和连续帧请求。父级滚动裁剪本身不会自动更改此标记，大型不可见面板应由应用显式暂停。窗口不可绘制时沿用 WidgetHost 的暂停逻辑。

## 刷新与错误

默认按需渲染：首次布局、尺寸/缩放变化、业务 `revision` 变化或输入返回失效时才重新执行 GL。`request_redraw()` 更新 revision；调用后仍须让应用返回 `ViewAction::rebuild()`。这不是跨线程唤醒句柄。

设置 `frame_interval = Some(Duration::from_millis(16))` 可请求连续帧；这是最小调度间隔，不保证 60 FPS。设为 `None` 或零时按需刷新。时间来自宿主的 monotonic elapsed，而非控件自己读取系统时钟。

GL 工作在 `Widget::animate` 阶段进行，`paint` 只提交缓存图像。手动使用 `Tree` 时，先完成 layout，再调用 `advance_animations(elapsed)`，最后生成 scene；WidgetHost 已按此顺序执行。

`on_error` 接收上下文、初始化、回调、分配或回读错误。失败后显示 `fallback_color`，停止该控件的帧循环并释放会话，避免无限重试；修改 revision（`request_redraw`）可以重试。默认错误处理打印诊断，应用可用回调保存错误并显示 UI。普通布局校验失败仍走原有 LayoutError。

## 像素约定与限制

- 回读自动将 OpenGL 自下而上的行顺序翻转为 ZUI 的自上而下。
- RGBA8 附件按 sRGB 编码字节解释，框架关闭 FRAMEBUFFER_SRGB；业务 shader 应输出最终显示编码颜色，线性工作流需自行转换。
- 默认 `OpenGlAlphaMode::Opaque` 强制 alpha=255。`Straight` 会转成 ZUI 所需的预乘像素；`Premultiplied` 要求输出 RGB 不超过 alpha，否则报告错误。业务自行设置与此模式匹配的混合参数。
- 框架在回读前重设 READ_FRAMEBUFFER、读缓冲、PIXEL_PACK_BUFFER 与像素打包步长，避免业务状态破坏回读。
- 当前为单采样目标。业务可自行使用 MSAA FBO 并 resolve 到框架 FBO。
- 当前是同步 GPU→CPU 回读，GPU 后端还需重新上传。适合先接入已有 GL 代码，不承诺高分辨率高帧率，不包含跨 API 零拷贝或跨控件资源共享。
- 原生适配仅实现 Windows/macOS；其他平台返回明确错误。macOS 的 OpenGL 已被 Apple 弃用，要求新 GL 特性的应用应另行评估。

## 示例与验证

```sh
cargo run -p zui-opengl-example -- cpu
cargo run -p zui-opengl-example --features gpu -- gpu
cargo run -p zui-opengl-example -- cpu --snapshot artifacts/opengl.png
cargo run -p zui-opengl-example --features gpu -- gpu --smoke-ms 1000
cargo test -p zui-opengl
# 真实驱动验证必须在有图形驱动的桌面环境运行，使用独立主线程测试入口。
cargo test -p zui-opengl --test native -- --native
```

示例演示独立 vertex/fragment shader、VAO、旋转三角形、鼠标/方向键输入、暂停和按需重绘。像素预览使用 CPU 合成；窗口可选 CPU/GPU 合成。
