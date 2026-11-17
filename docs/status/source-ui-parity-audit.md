# 原版界面代码对照排查（2026-11-17）

结论：能够通过代码发现差异，而且目前仍有多处。此前验证“按钮能打开、项目没有被修改”，只能证明基本交互正常，不能证明已经复刻原版界面。

对照基准为仓库内锁定的 LDtk 1.5.3：`.reference/ldtk`，结合用户截图核对模板、Haxe 事件绑定和 SCSS。以下剩余项均来自静态代码对照，尚未作为本轮修复完成项。

## 本轮修复：Application settings

原版 [editAppSettings.html](/Users/bugs/Projects/LevelEditor/.reference/ldtk/app/assets/tpl/editAppSettings.html:1) 和 [EditAppSettings.hx](/Users/bugs/Projects/LevelEditor/.reference/ldtk/src/electron.renderer/ui/modal/dialog/EditAppSettings.hx:1) 定义四分组双栏弹窗。旧实现调用通用 `form()`，采用单列字段表，控件顺序、类型、选项文字、范围及日志区域均不相同。

已改为 [application_settings.rs](/Users/bugs/Projects/LevelEditor/src/editor/application_settings.rs)：

- 使用原版 940px 内容宽度、380px 左栏、32px 栏间距、居中模态窗口、图标标题和底部关闭按钮；小窗口通过滚动访问内容。
- 恢复 General、Visuals、Controls、Advanced options 分组与顺序。应用设置专用的标签不换行，不影响其他表单的换行规则。
- UI 缩放使用 18 个百分比选项；单层强度使用 0–100% 的 11 个选项；相邻关卡距离为 Disabled / 64 / 96 / 128 pixels。
- 恢复字段外观、世界视图切换和导航布局的可读选项，不再直接显示 `FR_Outline`、`ZoomInAndOut` 等内部枚举名。
- 滚轮速度改为 25–300% 输入，存储为 0.25–3 的倍率；关闭面板前提交输入草稿。
- 关闭“重新打开上次项目”会清除保存的项目；开启时记录当前项目；全屏和缩放立即应用，GPU 偏好变化在关闭时提示重启。
- 恢复日志路径、查看和定位按钮，以及待更新版本提示。帮助键区分 GPU 与更新，修复了上游重复 `gpu` 键引起的文案串项。
- 保存失败时保留错误，避免关闭时被“设置已保存”覆盖；关闭或切换项目会清理设置会话；更新子窗口按 Escape 时仅关闭子窗口。

有意保留的移植行为：现有更新服务支持跨平台自动下载，安装仍需明确确认，因此 Auto update 的说明与原版 Windows 自动安装政策不同。已有非标准强度数值（例如 75%）保留到用户选择新选项，避免仅打开设置就隐式修改已有配置；原版会在打开表单时向上归入预设值。

## 分组标题悬停补充修复

已对照 `app.scss:1342–1402` 补齐共享分组标题：悬停为黄色标题与内容边框，折叠标题悬停时文字保持黑色，移出恢复原色，点击正常折叠／展开。世界、关卡、自定义字段及实体设置使用同一修复。展开时使用原版 CSS 指定的原生备用 `ZoomOut` 指针，折叠时使用 `Cell`；原版自定义 SVG 收起指针的图案尚未移植。

两项真实 egui 回归先失败后通过；完整工作区通过（应用 354 passed / 6 ignored），原生测试程序 358 passed / 6 ignored，严格 Clippy 通过。debug Metal 捕获验证展开／折叠／移出三场景，记录指针 `ZoomOut` / `Cell` / `Default`，文档保持未修改，全部 surface 错误为 0，三张截图均已检查。证据：`.tools/collapser-hover-native/results.json` 与同目录 PNG。

本次按用户指定的 `just run` 调试流程重编译 debug 程序；正在运行的旧进程需要重启才能加载修改。

## Cmd+W 与设置样式补充修复

`Cmd+W` 使用关闭工程的 `Action::Close`，返回 Home；退出应用为 `Cmd+Q`。原生快速按键复现出另一个问题：读取整帧结束时的修饰键会漏掉同帧已经释放的 Command。现改为读取每个按键事件携带的修饰键。新增真实 App 回归覆盖干净工程、未保存工程和 Home，确认不会发出关闭窗口命令，未保存工程继续走保存确认。

设置齿轮已核对为原版 `assets/icons/settings.svg`，与锁定参考的文件 SHA-256 完全相同；编辑器工具栏、Home 和设置标题都复用该资源。另按原版 SCSS 补齐大写窗口标题、黄色大写分组标题、字体、上下间距和横线。样式回归与快捷键回归均先失败后通过。

本机 debug 应用通过操作系统鼠标打开设置，实际显示原版齿轮及新标题样式；随后通过操作系统 `Cmd+W` 关闭工程，看到同一窗口返回 Home，应用继续运行。未修改示例文档。本轮工作区应用测试 356 passed / 6 ignored，原生测试程序 360 passed / 6 ignored，严格 Clippy 与格式检查通过。日志位于 `.tools/cmdw-native/{key-red,key-green,style-red,style-green,workspace,example,clippy}.log`。本轮未重建 Windows/Linux 发布包。

## 保存确认、备份提醒、切层提示与应用名称

已对照原版 `UnsavedChanges.hx`、`Editor.hx:113 / 1407`、`Notification.hx:111` 和对应 SCSS 修复：

- 离开未保存工程时显示无标题的原版问句，下方横排 `YES / NO / CANCEL`；前两项橙色，取消为灰色，保留异步保存后继续、放弃和取消流程。
- 恢复大工程打开时的 `ENABLE BACKUPS` 提醒与纵排双按钮；至少 8 个关卡、未开启备份、非备份文件、非内置示例、未设置 `IgnoreBackupSuggest` 才会显示。两个选项分别开启备份或保存忽略标志，打开弹窗本身不改文档。
- F1–F10 或图层列表切换到不同图层时，在画布上方显示原版类型图标和图层名称，1.2 秒淡出；重复选择当前图层不重复提示，关闭／切换工程清理提示。
- 窗口标题、应用菜单、设置中的更新窗口及 macOS 本地应用包统一显示 `LevelEditor`。本地打包输出为 `dist/LevelEditor.app`。更新 ZIP 内部继续使用旧客户端要求的根目录名，避免改名导致已安装客户端拒绝更新；内部产品标识与用户配置目录保持兼容。

三项回归均先失败后通过，另有备份边界回归及 ARM/Intel 更新包兼容性失败复现与修复。debug Metal 捕获的保存确认和备份提醒已人工检查，全部 surface 错误计数为 0：[保存确认截图](/Users/bugs/Projects/LevelEditor/.tools/project-dialogs-native/close.png)、[备份提醒截图](/Users/bugs/Projects/LevelEditor/.tools/project-dialogs-native/backup.png)。操作系统原生 F2 按键实测看到 `Wall_tops` 图标和淡出提示，同一窗口与应用菜单均显示 `LevelEditor`。

最终完整工作区通过（应用 360 passed / 6 ignored），原生测试程序 364 passed / 6 ignored，7 项 Python 打包／命令集成测试、严格 Clippy、格式和差异检查通过，Debug 应用与原生测试程序均已重编译。证据日志为 `.tools/project-dialogs-{red,green,workspace,example,clippy,python,package-red,package-green}.log`，原生捕获日志为 `.tools/project-dialogs-native/{close,backup}.log`。

## Home 与快捷键提示补充修复

根据用户后续对比图，对照 `Home.hx`、`pages/home.html`、`app.scss:812 / 915 / 5850–6270` 修复 Home：

- 示例关闭按钮使用原版尺寸、零内边距和绿色边框，不再被通用按钮内边距撑大；顶排按钮按照文字、图标与间距计算宽度，白色标签与下拉符号整体居中。
- 删除搜索框与最近列表之间额外添加的 32px 空白；示例区标题、卡片间距、滚动条颜色及已打开示例的底色和文字透明度按原版恢复，并补充缩略图圆角与内阴影。
- 最近项目恢复普通粗体字体和路径次行；最近文件夹按路径分段绘制，斜线、普通目录和末级目录使用各自的字体和颜色，末级目录加粗。缺失路径的各段及斜线统一显示删除线。
- 最近列表的菜单按钮随内容行拉伸，独立处理点击；真实回归和 Debug Metal 注入点击确认，点击按钮下半部仍打开菜单，不会打开工程。

工具栏、画布选项和侧栏操作提示对照 `Tip.hx`、`JsTools.createKey`、`EditorTypes.hx` 与 `.key` CSS，改为绿色浮层、独立按键图标及平台符号。World 提示为 `~`；实际 Backtick、Shift W 等原版别名已核对，保留现有正确绑定。`assets/img/key.png` 并非原版 CSS 键帽背景，直接复用会出现像素黑边；最终使用 [keycap.svg](/Users/bugs/Projects/LevelEditor/assets/img/keycap.svg) 将原版 CSS 的米色、0.55 透明度、底边和圆角制作成九宫格背景，与符号分开绘制。实际纹理像素回归验证了颜色、透明度和线性采样。

五项 Home 回归、两项真实悬停／侧栏键帽回归和一项键帽纹理回归均经过失败复现与修复。完整工作区通过（应用 368 passed / 6 ignored），原生测试程序 372 passed / 6 ignored，严格 Clippy、格式及差异检查通过。Home、工具栏悬停和最近项目菜单三场景均通过 Debug Metal 捕获并检查，所有 surface 错误为 0，文档未修改：[Home](/Users/bugs/Projects/LevelEditor/.tools/home-parity-native/home.png)、[World 快捷键提示](/Users/bugs/Projects/LevelEditor/.tools/home-parity-native/world-tooltip.png)、[最近项目菜单](/Users/bugs/Projects/LevelEditor/.tools/home-parity-native/home-context.png)。这些输入由原生测试程序注入，本轮未将其计作操作系统输入分发验收。

该轮未重做 Help 页的快捷键列表或所有上下文菜单内容；浮层阴影使用原生绘制，缩略图内阴影为原生近似。其余面板差异继续按以下清单跟踪。

## 剩余差异清单

| 面板 | 已确认的差异 | 原版与当前代码依据 |
| --- | --- | --- |
| 图层 | 仍是通用字段表，缺少原版 General / Display / 类型专属分组；列表颜色、异步渲染、Kill tiles、Bake auto-layer 控件缺失；筛选标签仅给 Entities 图层；Biome 被放在 Rules 中。 | [当前字段表](/Users/bugs/Projects/LevelEditor/src/editor/panels.rs:673)、[原版模板](/Users/bugs/Projects/LevelEditor/.reference/ldtk/app/assets/tpl/editLayerDefs.html:20)、[原版绑定](/Users/bugs/Projects/LevelEditor/.reference/ldtk/src/electron.renderer/ui/modal/panel/EditLayerDefs.hx:276) |
| 图层数值行为 | 不透明度显示原始 0–1 小数，原版是百分比且基本透明度下限 10%；视差目前允许 −10–10，原版为 −1–1 的百分比，并有 X/Y 同步、缩放时锁定 Y 的交互。核心补丁服务没有补上这些 UI 规则。 | [当前](/Users/bugs/Projects/LevelEditor/src/editor/panels.rs:702)、[原版](/Users/bugs/Projects/LevelEditor/.reference/ldtk/src/electron.renderer/ui/modal/panel/EditLayerDefs.hx:334)、[核心补丁](/Users/bugs/Projects/LevelEditor/crates/ldtk-core/src/workflow.rs:501) |
| 项目 | 缺少实际更改文件扩展名的 Use .ldtk extension 控件；命名规则用普通文本代替变量块编辑器，默认关卡命名缺少 Reset；导出配置缺少对应输出位置的定位链接。 | [当前](/Users/bugs/Projects/LevelEditor/src/editor/project_panel.rs:232)、[原版扩展名操作](/Users/bugs/Projects/LevelEditor/.reference/ldtk/src/electron.renderer/ui/modal/panel/EditProject.hx:136)、[原版命名编辑器](/Users/bugs/Projects/LevelEditor/.reference/ldtk/src/electron.renderer/ui/NamePatternEditor.hx:30) |
| 项目高级选项 | 当前公开八项，而原版可见列表只有五项及说明文字；只剩一个世界时，当前仍不能取消 MultiWorlds，原版允许。 | [当前](/Users/bugs/Projects/LevelEditor/src/editor/project_panel.rs:550)、[原版选项列表](/Users/bugs/Projects/LevelEditor/.reference/ldtk/src/electron.renderer/ui/modal/panel/EditProject.hx:6)、[原版多世界开关](/Users/bugs/Projects/LevelEditor/.reference/ldtk/src/electron.renderer/ui/modal/panel/EditProject.hx:495) |
| 世界／关卡 | 主要分组已恢复，但选择的关卡不在当前激活深度时，属性和自定义字段没有像原版一样禁用并加条纹；背景图片仍缺少原版完整的记忆文件菜单。 | [当前关卡表单](/Users/bugs/Projects/LevelEditor/src/editor/world_level_panel.rs:331)、[原版禁用逻辑](/Users/bugs/Projects/LevelEditor/.reference/ldtk/src/electron.renderer/ui/LevelInstanceForm.hx:131)、[原版样式](/Users/bugs/Projects/LevelEditor/.reference/ldtk/app/assets/css/app.scss:8418) |
| 本地枚举 | 缺少 Tags 编辑及值排序；当前使用共享图集和 Use tile / Clear tile，原版每个值带图标预览与选择器。 | [当前](/Users/bugs/Projects/LevelEditor/src/editor/panels.rs:1690)、[原版模板](/Users/bugs/Projects/LevelEditor/.reference/ldtk/app/assets/tpl/editEnumDefs.html:48)、[原版排序](/Users/bugs/Projects/LevelEditor/.reference/ldtk/src/electron.renderer/ui/modal/panel/EditEnumDefs.hx:545) |
| 外部枚举 | 独立表单仍简化为路径、同步／删除和值名称，缺少完整图标／颜色表单、源文件定位和源文件缺失／修改提示。 | [当前](/Users/bugs/Projects/LevelEditor/src/editor/panel_layout.rs:1097)、[原版表单](/Users/bugs/Projects/LevelEditor/.reference/ldtk/src/electron.renderer/ui/modal/panel/EditEnumDefs.hx:385)、[原版警告](/Users/bugs/Projects/LevelEditor/.reference/ldtk/src/electron.renderer/ui/modal/panel/EditEnumDefs.hx:275) |
| 图集 | Tag source enum 仍是内部 UID 整数输入，原版是按标签分组的枚举选择器，并禁用不支持的外部枚举；Tiles layout 拆成了散列数字项，还额外显示了重映射策略表。 | [当前](/Users/bugs/Projects/LevelEditor/src/editor/panels.rs:1863)、[原版模板](/Users/bugs/Projects/LevelEditor/.reference/ldtk/app/assets/tpl/editTilesetDefs.html:37)、[原版选择器](/Users/bugs/Projects/LevelEditor/.reference/ldtk/src/electron.renderer/ui/modal/panel/EditTilesetDefs.hx:240) |
| 实体 | 已有专用表单，但缺少大尺寸实时预览和标签重命名／传播操作；九宫格边框独立编辑且允许 0，原版有继承、最小 1 和相同边框联动规则。 | [当前](/Users/bugs/Projects/LevelEditor/src/editor/panel_layout.rs:1143)、[原版预览](/Users/bugs/Projects/LevelEditor/.reference/ldtk/app/assets/tpl/editEntityDefs.html:20)、[原版标签传播](/Users/bugs/Projects/LevelEditor/.reference/ldtk/src/electron.renderer/ui/modal/panel/EditEntityDefs.hx:194)、[原版九宫格](/Users/bugs/Projects/LevelEditor/.reference/ldtk/src/electron.renderer/ui/modal/panel/EditEntityDefs.hx:414) |
| 帮助 | 文档链接／视频仍是普通文字而非原版按钮与缩略图卡片；快捷键仍是硬编码文本而非绑定解析和键鼠图标；打开 Release notes 后未关闭 Help。 | [当前](/Users/bugs/Projects/LevelEditor/src/editor/help.rs:53)、[原版模板](/Users/bugs/Projects/LevelEditor/.reference/ldtk/app/assets/tpl/help.html:4)、[原版绑定](/Users/bugs/Projects/LevelEditor/.reference/ldtk/src/electron.renderer/ui/modal/panel/Help.hx:18) |

优先处理图层、枚举、图集的通用表单与数值行为，再补项目／实体／世界的缺失交互，最后核对帮助页表现。面板能打开或字段名存在，不作为这些项目的完成证据。

## 验证记录

本轮设置页的旧实现有四项真实 egui/App 回归失败；复核发现的帮助、保存错误和会话清理问题另有三项失败复现。日志位于 `.tools/app-settings-{red,review-red}.log`。最终工作区测试通过（应用 352 passed / 6 ignored），原生测试程序 356 passed / 6 ignored，严格 Clippy 和格式检查通过。另有更新子窗口 Escape 隔离的失败复现与通过验证，见 `.tools/app-settings-nested-red.log`。原生捕获和构建结果另见 `.tools/app-settings-*`。

其他清单项目是静态排查结果，未在本轮修复或标记验收通过。上轮九个面板的原生“打开测试”不等同于这些界面已与原版一致。

原生实测：编辑器和 Home 两个入口均通过实际 Metal 窗口点击，捕获到 Settings、无焦点遮罩、文档未修改，全部 surface 错误计数为 0。两张最终截图均已人工检查：[编辑器入口截图](/Users/bugs/Projects/LevelEditor/.tools/app-settings-native/editor-settings.png)、[Home 入口截图](/Users/bugs/Projects/LevelEditor/.tools/app-settings-native/home-settings.png)。输入通过测试程序的原生 input hook 注入，未将此算作操作系统鼠标分发验收。

Mac/Windows/Linux 的 debug/release 便携目录均重新构建，并核验各自的 253 个资源文件和许可证；Mac 两种模式及应用包启动检查通过，应用包通过 deep/strict ad-hoc 签名校验。Windows/Linux 本轮仅交叉构建与产物核验，没有目标系统桌面实测。最新产物哈希见 `build-command.json`，原始证据见 `.tools/app-settings-native/`。
