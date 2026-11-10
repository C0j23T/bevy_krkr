# NVL Maker / KAGeX 适配

## 目标与基线

使 NVL Maker 导出的同一份游戏脚本和资源能够在 Kirikiroid2 与 bevy_krkr 中运行，核心工作是补齐 LayerEx 插件接口及其图形实现。基线为 `editor/THE_NVL_Maker_ver3.87_sim/project/template/Data`、随附皮肤和实际使用的编辑器功能。

基本运行包括启动、标题菜单、对话、分支选择、历史记录、自动/跳过、BGM/SE/语音、普通转场、存档、读档、退出与再次启动。使用动态图层的工程还必须完成 Layer 视频播放。上游源码可取得、模块能注册或启动脚本返回成功，都不能单独证明游戏可用。

本次交付是适配方案与源码准备。新增 submodule 尚未加入运行时编译清单；以下“待实现”条目均不能视为已支持。原版 Kirikiroid2 的兼容结论也必须以选定版本的实机运行结果为准。

## 当前基础

TJS2、KAGParser、Storage、Window/Layer、字体和音频由 Kirikiroid2 核心提供，Bevy 负责图形与输入等宿主能力。

| 模块或能力 | 当前情况 | 对 NVL Maker 的处理方向 |
| --- | --- | --- |
| KAGParser / KAGeX 系统脚本 | 核心已有 KAGParser；没有完整 KAGParserEx 兼容结论 | 保留模板的 KAGeX 脚本，按其实际调用补齐原生接口 |
| `Scripts.foreach/getObjectKeys` | 已由 `bevy_scripts.dll` 自动注册 | 维持单一实现，与 KAGParserEx 接入一起验证 |
| `csvParser.dll` | 已编译，`Plugins.link()` 后注册 | 验证 UI 表格读取与继承；`UILoader.tjs:6` 依赖它 |
| `saveStruct.dll` | 已编译；核心也有 Array/Dictionary `saveStruct` | 区分核心存档方法与插件扩展，验证真实存读档 |
| `addFont.dll`、`dirlist.dll`、`fftgraph.dll`、`getSample.dll`、`getabout.dll`、`varfile.dll`、`win32dialog.dll`、`wutcwf.dll` | 已在编译清单中 | 按实际功能验证；编译入库不代表宿主行为全部具备 |
| `wuvorbis.dll` / WAV | 核心已有 Vorbis/WAV 解码路径 | 用 OGG/WAV 验证播放、循环点和等待结束，无需加载 Windows DLL |
| `krmovie.dll` / VideoOverlay | 有 Bevy 视频后端，需 `video-ffmpeg` 与 `KrkrVideoPlugin` | 验证标准视频标签；此能力不等于 LayerExMovie |
| `KAGParser.dll`、`menu.dll` | 模板只在 `kirikiriz` 预处理分支中加载；核心已有解析器和 MenuItem 对象 | 验证 `Window.menu` 等脚本对象契约，原生菜单显示另列待办 |
| ZIP/7z/XP3 读取 | 已有资源读取实现 | 不将读取压缩包等同于兼容 `minizip.dll` 的脚本 API |

构建清单见 [`codegen/src/port.rs`](../codegen/src/port.rs)，插件生成逻辑见 [`crates/krkr_sys/build.rs`](../crates/krkr_sys/build.rs)，加载入口见 [`BevyPlugins.cpp`](../Kirikiroid2/src/core/environ/bevy/BevyPlugins.cpp) 与 [`InternalPlugins.cpp`](../Kirikiroid2/src/plugins/InternalPlugins.cpp)。

## 立即适配

### LayerExImage：基本图形依赖

`system/LayerEx.tjs:29` 加载 `layerExImage.dll`；`AffineLayer.tjs:677` 起转发 `light/colorize/noise`，`AnimationLayer.tjs:842`、`KAGEnvironment.tjs:904` 和 `KAGEnvCharacter.tjs:589` 实际使用亮度调整。缺失接口可能中断演出，也可能被脚本捕获后显示错误背景，必须优先解决。

上游 [layerExImage](https://github.com/wamsoft/layerExImage) 已接入 [`layerExImage/`](../layerExImage/)。以固定版本的 [`manual.tjs`](../layerExImage/manual.tjs) 和算法为依据，支持六个接口：

| 接口 | 兼容方向 |
| --- | --- |
| `light(brightness, contrast)` | 保留查表公式、截断和饱和行为，通过 GPU LUT 或等价运算执行 |
| `colorize(hue, sat, blend)` | 保留上游 HSL 取值范围与混合算法，不把 hue 按普通角度重新解释 |
| `modulate(hue, saturation, luminance)` | 按上游定义调整色相、饱和度、亮度，与 colorize 分别处理 |
| `noise(level)` | GPU 生成彩色噪声；验证分布、幅度、Alpha，不承诺跨 C 运行库的 rand 序列一致 |
| `generateWhiteNoise()` | GPU 生成灰度白噪声，保持 Alpha |
| `gaussianBlur(radius)` | 在 GPU 上实现与上游权重、边界和 Alpha 处理一致的高斯滤波；不以 box blur 代替 |

C++ 保留 TJS 参数解析和 Layer 语义，通过现有渲染命令传递纹理、裁剪区域与参数。Rust 在现有 Bevy 渲染器中执行计算；上游 CPU 算法可作参考结果，不作为逐帧回读方案。重点覆盖 BGRA/RGBA 转换、负 pitch、imageLeft/imageTop、裁剪、透明像素与同图读写。

### LayerExMovie：动态图层工程的运行依赖

`system/Initialize.tjs:5` 尝试加载 `layerExMovie.dll`。`GraphicLayer.tjs:513` 调用 `openMovie(filename, alpha)`、`startMovie(loop)`；KAGeX 的 `movie` 图层命令会经 `KAGEnvLayer.tjs:313` 进入这条路径。无视频工程可先验收基础流程，使用此功能的工程必须同时验收本项。

源码已位于现有 Kirikiroid2 submodule 的 [`layerExMovie.cpp`](../Kirikiroid2/src/plugins/layerExMovie.cpp)，不另克隆完整引擎副本。

- 保留 Layer 的 `openMovie`、`startMovie`、`stopMovie`、`isPlayingMovie`，以及 `onStartMovie/onUpdateMovie/onStopMovie` 回调。
- 将旧播放器接到当前 Rust 视频解码、时钟、音频和纹理上传路径，视频帧写入普通 Layer 纹理，参与正常层级、变换、透明度与转场。
- 验证 `alpha=true` 的半宽视频布局及蒙版解释；标准视频的原生 Alpha 与这种打包方式分别处理。
- 验证循环、EOF、等待、停止、场景切换、多个显示 Layer 共享控制层，以及销毁和重启。
- 无视频后端时，在请求播放时给出明确错误；不伪装成功并让脚本无限等待。

### KAGeX 解析扩展：同步审计

上游 [KAGParserEx](https://github.com/wamsoft/KAGParserEx) 已接入 [`KAGParserEx/`](../KAGParserEx/)。其完整实现包含多行标签、`taglist`、参数宏、`emb escape=false` 等能力；当前两项 Scripts 扩展不能证明这些行为兼容。

先对照 [`readme.txt`](../KAGParserEx/readme.txt) 审计模板、宏与编辑器输出中的实际使用。被基础工程使用的差异与 LayerExImage 同批解决；其余扩展列为后续能力。上游使用与核心同名的解析器类型并替换全局 KAGParser，不能直接把两份实现链接到同一命名空间。优先对核心解析器补足必要语义；若采用替换实现，需隔离 C++ 符号，处理原生类 ID、加载时机、状态保存恢复及重复会话。`Scripts.foreach/getObjectKeys` 保持唯一注册来源。

## 待适配：功能完整性

下表以未使用相应功能的基础工程为前提。若游戏在启动、菜单或主流程中调用这些 API，对该工程立即升级为必需项；不提供吞异常或统一降级转场来掩盖缺失行为。

| 插件 | 功能 | 兼容方向 |
| --- | --- | --- |
| `perspective.dll` | 四点透视复制 | 接入现有 [`layerExPerspective.cpp`](../Kirikiroid2/src/plugins/layerExPerspective.cpp)，复用 `OperatePerspective` 和 Bevy 投影映射；验证裁剪、采样、退化四边形、Alpha 和同图复制 |
| `layerExRaster.dll` | 扫描线波浪扭曲 | 按 [`manual.tjs`](../layerExRaster/manual.tjs) 实现 `copyRaster(src,maxh,lines,cycle,time)`，用 GPU 逐行位移，保持未覆盖区域；上游算法作为校验依据 |
| `layerExSave.dll` | PNG/TLG 保存、裁剪、差异和通道处理 | 复用现有编码与 Storage；GPU 快照只在保存时读回，差异/裁剪可使用 GPU 归约；异步任务持有像素快照，进度/完成事件回到 TJS 主线程 |
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

新增源码均在项目根目录，以 submodule 固定提交，并已在各工作副本创建本地 `bevy-port` 分支。克隆仅用于源码准备，不执行上游 CMake，也不自动引入其相邻 SDK 或工具链。

| 目录 | 来源 | 固定提交 |
| --- | --- | --- |
| [`layerExImage/`](../layerExImage/) | [wamsoft/layerExImage](https://github.com/wamsoft/layerExImage) | `3aff625dba5c545ba722bb43b0a5ca1ad4f4336d` |
| [`layerExRaster/`](../layerExRaster/) | [wamsoft/layerExRaster](https://github.com/wamsoft/layerExRaster) | `a4c9be0c7510cca552e27c0c3e931b2eaee399af` |
| [`layerExSave/`](../layerExSave/) | [wamsoft/layerExSave](https://github.com/wamsoft/layerExSave) | `b0939dc916f1b39775768cbb25e6a54b4e217410` |
| [`layerExDraw/`](../layerExDraw/) | [wamsoft/layerExDraw](https://github.com/wamsoft/layerExDraw) | `f665a153fa774e47069f53e303329be15975d1e0` |
| [`KAGParserEx/`](../KAGParserEx/) | [wamsoft/KAGParserEx](https://github.com/wamsoft/KAGParserEx) | `61d5f1e65f500b5aef5a621333f86f3457939b31` |
| [`extrans/`](../extrans/) | [wamsoft/extrans](https://github.com/wamsoft/extrans) | `bd0611eced7e2731a8c8403148e06abea827467e` |
| [`extNagano/`](../extNagano/) | [wamsoft/extNagano](https://github.com/wamsoft/extNagano) | `4b403e2f684756c490975b9d1697289aa1f34240` |

Movie、Perspective 与 Layer 辅助实现复用现有 [`Kirikiroid2/`](../Kirikiroid2/) submodule。已编译插件及现有 Rust shim 无需再克隆一份上游；其余插件在来源确认后按同一规则接入。

```sh
git submodule update --init --recursive
git -C layerExImage switch bevy-port
```

submodule 记录提交，不记录检出的本地分支。新克隆通常处于 detached HEAD；若本地还没有分支，使用 `git -C layerExImage switch -c bevy-port`，其余目录同理。本次未发布远端分支，因此新增条目没有把 `.gitmodules` 的远端跟踪分支设为 `bevy-port`。后续移植提交应先在可访问的 fork 发布，再更新父仓库 URL 与 gitlink；日常复现使用固定提交，不使用 `update --remote`。

保留上游版权和随附许可证；依赖实际进入构建时同步维护 `LICENSE-THIRD-PARTY.md`。上游最新版可能包含 NVL Maker 旧 DLL 没有的 API，验收以模板和所选 DLL 版本为准。

## 编译、注册与 GPU 分工

1. 在现有插件清单中登记模块名、源码及必要的附加编译单元。为根目录插件增加明确的工作区路径解析、include 路径和文件变更追踪，不依赖开发机器绝对路径。
2. Image/Raster/Draw 使用现有 ncbind 绑定；上游未声明 `NCB_MODULE_NAME` 的入口需要补齐。KAGParserEx 和转场插件的 V2Link/onV2Link 需接入现有注册生命周期，不能假定每个上游都使用 ncbind。
3. 复用生成包装器及保留符号，确保插件进入最终静态链接。`module` 与注册名一致；游戏保留 `Plugins.link("layerExImage.dll")` 等原有调用。
4. 按脚本需要加载接口。只有必须早于游戏初始化存在的能力才自动注册；支持重复 link，退出时在 TJS 对象仍有效期间清理回调、类和任务。
5. C++ 保留 Layer/TJS 语义，Bevy 统一拥有纹理和 GPU 运算。优先扩展现有渲染命令及 WESL 内核，不为每个插件建立一套 FFI 或纹理管理器。
6. 普通图形效果不逐帧回读。保存、导出和显式像素访问允许必要快照；GPU 同图读写使用快照或独立输出，保证顺序与裁剪。

FFI 按完整操作传递字节、尺寸、pitch、纹理标识和参数。对于图片解码，C++ 从 `tTJSBinaryStream` 读成连续字节，提交 Rust 的解码操作，Rust 返回像素数据与尺寸；不把底层解码库的 create/read/get/free 等 API 逐项暴露给 C++。明确缓冲所有权、生命周期、长度与错误回传，复用已有释放接口，不让 C++ 异常跨 ABI。

## 验收与执行顺序

| 阶段 | 内容 | 验收 |
| --- | --- | --- |
| A | 导出项目基线与 KAGeX 调用审计 | 固定模板、皮肤、两端运行器版本和实际使用的接口；记录第一个真实失败点 |
| B | LayerExImage 与必要解析语义 | UI/对话可见；亮度、着色、噪声、模糊符合参考；无未处理的缺失成员 |
| C | 基础游戏闭环 | 选择、历史、自动、跳过、普通转场、音频、存读档、退出后重进均有效 |
| D | LayerExMovie | 在视频工程中验证帧更新、透明度、循环、等待及销毁；标准 VideoOverlay 单独验证 |
| E | 待适配功能 | 按实际工程使用依次接入 Perspective、Raster、Save、Draw 和附加转场 |

测试遵循 [编写约定](../AGENTS.md)，只运行受影响的目标：

```sh
cargo test --test plugins --test scripts_ext
cargo test --test render
cargo test --test restart
```

第一批检查注册及 Scripts，第二批检查 GPU 输出，第三批在修改生命周期时运行。新增插件测试按能力单独执行，使用上游算法或独立样例验证像素与参数语义，不只检查方法是否存在。改视频时再运行：

```sh
cargo test --features video-ffmpeg --test video
```

现有 [`tests/common/mod.rs`](../tests/common/mod.rs) 可无窗口初始化 GPU；模板验收也应使用该路径，而非绕过渲染初始化直接创建 Layer。测试使用临时游戏副本及独立存档目录，不写入编辑器原始模板。图形合成通过像素检查后，仍需在有显示环境中确认可见 UI 与输入，在原版 Kirikiroid2 上确认触摸、视频和存档。

每个实际工程还需比较目录版与 XP3 版、UTF-16 脚本和中文资源名、字体度量、资源搜索优先级、存档重载及音频循环点。启动日志中的 `Startup script ended`、`Plugins.link()` 未抛异常或错误被脚本捕获都不是验收通过条件。跨运行器存档互通另行验证；同一游戏资源可运行不等于存档天然可交换。

## 当前验证边界

已核对模板插件加载点、LayerEx 调用链、上游 API 与固定源码提交；七个新增仓库已建立本地分支。本次未修改插件实现或编译清单，未宣称完成游戏运行验收。后续按上面的阶段逐批推进。
