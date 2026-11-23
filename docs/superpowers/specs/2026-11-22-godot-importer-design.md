# Godot importer 接入方案

## 目标与目录

在 LevelEditor 仓库提供可复制到 Godot 游戏中的 LDtk 导入插件。用户指定入口为 `importer/godot/`；Rust 编辑器和 Godot 插件保留独立运行方式。

```text
importer/godot/
  addons/ldtk-importer/   # 插件及其上游 MIT 许可证
  project.godot          # 独立的最小验证工程
  tests/                # 小地图、PNG 图集和实际导入检查
  README.md             # 安装、验证、限制和上游来源
```

## 方案选择

- 同仓库独立目录：版本和地图格式一起维护，普通 clone 即可取得插件。用户已指定使用 `importer/godot/`。
- 独立仓库：可单独发布，但增加一次版本同步和依赖管理。本次不采用。

## 实现范围

1. 以 `heygleeson/godot-ldtk-importer` 的固定提交 `0ecab8d1cc6531801b487157848059c3b8971aa0` 为来源，保留作者、MIT 许可证、插件路径及上游记录。普通源码文件直接纳入仓库，避免要求使用者初始化子模块。
2. 插件安装后识别 `.ldtk` 并通过 Godot 导入流程生成 `PackedScene`，保留图集、图块层、实体字段与上游导入后脚本入口。
3. 使用本机 Godot `4.7.2.stable.official.ed1daf0bf` 实际验证；对于当前 LevelEditor 保存的 1.5.4 JSON，只修复验证中发现的必要兼容问题。不能仅修改版本字符串就宣称兼容。
4. 提供小型测试地图，检查实际 `PackedScene` 加载、图块数量/坐标、实体与字段、再次导入。单独记录尚未覆盖的输入和上游限制。
5. 增加根 README 导航、第三方声明及 Godot 缓存忽略规则。

## 范围边界

本次在 LevelEditor 仓库内接入和验证，不直接修改 Dungeon 或 Bloodlands。不把游戏专用怪物、门、宝箱、碰撞和 AI 行为硬编码进通用插件。不改变 Rust 发布流程或默认安装用户的 Godot 游戏。

## 验收

- Godot headless 编辑器能注册插件并真实导入测试 `.ldtk`。
- 导入结果是可实例化的 `PackedScene`，检查图块层及实体字段数据。
- 再次保存/导入后可加载更新的地图，不残留导入失败。
- 插件可单独复制 `addons/ldtk-importer/` 使用，所需脚本和许可证齐全。
- 明确记录 Godot 与 LDtk 测试版本；不把未测试版本标为已支持。

## 当前确认状态

同仓库和 `importer/godot/` 目录已由用户指定。用户于 2026-11-22 明确选择“按方案实施”，接入范围已确认。
