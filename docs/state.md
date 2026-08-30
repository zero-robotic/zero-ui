有。把 **Qt / GTK 这类成熟桌面框架几十年的经验**和 Rust 的所有权、enum/trait、消息传递结合起来，我认为确实可以比上一版“全局 Store + Selector + Command”再进一步。

我现在更推荐的“终局形态”是：

> **Retained Runtime + Declarative Component + Typed Property/Signal + Scoped Action + Model/View + Optional Store**

最关键的变化是：

> **Store 从“框架核心”降级成“应用状态管理的一种工具”。**

框架本身不应该要求所有共享数据都进入 Store。

---

# 1. 先看 Qt/GTK 为什么成熟

Qt 实际上同时提供了几套不同机制：

```text
QObject Tree
Property
Signal / Slot
Event
Action
Model / View / Delegate
```

它并没有试图用一种机制解决全部 UI 问题。Qt 的 Signal/Slot 强调类型安全和发送方/接收方解耦；Model/View/Delegate 则专门解决集合数据与呈现解耦。([Qt Documentation][1])

GTK 也是类似：

```text
GObject
Property / Binding
Signal
GAction
GListModel
ListView
```

尤其值得借鉴的是 GTK 的 Action 有明确作用域，可以属于 application、window 或 widget，而列表数据又单独通过 `GListModel` 建模。([https://docs.gtk.org][2])

所以成熟框架告诉我们的其实是：

> **UI 中不同关系，本来就应该使用不同抽象。**

---

# 2. 我认为 Rust 最终应该做成两层

这是我现在最推荐你的总体设计：

```text
               Application
                    │
          Declarative Component Layer
                    │
             View Description
                    │
              Reconciliation
                    │
                    ▼
        ┌───────────────────────┐
        │   Retained UI Core    │
        │                       │
        │ Widget Arena          │
        │ WidgetId              │
        │ Layout                │
        │ Event                 │
        │ Focus                 │
        │ Accessibility         │
        │ Rendering             │
        └──────────┬────────────┘
                   │
          Wayland / X11 / BSD
```

也就是说：

### 底层

真正维护：

```text
Widget Tree
Layout Tree
Focus Tree
Hit Test
IME
Pointer Capture
Accessibility
Animation
Scene
```

它是 **retained mode**。

### 上层

开发者写：

```rust
fn view(&self) -> impl View {
    column((
        label(self.title.clone()),
        button("Save")
            .on_click(AppAction::Save),
    ))
}
```

View 本身是轻量描述。

Runtime 把它 reconcile 到真正的 retained Widget Tree。

Xilem 当前也是类似方向：高层使用轻量 View，并把它 diff 到 retained UI；其底层 Masonry 则专门负责 widget tree 和基础 UI runtime。([GitHub][3])

对于你想做 Linux/BSD Desktop，我认为这种**双层结构明显优于纯 retained 或纯 reactive**。

---

# 3. 最核心：不要只有一种“共享数据”

我会把数据关系明确分成 **6 类**。

```text
                    UI DATA

          ┌────────────┼──────────────┐
          │            │              │
      Property      Component       Model
      Binding         State        Collection
          │            │              │
       UI数据       局部状态       大规模数据

          ┌────────────┼──────────────┐
          │            │              │
        Store        Context        Service
      业务状态       环境数据        系统能力
```

每种机制解决完全不同的问题。

这比“所有东西 Store 化”更健康。

---

# 4. 第一类：Property —— 控件属性

这是应该从 Qt / GTK 强烈吸收的东西。

例如：

```rust
struct Slider {
    value: Property<f32>,
    min: Property<f32>,
    max: Property<f32>,
    enabled: Property<bool>,
}
```

可以：

```rust
slider.value().set(0.8);
```

也可以绑定：

```rust
slider.bind_value(volume);
```

甚至：

```rust
label.bind_text(
    volume.map(|v| format!("{v:.0}%"))
);
```

形成：

```text
volume
  │
  ├──── Slider.value
  │
  └──── map()
          │
          ▼
      Label.text
```

GTK 本身就有 Property Binding，可以让一个对象的属性变化自动更新另一个对象的属性。([https://docs.gtk.org][4])

但是 Rust 版本可以比 GTK 更好：

```rust
Property<f32>
Property<String>
Property<Color>
```

全部编译期类型安全。

而不是：

```text
property name: "volume"
GVariant
runtime type checking
```

---

# 5. 第二类：Signal —— 控件向外通知

这里应该借 Qt。

例如：

```rust
Button {
    clicked: Signal<()>,
}
```

Slider：

```rust
Slider {
    value_changed: Signal<f32>,
}
```

TextField：

```rust
TextField {
    submitted: Signal<String>,
}
```

使用：

```rust
slider
    .value_changed()
    .connect(|value| {
        println!("{value}");
    });
```

但是我不会完全复制 Qt。

Qt：

```cpp
connect(
    slider,
    &Slider::valueChanged,
    label,
    &Label::setValue
);
```

Rust 框架更推荐：

```rust
slider.on_value_changed(|cx, value| {
    cx.dispatch(
        AudioAction::SetVolume(value)
    );
});
```

也就是说：

> Signal 更适合作为 **Widget Output**。

而不是让整个应用变成：

```text
Signal
 ↓
Signal
 ↓
Signal
 ↓
Signal
```

否则又会掉进 reactive spaghetti。

---

# 6. 第三类：Action —— “我要做什么”

这里我会大量参考 GTK。

GTK 的 Action 本质上就是：

> 一个不包含 UI 表现信息的功能动作。

而且可以具有 application/window/widget 等不同 scope。([https://docs.gtk.org][5])

Rust 可以把它做得更漂亮：

```rust
enum AppAction {
    Quit,
    OpenSettings,
}

enum WindowAction {
    Close,
    Minimize,
    Maximize,
}

enum EditorAction {
    Copy,
    Paste,
    Undo,
    Redo,
}
```

然后：

```rust
button.on_click(
    Action::new(EditorAction::Save)
);
```

甚至快捷键：

```rust
shortcut("Ctrl+S")
    .action(EditorAction::Save);
```

菜单：

```rust
menu.item("Save")
    .action(EditorAction::Save);
```

Toolbar：

```rust
toolbar.button("Save")
    .action(EditorAction::Save);
```

三者全部调用：

```text
EditorAction::Save
```

这对 Desktop 极其重要。

---

# 7. Action 必须具有 Scope

我建议直接吸收 GTK 的这个设计。

例如：

```text
Application
 ├── app.quit
 ├── app.settings
 │
 └── Window
      ├── win.close
      ├── win.maximize
      │
      └── Editor
           ├── editor.copy
           ├── editor.paste
           └── editor.undo
```

但是 Rust 不要用字符串作为主要 API。

可以：

```rust
cx.action(EditorAction::Copy);
```

Runtime 根据 component tree 向上查找：

```text
TextEditor
   │
   ▼
Editor scope
   │
   ▼
Window scope
   │
   ▼
Application scope
```

这比：

```rust
WidgetId + Command
```

在很多场景下更优秀。

因为发送者甚至不需要知道接收者是谁。

---

# 8. Command 仍然存在，但只解决“命令式 Widget 操作”

例如：

```rust
text_input.focus();
```

这种操作不是 Action。

它是：

> 对某个具体 UI entity 的 imperative request。

所以：

```rust
cx.command(
    text_field,
    TextFieldCommand::Focus
);
```

适合：

```text
focus
scroll_to
select_all
open_popup
start_editing
ensure_visible
```

因此：

```text
Action
```

和：

```text
Command
```

千万不要混。

我的分类会是：

| 类型         | 含义                |
| ---------- | ----------------- |
| `Signal`   | Widget 发生了什么      |
| `Action`   | 用户/程序想做什么         |
| `Command`  | 让具体 UI Entity 做什么 |
| `Property` | Widget 当前是什么状态    |

这四个概念非常重要。

---

# 9. 第四类：Model —— 列表/树形数据千万不要塞普通 Store

这是上一版里还不够完善的地方。

假设做文件管理器：

```text
100,000 files
```

千万不要：

```rust
Signal<Vec<File>>
```

或者：

```rust
Store<Vec<File>>
```

然后每次：

```rust
files.set(new_vec);
```

专业桌面框架应该有：

```rust
trait ListModel {
    type Item;

    fn len(&self) -> usize;

    fn get(
        &self,
        index: usize,
    ) -> Option<&Self::Item>;
}
```

并支持：

```rust
enum ModelChange {
    Insert {
        index: usize,
        count: usize,
    },

    Remove {
        index: usize,
        count: usize,
    },

    Update {
        index: usize,
        count: usize,
    },

    Reset,
}
```

于是：

```text
FileModel
    │
    ├─ ListView
    ├─ GridView
    └─ TreeView
```

GTK 4 就明确采用这种模型：View 读取 `GListModel`，并可以再包 FilterModel、SortModel 等适配模型。([https://docs.gtk.org][6])

---

# 10. 再做 Adapter Model

这是桌面框架非常值得拥有的能力。

```text
DirectoryModel
      │
      ▼
 FilterModel
      │
      ▼
 SortModel
      │
      ▼
 SelectionModel
      │
      ▼
 ListView
```

Rust：

```rust
let model = directory
    .filter(|file| !file.hidden)
    .sort_by(|a, b| a.name.cmp(&b.name))
    .selectable();
```

然后：

```rust
ListView::new(model);
```

这样 File Manager、Settings、Process Manager、Launcher 都会非常舒服。

---

# 11. 第五类：Component State

不要所有状态都放 Widget，也不要全放 Store。

例如：

```rust
struct SearchPanel {
    query: String,
    expanded: bool,
}
```

这是组件自己的 state。

我会定义：

```rust
trait Component {
    type Message;

    fn update(
        &mut self,
        message: Self::Message,
        cx: &mut ComponentCtx,
    );

    fn view(
        &self,
        cx: &ViewCtx,
    ) -> impl View;
}
```

例如：

```rust
enum SearchMsg {
    QueryChanged(String),
    Submit,
}
```

于是：

```text
Event
  ↓
Component Message
  ↓
Component State
  ↓
View
```

这其实借鉴：

```text
Elm
SwiftUI
Flutter
Xilem
```

但又保留 retained runtime。

---

# 12. Store 应该放在哪里？

现在我会改变上一条里的建议：

不要：

```text
Application
    ↓
一个巨大 Store
    ↓
整个 UI
```

而应该：

```text
Application
 │
 ├── DesktopModel
 │
 ├── WindowManagerModel
 │
 ├── NotificationModel
 │
 └── Component Tree
       │
       ├── FileManager Store
       ├── Settings Store
       └── Local Component State
```

也就是说：

> **状态归属于最小合理作用域。**

这比 Redux 那种单一 Global Store 更适合 Desktop。

---

# 13. Widget Runtime 本身使用 Arena，而不是 Rc 树

这是 Rust 特别应该区别于 Qt 的地方。

Qt 的核心是 `QObject` 对象树，而且对象树承担所有权管理。Qt 官方也把 hierarchical object tree 作为核心对象模型能力之一。([Qt Documentation][7])

Rust 不应该复制：

```rust
Rc<RefCell<dyn Widget>>
```

我会用：

```text
WidgetArena
```

内部：

```rust
struct WidgetNode {
    parent: Option<WidgetId>,
    children: SmallVec<WidgetId>,
    widget: Box<dyn Widget>,
}
```

ID：

```rust
struct WidgetId {
    index: u32,
    generation: u32,
}
```

结构：

```text
WidgetArena
 │
 ├── ID 1 Window
 │
 ├── ID 4 Column
 │
 ├── ID 8 Button
 │
 └── ID 9 Label
```

真正的 owner 永远只有：

```text
Runtime
```

Widget 之间只是：

```text
WidgetId
```

不是对象指针。

这会让 Rust 所有权模型极其干净。

---

# 14. Component 和 Widget 必须分开

这是我尤其推荐你从一开始就定下来的边界。

### Widget

是 runtime entity：

```text
layout
paint
hit test
focus
pointer
IME
accessibility
```

例如：

```text
TextInput
ScrollView
List
Button
```

### Component

是 application abstraction：

```text
state
message
view
business logic
```

例如：

```text
FileManager
SettingsPanel
DesktopPanel
NetworkMenu
```

于是：

```text
Component
    │
    ▼
View
    │
 reconciliation
    ▼
Widget
    │
    ▼
Layout / Paint
```

---

# 15. Context 继续保留

非常适合：

```text
Theme
Locale
FontSystem
ScaleFactor
Accessibility
Clipboard
Platform
Window
```

使用：

```rust
let theme = cx.get::<Theme>();
```

支持树形覆盖：

```text
App
 Theme=Light
 │
 └── Window
       │
       └── Panel
            Theme=Dark
```

这一部分不进入 Store。

---

# 16. Service 再单独一层

Linux/BSD Desktop 以后一定会大量调用：

```text
DBus
PipeWire
NetworkManager
UDisks
BlueZ
Wayland
Filesystem
Notifications
Polkit
```

这些不能塞到 Widget。

应该：

```rust
trait Service {
    ...
}
```

然后：

```rust
cx.service::<Clipboard>();
cx.service::<FileSystem>();
cx.service::<NetworkManager>();
```

异步：

```rust
cx.spawn(async move {
    let files = fs.list(path).await;

    AppAction::DirectoryLoaded(files)
});
```

---

# 17. 最终完整架构

如果现在让我替你定整个框架，我会定成下面这样：

```text
                         Application
                              │
          ┌───────────────────┼───────────────────┐
          │                   │                   │
       Services            App Model            Actions
    DBus/FS/Audio/...          │              App/Window/
                               │               Component
                               │
                               ▼
                       Component Tree
                               │
                    ┌──────────┼──────────┐
                    │          │          │
                  State     Context      Model
                    │          │          │
                    └──────────┼──────────┘
                               ▼
                         Declarative View
                               │
                         Reconciliation
                               │
                               ▼
                     ┌──────────────────┐
                     │  Retained Core   │
                     │                  │
                     │ Widget Arena     │
                     │ WidgetId         │
                     │ Property         │
                     │ Signal           │
                     │ Command          │
                     └────────┬─────────┘
                              │
            ┌─────────────────┼──────────────────┐
            │                 │                  │
          Layout            Events             Scene
            │                 │                  │
            └─────────────────┼──────────────────┘
                              ▼
                           Renderer
                              │
                       Vello / Skia / wgpu
                              │
                              ▼
                     Wayland / X11 / BSD
```

---

# 18. 然后定义一个非常重要的通信规则

我建议直接把它写进框架设计文档：

> **不同关系使用不同通信机制。**

具体就是：

| 情况                    | 最佳机制                    |
| --------------------- | ----------------------- |
| Button 点击             | `Signal / Callback`     |
| Widget 属性联动           | `Property / Binding`    |
| 用户执行 Save             | `Action`                |
| 让某 TextInput focus    | `Command + WidgetId`    |
| Component 内部状态        | `Component State`       |
| 跨 Component 业务数据      | `Feature Store / Model` |
| 大列表/树                 | `ListModel / TreeModel` |
| Theme / DPI           | `Context`               |
| 文件/DBus/网络            | `Service`               |
| Parent 操作自己的 Child    | 允许直接调用                  |
| 任意 Widget 找另一个 Widget | **禁止直接引用**              |

最后这一条非常关键。

---

# 19. 不建议的“终极架构”

我会明确避免四个方向。

### 纯 Qt 模型

```text
QObject everywhere
```

Rust 没必要再造一个动态对象系统。

Qt 的 meta-object system 很强，但它本质上弥补了 C++ 原生缺少 introspection、properties、动态 messaging 的问题。([Qt Documentation][8])

Rust 已经有：

```text
trait
enum
generics
proc macro
derive
ownership
```

所以应该尽可能静态化。

### 纯 Signal

最终容易：

```text
Signal → Effect → Signal → Effect
```

难追踪。

### 纯 Redux

大型桌面所有东西都进一个 Store，会变得非常笨重。

### 纯 Immediate Mode

做工具型 UI 很舒服，但对：

```text
IME
Accessibility
复杂文本
Desktop
超大 List
Drag & Drop
Window Management
```

retained runtime 通常更自然。

---

# 20. 如果让我给这个架构一个名字

我会称它：

> **Rust Native Hybrid UI Architecture**

核心就是：

```text
Retained UI Core
        +
Declarative Component
        +
Typed Property / Signal
        +
Scoped Action
        +
Model / View
        +
Optional Store
```

其中我认为最关键的一次架构升级，就是从上一版：

```text
Store
 ↓
Selector
 ↓
Widget
```

升级到：

```text
             Model
              │
       Component State
              │
Property ─── View ─── Context
              │
              ▼
        Retained Widget
```

而业务流程：

```text
Signal
  ↓
Action
  ↓
Component / Model / Store
  ↓
View
```

命令式操作：

```text
Command
  ↓
WidgetId
  ↓
Runtime
```

这套体系既保留了 **Qt/GTK 在大型桌面软件里已经验证几十年的工程经验**，又不会把 C++/GObject 那套动态对象模型生搬硬套进 Rust；同时也符合现在 Rust UI 在探索的“高层 declarative + 底层 retained runtime”方向。([GitHub][3])

如果目标真的是以后做到 Linux/BSD Desktop，我会**优先把 `WidgetArena + WidgetId + Property + Action Scope + Model` 这五块设计对**。Renderer 反而可以后换；这五个一旦 API 定错，后面改起来最痛。

[1]: https://doc.qt.io/qt-6/signalsandslots.html?utm_source=chatgpt.com "Signals & Slots | Qt Core | Qt 6.11.2"
[2]: https://docs.gtk.org/gtk4/actions.html?utm_source=chatgpt.com "Gtk – 4.0: Overview of actions in GTK"
[3]: https://github.com/linebender/xilem/blob/main/ARCHITECTURE.md?utm_source=chatgpt.com "xilem/ARCHITECTURE.md at main · linebender/xilem · GitHub"
[4]: https://docs.gtk.org/gobject/method.Object.bind_property.html?utm_source=chatgpt.com "GObject.Object.bind_property"
[5]: https://docs.gtk.org/gio/iface.Action.html?utm_source=chatgpt.com "Gio.Action"
[6]: https://docs.gtk.org/gtk4/section-list-widget.html?utm_source=chatgpt.com "Gtk – 4.0: List Widget Overview"
[7]: https://doc.qt.io/qt-6/qtcore-index.html?utm_source=chatgpt.com "Qt Core | Qt 6.11.2"
[8]: https://doc.qt.io/qt-6/metaobjects.html?utm_source=chatgpt.com "The Meta-Object System | Qt Core | Qt 6.11.2"
