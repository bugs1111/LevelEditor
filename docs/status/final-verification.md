# 最终集成验证记录

日期：2026-11-17（Asia/Shanghai）。原版基准：LDtk 1.5.3，检出提交 `2b7b5512f2f88fa281cb6492debbb26ead7838d4`。本记录区分已实现、测试结果和仍未建立的验收保证。

| 检查 | 已观察结果 |
|---|---|
| 最新完整工作区及验收目标 | `v0.1.1` 完整工作区：原生应用 316 passed、6 ignored，其余核心目标和文件事件 harness 全部通过；native_smoke example：320 passed、6 ignored。0 failed，日志 `.tools/home-memory-followup/workspace-final.log`、`smoke-tests-final.log`；共享测试不相加作为独立用例数 |
| 原版数据/算法/性能专项 | 13 项显式 ignored 检查已独立通过；49 工程与 3 外部关卡 roundtrip，57,682 官方 tile 精确匹配，66 次增量编辑与全量一致 |
| 原生操作回归 | 字段右键菜单、同行布局、默认占位、保存前提交、跨世界引用、数组拖动及 wrapper 保留均有实际 egui 输入测试；更新器另覆盖未保存保护和真实临时 Rust 子进程重启 |
| GPU/渲染专项 | release 串行执行的 4 项检查全部通过：真实 GPU Gaussian、两遍 60-sample 字段轮廓、4 个代表性官方样本的实际网格/世界渲染、实体密集场景绘制 |
| 历史性能复测 | 13 MB 关卡局部历史预热 begin+commit 平均 3.782091 ms；bounds 随事务保存，不为高亮克隆关卡 |
| macOS 静态检查 | 工作区及全部目标严格 Clippy、格式检查、diff check 通过 |
| 远程原生平台 | `44036712278` 在 `554e86a` 上四平台全部通过：格式、严格全目标 Clippy、完整工作区测试、文件事件 harness、5 项 capture 回归、release 构建及打包 |
| 四平台 GUI/GPU | 每平台 6 个原生窗口场景和 3 项显式 GPU 测试全部通过，24 张截图均已逐张检查；运行、适配器和状态证据见下节及 `gui-platform-acceptance-results.json` |
| Home 高内存 | 复现并修复取帧失败时 GPU staging buffer 未提交的累积；原 handler 的 1 秒故障峰值 1,878,000 KiB，修复后 10 秒、73,388 次 Timeout 的峰值 198,624 KiB，最多 10 个缓冲，恢复截图通过 |
| 应用包 | 0.1.1 四平台 CI ZIP 的大小、SHA-256、ZIP CRC、架构、安装布局、原版资源及许可文件均校验通过；两个 macOS 包严格 ad-hoc 签名验证通过，CI ARM 包实际冷/热打开通过。正式发布被自动审批拦截，当前 published stable 仍为 v0.1.0；0.1.1 真实发布源复验待发布后执行 |

核心和原生的 ignored 检查需要资源或 GPU，默认测试跳过不代表缺少运行证据；上表分别记录其独立运行。`block 0.1.6` 依赖仍输出 Rust future-incompatibility 提示，严格 Clippy 当前通过。

复现最终渲染检查：

```sh
scripts/cargo test --release --bin ldtk-rs canvas:: --locked --offline -- --ignored --nocapture --test-threads=1
scripts/cargo build --release --bin ldtk-rs --example native_smoke --locked --offline
scripts/package-macos
scripts/verify-macos-bundle
```

首次 v0.1.0 发布及其真实私有更新源已完成；本轮 0.1.1 修复、GUI/GPU 和包验收见下节。历史发布证据保留在 [platform-acceptance.json](platform-acceptance.json)。具体截图/输入/计时方法见 [acceptance-report.md](acceptance-report.md)；旧的加载中、焦点遮罩或被外部输入污染的截图不作为通过证据。更新器实现及临时安装证据见 [updater-report.md](updater-report.md)。

本机窗口为真实 Metal 输出；验收程序显式模拟前景输入，计时分别记录 App.update CPU、eframe CPU rendering 和 frame-start 间隔。这些数值不等于实际呈现 FPS、GPU 时间戳或输入到屏幕延迟。四平台已运行下述原生 GUI 场景；跨应用全界面逐像素等价、所有实体硬件上的呈现和 OS 输入仍不属于这些检查的保证范围。

本机详细日志位于 `.tools/final-workspace-tests7.log`、`final-native-tests.log`、`final-native-gpu.log`、`final-history-latency.log`、`final-clippy7.log`、`final-windows2.log`、`final-linux2.log`、`final-package.log`；`.tools` 是本地验证输出，不纳入源码发布。

## 0.1.1 GUI/GPU、Home 修复和发布包验收

应用源码为 `554e86a52123a964e1d65a11b50a5daa23455fba`。Linux GUI 重跑提交 `2d55ca2` 只补 CI 的 `libxkbcommon-x11-0` 运行库、软件驱动定位及适配器分类；已用 `git diff --exit-code` 确认两者的 `src`、`crates`、Cargo 输入、assets/examples 完全相同。

| 目标 | GUI / GPU | 实际适配器 | GUI job |
|---|---|---|---|
| ARM macOS | 6/6、3/3 通过 | Apple Paravirtual / Metal，虚拟 | [133881216916](https://github.com/bugs1111/LevelEditor/actions/runs/44036707023/job/133881216916) |
| Intel macOS | 6/6、3/3 通过 | Apple Paravirtual / Metal，虚拟 | [133881216752](https://github.com/bugs1111/LevelEditor/actions/runs/44036707023/job/133881216752) |
| Windows MSVC | 6/6、3/3 通过 | Microsoft Basic Render Driver / DX12，软件 | [133881216784](https://github.com/bugs1111/LevelEditor/actions/runs/44036707023/job/133881216784) |
| Linux GNU | 6/6、3/3 通过 | Mesa llvmpipe/lavapipe / Vulkan，软件，Xvfb | [133889816967](https://github.com/bugs1111/LevelEditor/actions/runs/44039332275/job/133889816967) |

- 六场景为 Home、实体、实体表单、按住的实体拖动、世界视图和实际绘制。每个场景均取得非空 PNG、有效 viewport/DPI 和零 surface error；四平台绘制均由 484 增至 497 格、最终 checksum `d13146f10a0c8f`。拖动时均为 `EntityMove`、9 实体、`dirty=false`，捕获在释放前，不声称已提交坐标。
- 24 张 CI PNG 已逐张目视检查。macOS 原始 artifact 的旧分类虽写 hardware，实际名称为 Apple Paravirtual，汇总已更正为 virtual，保留原件。Windows 白色样式问题已通过固定原版深色主题及真实系统主题事件 RED/GREEN 回归修复；窄窗口输入按实际 viewport 转换。
- 同一 CI ARM 可移植验收程序在物理 Apple M6/Metal 再次通过 6 场景与 3 GPU 测试，六张图均已检查。物理 RTX 2060/Vulkan 驱动 595.99.02 通过 3 项离屏 GPU 测试，各明确执行 1 项、0 failed、0 ignored；该设备未验收原生窗口呈现或 GUI 输入。
- Home 根因、真实 GPU RED/GREEN、受控 Metal 故障前后和 1/5/10 秒压力结果见 [内存调查](home-memory-investigation.md)。修复只在取帧错误时提交待上传数据，再保留原 handler 的恢复策略；成功帧未添加等待、休眠或帧率限制。初次历史高占用的 OS 触发事件未被记录，不能追溯断言为同一事件。
- [四平台构建 44036712278](https://github.com/bugs1111/LevelEditor/actions/runs/44036712278) 全部通过，原生 app 测试分别为 macOS 两架构 316/6 ignored、Windows 313/6、Linux 315/6，均 0 failed。不同数目来自条件编译。5 项 capture 回归、严格 Clippy、格式、工作区其余测试和打包均通过。
- 四个 CI 包分别核对 253 个 assets/examples、ZIP CRC、安装标记、架构、许可文件及 SHA/大小；Windows 39 个文本仅有 CRLF 差异。两个 macOS 包严格 ad-hoc 签名验证通过，CI ARM `.app` 在中文、空格和 `#` 路径冷启动及同进程热打开通过。未建立 Apple 公证结果。

机器证据：[GUI/GPU](gui-platform-acceptance-results.json)、[0.1.1 包验收](release-0.1.1-acceptance.json)、[Home 压力结果](home-memory-followup.json)。原始本机材料为 `.tools/final-gui-artifacts/`、`.tools/physical-m6-final/`、`.tools/release-v0.1.1/`。正式 Release 创建被自动审批拒绝，包括补充旧会话授权后的重试；未上传发布包。当前需要用户明确批准将这四个已验收 ZIP 和清单发布到指定私有仓库，随后再做 0.1.1 真实发布源下载/暂存复验。

## 历史：第四轮四平台与 v0.1.0 发布包验收

2026-11-17 收尾核验：[CI 44018713667](https://github.com/bugs1111/LevelEditor/actions/runs/44018713667)，源码提交 `dbaa6c68557dfba10a0d1009726561e7a25cf239`。[v0.1.0](https://github.com/bugs1111/LevelEditor/releases/tag/v0.1.0) 于北京时间 12:57 发布，目标提交与本地验收代码一致。

| 原生 runner / 目标 | 应用测试 | 工作区其余目标 / capture 回归 / release 包 |
|---|---|---|
| [macos-15 / ARM64](https://github.com/bugs1111/LevelEditor/actions/runs/44018713667/job/133822797423) | 315 passed、0 failed、5 ignored | 全部通过 / 5 passed / 通过 |
| [macos-15-intel / x86_64](https://github.com/bugs1111/LevelEditor/actions/runs/44018713667/job/133822797183) | 315 passed、0 failed、5 ignored | 全部通过 / 5 passed / 通过 |
| [windows-latest / MSVC x86_64](https://github.com/bugs1111/LevelEditor/actions/runs/44018713667/job/133822797450) | 312 passed、0 failed、5 ignored | 全部通过 / 5 passed / 通过 |
| [ubuntu-24.04 / GNU x86_64](https://github.com/bugs1111/LevelEditor/actions/runs/44018713667/job/133822797486) | 314 passed、0 failed、5 ignored | 全部通过 / 5 passed / 通过 |

不同平台的测试数来自条件编译；子进程执行的重复用例和共享 app/example 测试不累计为独立用例。各 runner 均完成格式及严格全目标 Clippy。默认 ignored 的 GPU/语料/真实私有源检查不能据本表视为四个平台都运行过；本机专项证据和下列真实源测试分别记录。

- 使用 `gh release download v0.1.0 --repo bugs1111/LevelEditor` 将 manifest 和四个 ZIP 下载到 `.tools/release-v0.1.0/`。四包均通过 manifest 大小/SHA-256、ZIP CRC、安全根目录、安装标记、许可文件、执行权限及 Mach-O/PE/ELF 架构检查。每包逐项核对 253 个 assets/examples 文件；Windows 的 39 个 UTF-8 文本仅 CRLF 换行不同，其余资源字节一致。校验值及差异清单保存在 `platform-acceptance.json`。
- 两个下载版 `.app` 用 `ditto -x -k` 解包后，`codesign --verify --deep --strict` 均通过；版本和 bundle identifier 正确。这是 ad-hoc 签名验证，不是 Apple 公证。
- 显式运行 `updater::tests::live_private_release_downloads_verifies_and_stages_without_installing`：1 passed、0 failed。应用自身从真实私有 GitHub 源检查 `v0.1.0`，下载 host 包、校验、暂存并清理；认证仅传给子进程环境，未安装或替换现有应用。日志 `.tools/live-private-release-acceptance.log`。
- 对实际下载的 ARM `.app` 运行 `scripts/verify-macos-bundle`：中文、空格和 `#` 路径冷启动成功，热打开第二个工程成功且复用同一进程。复制示例及独立设置位于临时目录；测试进程已退出，stderr 为空。日志 `.tools/published-bundle-cold-warm.log`。

该历史轮完成四平台原生 CI、v0.1.0 发布包和上述本机运行验收。后续 GUI/GPU 及 Home 缺陷修复结果见上方 0.1.1 记录；所有实体硬件的完整 GUI 呈现、输入延迟及全界面视觉等价仍不在 CI 保证范围。

## 私有仓库与系统验收历史

- 首次源码提交 `ac127fb` 已推送到用户指定的私有 `bugs1111/LevelEditor`。首次远程运行 `43921626476` 的 Apple Silicon macOS 构建、测试和打包成功；Linux 实际测试发现字体度量导致检查器 Edit 按钮偏离右边，已改为真正的右侧布局并添加随包字体回归。该轮已取消，未发布版本；Windows/Intel 取消结果不记作通过。
- 最新嵌套三列表单回归验证完整字形可读、clip 内、无跨列、原版 12.667pt 字号和面板不超过 1000pt。实际 Color 截图已确认。修复前只扩大 clip 限制的版本保留为拒绝证据，不记为成功。
- 最新全部目标测试和严格 Clippy 通过，日志为 `.tools/verified-labels-workspace.log`、`verified-labels-clippy.log`；检查器 13 项（含 fallback 字体）通过。
- 最新应用包的真实 LaunchServices 冷/热打开检查通过：中文与空格文件路径、同一进程复用，使用独立设置和复制示例，结束时仅停止本次创建的测试进程。日志 `.tools/bundle-cold-warm.log`。
- Home streaming debug 复测 capture RSS 为 248,480 KiB；先前 release 为 1,506,608 KiB。相同设置含 13 MB 最近工程，等待 3 秒、采样至少 2 秒；构建模式不同，不据此推导 CPU 提升或精确降幅。核心 16 MiB ignored JSON 字段测试另证实流式元数据读取单次分配低于 256 KiB。
- 第二轮 `44009550954` 的 ARM 扫描测试曾把软 1ms 分帧扫描当成一次调用完成；回归现主动制造暂停并驱动恢复，生产扫描预算不变。Windows 测试也已改用平台原生绝对路径，不再硬编码 Unix 路径。修复提交 `2bf0201`。
- 第三轮 `44012907455` 收齐所有失败目标：Intel 更新助手漏读子进程退出前已排队的首帧 ACK；Windows 文件字段在路径相对化前误解析 `\\n`/`\\r`，资源搬迁以只读文件句柄刷盘被拒绝，以及发布说明 CRLF 版本号残留 `\r`。ACK、路径、CRLF 均已有先失败后通过的确定性回归；资源搬迁现保留写句柄直到权限恢复和刷盘，并补只读资源复制回归。这些修复已由第四轮四平台原生 CI 验证通过，随后发布 `v0.1.0`。
- 实际 CommonMark/file/image loader 回归还复现并修正了打包路径含空格和 `#` 时发布说明图片无法解析的问题：只为图片目标添加 CommonMark 尖括号，保留原始文件路径；普通链接、正文和原图不变。
- release Home 强制重绘复测曾观察到 1,246,720 KiB capture RSS；同一字节不变的程序后续恢复约 252 MiB，触发条件未查明，不能据 debug 或后续低占用宣称彻底解决。正常应用无强制重绘的独立进程在 3–7 秒为 197,024–197,104 KiB；vmmap 显示高占用主要增长在 graphics/IOSurface。新增验收程序 SurfaceError 计数保持原 handler 行为。详细证据、控制与未解决边界见 [Home 内存调查](home-memory-investigation.md)。
