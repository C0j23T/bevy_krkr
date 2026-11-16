# Emote / PSB 集成调查

## 范围

目标是在 bevy_krkr 中实现 `psbfile.dll`、`motionplayer.dll`、`emoteplayer.dll` 和所需的 `drawdeviceD3D.dll` 兼容面，并以 `test_games/snbk` 为首个实际验收游戏。本文记录：

- snbk 实际使用的 PSB/MDF 资源形态；
- `emote-psb`、`yuzu-psb` 和 `hiroshil/emote-psb-rs` 的差异；
- `krkrsdl3` 中 PSBFile、emoteplayer、渲染后端的实现方式；
- 推荐的分层架构、加密/解压链路、实施顺序和验收点。

调查方式：扫描 snbk 的 XP3/7z 归档并用 `emote-psb 0.8.0`、`yuzu-psb 0.1.0` 实际解析样本；阅读 `~/Workspace/krkrsdl3` 的 `plugins/psbfile`、`plugins/emoteplayer`、`plugins/DrawDeviceD3D` 和 `core/render`。

## 结论

- **PSB/MDF 容器层使用 `emote-psb`**，不把 `yuzu-psb` 作为实现基础。
- **新增 Rust `krkr_emote` crate** 承载 emote 语义、动画、物理、网格、蒙版和绘制列表；它依赖 `emote-psb`，但不包含 TJS 和 Bevy 类型。
- **TJS 原生类留在 C++**，通过 ncbind 注册 `emoteplayer.dll` / `motionplayer.dll`，通过 FFI 调用 `krkr_emote`。这符合项目现有的 TJS/Storage 适配方式。
- **渲染复用 Bevy GPU 与现有渲染命令**，在 `krkr_plugin` 中扩展渲染目标、纹理、网格、蒙版和混合；不要逐帧 CPU 回读。
- `hiroshil/emote-psb-rs` 是 `storycraft/emote-psb-rs` 的 fork，主要增加 MT19937/XOR MDF shell 和 CLI。对 snbk 不是必需；可作为工具或后续可选 MDF 扩展。

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

## 候选库比较

### emote-psb 0.8.0

- 许可：MIT。
- API：
  - `PsbFile<T: BufRead + Seek>::open`
  - `deserialize_root::<T>()`
  - `open_resource` / `open_extra_resource`
  - `PsbValue`：Null/Bool/Number/String/List/Object/Resource/ExtraResource/Compiler 类型
  - `PsbWriter`、`MdfReader`、`MdfWriter`
- 优点：
  - 支持 PSB v2/v3/v4、普通 MDF、普通/extra 资源。
  - 输入是任意 `Read + Seek` 流，可直接接 C++ Storage 的 `KrkrStream`。
  - 资源是流式访问，不强制全量复制。
  - 支持 serde 类型化反序列化，适合 `krkr_emote` 自定义模型。
  - 依赖（flate2、serde、indexmap、smol_str、byteorder、adler2、thiserror、derive_more、scopeguard）已在本项目 `Cargo.lock` 中出现。
- 缺口：
  - 真实 PSB v1 名称表未覆盖；参考实现的 v1 分支需要另行补。
  - 不实现 seed/回调解密，只读 `encrypted` 标志。
  - `MdfReader` 把 MDF 头的 u32 当压缩长度，而参考实现当解压后长度；snbk 因为该值大于压缩流长度仍能解析，但语义需要修正。
  - `MdfReader` 只实现 `Read`，不能直接喂给 `PsbFile::open`，需要先读到 `Vec` 再 `Cursor`。
  - 边界检查少于参考实现；损坏数据可能触发超大 `reserve`。

### yuzu-psb 0.1.0

- 许可：MIT；只是 `lyco-engine` 内部为柚子社/千恋万花资源做的 160 行门面。
- API：`PsbFile::parse(&[u8]) -> serde_json::Value`。
- 限制：
  - 不支持 MDF，直接失败。
  - 输入必须整段 `&[u8]`，内部再 `to_vec()`。
  - 解析时把所有普通/extra 资源复制进 `Vec<Vec<u8>>`。
  - 对象转 `serde_json`，对象顺序和 float/double 类型细节丢失；资源用 `{"__PSB@RESOURCE": idx}` 标记。
  - 没有写入 API、没有名称/字符串表、没有资源 offset。
- 结论：不适合作为 `psbfile`/`emoteplayer` 的基础；其 `resource_ref` 逻辑只有十几行，可按需借鉴。

### hiroshil/emote-psb-rs

- 是 `storycraft/emote-psb-rs` 的 fork，版本仍为 `0.8.0`，比上游多 4 个提交。
- 主要增量：
  - MDF 的 MT19937/XOR shell（FreeMote 风格 `.psb.m`），`MdfReader::open_with_seed` / `open_with_key`。
  - MDF 头字段修正：写入/读取解压后长度。
  - `examples/emote_psb_cli.rs`：info/dump/pack/unpack/repack/encode-mdf/decode-mdf/convert。
  - `cargo test` 通过，但没有专门的 MT19937 测试样本。
- 注意：
  - `MdfReader` 改为非泛型并整段解码，API 与上游不兼容。
  - MT19937 不是 krkrsdl3 的 emote 加密，二者是不同层。
  - snbk 不需要 MT19937；可作为 CLI 工具或后续可选模块，不建议作为 `krkr_emote` 的核心依赖。

## krkrsdl3 实现分析

### psbfile

- `PSBData`：header 解析、`isEncrypted` 启发式、`getHeaderLength`、`parsePSBArray` 越界检查。
- `PSBFile`：
  - 支持 PSB v1–v3；`version > 3` 直接拒绝。
  - 支持普通 MDF；`MFL` 只打印提示，实际不支持。
  - 不支持 seed/回调解密；加密文件只检测并失败。
  - 读取 v1 名称表（offsetEncrypt 处的索引 + offsetNames 处的 NUL 结尾字符串）和 v2+ 的 charset/namesData/nameIndexes 表。
  - 把整个 PSB 一次性转成 TJS 字典/数组；`Resource` 转 octet；`ExtraChunk` 只注册 media（v4 又被前面的版本检查拒绝）。
  - 注册全局 `PSBFile` 类和 `psb` 存储 media。
- `PSBMedia`：
  - 路径格式 `psb://<文件>/<chunk key>`，按第一个 `/` 分割。
  - 文件名大小写归一化；未加载的文件按需加载。
  - 资源按 offset/length 读取到内存流；`GetListAt`、`GetLocallyAccessibleName` 是 TODO。

### emoteplayer

- `emotefile` 是独立的 emote 专用 PSB 读取器：
  - 支持 v1–v4、普通 MDF、extra chunk。
  - 支持 seed 解密和 TJS 回调解密。
  - 支持 lzfs（LZ4 frame）解压。
  - 按 offset 懒解析成 `emoteframe`、`emotenode`、`emotemotion`、`emoteobject`、`emoteicon`、`emotesource` 等结构。
  - `emoteicon::ensureLoad` 按需解码 RL/调色板/RGBA8 并创建纹理。
- `emoteplayerclass` 提供 TJS 侧：
  - `Motion`、`ResourceManager`、`EmotePlayer`、`Player`、`SeparateLayerAdaptor`、`D3DAdaptor`。
  - 时间轴、变量、物理、网格、蒙版、混合、hit test、serialize/unserialize 等。
- `ResourceManager`：
  - 缓存 key 是 `TVPGetPlacedPath`；`lzfs://./` 前缀会去掉。
  - `setEmotePSBDecryptSeed` / `setEmotePSBDecryptFunc` 是静态设置。
- `motionplayer.dll`：9 行别名模块，内部 `LoadModule("emoteplayer.dll")`。

### drawdeviceD3D 与渲染

- `iTVPRenderBackend`（`core/render/TVPCompositor.h`）已经抽象：
  - `CreateTarget` / `SetTarget` / `ClearTarget`
  - `CreateTexture` / `UpdateTexture` / `LockTexture`
  - `SetMask`
  - `SetBlendMode`
  - `DrawMesh`
  - `LayerSetBlend` / `LayerDrawRect`
- `DrawDeviceD3D` 提供 `DrawDeviceD3D`、`D3D`、`D3DLayer`、`D3DImage`、`D3DPicture`、`D3DEmotePlayer` 等 TJS 类。
- `D3DEmotePlayer` 是 `emoteplayer::EmotePlayer` 的包装，使用 `iTVPRenderBackend` 完成网格/蒙版/纹理绘制。
- `D3DAdaptor.captureCanvas` 需要 GPU 目标拷贝/回读；bevy_krkr 只在脚本显式请求时做 readback，避免逐帧回读。

### 与 emote-psb 的差异

| 维度 | krkrsdl3 psbfile | krkrsdl3 emoteplayer | emote-psb | 备注 |
|---|---|---|---|---|
| PSB 版本 | v1–v3 | v1–v4 | v2–v4 | v1 需要补丁；v4 psbfile 拒绝 |
| MDF | 普通 zlib | 普通 zlib | 普通 zlib | 头字段语义需修正 |
| MFL | 不支持 | 不支持 | 不支持 | 与参考一致 |
| lzfs | 不支持 | LZ4 frame | 不支持 | 在 `krkr_emote` 容器层实现 |
| seed 解密 | 不支持 | 支持 | 不支持 | 在 `krkr_emote` 容器层实现 |
| TJS 回调解密 | 不支持 | 支持 | 不支持 | C++ 持有 TJS closure |
| 加密头启发式 | 有 | 有 | 无 | 需要移植 |
| v4 extra chunk | 拒绝 | 支持 | 支持 | `krkr_emote` 可使用 |
| 根值 | 直接 TJS 树 | offset 懒解析成 emote 结构 | `PsbValue`/serde | 需要语义层 |
| 资源 | 内联 octet + media offset | `ensureLoad` 按需 | `open_resource` 流 | emote 资源必须按需 |
| TJS 类/media | 内置 | 内置 | 无 | C++ 适配层 |
| 动画/物理/渲染 | 无 | 有 | 无 | `krkr_emote` + Bevy |
| 写入 | 无 | 无 | `PsbWriter`/`MdfWriter` | 非必需 |

## 加密与压缩链路

参考实现的加载顺序：

```text
Storage 流（xp3filter 已处理）
  → lzfs?（LZ4 frame 解压）
  → mdf?（zlib 解压）
  → 读入内存
  → 解析 PSB header
  → seed 解密（header 字段 / v2 body）
  → TJS 回调解密（可选）
  → 再解析 header
  → 仍加密则失败
  → 解析 names/strings/chunks/extra
  → 解析 root
```

### lzfs

- 检测 4 字节 LZ4 frame magic `04 22 4d 18`。
- 参考实现 `Lz4Stream::Lz4FrameInfo` 读取 flags、block size、content length、dictionary id，忽略 block/content checksum。
- 解压到内存后继续 MDF/PSB。
- `ResourceManager` 会去掉 `lzfs://./` 前缀；脚本侧的 `uselzfs` 分支也依赖该路径。
- Rust 侧可用 `lz4_flex` 的 frame 解码器，或移植参考的 block 解码；必须先验证与 `lzfs.dll` 的输出一致。

### MDF

- 普通 MDF：`mdf\0` + u32 + zlib 数据。
- 参考 psbfile/emoteplayer 把 u32 当**解压后大小**，`uncompress` 读取剩余字节。
- `emote-psb` 上游把 u32 当压缩长度做 `stream.take(size)`；snbk 的头值大于压缩流长度，因此仍能读完，但语义不正确。
- `hiroshil` fork 已改为按解压后大小写入/读取；`krkr_emote` 应采用该语义。
- MT19937 MDF shell 是可选扩展，不是 krkrsdl3 的 emote 加密。

### PSB seed 解密

`ResourceManager.setEmotePSBDecryptSeed(seed)` 设置静态 seed；加载时：

```text
key[0] = 0x075BCD15
key[1] = 0x159A55E5
key[2] = 0x1F123BB5
key[3] = seed
```

`emote_decrypt`：

```text
if v == 0:
    b = key[3]
    a = key[0] ^ (key[0] << 0xB)
    key[0] = key[1]
    key[1] = key[2]
    c = a ^ b ^ ((a ^ (b >> 0xB)) >> 8)
    key[2] = b
    key[3] = c
    v = c
    count = 4

data[i] ^= v & 0xff
v >>= 8
count--
```

需要解密的 header 字段：

- `offsetEncrypt`
- `offsetNames`
- `offsetStrings`
- `offsetStringsData`
- `offsetChunkOffsets`
- `offsetChunkLengths`
- `offsetChunkData`
- `offsetEntries`
- `version > 2`：`checksum`
- `version > 3`：`offsetExtraChunkOffsets`、`offsetExtraChunkLengths`、`offsetExtraChunkData`

`version == 2` 还会从 `offsetEncrypt` 到 `offsetChunkOffsets` 解密整个 body。

### 加密判定

参考实现使用 offset 启发式，而不是只看 `encrypt` 字段：

```text
isEncrypted =
    offsetEncrypt > MAX_LENGTH + 16 ||
    offsetNames == 0 ||
    (version > 1 && offsetEncrypt != offsetNames && offsetEncrypt != 0)
```

`getHeaderLength()` 返回 40/44/56。参考代码中的 `isEncrypted() && getHeaderLength() > fileSize` 条件看起来可疑，移植时需要用真实加密样本确认；不要只凭 `encrypt=1` 判断，否则会破坏实际未加密的文件。

### TJS 回调解密

`ResourceManager.setEmotePSBDecryptFunc(fn)` 保存 TJS closure。加载时：

1. 把文件读入可写内存；
2. 用 `CBinaryAccessor` 包成 TJS 二进制；
3. 调用 `fn(buffer, length)`，原地修改；
4. 重新解析 header。

新架构中：

- TJS closure、异常捕获和 `CBinaryAccessor` 留在 C++；
- Rust 侧只接受不抛异常的 `&mut [u8]` 回调 ABI；
- 回调失败返回错误码，不允许 C++ 异常或 Rust panic 跨 FFI；
- seed 解密和回调解密的顺序统一在 `krkr_emote::container::open` 中处理。

### PSBFile 与 emoteplayer 的差异

- `psbfile.dll` 的 `PSBFile` 不支持解密，只检测加密并失败。
- `emoteplayer.dll` 的 `emotefile` 支持 seed/回调解密。
- 默认保持该差异；如果希望 `PSBFile` 也支持解密，复用同一 crypto 模块即可，但这是对参考实现的扩展。

## 特性 parity 清单

### 容器与存储

- PSB v1/v2/v3/v4。
- v1 名称表、v2+ 名称表。
- 字符串表、普通资源、extra 资源。
- 普通 MDF；`MFL` 维持不支持。
- lzfs LZ4 frame。
- seed 解密、v2 body 解密、TJS 回调解密、加密头判定。
- `PSBFile`：构造(path/octet)、`load(path)`、`root`。
- `psb` media：`psb://`、大小写归一化、懒加载、资源打开；`GetListAt`/`GetLocallyAccessibleName` 维持参考的 TODO 行为。
- 可选：MT19937 MDF shell。

### Emote 语义

- `spec` → `isKrkr`/`isMotion`/`colorType`。
- `isMirror`、`src` 缺省、`color` 缺省。
- metadata、object、motion、node、frame、source、icon、selector、attrcomp。
- 主/差分时间轴、变量、blend ratio、fade in/out、loop、skip、pass。
- 物理：bust/hair/parts、wind、outer force。
- 网格：细分、combine/transform、模板蒙版。
- icon：`RL`/`none`、调色板、RGBA8、BGRA/RGBA 交换。

### TJS

- `Motion`：`getD3DAvailable`、`enableD3D`、`MaskModeAlpha`、`PlayFlagForce`、子类。
- `ResourceManager`：`load`、`unload`、`unloadAll`、`clearCache`、decrypt seed/func。
- `EmotePlayer` / `Player`：
  - 属性：`animating`、`playing`、`allplaying`、`loopTime`、`tickCount`、`speed`、`bustScale`、`hairScale`、`partsScale`、`outline`、`motionKey`、`motion`、`chara`、`variableKeys`、`tags`、`useD3D`。
  - 方法：`serialize`、`unserialize`、`play`、`initPhysics`、`clear`、`progress`、`draw`、`assign`、`setCoord`、`setScale`、`setRotate`、`setColor`、`setVariable`、`getVariable`、`setOuterForce`、`setDrawAffineTranslateMatrix`、`setCameraOffset`、`startWind`、`stopWind`、`contains`、`hitTest`、`skip`、`skipToSync`、`pass`、`stop`。
  - 时间轴：`playTimeline`、`stopTimeline`、`getTimelinePlaying`、`fadeInTimeline`、`fadeOutTimeline`、`setTimelineBlendRatio`、`getLoopTimeline`、`getTimelineTotalFrameCount`、`getMainTimelineLabelList`、`getDiffTimelineLabelList`、`getPlayingTimelineInfoList`、`getVariableFrameList`。
  - `Player` 额外：`getCommandList`、`getLayerGetter`、`getLayerMotion`、`setFlip`、`setSlant`、`setZoom`。
- `SeparateLayerAdaptor`：`absolute`、`isPrimary`、`parent`、`assign`、`clear`。
- `D3DAdaptor`：`setClearColor`、`captureCanvas`、`unloadUnusedTextures`。
- `motionplayer.dll` 别名模块。

### 渲染

- 渲染目标、纹理、`DrawMesh`、`SetMask`、混合模式。
- `D3DAdaptor.captureCanvas` 的 GPU 拷贝/按需回读。
- `D3DEmotePlayer`、`D3DLayer`、`D3DImage`、`D3DPicture`、`DrawDeviceD3D`。
- 前后平面、转场、`primaryLayers`、`children`、`setScreenRect`、`setPrimarySize`、`setOffset`、`recreate`、`capture`。
- Bevy 侧优先复用现有渲染命令，避免逐帧 CPU 回读。

## 推荐架构

```text
TJS 脚本
  → C++ EmotePlayer / PSBFile / Motion（ncbind）
      → krkr_shim C ABI
          → krkr_emote（Rust）
              → emote-psb（PSB/MDF 容器）
              → 加密/lzfs/MDF 预处理
              → emote 模型/动画/物理/网格/绘制列表
          → krkr_plugin::render（Bevy GPU）
```

### 新增 crate：krkr_emote

职责：

- `container`：Storage 流读入、lzfs、MDF、PSB 解密、header 判定。
- `psb`：`emote-psb` 包装、v1 补丁、资源访问。
- `crypto`：seed XOR、v2 body、header 字段。
- `model`：EmoteFile、motion/node/frame/source/icon/selector。
- `runner`：时间轴、变量、blend、fade、loop、skip。
- `physics`：bust/hair/parts、wind、outer force。
- `mesh`：细分、变换、模板蒙版。
- `draw`：与渲染后端无关的 draw list。
- `icon`：RL/调色板/RGBA8。

约束：

- 不依赖 TJS、Bevy。
- 资源按需读取，不 eager 复制全部 `.mtn`/`.pimg` 资源。
- 对象顺序需要与参考实现的 `std::map` 语义核对；必要时使用 `BTreeMap`/`IndexMap`。

### krkr_shim FFI

按完整操作设计，不逐项映射 TJS：

- `krkr_emote_open(KrkrStream*, options)`：容器预处理 + 解析。
- `krkr_emote_read_resource(...)`：按索引/键读取资源。
- `krkr_emote_player_create/apply/advance/hit_test/serialize`。
- `krkr_emote_player_draw(...)`：输出 draw list。
- 回调解密使用不抛异常的 C ABI。

### C++ TJS 适配

- 在 `Kirikiroid2/src/core/environ/bevy/plugins` 新增 `EmotePlayer.cpp`、`PsbFile.cpp`、`MotionPlayer.cpp`。
- 在 [codegen/src/port.rs](../codegen/src/port.rs) 注册 `emoteplayer.dll`、`motionplayer.dll`、`psbfile.dll`。
- 复用现有 ncbind 注册、Storage 流和 `krkr_shim` 调用方式。
- TJS 类生命周期、参数转换、异常捕获留在 C++。

### Bevy 渲染

- 在 [crates/krkr_plugin/src/render](../crates/krkr_plugin/src/render) 扩展渲染命令：
  - target 创建/绑定/清屏；
  - texture 创建/更新/采样；
  - mesh 顶点/索引；
  - mask；
  - blend；
  - 可选 GPU 拷贝/按需回读。
- 纹理只在 icon 首次加载时上传；每帧只提交 draw list。
- `captureCanvas` 只在脚本显式调用时执行 GPU copy/readback。

## 实施顺序

1. **PSBFile + `psb` media + 普通 MDF**
   - 解除 snbk 启动时 `Member "PSBFile" does not exist`。
   - 验证 92 个 MDF `.scn`、141 个 `.pimg`。

2. **容器兼容层**
   - lzfs、PSB v1/v2/v3/v4、seed/回调解密、header 判定、MDF 头语义修正。
   - 可选 MT19937 MDF。

3. **krkr_emote 模型**
   - spec/isKrkr/isMotion/colorType/isMirror。
   - icon RL/调色板/RGBA8。
   - metadata/object/motion/node/frame/source/icon/selector。

4. **动画与物理**
   - 时间轴、变量、blend/fade/loop/skip。
   - physics、mesh、mask/stencil、draw list。

5. **C++ TJS 适配**
   - 6 个类和全部方法/属性。
   - ResourceManager 缓存、`lzfs://./`、decrypt seed/func。

6. **Bevy 渲染**
   - target/texture/DrawMesh/SetMask/blend。
   - captureCanvas 按需 copy/readback。

7. **D3D 兼容层**
   - `D3DAdaptor`、`D3DEmotePlayer`、`DrawDeviceD3D` 的脚本面。
   - 能用 Bevy 后端就不搬 D3D。

8. **剩余 API**
   - serialize/unserialize、hitTest、getCommandList、getLayerGetter、setFlip/setSlant/setZoom 等。

## 验收

- 容器：PSB v2/v3 真实样本；v1/v4、加密、lzfs、MT19937 使用构造或真实样本。
- snbk：92 个 MDF `.scn`、33 个 `.mtn`、141 个 `.pimg` 全部能打开；`PSBFile.root` 结构正确。
- emote：同一 `.mtn` 在参考实现和 `krkr_emote` 中比较帧时间、节点变换、draw list、hit test。
- 渲染：参考 software 后端做 golden；Bevy 端截图/帧对比。
- TJS：用 `D3DaffineSourceEmote.tjs`、`psdlayer.tjs`、`StorageData.tjs` 跑通；核对 `Motion`/`EmotePlayer`/`PSBFile` 属性和方法。
- 性能：`.mtn`/`.pimg` 资源按需加载；无逐帧 GPU 回读。

## 风险与待确认

- 参考实现的加密判定 `getHeaderLength() > fileSize` 可疑，需要真实加密样本确认；不要只凭 `encrypt` 字段解密。
- `emote-psb` 不支持真实 PSB v1 名称表和 seed/回调；需要在 `krkr_emote`/fork 中补。
- `emote-psb` 的 MDF 头语义与参考不同；必须修正，不能依赖 `take(size)` 的偶然可用。
- lzfs 与 `lz4_flex` frame 的兼容性需要用真实样本验证。
- MT19937 MDF shell 没有专门测试；只按真实 `.psb.m` 样本验收。
- 参考实现的对象顺序来自 `std::map`；`PsbValue` 的 `HashMap` 顺序不同，涉及列表输出的 API 需要核对/排序。
- TJS 回调解密跨 FFI 时必须禁止异常/panic 穿越；失败返回错误码。
- 参考实现的 `PSBMedia::GetListAt`、`GetLocallyAccessibleName` 是 TODO；不要超出参考行为宣称兼容。

## 参考

- [emote-psb](https://github.com/storycraft/emote-psb-rs)
- [yuzu-psb](https://github.com/lilyco-42/lyco-engine)
- [hiroshil/emote-psb-rs](https://github.com/hiroshil/emote-psb-rs)
- `~/Workspace/krkrsdl3/plugins/psbfile`
- `~/Workspace/krkrsdl3/plugins/emoteplayer`
- `~/Workspace/krkrsdl3/plugins/DrawDeviceD3D`
- `~/Workspace/krkrsdl3/core/render/TVPCompositor.h`
- [插件迁移清单](plugin-migration.md)
- [AGENTS.md](../AGENTS.md)
