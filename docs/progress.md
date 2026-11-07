# 进度条

ProgressBar/View 提供确定进度、不确定进度、四个增长方向及主题样式。它是被动显示控件，不接收焦点、不发出修改值的动作；应用更新 View 中的进度值。

```rust
let view = ProgressBarView::themed(&theme, ProgressValue::Determinate(0.35));
let busy = ProgressBarView::themed(&theme, ProgressValue::Indeterminate);
```

## 数值与布局

Determinate 接收有限的 0..=1 比例，0 仅绘制轨道，1 填满轨道。NaN、无穷和越界值作为非法样式报错，不静默截断。使用字节数或任务数量时，由应用按业务语义计算比例；进度条不依赖带步进的 Slider 数值范围。

ProgressDirection 支持 LeftToRight、RightToLeft、BottomToTop 和 TopToBottom。轨道使用扣除 padding 后的内容区域，并在横向进度条中垂直居中、纵向进度条中水平居中。thickness 默认 8，natural_length 默认 200；确定尺寸由布局容器决定，厚度受实际内容空间限制。空内容区域不绘制，也不请求动画帧。

普通数值、颜色和禁用状态更新只需要绘制失效。方向或自然尺寸变化触发布局，非法配置必须重新验证；同一 Key 下保留控件身份。控件由公共 Widget 契约实现，核心和 CPU/GPU 后端不增加进度条分支。

## 不确定进度与动画

不确定进度以固定长度片段在轨道内部往返，默认占轨道的 25%，完整往返周期 1500 ms，请求帧间隔 33 ms。时间从公共 animate 回调传入；直接计算当前相位，不创建定时器或补跑遗漏帧。帧请求仍由上层按窗口可见性和其他期限合并。

ProgressAnimation 可配置 cycle 和 frame_interval；两者必须非零，帧间隔不得超过周期。segment_fraction 必须有限且严格介于 0 和 1。主题颜色改变保留动画相位，模式、动画配置或禁用状态改变重置计时。确定进度、禁用、空内容区域不请求动画；animation 为 None 时，不确定进度显示居中的静止片段，供减少动态效果的界面使用。

在独立 Tree 中需推进 advance_animations；WidgetHost 已接通此流程。没有时间推进时仍显示初始片段。共享调度说明见 [按需动画](animation.md)。

## 样式与验证

ProgressBarStyle 提供轨道、正常填充、禁用填充颜色以及自然长度、厚度、片段比例；默认值来自语义主题。文字标签与百分比通过 Text/布局容器组合，不让进度条承担文字排版或业务状态解释。

测试覆盖数值更新不重排、无焦点/动作、主题更新保留相位、停止条件、非法值恢复，以及四个方向的 0%、25%、100%、动画与禁用像素。CPU 和真实 GPU 均验证 1×/2× 的方向和端点；动画与禁用像素另在 1× 验证。已生成并目视检查 `artifacts/progress-cpu.png`，桌面示例的滑块下方同步显示当前比例。

```sh
cargo run -p zui-render-check --locked -- cpu artifacts/progress-cpu.png progress
cargo test -p zui-render-check --all-features --locked --test progress
# 真实 GPU 环境
cargo test -p zui-render-check --all-features --locked --test progress -- --ignored
```

当前为矩形轨道，不含圆角、缓冲进度、分段标记、圆环进度、自动数值缓动或无障碍语义；这些仍在完整控件目录中。原生窗口交互与嵌入式实机性能需继续验证。
