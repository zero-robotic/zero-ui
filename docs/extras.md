# 可选扩展组件 zui-extras

原 `zui-charts` 和 `zui-dashboard` 合并为一个 `zui-extras` crate；不保留旧 crate 的兼容包装。后续大屏相关图表、仪表、指标卡、面板、装饰和组合组件统一放入此 crate，不再按组件类别拆分 crate。

## 按需启用

默认 feature 为空，不编译组件，也不引入 ZUI 依赖。基础控件和运行时不依赖扩展组件。

```toml
[dependencies]
zui-extras = { path = "crates/zui-extras", default-features = false, features = ["dashboard"] }
```

路径按应用位置调整；工作区成员使用 `workspace = true` 替代 `path`。

| feature | 模块与依赖 |
| --- | --- |
| `charts` | `zui_extras::charts`：Sparkline、样本序列、值域和历史缓冲；仅依赖公共 API、类型、绘制与主题 |
| `dashboard` | `zui_extras::dashboard`：MetricCard、DashboardPanel；自动启用 `charts` 并引入标准控件 |

只使用图表时启用 `charts`，不会引入 `zui-widgets`。两种功能均保持 `no_std + alloc`，不依赖核心运行时、窗口、布局引擎或 CPU/GPU 渲染后端。

```rust
use zui_extras::charts::{SparklineHistory, SparklineSeries, SparklineView};
use zui_extras::dashboard::{DashboardPanelView, MetricCardView};
```

## 模块边界

大屏组合可以依赖图表；图表不得反向依赖大屏组合。各模块通过公开类型协作，不访问兄弟模块的私有实现。新增大屏能力优先增加内部模块，必要时增加 feature。采样、网络与业务调度仍由应用负责，不迁入组件库。

渲染验证工具、窗口验证工具及大屏示例显式启用 `dashboard`，因此保留原有展示与测试覆盖。传统控件示例无需启用此扩展。

## 验证

```sh
cargo check -p zui-extras --no-default-features --locked
cargo test -p zui-extras --no-default-features --features charts --locked
cargo test -p zui-extras --no-default-features --features dashboard --locked
python3 tools/check-dependencies.py
python3 tools/check-mcu.py
```

依赖检查覆盖空 feature、仅 charts、dashboard 和 CPU/GPU 隔离；MCU 脚本显式启用 dashboard，避免空 feature 编译造成假通过。
