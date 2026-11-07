# 独立自定义控件示例

`Meter` 展示如何仅通过 `zui-api::widget::Widget`、ZUI 几何值和 `Painter` 实现控件。该包不依赖核心、布局引擎、窗口系统或 Vello。

控件提供固有测量和局部坐标绘制，数值必须是 0 到 1 之间的有限数。更改数值只需要 `Invalidation::Paint`。实例由上层创建并插入 `zui-core::Tree`；核心通过公共契约调用，不需要添加 Meter 分支或注册表。

`MeterView` 提供独立的声明式属性描述，构造时校验数值，分别实现 `ViewSpec::build` 和 `update`。应用可用 `View::new(MeterView::new(value, foreground, background)?)` 生成描述，再设置 Key 和布局样式，通过 `Tree::reconcile` 更新。重新生成描述不会重新创建已有兼容控件，详见 [View 协调规则](../../docs/view-reconciliation.md)。

上层完整装配与验证位于 `tools/render-check/tests/custom_widget.rs`，包含真实像素断言和跳过重新布局的检查：

```sh
cargo test -p zui-render-check --test custom_widget --locked
cargo test -p zui-render-check --all-features --test custom_widget --locked -- --ignored
```

第二条命令需要真实 GPU。该示例验证自定义控件扩展边界，尚未提供文字、事件、焦点或无障碍语义，不替代后续正式控件库。
