# 矢量图标

ZUI 的图标统一由 `zui-widgets::Icon` 表示，与字体、操作系统及 CPU/GPU 后端无关。实现位于 widgets 的独立 icon 模块，仅调用下层公共 Path/Painter 接口；core、paint 和渲染后端不识别具体图标语义。保持 no_std + alloc，无新增依赖。

## 使用与扩展

```rust
use zui_api::view::View;
use zui_widgets::{Icon, IconKind, IconView};

let arrow = Icon::builtin(IconKind::ChevronDown);
let view = View::new(IconView::themed(&theme, arrow));
```

内置 ChevronDown/Right/Up/Left、Check、Close、Plus、Minus。所有内置图标采用 24×24 设计坐标和 2 单位描边，居中等比缩放并裁剪到目标区域。独立 IconView 的默认尺寸取主题 Body 字号、颜色取 Text，可通过 size/color 覆盖；它不获取焦点或拦截命中。

`Icon::custom(path, stroke_width)` 接受相同坐标系的公共 Path；None 表示填充，Some 表示描边。非法路径或非有限、非正、超过 24 的描边宽度返回 None。几何通过 Arc 共享，应用可在自己的资源表中按名称注册和复用 Icon，无需全局可变注册表或修改框架枚举。未来 SVG 解析应由独立适配模块转换为公共几何，不进入基础控件和绘制层。目前为单色图标，不包含 SVG 解析、多色分层或动画图标。

## 控件集成

- ButtonView.trailing_icon：在文字右侧预留图标及间距，图标颜色随普通、悬停、按下、禁用状态变化。
- Select：内置 ChevronDown，替换原来追加的 Unicode 三角字符。
- ListItem.leading_icon/trailing_icon：支持自定义图标；同一 ListBox 按整列是否存在图标预留空间，保持文字对齐。
- Menu：勾选使用 Check，子菜单使用 ChevronRight，不再把图标拼进标签文本。

文字继续由文本引擎布局，图标不需要字体回退。结构体字面量构造 ButtonView/ListItem 的调用方需补充新增字段；使用 themed/new 构造器的调用方无需修改。

## 验证

icons 集成测试覆盖无字体环境、内置/自定义图标、1×/2× 像素、居中等比显示、被动命中、失效级别和非法参数。P0 回归新增 Reject 缺字策略下的中文下拉按钮、勾选与子菜单布局，防止重新引入字体符号依赖。原有菜单、下拉和列表交互测试继续保留。
