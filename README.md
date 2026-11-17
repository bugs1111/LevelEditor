# LDtk Rust 移植

正在进行的全 Rust 原生桌面移植，包含窗口、界面、GPU 画布和编辑核心，无 Electron、WebView、JavaScript 或 Haxe 运行时。

核心编辑、数据和导出链路已实现，原版 icons、字体、图集、发布说明图片及完整 examples 已复用。功能实现、原生 UI 与实际验证分开记录在[功能对照表](docs/status/parity.md)和[验收报告](docs/status/acceptance-report.md)中；测试通过或局部像素一致不代表全部界面状态和平台已完成保真验收。

上游基准：[deepnight/ldtk](https://github.com/deepnight/ldtk/tree/2b7b5512f2f88fa281cb6492debbb26ead7838d4)，提交 `2b7b5512f2f88fa281cb6492debbb26ead7838d4`。本机对照安装为 LDtk 1.5.3。

## 运行

```sh
scripts/run
scripts/run examples/original/Typical_TopDown_example.ldtk
```

脚本优先使用项目内 `.tools/` 工具链，也支持系统已安装的 Cargo。首次安装 Rust 可运行 `scripts/bootstrap-rust`，该脚本不修改全局 shell 配置。

macOS 应用打包：

```sh
scripts/package-macos
```

生成 `dist/LDtk Rust.app`。本机实际运行平台为 Apple Silicon macOS；Windows/Linux 的编译检查与其他平台的实际 UI/GPU 验收分开记录。包支持 Finder 打开 .ldtk，并保留原版默认文件关联。

打包后可运行 `scripts/verify-macos-bundle`，使用独立设置和复制的示例检查含空格/中文路径的冷启动、运行时打开文件与进程复用；结束时关闭本次测试创建的进程。

基本操作：左键绘制、右键擦除、Alt 点击取样，空格加左键或中键拖动平移，滚轮以鼠标位置为中心缩放。轻按空格适配视图，`~` 世界视图，双击关卡进入，`1–0` 选择图层，`G` 网格，`X/Y` 瓦片翻转。Cmd/Ctrl+S 保存，Cmd/Ctrl+Z 撤销，Cmd/Ctrl+Y 或 Cmd/Ctrl+Shift+Z 重做。

新建先选择位置，保存成功后进入编辑器；Home 可导入 Ogmo、打开全部官方示例和最近工程、浏览备份。支持 IntGrid/Tiles/AutoLayer/Entities、自动规则与向导、混合选择/剪贴板、Point/EntityRef/数组默认、多世界、定义和枚举编辑、所有原版导出、自定义命令/钩子、资源重载、备份/崩溃恢复及多语言文本编辑器。重复导入遇到已有同名工程会使用新文件名，保留原文件。

## 检查

```sh
scripts/cargo test --workspace --locked
scripts/cargo clippy --workspace --all-targets --locked -- -D warnings
scripts/cargo fmt --all -- --check
scripts/cargo build --release --locked
```

原版数据语料检查需要 `.reference/ldtk` 是上述固定提交的源码检出：

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

仓库目前为私有：在应用设置的 `Check for updates` 窗口输入仅本次会话有效的 GitHub token，或由启动环境提供 `GH_TOKEN` / `GITHUB_TOKEN`。token 需要仓库 Contents 读取权限，不写入应用配置文件。

仓库的 `Build and release` 工作流会在四个平台运行检查、测试并打包。标签 `v<版本>` 或手动选择发布会生成包含所有平台校验值的 `ldtk-rs-updates.json`；稳定通道拒绝预发布版本。macOS 本地仍可用 `scripts/package-macos` 生成应用，用 `python3 scripts/package-release.py --target aarch64-apple-darwin` 生成对应 ZIP。

[v0.1.0 四平台下载](https://github.com/bugs1111/LevelEditor/releases/tag/v0.1.0)已发布。四平台原生 CI、发布包校验、真实私有更新源和 ARM macOS 下载包冷/热打开均已通过，具体结果与验收范围见[最终集成记录](docs/status/final-verification.md)。
