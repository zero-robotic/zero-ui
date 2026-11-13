# README 预览资源

本目录图片由仓库的真实控件和 CPU 离屏渲染路径生成，逻辑尺寸为 640×360。它们展示控件组合与样式，不是完整原生窗口截图，也不表示 GPU 或嵌入式实机验收。

在仓库根目录重新生成：

```sh
cargo run -p zui-render-check --no-default-features --features cpu --locked -- cpu assets/readme/desktop-dark.png desktop-dark
cargo run -p zui-render-check --no-default-features --features cpu --locked -- cpu assets/readme/desktop-light.png desktop-light
cargo run -p zui-render-check --no-default-features --features cpu --locked -- cpu assets/readme/dashboard.png panel
```

控件样式变化后同步更新图片并检查文字、裁剪和主题对比度。图片使用仓库固定的 Noto Sans SC 字体，来源与许可见[字体说明](../fonts/README.md)。
