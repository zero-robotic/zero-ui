# 文本剪贴板

已实现公共文本剪贴板契约、事件服务注入、输入框复制/剪切/粘贴和桌面原生适配。自动化测试使用替身验证，不读取或覆盖开发机器的系统剪贴板；跨应用复制粘贴仍需实机验收。

## 模块与所有权

原 `zui-clipboard` crate 已合入 `zui-app`，不再保留兼容包装。`zui-app` 默认不启用原生剪贴板；工作区应用需要显式配置：

```toml
zui-app = { workspace = true, features = ["clipboard"] }
```

原 `zui-clipboard/wayland` 改为 `zui-app/clipboard-wayland`，会自动启用 `clipboard`。仅使用 CPU 或 GPU feature 不会引入 arboard。原生连接仍为懒加载，单元测试不访问系统剪贴板。

`zui-api::clipboard::Clipboard` 定义 read_text(max_bytes)、write_text 和框架错误类型，不包含平台句柄。`EventServices` 在单次事件路由中借用该服务；核心只转交服务，不解释剪贴板内容，也不识别输入框。既有 Widget::event 实现仍可使用，需服务的控件实现 event_with_services。直接焦点/捕获通知不携带服务。

`Tree::dispatch_input_with_services` 支持显式注入；普通 dispatch_input 使用 UnavailableClipboard，未配置时报告 Unavailable，不伪装成成功。WidgetHost 通过 with_clipboard 持有注入实例。控件不保存服务引用，不通过全局单例查找平台能力。

`zui_app::clipboard::NativeClipboard` 是 zui-app 中由 `clipboard` feature 启用的桌面适配器，实现仅使用公共 API 与锁定的 arboard 3.6.1；核心、控件、平台契约与渲染器均不依赖它。首次使用时连接，随后保留实例，构造本身不访问剪贴板。桌面窗口示例显式注入：

```rust
let host = WidgetHost::new(model, layout, text, limits)?
    .with_clipboard(zui_app::clipboard::NativeClipboard::default());
```

arboard 默认图像功能已关闭，本阶段只处理 UTF-8 文本。Linux 默认使用 X11，原生 Wayland 数据控制可通过 zui-app 的 `clipboard-wayland` feature 启用，仍取决于合成器支持；本轮仅验证 macOS 编译。Linux 的剪贴板内容所有权可能随实例销毁而丢失，因此不能每次操作后立即丢弃连接。详见 [arboard 3.6.1 文档](https://docs.rs/arboard/3.6.1/arboard/struct.Clipboard.html) 与 [功能配置](https://docs.rs/crate/arboard/3.6.1/features)。无桌面服务的嵌入式配置可不链接此适配器，按应用需要注入替代实现。

## 编辑语义

- Control 或 Super 加 C/X/V 发起复制、剪切和粘贴；按键重复不重复执行剪贴板操作。
- 复制只写入选中的原始文本；空选区不覆盖剪贴板，不改变编辑历史。
- 剪切先写入剪贴板，成功后才删除选区；失败返回 Rejected，原文、选择区和撤销历史保留。平台写入失败不保证操作系统剪贴板本身能回滚。
- 粘贴先读取，再按单行规则和剩余文本预算校验，最后一次性替换选区；不自动删除换行。空文本不删除已有选区。失败不改变编辑状态，成功属于一个撤销单元。
- 只读允许复制与选择，禁止剪切和粘贴；禁用或非空预编辑期间不执行剪贴板操作。

返回文本字节限制由适配器检查，控件的编辑缓冲区再次检查最终文本预算。该限制不代表操作系统读取过程的内存分配上限。当前为同步调用，系统剪贴板阻塞可能影响 UI 响应；后续若引入异步服务，必须加上节点身份、编辑版本与选择区有效性检查，不能将晚到文本直接写入新的焦点控件。

## 验证范围

控件测试覆盖 Unicode 文本复制、剪切失败原子性、粘贴、撤销/重做、读取失败、不可用服务、预算、单行拒绝、只读、重复键与预编辑隔离。Host 集成测试验证注入服务确实接入核心路由，并将剪切/粘贴动作回写应用模型。

图片、HTML、文件列表、Linux primary selection、跨应用实机互操作和异步剪贴板不在当前实现范围。公共契约继续保持 no_std + alloc，桌面适配器需要 std；不据此宣称 MCU 支持已完成。
