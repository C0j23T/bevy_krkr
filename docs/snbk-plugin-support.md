# snbk 插件支持与 krkrsdl3 对照报告

核对日期：2026-11-17。

本报告整理 snbk 经引擎 Storage/xp3filter 解包、应用资源覆盖和启动 patch 后的插件需求，以及 `~/Workspace/krkrsdl3` 对相同缺口的支持程度。

## 范围与结论

| 对象 | 检查版本与验证范围 |
| --- | --- |
| bevy_krkr | 根仓库 `fb2d14d` 及工作区 scriptsEx 补齐；Kirikiroid2 `50d0ff3` 及工作区 scriptsEx 补齐；源码、既有专项证据、本次解包/场景解析/能力探针和启动专项 |
| krkrsdl3 | 本地 `~/Workspace/krkrsdl3`，`88d12cef068a1c8e1b90be3f4f948f567d6dc4bc`；只检查源码和插件注册，未构建、未运行 snbk |
| 游戏样本 | 本地 `~/Workspace/bevy_krkr/test_games/snbk`；资源和官方 DLL 不入仓 |

本次结论：

- bevy_krkr 已能读取全部游戏归档和 patch 场景，但实际启动仍在 `override.tjs → callAfterInit` 失败，尚未到标题和交互。
- EmotePlayer 和 LayerExDraw 均为部分支持。绘图或动画专项通过不能替代完整游戏调用链验收。
- krkrsdl3 的音频原插件名称有正式注册，对话框有实际 SDL 交互，这些仍可作为迁移参考。
- krkrsdl3 的 Emote 同样存在 `assign/setColor/pass/skipToSync` 缺口，音频增益回调也未找到实现。它的 LayerExDraw 记录保存实际写 PNG，不是完整 EMF+ 参考实现。
- 下文按游戏引用功能分类，不能据此推导插件所有公开功能均兼容。明确缺失与“尚未验收”分别注明；条件分支不会自动计为默认流程必需项。

## 解包与 Patch

### 读取方法

本地游戏位于主 checkout，Codex worktree 未复制游戏和源码子模块。导出使用主 checkout 的现有引擎接口：

1. `krkr_engine_init` 初始化游戏项目及内置插件。
2. `Plugins.link('xp3filter.dll')` 确认过滤插件可链接。
3. XP3 索引或 7z 文件表只用于枚举名字；内容全部通过 `krkr_xp3_open/read_file` 的引擎归档/Storage 路径读取。
4. 每个归档分别导出，再生成 patch 优先的合并目录。
5. TJS 字节码按仓库 loader 的字符串表结构提取；文本流按 BOM、简单编码或压缩格式解码。字节码字符串表只能证明存在引用，需结合调用反汇编、配置和启动日志判断启用情况。
6. patch 场景通过本项目外部 `psbfile` 插件解析为根对象，借助 JSON 插件导出后做结构化统计；没有执行场景中的任意 eval 指令。

样本没有独立的 `xp3filter.tjs`，没有额外添加解密脚本。因此准确表述是“经过启用 xp3filter 的引擎读取路径”，不是声称每个文件都执行了自定义解密过滤。

### 归档结果

| 归档 | 成员数 |
| --- | ---: |
| `bgimage720.xp3` | 97 |
| `bgm.xp3` | 103 |
| `data.xp3` | 1,141 |
| `data720.xp3` | 212 |
| `evimage720.xp3` | 420 |
| `fgimage720.xp3` | 1,767 |
| `patch.xp3` | 340 |
| `video720.xp3` | 7 |
| `voice.xp3` | 44,113 |
| 合计 | 48,200 |

48,200 个成员全部导出成功。游戏的 `.xp3` 后缀同时用于 XP3 和 7z 容器，不能根据通用归档工具无法打开某个 XP3 来判断资源不存在。

按规范化完整路径合并，覆盖 154 项，得到 48,046 个归档文件。这是磁盘合并目录的数量，不等于引擎自动搜索路径下的最终激活数量。游戏还通过 `main/`、`system/`、`sysscn/` 等搜索路径按文件名覆盖资源，必须单独考虑该语义。

### 场景覆盖

| 来源 | 场景数 | 格式 |
| --- | ---: | --- |
| `data.xp3` | 95 | 普通 PSB |
| `patch.xp3` | 93 | 92 个 MDF、1 个普通 PSB |

patch 的 93 个场景均解析成功。按 Storage 文件名覆盖原包的 92 个场景，最终有 96 个不同文件名的 `.scn`；原包独有的三个为 `charvoice.scn`、`classlist.scn`、`scenelist.scn` 元数据。

patch 场景对象中发现的非空转场 `method`：`crossfade`、`universal`、`mosaic`、`turn`、`ripple`。递归统计包含状态快照，出现次数不能当作实际播放次数。本次未发现 `morphing`。

`mosaic/turn/ripple` 归属 `extrans.dll`。`extNagano.dll` 在 `sysscn/first.ks` 中按 `extNaganoEnabled` 加载，配置启用，但不能把上述三个方法归到它名下。

### 启动 Patch

同级 `patch.tjs` 已纳入盘点和合并目录，启动专项通过 `exec_startup` 实际执行它：

- 加载 `motionplayer.dll`。
- 设置 `Motion.Player.useD3D=0`。
- 设置 `-hdresomode=1080`。
- 调整 `Window.movieQualitySelectMenuItem`。

原游戏归档未被改写。实际启动按引擎正常行为写入 savedata 和日志；离线合并目录用于调查，不代表该目录项目已完整运行验收。

## EmotePlayer 现状

### 已实现与验证

`motionplayer.dll` 是 `emoteplayer.dll` 的内置别名。C++ 负责容器、资源管理、动画、物理和 TJS；Rust/Bevy 负责 GPU 曲面网格、蒙版及混合。正常绘制不逐帧执行 GPU→CPU→GPU 像素往返。

已有专项证据覆盖：

- 真实 `sd001.mtn` 经 ResourceManager、Player、Layer 绘制，平移影响目标像素。
- 四层 root/affine/Bezier/icon 曲面链、八种继承 mask，并与独立 C++ evaluator oracle 交叉核对。
- normal/multiply/additive/mode 21、mode 6 跳过、opacity/tint 和 alpha 蒙版。
- 时间轴、变量、位置/缩放/旋转插值、序列化恢复、主/差分时间轴形变。
- 物理、风力/外力、hit test、Player 状态隔离、资源重载与两轮引擎重启。

详见 [Emote 集成与专项说明](emote-integration.md)。

### 明确缺口

Kirikiroid2 检查点中的 `plugins/emoteplayer/emoteplayerclass.cpp`：

| 接口 | 当前行为 | 游戏引用证据 |
| --- | --- | --- |
| `assign` | 只打印 TODO | 游戏动画适配代码存在 assign 引用；不能仅凭字符串判定每处都是原生 Player.assign 调用 |
| `setColor` | 只打印 TODO | `system/AffineSourceMotion.tjs` 包含 Player 颜色设置调用 |
| `pass` | 只打印 TODO | 同一动画适配脚本的帧推进路径 |
| `skipToSync` | 更新 `clockPassed`，仍标 TODO | 同一动画适配脚本的跳过/同步路径 |
| `stop` | 设置停止状态，仍标 TODO | 同一适配脚本存在 stop 调用，完整语义需继续验收 |

因此 Emote 不能列为“游戏所用功能全部实现”。当前整游戏启动异常也尚未证明由这些接口中的某一项引起。

## LayerExDraw 现状

### 已实现与验证

Vello 已接入 Bevy 的同一设备、队列和纹理。Path、Matrix、clip、Appearance、图像仿射和 FreeType 字体轮廓走 GPU。

已有专项覆盖实色/线性渐变/PathGradient、53 种 Hatch、描边/虚线/inset/compound/箭头、图像裁剪与仿射、文字轮廓、内部 record 重放、PNG 保存和颜色区域扫描。

### 剩余边界

- EMF+ 已有编解码子集、外部 fixture 和独立 Wine 结构/播放验证，不能描述为完全没有实现。
- 外部 DrawImage/DrawString 记录、压缩或嵌套 metafile、部分 pen/brush/clip/page/compositing 组合仍不完整。
- record 纹理快照误入资源池问题已有局部修复和回归草稿，但该回归没有取得验收结果。
- 字体、文字度量和 GDI+ 参考像素一致性、资源释放/重启、Android 和性能未完整验收。
- 单独存在的 `LayerExDrawFont.inc` 不代表它已被当前 `LayerExDraw.cpp` 接入并验证。

snbk 的主要引用为图像绘制、流程图的线/Bezier/矩阵/箭头，以及片尾文字测量和 `drawPathString`。主要接口已存在，但没有实际流程图/片尾画面验收，下面保守列为部分覆盖；未发现游戏要求完整 EMF+，不能把所有 EMF+ 缺口当作 snbk 必需阻塞。

当前边界见 [GPU 后端与绘图验证](layer-ex-draw-vello.md)、[EMF+ 实现范围](layer-ex-draw-record-task.md) 和 [检查点验证状态](work-snapshot.md)。旧迁移表中的“多层曲面待验证”“EMF+ 待实现”等概括已落后于这些证据。

## snbk 插件分类

### 用到的功能已经实现

本节表示对应调用有实现或已有相关专项证据，不表示插件整体兼容，也不表示该功能已在 snbk 完整交互中验收。

| 插件 | 本游戏引用的功能与验证边界 |
| --- | --- |
| `psbfile.dll` | PSB v2/v3、MDF 场景、PIMG、`PSBFile.root`、`psb://`；本次 93 个 patch 场景解析成功 |
| `textrender.dll` | TextRenderBase 排版、ruby、图像尺寸/文字绘制回调、链接、滚动；实际绘字由游戏 TJS/Layer 完成 |
| `psd.dll` / `psdfile.dll` | psdlayer 的 PSD 备用路径、图层信息和像素读取；主要打包图像为 PIMG，需要部署外部 `.so` |
| `csvParser.dll` | UI/设置 CSV 解析、逐行回调 |
| `fstat.dll` | 文件大小、存档复制/移动/删除、目录操作 |
| `saveStruct.dll` | Array/Dictionary 结构保存、save2、toStructString |
| `addFont.dll` | System.addFont、Storage 字体注册；不等于完整 LayerExDraw 字体语义验收 |
| `layerExImage.dll`、`layerExBTOA.dll`、`shrinkCopy.dll` | 图像加工、蓝通道转 alpha、缩小复制，也由 PackinOne 装载；独立 shrinkCopy 链接在 custom.tjs 中被注释 |
| `layerExRaster.dll` | copyRaster 波形图像处理；场景对象包含 raster 参数 |
| `layerExSave.dll` | 图层 PNG/TLG5 导出及相关保存操作，按需链接 |
| `getSample.dll` | WaveSoundBuffer.sampleValue、采样设置和口型采样接口；实际游戏音频/口型同步未完整验收 |
| `KAGParserEx.dll` | 所需 KAGParser 能力内建；兼容加载接受 DLL 名称，不是加载 Windows DLL |
| `extrans.dll` | 本次发现的 mosaic、turn、ripple 已实现并有相关专项证据 |
| `extNagano.dll` | 配置启用且加载路径存在，已有多种转场实现；本次未确认场景使用其专属方法。morphing 已知末帧误差仍在，但未在此次场景统计中发现 |
| `krmovie.dll` 对应常规视频路径 | Movie.tjs 基于 VideoOverlay，FFmpeg 宿主提供播放；包内 7 个 WMV，未做这 7 个视频的逐项播放验收，也不代表完整 Windows mixer/MFEVR 兼容 |
| `scriptsEx.dll` | 已按 krkrsdl3 补齐 clone/foreach/getObjectKeys/getObjectCount/getObjectContext/isNullContext/equalStruct/equalStructNumericLoose/getMD5HashString/propGet/propSet/safeEvalStorage/stringFuzzySearch/rehash；专项回归通过; |
| `PackinOne.dll` | 图像、字体、文件、结构保存子模块可装载；已改为组合 `scriptsEx.dll` |

### 用到的功能部分实现

| 插件 | 已有能力与缺口 |
| --- | --- |
| `motionplayer.dll` / `emoteplayer.dll` | 动画/GPU 绘制已有专项验证；setColor、pass、skipToSync 等游戏引用接口仍为空操作或不完整，assign 也存在空实现 |
| `layerExDraw.dll` | 主要绘图接口已有实现；字体、文字度量、实际流程图/片尾画面尚未验收，属于保守分类 |
| `wuvorbis.dll` | Vorbis 解码内建，DLL 链接被兼容忽略；游戏启用 SOUNDGAIN，所查询的 setGainQueryCallback 缺失 |
| `wuopus.dll` | 默认构建有 Opus 解码器；原 DLL 名称链接失败，Opus 增益回调也未找到实现；能解码不等于完整原插件兼容 |
| `windowEx.dll` | 窗口/屏幕信息、系统度量和光标等已有宿主接入；单显示器近似及部分 Win32 空操作，窗口选项/事件未完整验收 |
| `win32dialog.dll` | 控件模型和消息/输入协议已有；宿主交互 UI 仍只记录请求并返回取消，不能完成原生设置/确认交互 |

### 完全没实现

本次没有确认默认流程存在“必须加载且完全没有替代实现”的插件。以下为实际附带脚本中存在引用、按条件启用的未实现扩展；部分实现本身仍足以阻止完整兼容。

| 插件 | 功能与条件 |
| --- | --- |
| `win32ole.dll` | SAPI/MSSP 文本语音和 COM；包含 speech 脚本，TextSpeechEnabled 默认关闭 |
| `kztouch.dll` | Windows touch 辅助接口；gesture 按引擎/平台判断加载，本次启动日志 enableTouch 为 0 |
| `k2compat.dll`、`menu.dll` | K2/KRKRZ 兼容、窗口/菜单延迟适配分支；未找到对应插件实现，不等于核心 MenuItem 或全部菜单功能不存在 |

### 未计为默认必需

- AlphaMovie 只见框架备用路径，没有 AMV 资源；该插件存在外部实现和合成样本验证，不能因为游戏目录没有部署 `.so` 就说源码未实现。
- motionplayer_nod3d 是备用加载选项；当前 patch 选择 motionplayer 并设置 useD3D=0。
- D3D drawdevice 默认关闭；Bishamon 加载被注释。
- Flash/gfxEffect、Squirrel、外部 speech process/shellExecute、debugutil 的 base64/minizip/clipboard 等存在通用脚本引用，但未确认为默认游戏必需。
- 随包 krkrsteam、yuzuex、multiimage、kagexopt 等 DLL 未找到明确的当前脚本必需加载证据。DLL 文件存在、注释或字节码字符串出现不能单独证明启用，也不能证明这些接口已兼容。

## krkrsdl3 对照

以下均为 `88d12ce` 的源码事实，不是运行验收。该 checkout 的构建入口在外部构建仓库，本次未核对最终产物实际包含哪些可选模块。

### EmotePlayer / MotionPlayer

`plugins/motionPlayer.cpp` 注册 motionplayer.dll 并加载 emoteplayer.dll。动画、物理、资源管理和渲染后端已有实现，但与本项目存在相同缺口：

| 接口 | krkrsdl3 源码位置与行为 |
| --- | --- |
| assign | `plugins/emoteplayer/emoteplayerclass.cpp:1038`，只打印 TODO |
| setColor | 同文件 `:1064`，只打印 TODO |
| skipToSync | 同文件 `:1145`，只更新部分时钟状态并打印 TODO |
| pass | 同文件 `:1153`，只打印 TODO |
| stop | 同文件 `:1157`，设置状态并保留 TODO |

因此不能直接从该参考仓库获得上述功能的完整实现，也不能因本项目最初参考它迁移而推断两边都完整兼容。

### LayerExDraw

`plugins/LayerExDraw/LayerExDraw.cpp` 使用 PlutoVG 软件绘制。`reset` 将可写 Layer 像素 buffer 包装为 PlutoVG surface/canvas；不是本项目的同设备 Vello GPU 操作路径。

已有 Path/Matrix、形状、画笔、图像、字体加载、drawString/drawPathString、文字测量及内部记录结构。但有明确简化和空实现：

| 能力 | 源码行为 |
| --- | --- |
| 路径范围 | getPathExtents 在 `:1436` 返回空 RectF；drawPath 等返回范围不能假设完整 |
| PathGradient | `:616` 将边界转为径向渐变，外围颜色取首项，不是完整任意边界 GDI+ PathGradient |
| 文字扩展 | drawPathString2、measureString2、measureStringInternal2、getGlyphOutline/getTextOutline 在 `:1846` 起仍为空或 TODO |
| 私有字体 | `:72` 有 Storage/字体流到 PlutoVG font face 的实际加载实现，但没有本次字体兼容验收 |
| saveRecord | `:2066` 调用 plutovg_surface_write_to_png，保存 PNG，而非 EMF+ |
| loadRecord | `:2080` 直接返回 false |

它可以参考对象、参数和部分文字/图像适配，但不能作为完整 EMF+、GDI+ 语义或 GPU 架构的完成标准。

### Vorbis / Opus

`plugins/wuvorbis.cpp` 和 `plugins/wuopus.cpp` 都有 Storage 输入流、解码器 creator 和正式 DLL 模块注册；`plugins/TVPPlugin.cpp:199` 的内置加载流程包含两者。

这能参考并补齐本项目原 DLL 名称的加载行为。对插件与核心源码搜索，未找到游戏要求的 setGainQueryCallback 增益查询回调实现；不能把解码器存在视为 SOUNDGAIN 扩展已支持。

### WindowEx

`plugins/windowEx.cpp` 已注册接口，但仍为部分兼容：

- minimize/maximize/showRestore 在 `:424` 起直接返回成功。
- isMaximized/isMinimized 固定 false，部分移动/缩放/系统菜单行为为空。
- getDisplayMonitors/getMonitorInfo 以桌面宽高构造一个屏幕。
- 部分光标和 DPI 接口为空或返回常量。

这里同样不能根据完整的接口注册表判为完整窗口功能；没有现成实现能一次补齐本项目的 Win32 行为。

### Win32Dialog

`plugins/win32dialog.cpp` 提供控件模型、模板数据、消息处理和结果回调，并通过宿主消息框/输入框完成实际交互：

- `runForm` 在 `:505` 起把编辑框、列表、复选/单选、按钮等转换成逐项提示。
- `environ/sdl3/sdl3_dialog.cpp:45` 使用 SDL_ShowMessageBox。
- `environ/sdl3/sdl3_input.cpp` 使用 SDL 窗口和文本输入实现 TVPInputQuery，TVPShowSimpleInputBox 调用它。
- WASM 分支使用浏览器 prompt/confirm/alert。

相比本项目只记录请求并返回取消，它的宿主交互可作为实现参考。但原生资源加载是 no-op，自绘/GDI、property sheet 等仍为空；modeless 只维护状态，不能当作真正的常驻原生表单。

### 未找到的插件

| 插件 | krkrsdl3 检查结果 |
| --- | --- |
| win32ole.dll | 未找到插件源码或模块注册，没有现成 SAPI/COM 实现 |
| kztouch.dll | 未找到插件；SDL 有手指事件和 TJS touch 回调，但 `core/main/TVPWindow.h:302` 的 enableTouch、触点数量/坐标/速度查询仍为空或固定值 |
| k2compat.dll | 未找到插件源码或注册，不能把游戏自带 k2compat TJS 适配脚本等同于原生模块 |
| menu.dll | 未找到插件；`core/script/tjsNativeMenuItem.cpp` 有核心对象/事件模型，部分原生菜单句柄和 UI 行为为空，不等于独立 menu.dll |

### 可复用程度

| 工作 | 参考价值 |
| --- | --- |
| Vorbis/Opus DLL 名称加载 | 有正式注册与加载参考；增益扩展需要另行实现 |
| Win32Dialog 的实际交互 | 有 SDL 输入/消息框流程参考；Bevy 宿主仍需实现自己的 UI 能力 |
| Emote 的空操作 | 参考仓库也缺，不能直接搬运解决 |
| LayerExDraw 对象和字体适配 | 可参考；软件绘制及 PNG 冒充 record 保存的行为不应照搬为本项目 EMF+ 契约 |
| WindowEx、COM、原生兼容/菜单/触控插件 | 不存在完整可直接迁移的覆盖面，需要按实际游戏调用继续实现 |

## 验证记录与证据

### 本次执行

| 检查 | 结果 |
| --- | --- |
| 引擎归档读取专项 | 48,200/48,200 成员成功 |
| patch PSB/MDF 解析专项 | 93/93 场景成功 |
| 能力探针 | wuopus.dll 链接失败 |
| snbk startup_and_scene | 失败，override.tjs 匿名回调经 callAfterInit 报 Not a function or invalid method/property type，尚未到标题/交互；未确认单一插件根因 |
| krkrsdl3 | 只做源码检查，未构建和运行 |

能力探针的摘要如下；字符串是接口存在性或链接结果，不是功能验收：

```json
{
  "clone": "Object",
  "foreach": "Object",
  "equalStruct": "Object",
  "gainCallback": "undefined",
  "OPUS": "undefined",
  "scriptsEx": "linked",
  "wuopus": "Cannot load Plugin wuopus.dll"
}
```

该探针没有覆盖每个可能的 Opus 专属回调名称；“OPUS undefined”也不能单独证明没有 Opus 解码器。增益扩展缺失的判断结合了游戏回调查询代码和源码搜索。

### 本地产物

调查输出位于 `/tmp/snbk-plugin-audit/`，可能随临时目录清理消失，不随本报告入仓：

| 路径 | 内容 |
| --- | --- |
| archives/ | 每个归档的独立原始导出 |
| patched/ | patch 优先的完整路径合并目录，另含同级 patch.tjs |
| text/ | 解码文本及 TJS 字节码字符串表，不是完整反编译源码 |
| scenes/ | patch 场景根对象 JSON |
| members.tsv | 归档成员清单 |
| origins.json、overrides.json | 完整路径来源和覆盖清单，不包含全部自动搜索路径激活规则 |
| plugins.json | DLL 字符串引用清单，包含条件分支和注释，不能直接作必需插件列表 |
| scene-summary.txt | 场景格式、转场方法和元数据统计 |
| capability-probe.json | 原始能力探针结果 |
| extract_inventory.rs | 临时归档读取/探针入口，保留在调查目录 |

日志分别为 `/tmp/snbk-plugin-extraction.log`、`/tmp/snbk-plugin-scenes.log`、`/tmp/snbk-current/startup.log`。独立解包/解析测试没有初始化图形宿主，退出时的 `Bevy renderer is not initialized` 提示不应被解读为绘图验收；这两项只验证 Storage 和容器读取。

### 后续验证优先级
1. 对游戏实际调用补齐 Emote 颜色、跳过/同步和 pass 等行为，按动画子系统验证。
2. 补齐音频插件名称兼容与增益回调，并检查真实 Vorbis/Opus 和口型同步。
3. 验收 LayerExDraw 的流程图箭头、字体/度量与片尾文字，以及记录纹理生命周期回归。
4. 接入宿主对话交互，再分批验收标题、正文、选择、存读档、回想、流程图和视频。

相关说明：[snbk 实机验收](snbk-acceptance.md)、[插件迁移清单](plugin-migration.md)、[外部插件部署](external-plugins.md)。
