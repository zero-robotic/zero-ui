# 验证字体

`NotoSansSC.ttf` 是未修改的 Noto Sans SC 可变字体，仅供排版测试和预览工程使用；框架库不会自动嵌入这份字体。默认文本服务不扫描系统字体；桌面示例显式启用 `zui-text/system-fonts`，使用系统字体。需要确定性输出或受限设备部署的产品可显式提供符合语言覆盖和资源预算的字体。

- 来源：[Google Fonts 字体文件](https://github.com/google/fonts/blob/main/ofl/notosanssc/NotoSansSC%5Bwght%5D.ttf)。本地重命名为 `NotoSansSC.ttf`，字体内容未修改。
- 许可证：SIL Open Font License 1.1，完整版权与条款保存在 [OFL-NotoSansSC.txt](OFL-NotoSansSC.txt)。分发字体时保留该文件。
- SHA-256：`a3041811a78c361b1de50f953c805e0244951c21c5bd412f7232ef0d899af0da`。

测试通过 `include_bytes!` 固定输入，避免系统字体版本或安装情况影响结果。此完整中文字体体积较大，不代表嵌入式或 MCU 产品的最终字体方案。
