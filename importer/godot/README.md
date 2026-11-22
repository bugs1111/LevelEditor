# LevelEditor → Godot

本目录在同一个仓库内独立维护 Godot 导入插件。LevelEditor 保存 `.ldtk`，插件在 Godot 编辑器中将地图导入为可实例化的 `PackedScene`，图块使用 `TileMapLayer`。Godot 游戏不需要运行 Rust 编辑器。

## 安装到游戏

1. 将这里的 **`addons/ldtk-importer/` 整个文件夹**复制到游戏的 `addons/` 下。目标为 `res://addons/ldtk-importer/plugin.cfg`。Godot 项目根目录是 `project.godot` 所在目录。
2. 在 Godot 的「项目 → 项目设置 → 插件」启用 **LDTK**。
3. **每个 `.ldtk` 工程单独使用一个目录**，将图集 PNG、背景图片及需要的外部 `.ldtkl` 一起放入，保持相对路径，例如 `res://maps/forest/world.ldtk`。同一工程内可以有多个关卡；不同工程不要共用生成的 `levels/` 和 `tilesets/` 目录，否则同名资源可能被覆盖。推荐直接在 LevelEditor 中编辑这份地图。
4. 等待 Godot 导入完成，将 `.ldtk` 从文件系统面板拖入自己的关卡场景，或通过代码加载：

```gdscript
extends Node2D

const MapScene: PackedScene = preload("res://maps/world.ldtk")

func _ready() -> void:
    add_child(MapScene.instantiate())
```

只需复制插件文件夹，`project.godot`、`demo.tscn` 和 `tests/` 属于本仓库的演示工程。

## 地图与游戏逻辑

- Tiles、AutoLayer 和有图集的 IntGrid 转成图块层，使用保存好的图块结果。
- 实体保存在 `LDTKEntityLayer.entities`；在地图的「导入」面板启用 `use_entity_placeholders` 可显示实体占位节点。
- IntGrid 数值不等于物理碰撞。可以编辑生成 TileSet 的 Physics Layer，或用 Tileset/Entity/Level/World Post-Import 脚本生成游戏需要的内容。
- 把 `PlayerSpawn`、`Enemy`、`Door` 等实体映射成游戏自己的场景，并读取其字段配置。
- 将游戏逻辑放在自己的父场景或 Post-Import 脚本中。地图导入场景由插件重新生成，手工改动该场景不能代替导入适配。

默认会在地图旁生成 `tilesets/`，单世界的 `pack_levels` 选项还会生成 `levels/`。主 `.ldtk` 保存后，Godot 自动重新导入。**仅修改外部 `.ldtkl` 不会自动重新导入主地图**：在 Godot 文件系统中选择主 `.ldtk`，在「导入」面板点击「重新导入」，或重新保存主文件后再使用地图。

`force_tileset_reimport` 默认关闭；开启会重建 TileSet，覆盖其中的手工设置。

上游 Post-Import 模板在 `addons/ldtk-importer/post-import/`。插件的节点类型及导入选项沿用上游，游戏专用行为由游戏项目维护。

## 打开横版演示

在仓库根目录执行，确保已安装的 Godot 可以通过 `godot` 命令调用。

```sh
just demo-godot
```

Godot 打开后，等导入完成再按 `F5`。演示使用仓库自带的 `Typical_2D_platformer_example.ldtk`，显示主关卡地形、玩家和怪物。这是静态预览。命令把演示工程生成到 `.tools/godot-platformer-demo/`，并使用仓库内的原版地图和图集。

如需打开最小验证工程，可在 Godot 项目管理器中导入本目录的 `project.godot`。`tests/maps/smoke.ldtk` 包含两块测试图块和 `PlayerSpawn` 实体。

无窗口验证使用相同的本机 Godot，不下载或复制引擎：

```sh
python3 importer/godot/tests/run.py
```

Godot 不在 PATH 时，可指定已有可执行文件：

```sh
python3 importer/godot/tests/run.py --godot /Applications/Godot.app/Contents/MacOS/Godot
```

测试会在临时目录建立独立地图项目，调用 Godot 的真实编辑器导入流程，再加载并实例化结果；检查图块数量/坐标/翻转、实体坐标、字段类型、可空数组、枚举数组类型、多世界覆盖图集，以及重新导入后的变化。不会修改 Dungeon、Bloodlands 或正式游戏地图。日志和结果清单保存在仓库 `.tools/godot-importer/`。

首次直接打开验证工程时，Godot 可能生成正常的 `.gd.uid` 脚本标识文件；固定上游没有附带这些文件，本次也未预生成。需要长期编辑插件源码时，可将生成的稳定 UID 与脚本一起纳入版本控制。

## 兼容范围

本次验收目标为本机 Godot **4.7.2.stable.official.ed1daf0bf**，测试地图格式为 LDtk **1.5.3 / 1.5.4**。其他 Godot 版本、全部官方样例及每项高级导入选项尚未进行穷尽验证。Godot 3 不提供本插件使用的 `TileMapLayer`。

测试图集使用 PNG；Aseprite 和编辑器内置图集不能据此视为已支持。先将游戏图集准备为 Godot 能加载的图片资源。

上游 `pack_levels` 模式下的跨关卡 EntityRef 解析仍有限制。游戏需要将实体替换成自定义场景时，还应通过上游的实例引用接口维护 EntityRef；可参考上游 README 的 Post-Import 示例。

多世界当前保存为一个总场景；单独生成各关卡的 `levels/` 文件仅在单世界路径实现。多世界内的实体字段、关卡节点与原始世界顺序已经纳入检查。

非空值和空的本地枚举数组在 Post-Import 阶段保留上游 `Array[String]` 类型。实测 Godot 4.7.2 的二进制场景重载会丢失字典内嵌数组的类型标记（值不变）；运行时给自己的强类型数组赋值时，可使用其 `assign()` 方法。这是独立最小 PackedScene 测试确认的引擎序列化行为。

来源、许可证和本地改动见 [UPSTREAM.md](UPSTREAM.md)。实际检查、修复依据与保留限制见 [VALIDATION.md](VALIDATION.md)。
