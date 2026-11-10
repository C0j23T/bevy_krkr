# NVL Maker / KAGeX 适配

## 目标与基线

使 NVL Maker 导出的同一份游戏脚本和资源能够在 Kirikiroid2 与 bevy_krkr 中运行，核心工作是补齐 LayerEx 插件接口及其图形实现。基线为 `editor/THE_NVL_Maker_ver3.87_sim/project/template/Data`、随附皮肤和实际使用的编辑器功能。

基本运行包括启动、标题菜单、对话、分支选择、历史记录、自动/跳过、BGM/SE/语音、普通转场、存档、读档、退出与再次启动。使用动态图层的工程还必须完成 Layer 视频播放。上游源码可取得、模块能注册或启动脚本返回成功，都不能单独证明游戏可用。

已完成项列于下方；立即实现项按顺序推进，其余能力按实际工程调用补齐。原版 Kirikiroid2 的兼容结论必须以选定版本的实机运行结果为准。

## 当前基础

TJS2、KAGParser、Storage、Window/Layer、字体和音频由 Kirikiroid2 核心提供，Bevy 负责图形与输入等宿主能力。

构建清单见 [`codegen/src/port.rs`](../codegen/src/port.rs)，插件生成逻辑见 [`crates/krkr_sys/build.rs`](../crates/krkr_sys/build.rs)，加载入口见 [`BevyPlugins.cpp`](../Kirikiroid2/src/core/environ/bevy/BevyPlugins.cpp) 与 [`InternalPlugins.cpp`](../Kirikiroid2/src/plugins/InternalPlugins.cpp)。

## 已完成

- `layerExImage.dll`
- `layerExMovie.dll`
- `KAGParserEx.dll`

## 立即实现

按以下顺序推进。三项均有固定参考源码，Perspective 与 Raster 可复用现有 GPU 命令和纹理，Save 补齐显式图片导出能力。

| 插件 | 实现范围 | 验收 |
| --- | --- | --- |
| `perspective.dll` | 接入现有 [`layerExPerspective.cpp`](../Kirikiroid2/src/plugins/layerExPerspective.cpp)，复用 `OperatePerspective` 和 Bevy 投影映射 | 四点透视复制；覆盖裁剪、采样、退化四边形、Alpha 和同图复制 |
| `layerExRaster.dll` | 按 [`manual.tjs`](../layerExRaster/manual.tjs) 实现 `copyRaster(src,maxh,lines,cycle,time)`，用 GPU 逐行位移，保持未覆盖区域 | 与上游算法比较位移方向、周期、时间、边界、裁剪和 Alpha；普通效果不逐帧回读 |
| `layerExSave.dll` | 按固定上游接口接入 PNG/TLG 保存、裁剪、差异和通道处理，复用现有编码与 Storage | 保存时才读回 GPU 快照；核对输出文件、透明度、裁剪及中文路径；异步任务持有快照，进度与完成事件回到 TJS 主线程 |

## 待实现

下表以未使用相应功能的基础工程为前提。若游戏在启动、菜单或主流程中调用这些 API，对该工程立即升级为必需项；不提供吞异常或统一降级转场来掩盖缺失行为。

| 插件 | 功能 | 兼容方向 |
| --- | --- | --- |
| `layerExText.dll` / `layerExDraw.dll` | 字体外观、渐变画刷、路径和文字绘制 | Text 已并入 [Draw 上游](https://github.com/wamsoft/layerExDraw)；以 NVL 旧接口为契约补适配，复用现有字体与 glyph 路径，Bevy 执行填充/合成，必要时 CPU 生成轮廓或覆盖率；仅添加模块别名不足以兼容旧 API |
| `extrans.dll` | wave、mosaic、turn、rotatezoom、rotatevanish、rotateswap、ripple | 按上游 provider 生命周期和参数注册，对每个效果增加 Bevy GPU 路径；已有 crossfade/universal 单独维持 |
| `extNagano.dll` | blurfade、book、morphing 等附加转场 | 使用已接入的恢复实现作为参考，逐效果与 NVL DLL 比较；上游说明部分算法近似，不能直接承诺像素一致 |
| `json.dll`、`lineParser.dll`、`expat.dll` | JSON、文本行、XML 数据接口 | 按项目实际调用补 TJS 对象、数据转换和编码行为；Rust 库只通过完整解析操作接入 |
| `agg.dll` | 矢量绘制扩展 | 确认脚本 API 后接现有 Bevy 图形路径，避免每帧 CPU 图像回传 |
| `fstat.dll`、`minizip.dll` | 文件信息、压缩包脚本操作 | 与现有 Storage/archive 能力对接；核对写入、枚举和路径语义 |
| `clipboardEx.dll` | 剪贴板扩展 | 由宿主实现文本和必要的数据格式 |
| `wump3.dll`、`wumsadp.dll` | MP3、MS ADPCM 音频 | 在 WaveSoundBuffer 解码路径补齐并验证 seek/loop；仅启用视频 FFmpeg 不代表音频解码已接入 |
| `krflash.dll` | Flash 内容与交互 | 单独评估内容转换或播放器；普通 MP4 播放不等价 |
| `win32ole.dll`、`wsh.dll`、`messenger.dll` | Windows 自动化、系统脚本及进程消息 | 按实际用途提供宿主能力或游戏内替代；不以空实现宣称兼容 |
| `tools/msgreceiver.dll` | 编辑器消息与调试联动 | 普通导出工程不依赖；需要联调时适配宿主消息队列 |
| `menu.dll` 的原生 UI、系统输入框/文件选择/外链 | 平台交互 | 基础流程使用游戏自己的按钮和对话框；需系统功能时通过宿主实现，保留脚本对象状态 |

`nvl/macro.ks:13–14` 会加载 extrans/extNagano，但默认演出主要使用 crossfade/universal。模块加载语句本身不证明效果已执行；应扫描实际 `trans method` 和动态参数。`saveStruct` 的核心存档也不依赖 LayerExSave，不能把两者合并判断。

## 源码与分支

项目根目录仅保留待实现插件的固定参考 submodule。Image/Movie 的运行时实现位于 Kirikiroid2 的 `src/core/environ/bevy/plugins`，解析扩展位于核心 KAGParser；参考仓库不参与运行时构建。

版权与来源记录见 [`LICENSE-THIRD-PARTY.md`](../LICENSE-THIRD-PARTY.md)。像素测试使用已生成的参考数据，重新生成时显式提供外部上游源码目录。Raster 使用自身目录内的公共头文件。

| 目录 | 来源 | 固定提交 |
| --- | --- | --- |
| [`layerExRaster/`](../layerExRaster/) | [wamsoft/layerExRaster](https://github.com/wamsoft/layerExRaster) | `a4c9be0c7510cca552e27c0c3e931b2eaee399af` |
| [`layerExSave/`](../layerExSave/) | [wamsoft/layerExSave](https://github.com/wamsoft/layerExSave) | `b0939dc916f1b39775768cbb25e6a54b4e217410` |
| [`layerExDraw/`](../layerExDraw/) | [wamsoft/layerExDraw](https://github.com/wamsoft/layerExDraw) | `f665a153fa774e47069f53e303329be15975d1e0` |
| [`extrans/`](../extrans/) | [wamsoft/extrans](https://github.com/wamsoft/extrans) | `bd0611eced7e2731a8c8403148e06abea827467e` |
| [`extNagano/`](../extNagano/) | [wamsoft/extNagano](https://github.com/wamsoft/extNagano) | `4b403e2f684756c490975b9d1697289aa1f34240` |

Movie、Perspective 与 Layer 辅助实现复用现有 [`Kirikiroid2/`](../Kirikiroid2/) submodule。已编译插件及现有 Rust shim 无需再克隆一份上游；其余插件在来源确认后按同一规则接入。

```sh
git submodule update --init --recursive
git -C layerExRaster switch bevy-port
```

submodule 记录提交，不记录检出的本地分支。新克隆通常处于 detached HEAD；若本地还没有分支，使用 `git -C layerExRaster switch -c bevy-port`，其余目录同理。本次未发布远端分支，因此新增条目没有把 `.gitmodules` 的远端跟踪分支设为 `bevy-port`。后续移植提交应先在可访问的 fork 发布，再更新父仓库 URL 与 gitlink；日常复现使用固定提交，不使用 `update --remote`。

保留上游版权和随附许可证；依赖实际进入构建时同步维护 `LICENSE-THIRD-PARTY.md`。上游最新版可能包含 NVL Maker 旧 DLL 没有的 API，验收以模板和所选 DLL 版本为准。

## 编译、注册与 GPU 分工

1. 在现有插件清单中登记模块名、源码及必要的附加编译单元。为根目录插件增加明确的工作区路径解析、include 路径和文件变更追踪，不依赖开发机器绝对路径。
2. Image/Movie 使用现有 ncbind 绑定并声明模块名。核心 KAGParser 不依赖 V2Link；后续转场插件的 V2Link/onV2Link 需接入现有注册生命周期，不能假定每个上游都使用 ncbind。
3. 复用生成包装器及保留符号，确保插件进入最终静态链接。`module` 与注册名一致；游戏保留 `Plugins.link("layerExImage.dll")` 等原有调用。
4. 按脚本需要加载接口。只有必须早于游戏初始化存在的能力才自动注册；支持重复 link，退出时在 TJS 对象仍有效期间清理回调、类和任务。
5. C++ 保留 Layer/TJS 语义，Bevy 统一拥有纹理和 GPU 运算。优先扩展现有渲染命令及 WESL 内核，不为每个插件建立一套 FFI 或纹理管理器。
6. 普通图形效果不逐帧回读。保存、导出和显式像素访问允许必要快照；GPU 同图读写使用快照或独立输出，保证顺序与裁剪。

FFI 按完整操作传递字节、尺寸、pitch、纹理标识和参数。对于图片解码，C++ 从 `tTJSBinaryStream` 读成连续字节，提交 Rust 的解码操作，Rust 返回像素数据与尺寸；不把底层解码库的 create/read/get/free 等 API 逐项暴露给 C++。明确缓冲所有权、生命周期、长度与错误回传，复用已有释放接口，不让 C++ 异常跨 ABI。

## 验收与执行顺序

下一批依次实现 Perspective、Raster、Save。每项完成源码接入、插件生命周期与独立行为测试后，再用实际调用该接口的游戏或样例验收；Draw 和附加转场留在后续列表。游戏若在启动或主流程中依赖其他待实现接口，优先处理该阻塞。

测试遵循 [编写约定](../AGENTS.md)，只运行受影响的目标：

```sh
cargo test --test plugins --test scripts_ext --test kag_parser
cargo test --test layer_ex_image --test render
cargo test --test restart
```

第一批检查注册及 Scripts，第二批检查 GPU 输出，第三批在修改生命周期时运行。新增插件测试按能力单独执行，使用上游算法或独立样例验证像素与参数语义，不只检查方法是否存在。改视频时再运行：

```sh
cargo test --features video-ffmpeg --test layer_ex_movie --test video
```

现有 [`tests/common/mod.rs`](../tests/common/mod.rs) 可无窗口初始化 GPU；模板验收也应使用该路径，而非绕过渲染初始化直接创建 Layer。测试使用临时游戏副本及独立存档目录，不写入编辑器原始模板。图形合成通过像素检查后，仍需在有显示环境中确认可见 UI 与输入，在原版 Kirikiroid2 上确认触摸、视频和存档。

每个实际工程还需比较目录版与 XP3 版、UTF-16 脚本和中文资源名、字体度量、资源搜索优先级、存档重载及音频循环点。启动日志中的 `Startup script ended`、`Plugins.link()` 未抛异常或错误被脚本捕获都不是验收通过条件。跨运行器存档互通另行验证；同一游戏资源可运行不等于存档天然可交换。

## 当前验证边界

本地基线为 NVL Maker 3.87 默认模板的 340 个文件，文件名排序后依次哈希相对路径、NUL 字节及文件 SHA-256，聚合摘要为 `114c7c4d2cc9bb10768fcc9882b059d6f7969d823622e52ac7b7ffed9cde3f43`。未在这份模板的 TJS/KS 中发现四项解析扩展的直接调用，扩展语义由独立样例验证。模板实际调用的 LayerExImage 入口包括 `AnimationLayer.tjs:842`、`KAGEnvCharacter.tjs:589` 和 `KAGEnvironment.tjs:904` 的亮度调整。

测试运行器使用 Bevy 无窗口 Vulkan 渲染，临时复制整个模板并使用副本的存档目录。运行模板验收：

```sh
NVL_MAKER_DATA=editor/THE_NVL_Maker_ver3.87_sim/project/template/Data \
  cargo test --test nvl_maker -- --ignored --nocapture
```

`NVL_MAKER_DATA` 也可指向 XP3 文件；测试将归档复制到临时项目。目录版和使用 [`pack_xp3.py`](../tests/fixtures/pack_xp3.py) 生成的压缩 XP3 版均已完成上述闭环，标题、对话、读档、历史、选择支、重启读档七组截图逐像素一致。UTF-16 脚本和中文资源名沿用模板原文件。

可设置 `NVL_MAKER_CAPTURE` 保存验收场景的 PPM 图像。启用 `video-ffmpeg` 并设置 `NVL_MAKER_VIDEO=1` 时，验收程序还将样例视频复制到临时项目，调用模板原有 `GraphicLayerEx.startMovie` / `MovieControlLayer`，验证两个显示层的颜色、蒙版、EOF 通知和控制层自动销毁。验收程序每帧同时推进事件和场景构建，等待脚本到达目标状态后检查实际像素；转场不能仅靠事件 tick 验证。

本地自动验收使用 NVIDIA RTX 4060 / Vulkan，视频样例使用 FFmpeg 软件解码。模板没有录制语音资源，语音路径使用自带 WAV 在 `se[1]` 缓冲中验证；特定游戏的语音标签和循环点需单独验收。

自动测试不替代原版 Kirikiroid2 实机验收。当前原版运行器、完整可见窗口交互、跨运行器存档及所有后续插件效果仍待验收。
