# MCU 扩展验证

首期产品仍使用 Vello CPU/GPU。MCU 扩展分开验证可移植库、固件装配与设备运行，避免把 `no_std` 声明或宿主机编译当作 MCU 支持完成。

## 可重复的裸机库编译

```sh
rustup target add thumbv7em-none-eabihf
python3 tools/check-mcu.py
```

脚本从仓库根目录调用 `cargo build --lib --target ... --no-default-features --locked --offline`，包含代码生成而不仅是类型检查；固定验证以下 12 个 crate：

- 基础契约：types、input、paint、theme、api、platform。
- 状态与运行时：edit、virtual、core。
- 控件与组合：widgets、charts、dashboard。

2026-10-28 在当前开发机上，以上库通过 `thumbv7em-none-eabihf`（Cortex-M4F/M7F）检查与构建。无需修改实现，也未引入反向依赖。除 types 等纯基础库外，相关代码依赖 `alloc`；固件仍须提供分配器。

脚本不自动安装目标、不联网解析依赖、不回退宿主目标、不跳过失败库。缺少目标时先安装目标；缺少缓存时先准备锁定版本的依赖。可通过 `--target` 选择脚本列出的其他裸机目标；目前仅上述 Cortex-M 硬浮点目标经过实际验证，其他目标选项不代表已通过。

## 证据范围

当前结果证明这组库及其实际依赖可为该无操作系统目标生成库产物。它不证明完整应用能链接、启动，或满足设备内存和实时性要求。泛型应用路径仍需固件实例化与最终链接验证。

Vello 渲染器、Parley 文本、Taffy 布局、桌面窗口及应用装配不在本检查集合内；不会为了通过检查将这些完整功能假装纳入 MCU 配置。无 GPU 嵌入式 Linux 的 Vello CPU 路径是另一条验证路线。

后续需要完成：

1. MCU 装配示例：启动代码、链接脚本、分配器和 panic 策略，形成可链接固件。
2. 在公共绘制、布局和文字契约下实现受限能力适配；由装配层注入，底层不引用具体应用或控件。
3. 显示刷新、输入、时钟等设备适配，以及硬件或模拟器运行验收。
4. 量化 RAM/Flash、峰值分配、帧缓冲策略、绘制与输入延迟，并明确哪些控件和效果可用。

这些工作完成前，项目只声明“保留 MCU 扩展能力，指定可移植库已通过裸机编译”。
