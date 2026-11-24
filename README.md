# Level Editor

正在进行的全 Rust 原生桌面移植，包含窗口、界面、GPU 画布和编辑核心，无 Electron、WebView、JavaScript 或 Haxe 运行时。

核心编辑、数据和导出链路已实现，原版 icons、字体、图集、发布说明图片及完整 examples 已复用。功能实现、原生 UI 与实际验证分开记录在[功能对照表](docs/status/parity.md)和[验收报告](docs/status/acceptance-report.md)中；测试通过或局部像素一致不代表全部界面状态和平台已完成保真验收。

当前移植对照为 LDtk 1.5.4 预览版，使用 [deepnight/ldtk 的固定提交](https://github.com/deepnight/ldtk/tree/53cfd5392e7cbbf83730cba6ff0330884a6d48d8)。已有的数据语料检查仍使用 [1.5.3 的固定提交](https://github.com/deepnight/ldtk/tree/2b7b5512f2f88fa281cb6492debbb26ead7838d4)。

## 运行

安装 `just` 后，可在项目目录执行：

```sh
just push
just push "提交说明"
just pull
just run
just run examples/original/Typical_TopDown_example.ldtk
```

`just push` 将已有提交推送到当前分支的上游（当前为 `origin/main`）。`just push "提交说明"` 会先暂存全部改动，有改动时提交，再推送。`pull` 仅允许快进更新。`run` 默认编译并启动 debug 版本，保留传入参数；运行状态和错误同时写入终端及 `LDtk.log`，重复状态不会每帧刷屏。默认开启 `RUST_BACKTRACE=1`，可用已有环境变量覆盖。

也可直接使用脚本：

```sh
scripts/run
scripts/run examples/original/Typical_TopDown_example.ldtk
```

脚本使用 `PATH` 上的 Cargo。首次安装 Rust 可运行 `scripts/bootstrap-rust`，rustup 默认使用当前用户的 `~/.cargo` 和 `~/.rustup`。安装后打开新终端即可在各项目中使用。

Linux 的 X11 运行环境还需要 Vulkan 驱动与 `libxkbcommon-x11`；Ubuntu/Debian 对应运行库为 `libvulkan1`、`libxkbcommon-x11-0`。GUI CI 使用独立 Xvfb 显示和 Mesa lavapipe，实际适配器及硬件验收范围见[GUI/GPU 记录](docs/status/gui-platform-acceptance.md)。

在 Mac 上构建便携程序：

```sh
just build
just build -m debug
just build -t mac -m debug
just build -t mac -a arm64
just build -t mac -a x86_64
just build -t win -m release
just build -t linux -m release
```

`-t` 默认是 `all`，依次构建 macOS、Windows 和 Linux。`-m` 默认是 `release`。macOS 默认使用本机工具链的架构，也可通过 `-a arm64` 指定 Apple Silicon，或通过 `-a x86_64` 指定 Intel Mac。`-t all` 搭配 `-a` 时，该参数只选择 macOS 的架构。构建成功后生成独立便携目录，包含可执行文件、原版资源、examples 和许可文件，可整体复制到目标系统。

| `-t` | 构建目标 | 输出目录 |
|---|---|---|
| `mac` | 本机 macOS 架构 | `dist/mac/<模式>/Level Editor.app` |
| `mac -a <架构>` | Apple Silicon 或 Intel | `dist/mac/<架构>/<模式>/Level Editor.app` |
| `win` | `x86_64-pc-windows-gnu` | `dist/win/<模式>/level-editor.exe` |
| `linux` | `x86_64-unknown-linux-gnu`，glibc ≥ 2.28 | `dist/linux/<模式>/level-editor` |

本机 macOS 构建可以直接使用 Homebrew Rust。跨平台或跨架构构建需要已安装并选中的 Rustup 工具链，脚本会为它按需安装缺少的 Rust target。Windows 使用 [Homebrew MinGW](https://formulae.brew.sh/formula/mingw-w64)，缺少编译器时执行 `brew install mingw-w64`。编译器运行库静态链接。Linux 构建所需的 [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) 0.21.8 和 Zig 0.15.2 会自动安装到 `~/.cargo/cross`，已设置 `CARGO_HOME` 时使用其 `cross` 子目录。其他项目可以通过 Cargo 的 `bin` 目录调用这两个命令。首次安装需要网络，后续构建复用已安装的工具和缓存。

这些命令使用 `Cargo.lock` 构建当前源码并打印最终路径。Linux 运行时仍需 Vulkan 驱动及 X11/Wayland 运行库。Windows/Linux 的交叉编译与目标系统的 GUI 运行分别验收；本地构建记录见 [构建命令验收](docs/status/build-command.md)。

macOS 的 `just build` 产物可在 Finder 中双击打开。发布流程也可以在根输出目录生成本机 Release 应用：

```sh
scripts/package-macos
```

生成 `dist/Level Editor.app`。四平台已通过六场景原生 GUI 和三项 GPU 自动验收，CI 实际使用的虚拟/软件驱动与物理 M6、RTX 2060 验证范围见 [GUI/GPU 记录](docs/status/gui-platform-acceptance.md)。包支持 Finder 打开 .ldtk，并保留原版默认文件关联。

打包后可运行 `scripts/verify-macos-bundle`，使用独立设置和复制的示例检查含空格/中文路径的冷启动、运行时打开文件与进程复用；结束时关闭本次测试创建的进程。

基本操作：左键绘制、右键擦除、Alt 点击取样，空格加左键或中键拖动平移，滚轮以鼠标位置为中心缩放。轻按空格适配视图，`~` 世界视图，双击关卡进入，`1–0` 选择图层，`G` 网格，`X/Y` 瓦片翻转。Cmd/Ctrl+S 保存，Cmd/Ctrl+Z 撤销，Cmd/Ctrl+Y 或 Cmd/Ctrl+Shift+Z 重做。

新建先选择位置，保存成功后进入编辑器；Home 可导入 Ogmo、打开全部官方示例和最近工程、浏览备份。支持 IntGrid/Tiles/AutoLayer/Entities、自动规则与向导、混合选择/剪贴板、Point/EntityRef/数组默认、多世界、定义和枚举编辑、所有原版导出、自定义命令/钩子、资源重载、备份/崩溃恢复及多语言文本编辑器。重复导入遇到已有同名工程会使用新文件名，保留原文件。

打开工程后，可在「项目导出」中选择 `Tiled JSON (.tmj)` 或 `Tiled TMX`。导出目录包含每个关卡的地图和对应的 `.world` 文件。在 Tiled 中使用「World → Load World」可按原世界坐标查看多张地图。

## 界面语言

默认跟随系统界面语言。没有对应翻译时使用英文。按 F12 打开应用设置，在「常规 → 界面语言」选择跟随系统、English 或简体中文，设置会立即生效并在下次启动时保留。

## 让 AI 助手编辑关卡（MCP）

打开已保存的工程后，按 F12，在「高级选项 → AI 助手（MCP）」复制客户端配置。

用 `level-editor` 可执行文件启动 MCP。每个会话绑定一个本地 `.ldtk` 项目，通过标准输入输出通信，无需启动桌面窗口。

在 Codex 中注册，替换为可执行文件和项目的实际绝对路径。

```sh
codex mcp add level-editor -- "/path/to/level-editor" --mcp "/path/to/My Game/world.ldtk"
```

macOS 应用包中的可执行文件是 `/Applications/Level Editor.app/Contents/MacOS/level-editor`，Windows 使用发布包里的 `level-editor.exe`。其他 MCP 客户端可使用以下 stdio 配置。

```json
{
  "mcpServers": {
    "level-editor": {
      "command": "/path/to/level-editor",
      "args": ["--mcp", "/path/to/My Game/world.ldtk"]
    }
  }
}
```

1. 用 `project_info`、`get_definitions`、`list_worlds`、`list_levels` 了解项目，再用 `get_level` 和 `get_layer` 确认关卡、图层和实体的 IID。
2. `paint_int_grid` 用图层网格坐标写入一个格子，值为 `0` 时清空。`paint_tile`、`erase_tile` 使用同样的网格坐标。`add_entity`、`move_entity` 使用相对于关卡原点的像素坐标，`remove_entity` 按实体 IID 删除。
3. 修改先保存在 MCP 进程内存中，调用 `save_project` 才会写入工程和外部关卡文件。保存前如果发现磁盘文件已更改，会保留会话中的修改并返回冲突。用 `reload_project` 读取磁盘版本，有未保存修改时必须显式传入 `discard_changes: true` 才会丢弃。退出会话会丢弃未保存修改。
4. 如果桌面编辑器同时打开了该工程，MCP 保存后可在外部修改提示中选择「从磁盘重新加载」查看结果。有未保存的桌面编辑时，编辑器会先询问是否保存。

MCP 保存使用工程的备份设置，范围为 LDtk 项目文档及其外部关卡。工具列表和参数以客户端查询到的 MCP schema 为准。

## Godot 地图导入

配套 Godot 插件在 [`importer/godot/`](importer/godot/README.md)，与 Rust 编辑器在同一仓库独立维护。将其 `addons/ldtk-importer/` 复制到游戏的 `addons/` 并启用插件后，Godot 可把 `.ldtk` 导入为场景和 `TileMapLayer`；实体与碰撞通过游戏自己的导入后脚本适配。

打开本仓库的 LDtk 横版地图演示，或运行最小地图的导入检查。命令使用已安装的 Godot。

```sh
just demo-godot
python3 importer/godot/tests/run.py
```

等待首次导入完成后，点击 Godot 编辑器右上角的 ▶「运行项目」，即可查看横版地图、玩家和怪物。快捷键以按钮提示为准。

安装方法、测试版本、上游来源与限制见 [Godot importer 文档](importer/godot/README.md)。

## 检查

```sh
scripts/cargo test --workspace --locked
scripts/cargo clippy --workspace --all-targets --locked -- -D warnings
scripts/cargo fmt --all -- --check
scripts/cargo build --release --locked
```

原版数据语料检查需要 `.reference/ldtk` 是上述 1.5.3 固定提交的源码检出：

```sh
scripts/cargo test -p ldtk-core --test upstream_corpus -- --ignored --nocapture
```

这项检查比较原版工程的全部 JSON 值和分关卡文档，不等于验证迁移、图像资源搬迁或在原版中的所有编辑行为。

原生窗口运行检查使用真实 GPU 后端，自动绘制、保存自身截图并退出：

```sh
scripts/cargo run --release --example native_smoke -- examples/topdown/Typical_TopDown_example.ldtk
```

## 结构

- `crates/ldtk-core/`：文档、外部分关卡、迁移/保存/备份/导出/导入、自动铺砖、完整编辑与历史。
- `src/`：原生界面、视口、输入、图层渲染和后台纹理解码。
- `examples/topdown/`：原版官方地图样本与相应图集。
- `docs/status/`：功能覆盖、测试证据、限制和进度。

原版与资源的声明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。

## 发布与更新

源码和 Rust 发布源为 [bugs1111/LevelEditor](https://github.com/bugs1111/LevelEditor)。更新从该仓库的 Releases 获取匹配系统/架构的 Rust 包，下载后验证大小及 SHA-256。安装前处理未保存修改，复制的 Rust 更新助手负责替换、重启和失败回滚；开发目录中的 `cargo run` 不允许原地替换。

公开 Releases 可直接检查更新。如果发布源需要身份验证，可在 `Check for updates` 窗口输入会话令牌，或由启动环境提供 `GH_TOKEN` / `GITHUB_TOKEN`。令牌需要仓库 Contents 读取权限，仅用于当前会话。

先提交要发布的改动并合入最新的 `origin/main`，再运行 `just pub`。它取源码版本和最新稳定 Release 中较高的版本，将补丁号加一。也可以通过 `just pub 0.2.0` 指定稳定版本。命令更新 `Cargo.toml` 和 `Cargo.lock`，将发布提交和标签一起推送到 `main`。

标签 `v<版本>` 触发 `Build and release` 工作流。四个平台完成检查、测试和打包后，工作流会发布 `level-editor-v<版本>-<构建目标>.zip` 和自动更新清单 `level-editor-updates.json`。macOS 本地可用 `scripts/package-macos` 生成应用，用 `python3 scripts/package-release.py --target aarch64-apple-darwin` 生成对应 ZIP。

从 v0.1.8 或更早版本升级时，请从 Releases 下载并手动替换一次应用，之后可继续使用应用内更新。MCP 客户端和启动脚本中的旧可执行文件路径也需要改为上文对应平台的新路径。

[v0.1.0 四平台下载](https://github.com/bugs1111/LevelEditor/releases/tag/v0.1.0)已发布。四平台原生 CI、发布包校验、真实私有更新源和 ARM macOS 下载包冷/热打开均已通过，具体结果与验收范围见[最终集成记录](docs/status/final-verification.md)。

0.1.1 已修复取帧失败时 GPU 上传缓冲累积导致的 Home 高内存。四平台构建、GUI/GPU 与应用包的历史验证结果见 [0.1.1 包验收](docs/status/release-0.1.1-acceptance.json)及 [Home 内存调查](docs/status/home-memory-investigation.md)。
