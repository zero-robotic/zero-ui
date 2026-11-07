# 现有控件完善与 P0 控件

这一批保持三层职责：控件表达交互与绘制；core 处理通用事件、焦点和节点生命周期；应用持有业务状态、弹出状态及消息映射。所有新增控件仍在 no_std + alloc 的 zui-widgets，未引入窗口或渲染器依赖。

## 现有控件

### TextInput

- `placeholder`：空内容且没有输入法预编辑时显示提示，不进入实际文本、选择或提交值。
- `invalid`：应用提供的校验状态，使用主题 Danger 对应的 `style.invalid_border`；不自动拒绝输入。
- `read_only`：保留焦点、选择和复制，阻止修改。
- `password`：每个 Unicode 字素显示一个圆点，中文、组合字符、emoji 序列保持正确的光标、选择和输入法几何。复制和剪切被禁止，粘贴正常。模型、草稿与撤销历史仍保留原文；这是显示/剪贴板策略，不是加密存储或内存清零机制。
- Shift+点击扩展选择，使用 core 在每次输入上提供的修饰键快照。

### ListBox

原有单选行为保持默认。`activation_only=true` 时，方向键、分页和字符定位只移动活动项，点击、Space 或 Enter 才发出 `ListSelectionChanged`；选中当前项也可确认，用于菜单和下拉框。

`multi_selection=Some(BTreeSet<ListItemId>)` 开启受控多选，返回 `ListMultiSelectionChanged`：普通点击/导航替换选择，Ctrl/Command 点击或 Space 切换单项，Shift 扩展范围，Ctrl/Command+Shift 合并范围，Ctrl/Command+A 选择所有可用项，Ctrl/Command+方向键仅移动活动项。范围跳过禁用项。应用收到动作后更新集合；控件不提前修改业务选择。多选与 activation_only 不能同时开启。

VirtualList 仍负责可见行实现与定位，行级业务选择由应用持有；没有把 ListBox 的全量行存储移入虚拟列表。

## Popup 与焦点范围

`PopupView` 支持居中、根坐标锚点和根坐标点定位。锚点优先向下展开，底部不足而上方足够时翻转；最终位置限制在视口内，过大内容受视口约束和裁剪。

应用在视口大小的 `stack` 中，先放普通页面，再放 Popup：

```rust
let popup = PopupView::new(
    PopupPlacement::Anchor(anchor_in_root),
    Size::new(240.0, 180.0),
    dismiss_action,
).build(content);
let root = stack([page.keyed("page"), popup.keyed("popup")])
    .style(|s| {
        s.width = Dimension::Percent(1.0);
        s.height = Dimension::Percent(1.0);
    })
    .build();
```

锚点由应用以根局部逻辑坐标提供；没有隐式 DOM 查询或自动绑定任意节点的 portal。窗口布局、滚动或触发点变化后，应用需要提供新的锚点。示例的触发区位于固定顶部工具区。

交互 Popup 使用公共 `Widget::focus_scope()` 契约。core 在布局收敛后进入最上层范围，Tab/Shift+Tab 不越界，显式越界焦点请求失败，背景输入不会送给应用快捷键。新增范围取消外部指针捕获，移除后恢复仍存在且可用的旧焦点；嵌套范围按层恢复。初始焦点优先进入可聚焦容器内的叶子控件；没有可聚焦子项时，范围自身作为键盘后备接收者。

Escape 或外部指针按下产生 `PopupDismissed { reason }`，应用决定何时移除描述。外部点击被当前层消费，不穿透激活背景。`close_on_outside=false` 适用于确认弹窗；Escape 仍可关闭。被动 Popup（`dismiss=None`）不占焦点，不自身参与命中，内容应使用非交互控件。

## Select / ComboBox

`SelectView::themed` 接收 ListItem、受控选中 ID、字体和 ActionId；`build()` 返回触发按钮，`popup(anchor,height,dismiss)` 返回应挂载到根层的列表。

`SelectEvent::Open` 请求展开，`Selected(id)` 提交选择。方向键、Home/End、PageUp/PageDown 和字符定位移动活动项，跳过禁用项，Enter/Space 或点击提交；Escape 关闭而不改变业务选中项。应用确认后移除弹出层并恢复触发按钮焦点。

`SelectView` 为非编辑选择；`ComboBoxView` 已独立实现搜索、自由文本和多选下拉，使用独立 ComboBoxEvent，见 [控件增强](advanced-controls.md)。

## Menu / MenuBar / ContextMenu

- `MenuView`：菜单项、禁用态、勾选标记、快捷键文字以及子菜单。
- `MenuBarView`：顶层按钮发出 `MenuEvent::Open(id)`，应用选择对应菜单描述。
- `ContextMenuRegion`：包装内容，右键发出区域局部位置；放在根层时可直接作为根坐标。Shift+F10 也可请求菜单。
- `MenuEvent::OpenPath(path)` 请求打开子菜单；Right 打开，Left 返回，Enter/Space 执行。应用持有 open_path；子菜单沿路径生成独立弹出层。路径上每层验证 ID，最多 16 层。
- `MenuEvent::Selected(id)` 表示执行命令；关闭和业务执行由应用处理。
- `MenuShortcut` 与 `resolve_shortcut()` 解析启用命令，忽略按键重复与禁用分支。应用决定快捷键相对文本编辑器和模态窗口的优先级；不在底层注册全局快捷键。

菜单当前使用列表行，已增加分隔条目和当前活动层悬停展开；尚不包含跨层悬停切换和原生系统菜单。子菜单仍可通过键盘及点击完整操作。

## Dialog / MessageBox

`DialogView` 组合标题、调用方提供的主体和底部内容，默认居中、遮罩、禁止外部点击关闭。`MessageBoxView` 提供消息文字和结果按钮，可配置 Accept/Cancel/Yes/No 等 `DialogResult`。

内容区提供滚动，视口缩小时仍可通过滚动与焦点揭示到达结果按钮。确认和取消发出动作，应用决定执行或保持业务状态。焦点范围、嵌套与关闭恢复复用 Popup；不存在 core 对 Dialog 类型的特殊判断。示例确认后把数字输入、Slider、Stepper、进度条和 Canvas 一起恢复为 50%。

## Tooltip

`TooltipView` 是根级提示宿主：接收纯内容工厂和根坐标 `TooltipAnchor` 列表，保留稳定内容节点。默认悬停 500 ms 后出现，移出、点击、滚动或 Escape 隐藏；静止时按期限唤醒，不持续刷新。Tooltip 绘制在普通内容后，不占焦点、不拦截指针。

`focused` 可由应用显式指定键盘提示索引；尚不自动查找任意后代焦点对应的提示。内容工厂可能重复调用，不能在其中修改模型或执行 I/O。core 对祖先发送成对的进入/离开通知，避免鼠标离开整个内容区后遗留提示。

## NumberInput

`NumberInputView` 复用 TextInput 的选择、输入法、剪贴板、撤销和绘制能力。应用提供有限 `value`、`NumericRange`、显示精度（0～12）。单位可用普通 row + Text 组合，不混入数值解析。

- 编辑中的空字符串、负号和小数点保留在本地，不立即写入业务数值。
- Enter 解析并检查范围：成功发出 `NumberChanged`，失败发出 `NumberRejected` 并显示错误边框；不静默截断或钳制非法输入。
- Up/Down 按步长提出新值，Shift 为十步；外部值仍由应用确认。
- Escape 取消编辑，失去焦点也恢复已确认值；没有待取消内容时，Escape 继续交给外层弹窗。本版不在失焦时自动提交。输入法预编辑期间，候选键优先交给输入法。
- 外部值、范围或精度改变会更新本地编辑描述，避免保留过时的编辑内容。

## 示例与验证入口

运行 `cargo run -p zui-controls-example -- cpu`。顶部展示下拉选择、菜单/右键菜单、确认弹窗、数字输入、密码与只读切换；下方展示列表多选及原有控件。GPU 使用 `--no-default-features --features gpu -- gpu`。

`tools/render-check/tests/p0.rs` 覆盖焦点范围/恢复、防穿透、边界定位、选择确认、子菜单、快捷键、数值编辑、密码字素映射和剪贴板策略、多选范围、Tooltip 时序、占位提示，以及共用 CPU/GPU 像素用例。真实 GPU 测试需要显式 `--ignored`。完整无障碍平台适配、触摸手势和真实嵌入式设备验收不属于本批完成范围。

### 矢量图标

Select 的下拉箭头及 Menu 的勾选/子菜单箭头已改用统一矢量图标，不再依赖字体符号覆盖。Button 和 ListItem 提供独立图标槽位；自定义路径与主题配置见 [图标说明](icons.md)。
