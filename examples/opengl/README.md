# OpenGL 控件

演示在 ZUI 页面中嵌入原生 OpenGL 三角形，使用按钮控制持续刷新或按需重绘。

```sh
cargo run -p opengl --locked
cargo run -p opengl --locked -- cpu --smoke-ms 1600
```

`src/main.rs` 负责页面、动作、窗口及快照入口；`src/triangle.rs` 负责 GL 资源和着色器绘制。点击画面或按左右方向键旋转，按钮可暂停/继续或请求重绘。

即使选择 CPU 合成，也需要原生 OpenGL 环境。此示例面向渲染扩展；入门请先阅读 [minimal](../minimal/README.md)。接口和限制见 [OpenGL 文档](../../docs/rendering.md#opengl-view)。
