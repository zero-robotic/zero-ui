# 复选框与受控状态

`zui-widgets::CheckBoxView` 提供未选中、选中和混合状态，持久控件类型为 `CheckBox`。它复用 Text 排版，以及控件层内部的按压/取消状态机；核心没有复选框分支。

```rust
use zui_api::{action::ActionId, layout::{Dimension, Edges, LayoutStyle}, view::View};
use zui_widgets::{CheckBoxView, CheckState};

let check = View::new(CheckBoxView::themed(
    &theme, "显示详细说明", families, CheckState::Checked, ActionId(2),
))
.keyed("show-details")
.styled(LayoutStyle {
    min_height: Dimension::Length(40.0),
    padding: Edges::all(8.0),
    ..Default::default()
});
```

## 状态归属

选中值是应用持有的受控属性，控件不自行保存第二份业务值。激活时输出 `CheckChanged { state }` 请求；应用在 `map_action` 中识别 ActionId 和载荷，将请求映射为 Message，并在 `message` 中修改自己的状态，随后重建 View。

| 当前值 | 激活请求 |
| --- | --- |
| Unchecked | Checked |
| Checked | Unchecked |
| Mixed | Checked |

Mixed 用于应用表达部分选中，不在三种值之间自动循环。应用可以接受、拒绝或校验请求；若不回写 View，视觉值保持不变，下一次激活仍基于当前值请求。仅选择值变化产生 Paint 失效，不重新测量标签。

外部选择值或动作 ID 在按下期间改变，会取消尚未完成的手势，避免使用旧状态生成新请求。

## 输入和布局

指示框和标签所在的整块控件区域均可点击；拖出松开取消，拖回松开激活。Tab 聚焦，Space 按下后松开切换，重复键不重复激活；Enter 不作为复选框默认激活键，保留给上层。Escape、失焦、禁用和输入取消清理按压状态。禁用时不能取得焦点/捕获，也不输出值变更请求，但仍阻止主按钮点击穿透。

测量时先扣除指示框宽度与间距，再将剩余宽度交给文字服务；自然高度取指示框与文字高度的较大值。最终准备使用最终内容宽度重新排版，指示框和文字垂直居中，绘制限制在内容区域内。标签可换行，当前指示框位于整个标签块的垂直中心。

`CheckBoxStyle` 在控件层提供默认、悬停、按下、禁用颜色，以及焦点颜色、指示框尺寸与标签间距。默认尺寸为 20 个逻辑单位，间距来自主题 Sm。尺寸必须有限且不小于 4，间距必须有限且非负；非法样式通过布局错误返回，不能发布场景。修改尺寸或间距会重新布局。

## 验证与运行

```sh
cargo run -p zui-render-check --locked -- cpu artifacts/checkbox-cpu.png checkbox
cargo run -p zui-window-check --no-default-features --features cpu --locked -- cpu --widgets
```

窗口示例已加入“显示详细说明”复选框，应用接收 `CheckChanged` 后更新说明文字。离屏预览展示三态和禁用外观。

测试覆盖标签点击、请求不自动回写、应用回写跳过测量、Mixed 转换、键盘/取消/禁用、外部状态变化、剩余宽度排版、非法样式及 CPU/GPU 的 1×/2× 实际像素；共享交互逻辑提取后，按钮回归继续通过。

触摸、无障碍选中/混合语义、任意标签插槽、RTL 指示框位置及真实桌面点击验收尚未完成。原生桌面自动化连接此前失败，本轮未将窗口冒烟等同于复选框交互实测。
