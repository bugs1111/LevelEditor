# LDtk Rust 移植

正在进行的全 Rust 原生桌面移植，包含窗口、界面、GPU 画布和编辑核心，无 Electron、WebView、JavaScript 或 Haxe 运行时。

**当前是可验证的第一阶段实现，尚未达到原版完整功能、视觉和操作流程对等。** 完整移植要求保留在[功能对照表](docs/status/parity.md)中；未实现的功能不会计为完成。

上游基准：[deepnight/ldtk](https://github.com/deepnight/ldtk/tree/2b7b5512f2f88fa281cb6492debbb26ead7838d4)，提交 `2b7b5512f2f88fa281cb6492debbb26ead7838d4`。本机对照安装为 LDtk 1.5.3。

## 运行

```sh
scripts/run
scripts/run examples/topdown/Typical_TopDown_example.ldtk
```

脚本优先使用项目内 `.tools/` 工具链，也支持系统已安装的 Cargo。首次安装 Rust 可运行 `scripts/bootstrap-rust`，该脚本不修改全局 shell 配置。

macOS 应用打包：

```sh
scripts/package-macos
```

生成 `dist/LDtk Rust.app`。当前本机产物为 Apple Silicon 架构；Windows、Linux 和 Intel Mac 构建尚未验证。

基本操作：左键绘制、右键擦除、Alt 点击取样，空格加左键或中键拖动平移，滚轮以鼠标位置为中心缩放。`F` 适配视图，`~` 世界视图，双击关卡进入，`1–0` 选择图层，`G` 网格，`X/Y` 瓦片翻转。Cmd/Ctrl+S 保存，Cmd/Ctrl+Z 撤销，Cmd/Ctrl+Y 或 Cmd/Ctrl+Shift+Z 重做。

新建工程后可在 Layers 添加 IntGrid、Tiles 或 Entities 图层。在 Tilesets 导入图片，再在 Layers 为 Tiles 图层选择图集；Entities 可创建基本实体定义。高级规则、完整定义编辑等尚未移植，见对照表。

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

- `crates/ldtk-core/`：完整 JSON 文档、独立分关卡存储、文件写入、基本编辑操作与历史。
- `src/`：原生界面、视口、输入、图层渲染和后台纹理解码。
- `examples/topdown/`：原版官方地图样本与相应图集。
- `docs/status/`：功能覆盖、测试证据、限制和进度。

原版与资源的声明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
