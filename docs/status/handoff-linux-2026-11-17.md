# LevelEditor → Linux 接续交接

交接日期：2026-11-17（Asia/Shanghai）。目标：在 Linux 上继续 Debug 桌面实测和原版 UI 对照，完成剩余交互／渲染验收；不是重新开始移植。

## 1. 接手点与源码

- 仓库：`https://github.com/bugs1111/LevelEditor.git`，私有仓库，使用用户已有的 GitHub 认证。
- 当前源码基准：`d80d33dc4d2b568790933fe4fb154ad7dc9cd9ed`（`d80d33d handoff to linux`）。
- Mac 本地分支 `rust-port` 跟踪 `origin/main`；交接核对时 HEAD 和本地 `origin/main` 都指向上述提交。没有为本交接查询远端服务器的更新状态。
- `22e2aa8` 包含前面的 UI 修正；`d80d33d` 又包含最新 Home 搜索框修正。不要只拿到 `22e2aa8` 就开始比较搜索框。
- 源码工作区已核对干净；本交接文档是随后新增的文件，尚未提交或推送。带上另附的 ZIP，避免 Linux 只拉源码而漏掉交接文档及本地证据。
- 应用显示名是 **LevelEditor**；Rust crate／可执行文件仍叫 `ldtk-rs`，配置目录和更新包内部标识为兼容旧安装而保留旧命名。

全 Rust 原生桌面程序：`eframe/egui + wgpu + ldtk-core`，不依赖 Electron、WebView 或 Haxe 运行时。参考源码只用于对照／语料测试。

## 2. 用户要求与工作方式

- 用户使用 `just run`，它启动 **debug** 程序，终端可看到运行日志和错误，默认 `RUST_BACKTRACE=1`。
- 用户重视原版 LDtk 1.5.3 的具体布局、字体、颜色、图标、鼠标悬停、光标和快捷键；“功能存在／按钮能打开／测试通过”不能作为 UI 已一致的结论。
- 对照固定原版源码和截图，先定位差异再修复；修改保持局部，不重构无关部分，不覆盖用户改动。
- 优先在本机验证当前 debug 源码，不要每个视觉调整都重复构建六个跨平台 release 产物。
- 没有新增发布授权；本次没有由助手提交、推送或发布 Release。

## 3. Linux 首次启动

新检出可以使用：

```sh
git clone --branch main https://github.com/bugs1111/LevelEditor.git
cd LevelEditor
git rev-parse HEAD
git merge-base --is-ancestor d80d33dc4d2b568790933fe4fb154ad7dc9cd9ed HEAD
```

已有检出先检查 `git status --short`，确认本机工作不会被覆盖，再执行 `just pull` 或 `git pull --ff-only`。不要为了匹配交接提交而重置现有工作区。

以下原生依赖以仓库 Ubuntu 24.04 CI 为基础，其他发行版使用对应包名：

```sh
sudo apt-get update
sudo apt-get install -y build-essential pkg-config python3 curl \
  libxkbcommon-dev libxkbcommon-x11-0 libwayland-dev \
  libxrandr-dev libxi-dev libxcursor-dev libxinerama-dev \
  libvulkan1 mesa-vulkan-drivers vulkan-tools
```

Linux 渲染后端为 Vulkan；真实桌面使用其已安装的硬件驱动。先记录实际系统、Rust、桌面会话和 Vulkan 适配器，不把 lavapipe 软件渲染当作物理 GPU 验收。

如果没有可用 Rust，可运行项目提供的安装脚本：

```sh
scripts/bootstrap-rust
. "$HOME/.cargo/env"
scripts/cargo -vV
scripts/cargo fetch --locked
scripts/run
```

已安装 `just` 时，日常入口：

```sh
just run
just run examples/original/Typical_TopDown_example.ldtk
just run examples/original/Typical_2D_platformer_example.ldtk
```

`scripts/run` 与 `just run` 等价。首次检出需要下载依赖，不要一开始就加 `--offline`。Mac 已验证 Cargo/Rust 1.100.0；Linux 使用能够构建当前 `Cargo.lock` 的工具链，先记录版本，不擅自升级锁文件中的 GUI/GPU 依赖。

Rust 工具链按主机架构安装到当前用户的 `~/.cargo` 和 `~/.rustup`，`scripts/cargo` 通过 `PATH` 调用 Cargo。用于构建 Linux 包的 Zig 工具和缓存位于用户的 `~/.cargo/cross`，已设置 `CARGO_HOME` 时使用其 `cross` 子目录。

`just build -t win|mac|linux -m debug|release` 是用户要求的 **Mac 主机交叉编译入口**，脚本明确拒绝 Linux 主机。在 Linux 上原生构建用：

```sh
scripts/cargo build --bin ldtk-rs --locked
scripts/cargo build --release --bin ldtk-rs --locked
```

## 4. 已完成且应保留的行为

| 项目 | 当前状态与主要代码 |
| --- | --- |
| 关闭工程／退出 | 工具栏出口和 Cmd/Ctrl+W 关闭工程回 Home；Cmd/Ctrl+Q 退出；窗口关闭仍走退出流程。未保存时使用原版问句与 `YES / NO / CANCEL`。`src/editor/project_dialogs.rs`、`src/editor/chrome.rs`、`src/editor/shortcuts.rs`。 |
| 关闭崩溃 | 工具栏关闭后立即检查工程是否已移除，避免同一帧继续 `unwrap()`。保留保存、放弃和取消回归。 |
| 快捷键丢失 | 读取每个 Key 事件自带的修饰键，避免同帧释放 Command 后读整帧修饰键而漏掉 Cmd+W。不要改回旧方式。 |
| 顶部工具栏 | Project／World／Level／定义等使用浮层；World 浮层与世界视图状态联动，保留底层侧栏与画布。不是直接替换整个左侧内容。 |
| 设置弹窗 | 双栏与分组按原版恢复；原版齿轮 SVG、字段控件、百分比、帮助、日志入口、保存会话及嵌套更新窗口 Escape 处理已修复。`src/editor/application_settings.rs`。 |
| 悬停分组 | 标题和内容边框变黄，移出恢复；展开为 ZoomOut、折叠为 Cell 指针。自定义 collapse SVG 光标尚未移植。 |
| 备份建议 | 大工程打开时显示原版 `ENABLE BACKUPS` 双按钮；开启备份或设置 `IgnoreBackupSuggest`，打开本身不改文档。 |
| F1–F10 | 切换可见图层时显示类型图标＋名称，1.2 秒淡出；重复选择当前图层不重复提示。 |
| 快捷键提示 | World 提示为 `~`；按平台显示 Cmd/Ctrl、Shift 等符号。工具栏、画布选项及侧栏指引使用绿色浮层和九宫格键帽。`assets/img/keycap.svg` 按原版 CSS 制作；旧 `assets/img/key.png` 不是原版 CSS 实际使用的键帽背景，直接复用曾出现错误的像素黑边，已纠正。 |
| Home | 示例关闭按钮、顶排按钮宽度与对齐、卡片／滚动条、搜索与最近列表间距、最近列表行高与整行菜单按钮、路径分段及末级目录粗体、缺失路径样式已修正。 |
| 最新搜索框 | `d80d33d`：灰色 `Search...`、白色输入与光标、16px 圆角、1px 描边、聚焦黄色边线及 4px 橙色光晕；图标／文字偏移计入边框。筛选测试通过。仅改 Home 局部样式。 |

产品显示名称已改为 LevelEditor。macOS 本地打包输出为 `dist/LevelEditor.app`；自动更新 ZIP 内部仍保留旧 `LDtk Rust.app` 根路径，因为已安装旧更新器只接受该格式。不要仅为了清理旧命名就破坏兼容性；ARM／Intel 打包回归已覆盖。

Linux 字体需要单独核对：Mac 使用系统 Arial／Arial Bold；Linux 找不到 `src/theme.rs` 指定的 Arial 文件时会回退内置 RobotoCondensed，字宽和换行可能不同。中文与符号字体也使用各平台可用回退。先确认加载的字体及 DPI，再调整布局，不要靠随意改字号掩盖字体差异。

## 5. 最近用户截图：相邻关卡预览

用户询问平台示例中 `53 cells / 848px` 下方区域的名称，已解释并复核：

- 下方淡化／简化显示的是**相邻关卡预览区**；示例中是 `Bottom` 关卡。
- `53 cells / 848px` 是**当前关卡宽度标尺**，53 × 16px = 848px。
- 当前移植版已经显示该区域，不要误判为整个功能未实现。
- 示例 `Typical_2D_platformer_example.ldtk`：当前关卡 `Your_typical_2D_platformer` 为 848×336；`Bottom` 位于世界坐标 (80,336)，尺寸 464×256。
- Mac debug 实测视口 `[424,250,1.1,false]` 显示了下方预览，文档未修改、surface 错误为 0。参考 `src/canvas.rs` 的相邻关卡分支和宽度标尺。

## 6. 剩余工作：不要宣称完整 UI 已复刻

详细表格见 `docs/status/source-ui-parity-audit.md`。Linux 接手优先顺序：

1. **先做 Linux 真实桌面回归**：Home、搜索框、键帽、所有工具栏浮层、World 视图切换、保存确认、窗口关闭。记录桌面会话、驱动、DPI 与字体。
2. **图层专用表单**：仍有通用字段表；原版分组、列表颜色、异步渲染、Kill tiles、Bake 等缺项。不透明度与视差应使用原版百分比、范围和联动行为。
3. **枚举／图集**：Tags、值排序、每项图标选择器、外部枚举完整表单；图集的标签源应为可读枚举选择器，Tiles layout 分组仍不同。
4. **项目／实体／世界**：扩展名切换、命名规则变量块、导出定位、高级项与 MultiWorlds 规则；实体大预览、标签重命名、九宫格继承；异深度关卡的禁用条纹、背景图片完整历史菜单。
5. **Help**：文档／视频卡片和动态快捷键图标列表仍未复刻；不要因为工具栏键帽已完成就把 Help 一并标记完成。

现有浮层阴影和缩略图内阴影仍有原生近似；完整上下文菜单的内容／图标不在上一轮修复范围。后续每项都需要源码依据、真实交互回归与截图检查。

## 7. Linux 验证命令

先在仓库根目录执行普通检查；其中某些更新器测试需要本机回环网络及测试子进程，沙箱拒绝不等于程序回归：

```sh
scripts/cargo fmt --all -- --check
scripts/cargo test --workspace --locked
scripts/cargo test --example native_smoke --locked
scripts/cargo clippy --workspace --all-targets --locked -- -D warnings
python3 -m unittest discover -s tests -p 'test_*.py'
```

本机最后一次结果：应用 **370 passed / 6 ignored**，native_smoke 目标 **374 passed / 6 ignored**，完整工作区、Clippy、格式及差异检查通过。这里的 6 ignored 不能视为 GPU 测试已执行；实时私有更新验证也需要独立认证，不要直接运行所有 `--ignored`。

真实桌面自动捕获（debug；每个场景会打开窗口，先保存自己的编辑）：

```sh
scripts/cargo build --example native_smoke --locked
python3 scripts/verify-gui-acceptance.py \
  --binary target/debug/examples/native_smoke \
  --output .tools/linux-gui-debug
```

六场景为 Home、Entities、实体表单、实体按住拖动、World、绘制。要求退出成功、非空 PNG、正确状态、适配器和 DPI 记录、surface 错误为 0、示例文件哈希未变。`results.json` 的成功仅证明其覆盖场景，不证明所有 UI 像素保真或 OS 鼠标分发。

没有桌面时，可复用 CI 的独立 Xvfb＋lavapipe；结果必须标为软件渲染：

```sh
sudo apt-get install -y xvfb xauth
mkdir -p .tools/linux-xdg-runtime
chmod 700 .tools/linux-xdg-runtime
export XDG_RUNTIME_DIR="$PWD/.tools/linux-xdg-runtime"
export VK_DRIVER_FILES="$(python3 -c 'from glob import glob; p=glob("/usr/share/vulkan/icd.d/lvp_icd*.json"); assert len(p)==1,p; print(p[0])')"
export LIBGL_ALWAYS_SOFTWARE=1
xvfb-run -a -s '-screen 0 1600x1000x24' \
  python3 scripts/verify-gui-acceptance.py \
  --binary target/debug/examples/native_smoke --output .tools/linux-gui-lavapipe
```

上述软件驱动变量只适用于该无桌面分支；在真实硬件桌面新开终端使用实际驱动，记录实际返回的 adapter。

三项 GPU 测试单独执行，每条必须确实执行 1 项、0 failed、0 ignored：

```sh
scripts/cargo test --bin ldtk-rs --locked canvas::blur::gpu_tests::gaussian_composites_original_layer_then_blurs_on_gpu -- --ignored --exact --nocapture --test-threads=1
scripts/cargo test --bin ldtk-rs --locked canvas::outline::tests::field_outline_uses_two_sixty_sample_passes_and_reuses_cached_flow -- --ignored --exact --nocapture --test-threads=1
scripts/cargo test --bin ldtk-rs --locked app::gpu_surface_tests::surface_error_submits_pending_uploads_without_a_present -- --ignored --exact --nocapture --test-threads=1
```

## 8. Home 高内存修复与验收边界

Home 高内存已定位并修复：egui-wgpu 上传缓冲后，surface 获取失败直接返回会滞留 pending writes。生产错误回调在返回原 handler 动作前调用空 `queue.submit`，提交待处理上传。不要改成 sleep、强制等待 GPU、限帧或无依据裁剪缓存。

Metal 可控 Timeout 复现曾达到约 1.8GiB RSS；修复后的 1/5/10 秒压力保持约 197–199MiB、8–10 个 GPU buffers，并能恢复截图。原始历史 OS 触发事件没有记录，不能声称已重建所有高内存原因。依据：`docs/status/home-memory-investigation.md`、`home-memory-followup.json` 及上述 GPU surface-error 回归。

历史四平台原生 CI 的六场景＋三 GPU 测试通过，物理 M6 也有记录；物理 RTX 2060 只验证过三项离屏 GPU 测试，**没有该设备真实桌面呈现／输入验收**。这些记录主要对应较早的 `554e86a` 等基准，不能当作当前 `d80d33d` UI 的 Linux 验收。

## 9. 证据、原版源码与转移包

建议先读：

- `docs/status/source-ui-parity-audit.md`：最新 UI 修复与剩余差异。
- `docs/status/gui-platform-acceptance.md`：历史平台、实际 adapter、覆盖边界和复现方法。
- `docs/status/home-memory-investigation.md`：内存故障机制、生产修复和证据。
- `docs/status/build-command.md`：Mac 交叉构建与本地产物记录；不是当前 Linux 桌面结果。

需要对照原版或执行原版语料测试时，再准备固定源码检出：

```sh
git clone --no-checkout https://github.com/deepnight/ldtk.git .reference/ldtk
git -C .reference/ldtk checkout 2b7b5512f2f88fa281cb6492debbb26ead7838d4
```

`.tools`、`.reference`、`target`、`dist` 均被 Git 忽略；普通 pull 不会带来 Mac 本地日志、截图或工具链。现有状态文档含 Mac 绝对路径，Linux 上应按仓库相对路径重定位。

本交接另附轻量 ZIP，内有本文件、状态文档快照、关键原版／移植版截图、最新日志、SHA-256 清单及源码提交号。源码从 Git 获取，不包含 Mac 工具链、编译产物、用户配置或 token。将 ZIP 解压到 Linux 任意目录，证据从其 `evidence/` 查阅。

给下一会话的接续指令：

> 读取 `docs/status/handoff-linux-2026-11-17.md`（或转移包内同名文件），确认包含 `d80d33d`，先在 Linux 完成 debug 桌面与 GPU 验收，再按 `source-ui-parity-audit.md` 继续 UI 保真。保留已修复的关闭流程、事件修饰键、九宫格键帽、Home 搜索框和 surface-error 上传提交。先检查本机状态，不重做已有移植，不把历史 CI 当作当前 Linux 实测，不擅自发布。
