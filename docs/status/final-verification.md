# 最终集成验证记录

日期：2026-11-17（Asia/Shanghai）。原版基准：LDtk 1.5.3，检出提交 `2b7b5512f2f88fa281cb6492debbb26ead7838d4`。本记录区分已实现、测试结果和仍未建立的验收保证。

| 检查 | 已观察结果 |
|---|---|
| 最新完整工作区及验收目标 | `cargo test --workspace --locked --offline --no-fail-fast`：原生应用 315 passed、5 ignored，其余核心目标和文件事件 harness 全部通过；`cargo test --example native_smoke --locked --offline`：319 passed、5 ignored。0 failed，日志 `.tools/ci-fourth-final-workspace.log`、`ci-fourth-final-smoke-tests.log`；共享测试不相加作为独立用例数 |
| 原版数据/算法/性能专项 | 13 项显式 ignored 检查已独立通过；49 工程与 3 外部关卡 roundtrip，57,682 官方 tile 精确匹配，66 次增量编辑与全量一致 |
| 原生操作回归 | 字段右键菜单、同行布局、默认占位、保存前提交、跨世界引用、数组拖动及 wrapper 保留均有实际 egui 输入测试；更新器另覆盖未保存保护和真实临时 Rust 子进程重启 |
| GPU/渲染专项 | release 串行执行的 4 项检查全部通过：真实 GPU Gaussian、两遍 60-sample 字段轮廓、4 个代表性官方样本的实际网格/世界渲染、实体密集场景绘制 |
| 历史性能复测 | 13 MB 关卡局部历史预热 begin+commit 平均 3.782091 ms；bounds 随事务保存，不为高亮克隆关卡 |
| macOS 静态检查 | 工作区及全部目标严格 Clippy、格式检查、diff check 通过 |
| 远程原生平台 | 第二轮 Intel macOS、Ubuntu 24.04 完整检查/测试/打包通过；第三轮 ARM macOS、Ubuntu 24.04 通过。Windows 实机 runner 已运行测试并发现问题，修复后仍须下一轮原生 CI 通过；不能用早期交叉编译结果代替 |
| 应用包 | ARM64 release `.app`，约 41 MiB；包含原版 examples/assets、项目图标与许可文件；暂存签名验证后替换，失败回滚另有测试 |

核心和原生的 ignored 检查需要资源或 GPU，默认测试跳过不代表缺少运行证据；上表分别记录其独立运行。`block 0.1.6` 依赖仍输出 Rust future-incompatibility 提示，严格 Clippy 当前通过。

复现最终渲染检查：

```sh
scripts/cargo test --release --bin ldtk-rs canvas:: --locked --offline -- --ignored --nocapture --test-threads=1
scripts/cargo build --release --bin ldtk-rs --example native_smoke --locked --offline
scripts/package-macos
scripts/verify-macos-bundle
```

首次 GitHub 发布/下载与最终四平台 CI 结果仍待补入。具体截图/输入/计时方法见 [acceptance-report.md](acceptance-report.md)；旧的加载中、焦点遮罩或被外部输入污染的截图不作为通过证据。更新器实现及临时安装证据见 [updater-report.md](updater-report.md)。

本机窗口为真实 Metal 输出；验收程序显式模拟前景输入，计时分别记录 App.update CPU、eframe CPU rendering 和 frame-start 间隔。这些数值不等于实际呈现 FPS、GPU 时间戳或输入到屏幕延迟。跨应用全界面逐像素等价、其他平台实际 GUI 及硬件运行尚未完成全面验收，不能由这些测试推导出无条件的完整移植保证。

本机详细日志位于 `.tools/final-workspace-tests7.log`、`final-native-tests.log`、`final-native-gpu.log`、`final-history-latency.log`、`final-clippy7.log`、`final-windows2.log`、`final-linux2.log`、`final-package.log`；`.tools` 是本地验证输出，不纳入源码发布。

## 私有仓库与系统验收后续

- 首次源码提交 `ac127fb` 已推送到用户指定的私有 `bugs1111/LevelEditor`。首次远程运行 `43921626476` 的 Apple Silicon macOS 构建、测试和打包成功；Linux 实际测试发现字体度量导致检查器 Edit 按钮偏离右边，已改为真正的右侧布局并添加随包字体回归。该轮已取消，未发布版本；Windows/Intel 取消结果不记作通过。
- 最新嵌套三列表单回归验证完整字形可读、clip 内、无跨列、原版 12.667pt 字号和面板不超过 1000pt。实际 Color 截图已确认。修复前只扩大 clip 限制的版本保留为拒绝证据，不记为成功。
- 最新全部目标测试和严格 Clippy 通过，日志为 `.tools/verified-labels-workspace.log`、`verified-labels-clippy.log`；检查器 13 项（含 fallback 字体）通过。
- 最新应用包的真实 LaunchServices 冷/热打开检查通过：中文与空格文件路径、同一进程复用，使用独立设置和复制示例，结束时仅停止本次创建的测试进程。日志 `.tools/bundle-cold-warm.log`。
- Home streaming debug 复测 capture RSS 为 248,480 KiB；先前 release 为 1,506,608 KiB。相同设置含 13 MB 最近工程，等待 3 秒、采样至少 2 秒；构建模式不同，不据此推导 CPU 提升或精确降幅。核心 16 MiB ignored JSON 字段测试另证实流式元数据读取单次分配低于 256 KiB。
- 第二轮 `44009550954` 的 ARM 扫描测试曾把软 1ms 分帧扫描当成一次调用完成；回归现主动制造暂停并驱动恢复，生产扫描预算不变。Windows 测试也已改用平台原生绝对路径，不再硬编码 Unix 路径。修复提交 `2bf0201`。
- 第三轮 `44012907455` 收齐所有失败目标：Intel 更新助手漏读子进程退出前已排队的首帧 ACK；Windows 文件字段在路径相对化前误解析 `\\n`/`\\r`，资源搬迁以只读文件句柄刷盘被拒绝，以及发布说明 CRLF 版本号残留 `\r`。ACK、路径、CRLF 均已有先失败后通过的确定性回归；资源搬迁现保留写句柄直到权限恢复和刷盘，并补只读资源复制回归。最新工作区、验收目标和严格全目标 Clippy 均通过，仍须下一轮四平台 CI。尚无已发布版本。
- 实际 CommonMark/file/image loader 回归还复现并修正了打包路径含空格和 `#` 时发布说明图片无法解析的问题：只为图片目标添加 CommonMark 尖括号，保留原始文件路径；普通链接、正文和原图不变。
- release Home 强制重绘复测曾观察到 1,246,720 KiB capture RSS；同一字节不变的程序后续恢复约 252 MiB，触发条件未查明，不能据 debug 或后续低占用宣称彻底解决。正常应用无强制重绘的独立进程在 3–7 秒为 197,024–197,104 KiB；vmmap 显示高占用主要增长在 graphics/IOSurface。新增验收程序 SurfaceError 计数保持原 handler 行为。详细证据、控制与未解决边界见 [Home 内存调查](home-memory-investigation.md)。
