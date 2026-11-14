# 开发工具

以下命令从仓库根目录运行。

- `python3 tools/check-dependencies.py`：检查内部依赖边及功能隔离；需要本地 Cargo 依赖缓存。
- `python3 tools/check-mcu.py --target thumbv7em-none-eabihf`：编译裸机目标上的可移植库；需要预先安装对应 Rust target。
- `render-check`：离屏渲染预览和渲染回归测试。
- `window-check`：窗口交互检查和刷新性能基准。

## 比较刷新性能

将 `window-check` 的 `refresh-bench` 输出保存为 CSV，然后比较相同工作负载的多轮结果：

```sh
python3 tools/compare-refresh.py --before before-1.csv before-2.csv --after after-1.csv after-2.csv
```

前后轮数、场景集合以及每个场景的 samples 和 checksum 必须一致。
CSV 必须包含 `scenario,samples,checksum,p50_us,p95_us,update_us,draw_us`；允许额外列。
脚本拒绝重复场景、缺失数据、无效采样数以及负数或非有限耗时。
校验失败时返回退出码 2，并显示文件或比较项相关的错误信息。

报告使用各轮 P50/P95 的中位数，以及各轮 update + draw 的中位数。
“耗时降低”出现负值表示变慢；基准耗时为零时显示 `N/A`。
此脚本适用于 CPU `refresh-bench` CSV，不适用于列结构不同的 `gpu-refresh-bench`。

运行 Python 工具回归测试：

```sh
python3 -B -m unittest discover -s tools/tests -v
```

## 共享实现

两个 Rust 工具保持独立，现有命令和功能开关不变：

- `window-check/examples/support/mod.rs`：CPU/GPU 基准共用的场景模型、输入事件和分位数统计；计时与渲染流程仍在各自入口中。
- `render-check/src/preview_support.rs`：预览共用的内置字体初始化；控件布局及交互步骤由各场景管理。
- `check-dependencies.py` 的 `selected_dependencies`：集中执行和解析 Cargo 依赖树，检查规则保留在调用处。
