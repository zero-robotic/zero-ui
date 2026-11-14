# 传统控件展厅

```sh
cargo run -p widget-gallery
cargo run -p widget-gallery -- cpu
cargo run -p widget-gallery --no-default-features --features cpu -- cpu
```

省略后端默认 GPU；默认构建同时支持显式 CPU。最后一条命令仅编译 CPU 后端。不指定后端也可直接传入 `--smoke-ms 1600` 等选项。

默认 1100×800 窗口，向下滚动查看全部展示区。Tab/Shift+Tab 切换焦点，选择控件可用方向键，Escape 退出。支持 `--smoke-ms 1600` 自动退出。

默认桌面主题参考 Blender 的中性灰控件风格：紧凑尺寸、小圆角、蓝色强调、内凹输入底色；明暗主题共享尺寸，切换时保持布局稳定。

## 展示及联动

- Button 切换整个展厅的明暗主题；CheckBox 控制说明显示；RadioGroup 改变模式说明。
- Slider、Stepper 和 ProgressBar 共享数值；下方 Canvas 也同步绘制进度。
- TextInput 的内容同步到 Tabs、Disclosure 和 Accordion 的摘要中。
- ToggleButton、Switch、Disclosure 共享展开状态，任意一个操作会同步另外两个。
- SegmentedControl、ListBox、Tabs 共享方案选择，同时切换 Image 的 Contain/Cover/Fill 模式，并把 VirtualList 定位到第 1/401/801 行。
- Accordion 展示状态摘要和操作提示；VirtualList 展示 1000 条带稳定 Key 的记录。
- 外层 Scroll 提供完整展厅滚动；Container、row、column、grid、stack 组织布局。Canvas 展示便捷绘制扩展。

示例覆盖基础控件、组合容器、Canvas 和弹出交互。图片在应用启动时生成，无需联网或外部路径；中文字体通过系统字体发现与回退加载，运行程序不嵌入字体。复制粘贴通过原生剪贴板适配器接入。

`main.rs` 负责命令行，`app.rs` 负责共享状态、消息分发和窗口生命周期，`app/views.rs` 负责页面组合与基础控件。功能分组按用途组织：`overlays.rs` 管理菜单/选择/弹窗，`data_views.rs` 展示树/表格/分栏，`composition.rs` 展示容器与控件联动，`catalog.rs` 展示表单和选择器，`advanced.rs` 展示搜索、多选、拖动及延迟加载。跨分组回归测试位于 `app/tests.rs`。控件发出公共 Action，应用映射为 Message、更新状态并重建 View；控件之间不持有彼此引用，下层库不依赖示例。测试覆盖公开动作映射、共享状态与宽窄窗口布局；原生手动交互和各目标设备仍应单独验收。

## 菜单、选择与弹窗

顶部新增 Select/ComboBox、菜单栏、右键菜单（或 Shift+F10）、确认弹窗、NumberInput 和延迟 Tooltip。下拉选择、数字输入和菜单命令都联动原有数值控件；弹窗确认恢复 50%，取消保持原值。数字输入 Enter 提交、Escape 取消，非法数值显示错误。密码输入按字素掩码，可用复选框切换只读。

下方新增 ListBox 多选展示：Ctrl/Command 点击切换，Shift 范围选择，Ctrl/Command+A 全选；选择数同步显示。弹出层关闭会恢复焦点，模态期间 Tab 不进入背景页面。冒烟模式在中途切换主题并打开确认弹窗。

接口及边界说明见 [弹出交互文档](../../docs/advanced-widgets.md#p0-controls)。

菜单快捷键示例：Ctrl+Shift+R 设置为 25%；模态弹窗打开时不会触发背景快捷键。

## 树、表格与分栏

滚动区域顶部新增 TreeView、Table、Splitter、TextArea。点击树节点或表格行同步选择设备；点击树箭头展开/收起，表头点击请求升降序排序；拖动中间分隔条调整宽度，也可聚焦后使用方向键。下方巡检文本支持多行输入、自动换行、IME、粘贴和撤销，并同步显示字数。完整接口及当前边界见 [数据视图控件](../../docs/advanced-widgets.md#p1-controls)。

## 清单补充控件

展厅顶部新增必填表单、分组框/分隔线、工具栏/状态栏、日期范围和时间、DataGrid 编辑/列宽/分页、Toast、文件模式/面包屑及 RGBA 颜色选择。数值单元格 Enter 提交、Escape 取消，示例校验范围为 0–100。文件选择使用演示数据，打开/保存仅显示请求，不读写磁盘。接口与边界见 [完整控件清单](../../docs/advanced-widgets.md#controls-catalog)。

新增增强交互及能力边界见 [控件增强](../../docs/advanced-widgets.md#advanced-controls)。
