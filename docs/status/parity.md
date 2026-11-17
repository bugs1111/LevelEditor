# LDtk 功能与验收对照

基准：LDtk 1.5.3，源码 2b7b5512f2f88fa281cb6492debbb26ead7838d4。本表替代初始阶段旧清单。功能实现、原生 UI 与实际验证是不同层次。

| 原版能力 | 当前实现与证据 |
|---|---|
| 原生 Rust/UI/GPU | eframe/egui + wgpu；Metal 实际窗口/截图；无 Electron、WebView、JS 或 Haxe 运行时 |
| JSON、外部关卡、迁移 | 保留未知键；49 个工程与 3 个外部关卡逐值 roundtrip；49 个工程二次迁移幂等 |
| 新建/打开/保存/另存 | 原生文件选择、后台读写、保存前提交输入、外部存储和资源路径迁移、覆盖冲突检测 |
| 备份/恢复/崩溃恢复 | 保存备份、目录/保留上限、内存和外部关卡恢复；Home 绑定原路径；正常退出不造崩溃文件 |
| 文件/资源监听 | 文档与资源分类、图集重载/重映射、找回/重试/忽略、枚举定位和修改冲突 |
| IntGrid/Tiles | 画笔/矩形/填充/擦除、稀疏 brush、镜像、Stamp/Random、堆叠、透明规则、图集重映射 |
| 自动铺砖 | 规则/顺序/噪声/biome/可选/模运算/偏移/kill tiles；官方 57,682 个保存 tile 精确匹配，66 次编辑与全量重建一致 |
| 规则 UI | 规则/组 CRUD、跨组拖动、剪贴板/重映射、47 片段向导、批量边界策略和一次撤销 |
| 实体/字段 | 全部类型、默认/null/wrapper、数组独立槽、约束/诊断、形状/图块/九宫格、数量/标签限制 |
| 引用/路径 | Point 单点/数组/路径/拖动；跨层/关卡/世界 EntityRef、返回/Follow、双向引用、自动串联及预览 |
| 文本编辑器 | 原生窗口、十种语言、Lucario/行号、括号/XML 标签、只读日志、外部文本；区分键入/粘贴/IME |
| 数组默认语义 | 按槽 Set/Insert/Remove/Move/Reset；未动默认槽和 wrapper 扩展保持；默认变化与双向引用回归 |
| 世界/关卡 | 四布局、深度、上下文菜单、位置预览/snap、跨世界移动、八方向缩放、确认/最后一项保护 |
| 定义/枚举 | 层/实体/字段/图集/枚举编辑/排序/复制/剪贴板；外部 JSON/Text/Haxe/CastleDB 同步与变更预览 |
| 选择/历史 | 混合层/稀疏/空白/实体/Point、精确移动/复制/粘贴、源描边/ghost；关卡历史及 LastChance |
| 背景/画布 | 五种定位/重复、pivot/默认色、视差/缩放/偏移/邻居/世界缓存、原 bitmap 字体与图块 |
| GPU 效果 | Gaussian/color transform、字段轮廓、层/世界遮罩、阴影；真实 GPU readback 与原版部分像素采样 |
| 操作流程 | 真实 egui 回归覆盖保存前提交、同帧末输入、搜索、release/焦点丢失、世界菜单/表单/picker、资源恢复 |
| Home/面板/资源 | 原版 icons/fonts/图集/完整 examples/thumbs；300pt sidebar、Home/定义表单/popout/颜色/右侧检查器持续以实际截图核对 |
| 格式/导入导出 | PNG/CSV/Super Simple/Tiled/GameMaker；14 官方样本实际导出；Ogmo 3.3+；保留源实验性限制并报告 |
| 命令/搜索/设置/帮助 | 原生参数/输出/取消、保存/加载钩子/信任、搜索、uiStates/lastUiDirs/recentDirColors、帮助/Markdown 发布说明 |
| macOS 集成 | document-open AppleEvent、启动前队列/运行时唤醒、Unicode、冷/热启动、签名包和文件类型声明 |
| Rust 自动更新 | 指定私有 GitHub Releases、会话认证、平台匹配、SHA-256、取消与解包；未保存保护、Rust helper 替换、首帧 ACK、失败回滚及缓存清理；临时安装与真实输入回归通过 |
| 流畅性 | 裁剪/批次绘制、后台加载、128MiB 资源缓存、局部历史/增量重算；实测区分 App 更新、CPU 渲染、帧间隔、GPU 呈现 |

## 验收边界

- 编辑/数据/导出链路已有实现和回归，继续以原版实际 UI 修正视觉/流程差异；完整逐像素等价尚未建立，动画相位未在两进程间同步。
- 49 个工程/552 层确定性重建中，477 个历史保存缓存匹配，75 个不同。历史缓存不能直接代表当前原版的重建结果；官方当前样本和独立编译原版算法向量另有精确比较。
- 保存有预检、暂存、fsync、发布顺序及可处理错误回滚，不承诺任意断电时独立文件之间的原子事务。
- Tiled/GameMaker/PNG 保留上游支持范围和局限；语料迁移幂等不等于每个历史版本加载器输出字节相同。
- 0.1.1 四平台格式、严格 Clippy、工作区测试、capture 回归、release 构建和打包通过；每平台六个原生 GUI 场景和三项 GPU 测试通过，24 张截图已逐张检查。CI macOS 为 Apple Paravirtual/Metal、Windows 为软件 DX12、Linux 为 Xvfb/lavapipe Vulkan；物理 M6/Metal 六场景及 GPU 检查通过，RTX 2060/Vulkan 三项离屏检查通过。见 [GUI/GPU 证据](gui-platform-acceptance-results.json)和 [0.1.1 包验收](release-0.1.1-acceptance.json)。Home 取帧错误时上传缓冲累积已复现并修复，压力结果见 [内存调查](home-memory-investigation.md)。0.1.1 发布被自动审批拦截，等待当前用户明确授权；已发布的 v0.1.0 真实私有源结果保留在 [历史证据](platform-acceptance.json)。所有实体硬件上的完整 UI 呈现、OS 输入及输入延迟未由这些检查建立保证。

证据：[最终集成检查](final-verification.md)、[formats](formats-report.md)、[workflow](workflow-report.md)、[autotile](autotile-report.md)、[renderer](renderer-report.md)、[resources](resource-recovery-report.md)、[acceptance](acceptance-report.md)。旧 review 是问题发现时的快照，最终修复和验证见执行账本。
