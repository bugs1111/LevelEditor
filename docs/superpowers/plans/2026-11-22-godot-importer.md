# Godot Importer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** 将可独立复制安装、经过真实 Godot 导入验证的 LDtk 插件接入 `importer/godot/`。

**Architecture:** 固定上游源码直接纳入仓库，保持 Godot 标准 `addons/ldtk-importer/` 路径。独立验证工程和小地图覆盖 PackedScene、TileMapLayer、实体字段、重新导入；Rust 编辑器不依赖 Godot 运行。

**Tech Stack:** Godot 4.7.2、GDScript、Python 3 标准库测试驱动。

**Spec:** `docs/superpowers/specs/2026-11-22-godot-importer-design.md`

## Global Constraints

- 上游固定为 `heygleeson/godot-ldtk-importer@0ecab8d1cc6531801b487157848059c3b8971aa0`，保留 MIT 作者和完整许可。
- 仅修改 LevelEditor 仓库；不修改 Dungeon/Bloodlands，不增加游戏专用行为。
- 验证本机 Godot 4.7.2 和 LDtk 1.5.4；只为观察到的问题做兼容修正。
- 用户已明确批准实施，直接在当前 `rust-port` 功能分支完成；保留改动供用户查看，不自动提交、推送或发布。

## Review Focus

- 插件单独复制到带空格目录时，所有脚本和资源仍可加载。
- LDtk 1.5.3 与 1.5.4 的图块、实体字段都保留实际值。
- 重新导入后没有保留上一次地图的图块和字段数据。
- 外部分关卡文件被读取，保留相对于关卡的坐标。
- 多世界数据与可空/数组字段不会因旧导入器假设而静默丢失。

---

### Task 1: 独立插件与真实导入

**Files:** `importer/godot/addons/ldtk-importer/**`、`project.godot`、`tests/**`、`UPSTREAM.md`。

**Interfaces:** 消费标准 `.ldtk` 和 PNG；产出可由 Godot `load(...).instantiate()` 使用的 `PackedScene`。

- [x] 写独立临时 Godot 工程的导入测试，断言实际图块坐标/数量、实体名/坐标及字段值。
- [x] 在插件缺失时运行测试，确认导入失败而非测试脚本语法错误。
- [x] 导入固定上游插件和完整 MIT 许可证，仅修复实际失败的兼容问题。
- [x] 运行真实 Godot headless 导入与场景实例化检查，覆盖 1.5.3/1.5.4、可空/数组字段和带空格目录。

Run: `python3 importer/godot/tests/run.py`
Expected: 全部检查通过，缺失插件时非零退出，Godot 无 SCRIPT ERROR/ERROR。

### Task 2: 再次导入、外部分关卡和使用文档

**Files:** `importer/godot/tests/**`、`importer/godot/README.md`、根 `README.md`、`THIRD_PARTY_NOTICES.md`、`.gitignore`。

**Interfaces:** 消费 Task 1 的独立插件；保持复制 `addons/ldtk-importer/` 即可安装。

- [x] 扩充真实重新导入与外部分关卡/多世界测试；每个发现的缺陷先记录失败再修复。
- [x] 文档写明复制/启用插件步骤、地图与图片路径、实体/碰撞适配、确实验证的版本和未覆盖输入。
- [x] 补充上游来源及根目录导航，忽略 Godot 导入缓存和测试生成资源。
- [x] 运行完整 importer 测试、差异检查和一次独立代码审查，处理重要发现。

Run: `python3 importer/godot/tests/run.py`、`git diff --check`
Expected: 所有实际导入与重导入检查通过；无意外 Rust 文件修改或缓存入库。
