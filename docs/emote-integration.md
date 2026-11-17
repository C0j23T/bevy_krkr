# Emote / PSB 集成调查

## 实现边界

- 容器、动画、物理与 TJS 适配使用 C++，逻辑放在 `Kirikiroid2`。
- Rust 承载 Bevy 宿主与 GPU 渲染能力。
- `psbfile.dll` 不涉及渲染，按标准 `tp_stub.h` ABI 独立为外部插件。
- `emoteplayer.dll` 参考 `~/Workspace/krkrsdl3/plugins/emoteplayer`；`motionplayer.dll` 是模块别名。
- 参考实现包含网格变形；通用网格操作内建到 `operations.wesl` 的 `ShaderComposer`，供后续 Spine / Live2D 复用。
- 纹理首次使用时上传；每帧提交变换、顶点、蒙版及绘制命令。仅显式抓帧接口允许读回。
- 本文的接口与验收项是迁移要求；实际完成状态见 [插件迁移清单](plugin-migration.md)。

## C++ 迁移工作区状态

`Kirikiroid2/src/plugins/emoteplayer/` 已迁入参考容器、时间轴、物理、几何与
TJS 绑定；上述 C++ 翻译单元已分别通过 C++17 语法检查并加入
`codegen/src/port.rs` 构建清单。真实容器、TJS 播放和 GPU 像素验收仍待完成。

`BevyEmoteRenderer.cpp` 的 target、texture、清空及 Layer GPU copy 使用现有
`iTVPTexture2D`，无独立窗口或 present。常规图标绘制提交曲面链；CPU 图标网格仅在
hit test 查询时计算，物理锚点与 shape 查询仍由 C++ 几何计算。
`D3DAdaptor.captureCanvas` 保留显式读回。

### 通用 GPU 曲面契约

`Kirikiroid2/src/core/environ/bevy/BevyMesh.h` 定义完整操作
`krkr_render_mesh(const TVPBevyMeshCommand*)`。调用时复制所有指针指向的数据，
异常不得跨越 C ABI；返回 0 表示成功。宿主尚需实现此函数与对应 Composer 操作。

- `target/source/mask` 为现有 Bevy 纹理 ID；mask=0 关闭 alpha 蒙版。
- `surfaces` 顺序为 player root 到 icon；`surface_count` 描述整条曲面链。
- `division_x/y` 是二维网格细分数；UV 顶点由 GPU 生成，索引为每格两个三角形。
- `blend` 沿用 Emote 0 普通 alpha、1/4 乘色、3 加色、6 跳过、21 纯色替换；
  `opacity/tint/color` 分别为透明度、乘性颜色及 mode 21 常量色。
- 每个 surface 的 `control_points[32]` 为 16 个双三次 Bézier 控制点，按参考行序。
- `models[32][16]` 是 `attachMat * buildMotionLinear(surface, mask)`，mask 为
  variant 左移 4 位；矩阵为列主序。C++ 预计算线性矩阵，GPU 计算曲面与每顶点贡献链。
- `placement = attachMat * (coordX,coordY,0,1)`，w 置 0；
  `geometry=(originX,originY,width,height)`，`offset=(currOx,currOy,0,0)`。
- `kind` 对应参考 `emoteRender.type`；`inherit_mask` 为当前继承标志。

GPU 顶点评估从 icon 向 root 反向遍历：以参考
`emotegeometry.h::emoteSurfaceEvaluator::evaluate` 为准，分别保留几何和祖先 placement
贡献。内层与 root 使用完整 0x1f0 线性 mask，其他贡献使用逐层相与的 inherit mask；
Bezier 的归一化坐标带轴交换，输出还执行最终 Y 翻转。不能用单个 patch 加总矩阵
替代这条链，也不能先由 CPU 生成变形顶点再交给 shader。

ABI 使用 C++ `static_assert` 固定：surface 为 2240 字节，command 为 88 字节；
后者指针与纹理 ID 采用 64 位对齐。Rust 结构必须为 `repr(C)`，WGSL storage buffer
可由宿主另行打包，不能直接假定 C 布局满足 WGSL 的 vec4 对齐。

构建接入已在 `EXTRA_SOURCES` 添加 `emotefile.cpp/PSBData.cpp/emoterunner.cpp/
emoteplayerclass.cpp`，并在 `PLUGINS` 添加 `emoteplayer.cpp`；仍需实现
`motionplayer.dll` 模块别名。`BevyEmoteRenderer.cpp` 已在 environ/bevy 的源码扫描目录，
因此加入任何构建验证前需先提供 `krkr_render_mesh` 符号。

## snbk 资源调查

### PSB/MDF 数量与版本

扫描 `data.xp3`、`data720.xp3`、`patch.xp3` 和 `evimage720.xp3`：

| 资源 | 数量 | 形态 | 说明 |
|---|---:|---|---|
| `.scn` | 188 | 95 个 PSB v3、1 个 PSB v2、92 个 MDF | 92 个 MDF 在 `patch.xp3`，内层全部为 PSB v2 |
| `.mtn` | 33 | 全部 PSB v3 | 合计约 84 MB，最大 `title_bg.mtn` 8.59 MB |
| `.pimg`（data720） | 30 | 全部 PSB v2 | 含 TLG 资源 |
| `.pimg`（evimage720） | 111 | 全部 PSB v2 | 7z 归档内的立绘/事件图 |
| 加密标志 | 扫描到的 PSB 均为 0 | 无 seed 加密 | 普通 PSB 的 `encrypt` 字段均为 0 |
| PSB v4 / extra chunk | 0 | 无 | snbk 不需要 v4 |

### 关键结论

- **92 个 patch 场景脚本是 MDF**。`yuzu-psb::PsbFile::parse` 直接把这些 MDF 当作 PSB，实测报 `invalid psb signature`；`emote-psb::MdfReader` 可以解析。`patch.xp3` 会覆盖原场景，因此 MDF 支持是 snbk 的硬需求。
- **`.mtn` 资源占绝大部分**。例如 `sd001.mtn` 1,319,952 字节，34 个资源合计 1,286,529 字节，最大单个 313,328 字节。emote 资源必须按需读取，不能像 `yuzu-psb` 那样解析时全部复制。
- **`.pimg` 需要 `psb://` 存储**。`default.tjs` 使用 `psb://window.pimg/`，`psdlayer.tjs` 使用 `PSBFile`、`psb://`、`loadPIMG`。根对象中的 `key -> Resource(idx)` 需要映射到资源流。
- **`.scn` 没有资源**，根键为 `hash/name/outlines/scenes`；`.pimg` 根键包含 `layers/width/height` 和多个 `*.tlg` 资源键；`.mtn` 根键包含 `metadata/object/source/spec`。
- **snbk 样本未出现 lzfs 或 MT19937 MDF**。这些按 krkrsdl3 兼容面实现，但不阻塞 snbk 的首轮验收。

### 实测

- `emote-psb 0.8.0`：解析 snbk 的 PSB v2/v3 `.scn`、`.pimg`、`.mtn`，解析真实 MDF `.scn`，`open_resource` 按需读取资源。
- `yuzu-psb 0.1.0`：解析普通 PSB 成功；解析 MDF 失败；解析时 `data.to_vec()` 复制输入，并 eager 读取全部资源。
- `hiroshil/emote-psb-rs` 的 CLI：对 snbk 的 MDF 执行 `info` 成功，但新增的 MT19937 能力在 snbk 中未使用。

## C++ 参考实现

| 部分 | 路径 | 能力与边界 |
| --- | --- | --- |
| PSB/MDF | `~/Workspace/krkrsdl3/plugins/psbfile` | PSB v1–v3、普通 MDF、`PSBFile` 与 `psb://`；v4 被拒绝；MFL 不支持 |
| Emote 容器 | `~/Workspace/krkrsdl3/plugins/emoteplayer/emotefile.*` | PSB v1–v4、extra chunk、LZ4 frame、seed / TJS 回调解密、资源按需加载 |
| 动画 / 物理 | 同目录 `emoterunner.*`、`emotephysics.h` | 时间轴、变量、差分混合、风力与外力 |
| 网格 | 同目录 `emotegeometry.h` | 16 控制点双三次 Bézier patch、网格细分、矩阵与节点变形 |
| TJS 类 | 同目录 `emoteplayerclass.*`、`emoteplayer.cpp` | `Motion`、`ResourceManager`、`EmotePlayer`、`Player`、两种 adaptor |
| 渲染契约 | `~/Workspace/krkrsdl3/core/render/TVPCompositor.h` | target、texture、mask、blend、`DrawMesh`、显式 capture |
| D3D 兼容面 | `~/Workspace/krkrsdl3/plugins/DrawDeviceD3D` | D3D layer/image/picture/player、前后平面与转场 |

## 容器约束

- 处理顺序：Storage → LZ4 frame（如有）→ MDF（如有）→ seed 解密 → TJS 回调解密 → PSB 名称、字符串与资源。
- LZ4 frame magic：`04 22 4d 18`；资源路径可含 `lzfs://./` 前缀。
- MDF：`mdf\0` + u32 解压后长度 + zlib 数据；校验输出大小，不能把长度当压缩数据长度。
- PSB v1 名称表在 `offsetEncrypt`；v2+ 使用 charset / namesData / nameIndexes。
- seed 初始化：`[0x075BCD15, 0x159A55E5, 0x1F123BB5, seed]`；v2 还需解密 body。
- 加密判定参考 offset 启发式；不能只凭 `encrypt` 标志判定。
- `PSBFile` 保持参考实现的只检测加密、不解密行为；Emote 容器支持 seed 与 TJS 回调。
- 回调及 TJS closure 留在 C++；异常不得穿过 Rust FFI。
- `psb://<文件>/<资源键>` 维持大小写归一化、懒加载与按 offset / length 读取；不扩展参考未实现的列目录行为。
- 保留参考对象排序、数值类型与资源按需读取语义，避免一次复制整份 `.mtn` 资源。

## GPU 迁移要求

- target 创建、选择、清空和 GPU copy；保持纹理生命周期。
- 顶点、索引、纹理坐标、矩阵变换与网格变形。
- 模板 / alpha 蒙版、嵌套层与混合模式。
- BGRA / RGBA 交换使用 `ChannelSwizzle` 通道数组。
- `D3DAdaptor.captureCanvas`、显式纹理锁定可读回；常规动画不得每帧读回上传。
- `operations.wesl` 的内建网格能力先覆盖真实 Emote 绘制需求，再提供其他动画库适配。
- 外部 WESL 操作注册只适合已有纹理与几何契约能表达的操作；网格资源、target 生命周期和 draw device 宿主行为不能仅靠片段操作体解决。

## 验收

- 容器：真实 PSB v2/v3、92 个 MDF `.scn`、33 个 `.mtn`、141 个 `.pimg`；v1/v4、加密和 LZ4 使用专门样本。
- TJS：`PSBFile.root`、`psb://`、`Motion`、资源管理、Player / EmotePlayer 的时间轴、变量和序列化。
- 动画：参考 C++ 引擎与 Bevy 比较同一 `.mtn` 的帧时间、节点变换、网格、蒙版和 hit test。
- snbk：`D3DaffineSourceEmote.tjs`、`psdlayer.tjs`、`StorageData.tjs`。
- 渲染：真实像素比较、资源卸载与重载、显式 capture；确认常规帧无 GPU→CPU→GPU 往返。

## 其他容器库调查

| 库 | 已有调查 | 当前用途 |
| --- | --- | --- |
| [emote-psb](https://github.com/storycraft/emote-psb-rs) 0.8 | 支持 PSB v2–v4、MDF、按需资源；MDF 长度语义与参考不同，缺 v1 / seed 解密 | 资源调查工具参考 |
| [yuzu-psb](https://github.com/lilyco-42/lyco-engine) 0.1 | 普通 PSB 可解析；真实 MDF 报 invalid signature；资源 eager 复制 | 不作为迁移基础 |
| [hiroshil fork](https://github.com/hiroshil/emote-psb-rs) | 增 MT19937 MDF shell 和 CLI；真实 snbk MDF 可解析，样本无需 MT19937 | 可选离线调查工具 |
