# 最终集成验证记录

日期：2026-11-17（Asia/Shanghai）。原版基准：LDtk 1.5.3，检出提交 `2b7b5512f2f88fa281cb6492debbb26ead7838d4`。本记录区分已实现、测试结果和仍未建立的验收保证。

| 检查 | 已观察结果 |
|---|---|
| 完整工作区 `cargo test --workspace --locked --offline` | 524 passed、0 failed、17 ignored；其中原生应用 285 passed、4 ignored |
| 更新器整合后的全部目标 `cargo test --workspace --all-targets --locked --offline` | 原生应用 308 passed、5 ignored；验收程序 311 passed、5 ignored；其余核心目标全部通过。日志合计 862 次通过执行（包含重复编译目标及子进程 fixture），0 failed |
| 原版数据/算法/性能专项 | 13 项显式 ignored 检查已独立通过；49 工程与 3 外部关卡 roundtrip，57,682 官方 tile 精确匹配，66 次增量编辑与全量一致 |
| 最后原生应用回归 | 285 passed、0 failed；字段右键菜单、同行布局、默认占位、保存前提交、跨世界引用、数组拖动及 wrapper 保留均有实际 egui 输入测试 |
| GPU/渲染专项 | release 串行执行的 4 项检查全部通过：真实 GPU Gaussian、两遍 60-sample 字段轮廓、4 个代表性官方样本的实际网格/世界渲染、实体密集场景绘制 |
| 历史性能复测 | 13 MB 关卡局部历史预热 begin+commit 平均 3.782091 ms；bounds 随事务保存，不为高亮克隆关卡 |
| macOS 静态检查 | 工作区及全部目标严格 Clippy、格式检查、diff check 通过 |
| Windows/Linux | `x86_64-pc-windows-gnu`、`x86_64-unknown-linux-gnu` 全工作区/目标严格 Clippy 通过；未在相应 OS 上运行 |
| 应用包 | ARM64 release `.app`，约 41 MiB；包含原版 examples/assets、项目图标与许可文件；暂存签名验证后替换，失败回滚另有测试 |

核心和原生的 ignored 检查需要资源或 GPU，默认测试跳过不代表缺少运行证据；上表分别记录其独立运行。`block 0.1.6` 依赖仍输出 Rust future-incompatibility 提示，严格 Clippy 当前通过。

复现最终渲染检查：

```sh
scripts/cargo test --release --bin ldtk-rs --locked --offline -- --ignored --nocapture --test-threads=1
scripts/cargo build --release --bin ldtk-rs --example native_smoke --locked --offline
scripts/package-macos
scripts/verify-macos-bundle
```

最新应用包实测、更新后 Home 内存、首次 GitHub 发布/下载与 CI 结果在本轮结束时补入。具体截图/输入/计时方法见 [acceptance-report.md](acceptance-report.md)；旧的加载中、焦点遮罩或被外部输入污染的截图不作为通过证据。更新器实现及临时安装证据见 [updater-report.md](updater-report.md)。

本机窗口为真实 Metal 输出；验收程序显式模拟前景输入，计时分别记录 App.update CPU、eframe CPU rendering 和 frame-start 间隔。这些数值不等于实际呈现 FPS、GPU 时间戳或输入到屏幕延迟。跨应用全界面逐像素等价、其他平台实际 GUI 及硬件运行尚未完成全面验收，不能由这些测试推导出无条件的完整移植保证。

本机详细日志位于 `.tools/final-workspace-tests7.log`、`final-native-tests.log`、`final-native-gpu.log`、`final-history-latency.log`、`final-clippy7.log`、`final-windows2.log`、`final-linux2.log`、`final-package.log`；`.tools` 是本地验证输出，不纳入源码发布。
