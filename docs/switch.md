# 受控开关 Switch

SwitchView/Switch 提供独立布尔开关，使用公共 Widget、Painter 和主题契约，不依赖核心、具体布局引擎或 Vello。标签通过 row 和 TextView 等普通控件组合；当前没有标签点击联动或无障碍名称关联。

```rust
let mut toggle = SwitchView::themed(&theme, model.enabled, ActionId(9));
toggle.transition = None; // 关闭运动，适用于减少动效或低资源配置
let view = View::new(toggle);
```

## 值与交互

checked 是应用控制的值。鼠标主键在控件内按下、仍在控件内松开时输出 SwitchChanged { checked: !checked }；不会提前修改内部值。应用处理动作并重建 View 后才确认变化。未回写时重复激活仍请求相同目标值。

复用现有 Interaction 状态机：鼠标按下请求焦点与捕获，拖出松开取消；聚焦后 Space 按下显示按压状态，释放激活，重复按下不重复触发。Escape、失焦、捕获丢失以及禁用/动作或值变化取消未完成激活。Enter 不切换，Control/Alt/Super 修饰的 Space 不激活。禁用开关仍可命中以阻止穿透，但不能聚焦或激活。触摸手势及原生设备输入验收仍待完成。

## 视觉、布局与动画

SwitchStyle 分别提供普通、悬停、按下、禁用配色，以及焦点环、轨道宽高和滑块内缩。每套颜色包含开/关轨道与开/关滑块颜色、边框。默认宽 48、高 26、内缩 3 逻辑像素，主题映射保证深色主题下关闭滑块可辨识。

轨道采用胶囊路径，滑块采用圆形路径，绘制裁剪到内容区域。轨道在可用内容区域居中；小于自然尺寸的区域会裁剪，不自动压扁形状。布局仍支持外层 padding、宽高和容器组合。宽高必须有限、宽不小于高、高至少 4；内缩至少 1 且两倍小于高度。非法样式在测量/准备阶段返回 InvalidStyle，固定尺寸不能绕过校验。

默认确认值变化后，滑块在 120 毫秒内平滑移动，轨道/滑块颜色立即采用确认值。首次构建直接显示当前值。动画使用注入的 elapsed 时钟，仅产生 Paint 失效和按需下一帧；途中反向从当前显示位置继续。transition=None 或禁用时立即归位并停止调度；Some(Duration::ZERO) 被拒绝。这里的开关过渡不是通用动画服务或所有控件动效已完成。

## 验证

三项行为测试覆盖受控请求、鼠标捕获与取消、Space/Enter/Escape、禁用、动画反向、减少动效和非法样式恢复。CPU/GPU 像素用例分别检查开关滑块位置、轨道颜色、圆角和内容边界，在 1×/2× 下验证。

```sh
cargo test -p zui-render-check --test switch --all-features --locked --offline
cargo test -p zui-render-check --test switch --all-features --locked --offline switch_track_and_thumb_render_gpu -- --ignored
cargo run -p zui-render-check --locked --offline -- cpu artifacts/switch-cpu.png switch
```

原生点击、触摸、屏幕阅读器语义与无 GPU 嵌入式性能仍需验收。预览是实际 Tree/Taffy/Parley/Vello CPU 组合生成的静态图，不代表这些设备验收已经完成。
