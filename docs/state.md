# 应用状态与绑定

推荐应用范式是声明式 View、有明确作用域的类型化状态、属性绑定与类型化事件。`zui-app::reactive` 适配现有 `ViewApplication` / `WidgetHost`，继续使用保留式控件树、原有布局与渲染管线。旧应用的 `map_action` / `message` 不需要迁移。

## 从最小应用开始

`ReactiveApplication::new` 调用一次创建闭包，传入 `Scope` 与 `Events<Model>`。组件实现 `Component::view` 描述界面；窗口关闭已有默认处理。状态和事件在创建阶段建立，视图构建只读取与描述。

```rust,ignore
let count = scope.property(0_u64);
let increment = events.on(|model: &mut Counter, _: &ButtonActivated| {
    model.count.set(model.count.get() + 1).unwrap();
    ViewAction::default()
});
```

在 `view` 中读取 `count.get()` 即登记依赖。`increment.view(|action| ButtonView::themed(..., action))` 用类型化事件构造按钮；不需要自行分配 ActionId、匹配不透明载荷或发送重建消息。完整可运行代码见 [minimal](../examples/minimal/README.md)。

## 状态归属和访问权限

- 控件持有 hover、光标、选择区、IME 和临时编辑草稿等内部状态。
- 页面或组件作用域持有共享选择、设置草稿和筛选条件。
- 业务模型持有持久化数据、权限与领域规则，使用事件或后台完成回调提交结果。
- `Computed<T>` 表达纯派生值，如按钮可用性和显示文本，不执行文件保存、网络请求等操作。

`Scope::property` 返回可写 `Property<T>`；`read_only()` 返回没有写接口的 `Read<T>`。可用 `Scope::named_property` 给诊断命名，名称不参与身份和绑定。属性句柄的克隆共享同一个值，不复制业务状态。相等写入不产生变更。

作用域控制写权限，组件字段持有值句柄。作用域结束后属性仍能读取最后值，写入返回 `WriteError::Disposed`；作用域注册的事件和后台结果也失效。`ScopeToken` 是弱生命周期能力，不延长组件寿命。

状态归属独立于布局父子关系。移动、隐藏或重新挂载控件不会重建仍由页面持有的属性。若希望关闭子页面即清理状态，应让子页面持有自己的 `Scope`，而不是把所有状态留在应用根作用域。

## 控件共享值

```rust,ignore
let size = scope.property(18.0);
let slider = events.binding::<SliderView>(&size);
let number = events.binding::<NumberInputView>(&size);

// 在 view 中：
slider.view(|value, action| SliderView::themed(&theme, range, value, action));
number.view(|value, action| NumberInputView::themed(&theme, value, range, families, action));
```

绑定的属性是初始值和后续值的唯一来源。控件请求经既有动作队列写入属性，下一次准备视图时读回。没有“控件 A 监听 B，B 再写 A”的双向回调环。

公共契约 `ActionView` 描述主要动作类型，`ValueView` 描述可控值和哪些动作请求修改它。当前按钮、滑块、数值输入、复选框、开关、切换按钮、单选组与文本输入已接入相应契约。第三方控件也能自行实现这些契约，核心无需注册或识别其类型。带显式构建错误的 Stepper，以及复合控件的弹层和多个动作，继续使用原有公开构建/动作接口，不隐藏它们的错误处理。

数值输入中的 `-` 等未完成/无效文本留在控件内部，只有合法提交改变属性。文本输入的 Changed 更新绑定，Submitted / Rejected 不被误当作新值。若提交需要触发业务操作，可注册类型化事件处理器并明确区分动作。

## 派生状态和更新边界

`Read::map` 或 `Computed::new` 创建只读派生值。依赖由每次读取发现，支持条件分支；派生值当前按读取求值，不缓存计算结果。递归计算检测到环时给出诊断，避免无限递归。框架禁止派生计算和视图观察期间写入属性；用户闭包中的任意外部副作用仍需由调用方避免。

属性更新立即可读；合并的是刷新工作，而非事务。所有控件动作在输入传播结束后按顺序处理，业务事件不会合并。普通 Rust 字段不会自动被观察：可以继续返回 `ViewAction::rebuild()`，或把需要自动刷新的界面数据通过属性暴露。外部业务模型可以将快照写入属性，不需要把整个业务层改写为响应式对象。

## 组件区域和自动局部刷新

`Region::new` / `Region::named` 创建独立观察区域，捕获所需属性和事件句柄；在父组件中放置 `region.view()`。

- 单个区域变化、父视图自身没有变化时，框架定位区域并调用既有子树协调接口。
- 父区域变化覆盖子区域；多个独立分支变化时保守重建根视图。
- 业务不需要持有 NodeId 或选择更新节点。
- 已隐藏的区域不参与刷新；重新挂载时读取最新状态。

区域的根样式和 key 必须写在区域构建闭包内部，不能在 `region.view()` 返回后额外修改，否则下一次局部更新无法复现那些修改。同一区域实例只能挂载一次；嵌套区域应有不同的根节点。普通小页面不必拆分区域。

上层使用通用 `ViewMarker` 与保留树建立关联，并验证缓存节点仍携带该标记；标记不改变 Key 匹配、布局和控件行为。控件自身的瞬时状态仍遵循原有 Key 规则，跨父节点移动不保证保留光标等内部状态；页面属性不会因此丢失。

## 事件生命周期

`Events::on` 注册类型化处理器，框架在执行时提供短期的 `&mut Model`，不需要长期捕获业务对象的可变借用。保留返回的 `Event<T>`；最后一个克隆释放时自动解绑。`Events::scoped(&child_scope)` 将注册关联到子页面，作用域结束即释放处理器，即使外部还留有旧事件句柄。

`Events` 可克隆并用于动态创建子组件。事件 ID 在同一个应用内不复用，避免旧动作误命中新订阅。回调执行期间不持有注册表借用，因此处理器可以安全地创建或销毁子组件及其订阅。

## 后台结果与外部唤醒

`ReactiveApplication::with_tasks` 在创建时额外提供 `Tasks<Model>`：

```rust,ignore
tasks.spawn(
    move || service.save(snapshot),
    |model, result| {
        model.apply_save_result(result);
        ViewAction::default()
    },
)?;
```

工作闭包只接收拥有所有权的数据快照。完成闭包在 UI 线程取得 `&mut Model`，可以写属性并触发正常刷新。`Tasks::spawn` 是阻塞任务的便捷入口；已有异步执行器可使用 `Tasks::sender()` 和 `TaskSender::post`，无需框架依赖特定执行器。

结果队列容量为 256，发送是非阻塞的，返回 Disposed / Full / Closed。成功发送后若作用域已结束，UI 仍会丢弃结果；销毁作用域不强制终止已经运行的工作线程。业务错误应作为 Result 值传回，任务 panic 由 JoinHandle 报告，应用按需要保留并检查句柄。这不是持久化消息队列。

桌面后端实现下层平台契约 `EventWaker`，将外部唤醒交给 `WindowEvent::ExternalWake`，独立于业务定时器和动画。平台不携带或解释任务结果。窗口未连接时结果可排队，重连会唤醒处理；不提供 EventWaker 的自定义平台须主动驱动事件来处理结果。单独在窗口循环之外调用 Property::set 不会唤醒平台，跨线程必须通过结果队列。

## 诊断、示例与验证

`ReactiveApplication::pending_refresh()` 返回根/区域的变化属性、版本、显式根重建标志和当前订阅数；`Observer::changes` 可单独用于状态适配。报告按需构建，不进入普通读取路径。

[settings](../examples/settings/README.md) 演示共享字号、输入草稿、派生可用性、局部刷新和后台保存快照。示例只保存到内存，不写系统配置。

```sh
cargo test -p zui-state -p zui-app -p minimal -p settings --no-default-features --features cpu --locked
python3 tools/check-dependencies.py
python3 tools/check-mcu.py --target thumbv7em-none-eabihf
cargo run -p settings -- cpu --smoke
cargo run -p settings -- gpu --smoke
```

跨层回归覆盖真实键盘动作、共享值、无效数值草稿、区域增删/移动、显式重建与自动更新共存。状态/事件/任务测试覆盖派生依赖切换、递归环、异常恢复、立即解绑、过期结果丢弃、有界队列与 UI 线程执行。
