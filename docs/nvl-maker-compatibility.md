# NVL Maker / KAGeX 适配

## 目标与基线

使 NVL Maker 导出的同一份游戏脚本和资源能够在 Kirikiroid2 与 bevy_krkr 中运行，核心工作是补齐 LayerEx 插件接口及其图形实现。基线为 `editor/THE_NVL_Maker_ver3.87_sim/project/template/Data`、随附皮肤和实际使用的编辑器功能。

基本运行包括启动、标题菜单、对话、分支选择、历史记录、自动/跳过、BGM/SE/语音、普通转场、存档、读档、退出与再次启动。使用动态图层的工程还必须完成 Layer 视频播放。上游源码可取得、模块能注册或启动脚本返回成功，都不能单独证明游戏可用。

已完成项列于下方，其余能力按实际工程调用补齐。原版 Kirikiroid2 的兼容结论必须以选定版本的实机运行结果为准。

## 当前基础

TJS2、KAGParser、Storage、Window/Layer、字体和音频由 Kirikiroid2 核心提供，Bevy 负责图形与输入等宿主能力。

构建清单见 [`codegen/src/port.rs`](../codegen/src/port.rs)，插件生成逻辑见 [`crates/krkr_sys/build.rs`](../crates/krkr_sys/build.rs)，加载入口见 [`BevyPlugins.cpp`](../Kirikiroid2/src/core/environ/bevy/BevyPlugins.cpp) 与 [`InternalPlugins.cpp`](../Kirikiroid2/src/plugins/InternalPlugins.cpp)。

## 已完成

- `layerExImage.dll`
- `layerExMovie.dll`
- `KAGParserEx.dll`：扩展功能内置于核心 KAGParser
- `perspective.dll`
- `layerExRaster.dll`
- `layerExSave.dll`
- `fstat.dll`
- `extrans.dll`
- `extNagano.dll`
- `AlphaMovie.dll`: 通过外部插件，软件渲染
- `psd.dll`: 通过外部插件

本批已接入以下插件的明确子集，尚未实现的接口仍列在后续范围，不能据此宣称整个原 DLL 已兼容：

- `layerExText.dll` / `layerExDraw.dll`：共享文字适配子集，提供 `GdiPlus.Font`、`Appearance.clear/addBrush/addPen`、`Layer.drawString/drawPathString/measureString/measureStringInternal` 和系统字体列表。支持实色、指定两端点的线性渐变、偏移、裁剪、Alpha、字体 ascent/descent/leading/行距；CPU 生成字体覆盖率，GPU 完成扩张与着色合成。`drawPathString` 以覆盖率扩张近似描边。私有字体加载明确报不支持；GDI+ Brush 类型/渐变变体、完整 Font/Path/Matrix API、Layer 几何绘制、变换、图像绘制及记录接口尚未实现，不能据此宣称插件整体兼容。

## 立即实现

| 插件 | 评估与实现范围 | 验收 |
| --- | --- | --- |
| Text/Draw 后续接口 | 补充矩形/路径、渐变 rect/angle/wrapMode、完整字体度量、变换和私有字体 API；根据实际脚本补齐旧版文字参数 | 与所选原 DLL 对照菜单、对话、历史字体布局及路径像素；当前独立样例不能替代 GDI+ 一致性验收 |
| `lineParser.dll` | 提升到下一批：解析操作不依赖图形后端，易用独立样例验证；先确定原插件的类名与编码、异常语义 | 嵌套值、转义、Unicode、空行及坏输入 |
| `minizip.dll` | 提升到下一批：shim 已有 `zip`；复用归档依赖，由 C++ 适配 Storage 与 TJS | 写入/枚举/读回、中文路径、重复项及损坏数据 |

`nvl/macro.ks:13–14` 会加载 extrans/extNagano，但默认演出主要使用 crossfade/universal。模块加载语句本身不证明效果已执行；应扫描实际 `trans method` 和动态参数。`saveStruct` 的核心存档也不依赖 LayerExSave，不能把两者合并判断。

## 待实现

若游戏在启动、菜单或主流程中调用下列 API，对该工程立即升级为必需项；不提供吞异常或统一降级转场来掩盖缺失行为。

| 插件 | 功能 | 兼容方向 |
| --- | --- | --- |
| `agg.dll` | 矢量绘制扩展 | 确认脚本 API 后接现有 Bevy 图形路径，避免每帧 CPU 图像回传 |
| `clipboardEx.dll` | 剪贴板扩展 | 由宿主实现文本和必要的数据格式 |
| `menu.dll` 的原生 UI、系统输入框/文件选择/外链 | 平台交互 | 基础流程使用游戏自己的按钮和对话框；需系统功能时通过宿主实现，保留脚本对象状态 |

## 源码与分支

Image/Movie/Raster/Save 的运行时适配位于 Kirikiroid2 的 `src/core/environ/bevy/plugins`，Perspective 位于 `src/plugins/layerExPerspective.cpp`，解析扩展位于核心 KAGParser。Save 的像素分析与编辑位于 Rust shim 的 `ffi/image_ops.rs`；PNG/TLG5 编码、压缩和后台任务位于 [`crates/krkr_shim/src/save_image.rs`](../crates/krkr_shim/src/save_image.rs)，PNG 复用 Rust `png` crate；C++ 仅适配 TJS 参数、图层快照、Storage 写入和主线程事件。无需额外检出上游仓库或链接 C 编码库。

版权、来源和版本记录见 [`LICENSE-THIRD-PARTY.md`](../LICENSE-THIRD-PARTY.md)。像素测试使用已生成的参考数据，重新生成时显式提供外部上游源码目录。后续适配仅引入实际需要的源码并保留许可证，以 NVL 模板和所选 DLL 版本确定接口范围。

```sh
git submodule update --init --recursive
```

## 编译、注册与 GPU 分工

1. 在现有插件清单中登记模块名、源码及必要的附加编译单元。为附加编译单元设置 include 路径和文件变更追踪，不依赖开发机器绝对路径。
2. Image/Movie 使用现有 ncbind 绑定并声明模块名。核心 KAGParser 不依赖 V2Link；后续转场插件的 V2Link/onV2Link 需接入现有注册生命周期，不能假定每个上游都使用 ncbind。
3. 复用生成包装器及保留符号，确保插件进入最终静态链接。`module` 与注册名一致；游戏保留 `Plugins.link("layerExImage.dll")` 等原有调用。
4. 按脚本需要加载接口。只有必须早于游戏初始化存在的能力才自动注册；支持重复 link，退出时在 TJS 对象仍有效期间清理回调、类和任务。
5. C++ 保留 Layer/TJS 语义，Bevy 统一拥有纹理和 GPU 运算。优先扩展现有渲染命令及 WESL 内核，不为每个插件建立一套 FFI 或纹理管理器。
6. 普通图形效果不逐帧回读。保存、导出和显式像素访问允许必要快照；GPU 同图读写使用快照或独立输出，保证顺序与裁剪。

`Kirikiroid2/src/core/environ/bevy/compat/tp_stub.h` 是 Bevy 宿主内编译插件的源码兼容入口，转发 TJS、Storage、连续事件和转场声明。独立共享库使用外部 krkrz 的 `tp_stub.cpp` 和 `V2Link/V2Unlink`，由宿主的外部加载器及 ABI 适配层接入；当前真实验收范围为 Linux `extrans.so`。依赖 Bevy 纹理、视频或 Rust FFI 的插件仍属于宿主适配器。

FFI 按完整操作传递字节、尺寸、pitch、纹理标识和参数。对于图片解码，C++ 从 `tTJSBinaryStream` 读成连续字节，提交 Rust 的解码操作，Rust 返回像素数据与尺寸；不把底层解码库的 create/read/get/free 等 API 逐项暴露给 C++。明确缓冲所有权、生命周期、长度与错误回传，复用已有释放接口，不让 C++ 异常跨 ABI。

## 验收与执行顺序

下一批推进上表中的剩余图形接口及数据解析/归档插件，各插件按范围分项实现与验收。已完成插件的实际工程可见交互仍需验收。游戏若在启动或主流程中依赖其他待实现接口，优先处理该阻塞。

测试遵循 [编写约定](../AGENTS.md)，只运行受影响的目标：

```sh
cargo test -p krkr_shim --test png --test zlib --test save_image --test image_ops
cargo test --test extra_plugins
cargo test --test extrans_geometry
cargo test --test plugins --test scripts_ext --test kag_parser
cargo test --test layer_ex_plugins --test layer_ex_image --test render
cargo test --test restart
```

`extra_plugins` 检查文字覆盖率、实色/描边/渐变/裁剪，独立计算 wave/mosaic/blurfade 在 0/25/50/75/100 的逐像素输出，并重复启动引擎检查 provider 清理。另有 type 1 和 exponent 样例；透明源覆盖半透明实色混合；原 DLL 精度仍需扩展验收。

`extrans_geometry` 将 turn、rotatezoom、rotatevanish、rotateswap 的 GPU 输出与上游 C++ 算法生成的参考 RGB 比较，覆盖双向图片、1/25/50/75/95/99 时间点、偏心旋转、加速度、背景和不完整 tile；另验证真实时钟下连续完成两轮。旋转组采用源图覆盖和背景填充，不做额外 crossfade 或越界边缘延伸。参考数据由 `tests/fixtures/generate_extrans_geometry.py` 生成，重新生成时显式提供外部 extrans 源码目录。

Rust shim 测试检查编解码、元数据、压缩级别、像素操作和后台快照；后续三批分别检查注册及 Scripts、GPU 输出、重启生命周期。新增插件测试按能力单独执行，使用上游算法或独立样例验证像素与参数语义，不只检查方法是否存在。改视频时再运行：

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
