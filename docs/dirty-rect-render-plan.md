# 方案 B：脏矩形局部合成计划

## 目标

方案 A 已经让静态场景跳过整棵可见层树的重建。方案 B 在此基础上，只重建本帧受影响的区域，降低文本、局部 UI、局部层动画更新时的 GPU 合成工作量。

方案 B 的目标是保持现有 Layer 的绘制语义、透明度和转场结果不变，同时避免每次更新都重新生成整张 primary layer 纹理。场景切换、全屏更新、视频帧和转场可以退化为接近全屏合成，不要求所有帧都获得局部化收益。

## 方案边界

- **方案 A：按 dirty 决定是否重建。** 每帧仍可调用 `krkr_scene_build`，但静态场景直接返回持久目标；普通层更新、转场和视频等 dirty 信号到达时才重建整棵可见层树。它利用 `tTVPLayerManager::UpdateRegion` 判断是否需要更新，并补充纹理失效、视频状态和场景切换信号。
- **方案 B：按 dirty 区域重建和合成。** 在 A 的缓存生命周期上读取 `UpdateRegion` 包围盒，只处理与区域相交的层内容，并把该区域写回同一个目标纹理。全屏变化和不可靠区域继续退化为全屏。
- **方案 C：恢复原版逐层 `CacheBitmap`/复杂区域绘制链。** 本计划不采用 C。端口已删除约 6000 行原版绘制代码；恢复它会带回逐层缓存、失效和资源管理成本，原版 GPU 路径本身也没有按 `CacheRecalcRegion` 局部重绘。

A 是静态帧避免重建的基础，B 是减少更新帧 GPU 工作量的后续优化；两者解决不同的成本。本文只实现和验收 B，不把 A 已有行为算作 B 的区域收益。

## 范围与约束

- C++ 继续负责 Layer/TJS 状态、更新区域和坐标适配；Rust 只负责 GPU 纹理、命令编码和目标纹理生命周期。
- 继续使用 `tTVPLayerManager::UpdateRegion`，不恢复原版已经删除的逐层 `CacheBitmap` 绘制链。
- 脏区先取 primary layer 坐标系中的包围盒，再进行一次合成；第一版不处理多个离散矩形。
- 不把 GPU 像素读回 CPU，也不通过 CPU 重建整张目标图。
- 方案 A 的静态缓存、活动转场强制更新、视频和窗口 dirty 信号继续有效。

## 当前基线

当前 `TVPBevyBuildScene` 在需要更新时调用 `TVPBevyScene::Build`，为需要合成的层创建临时纹理，再通过 `operation = 5` 合成。Rust `KrkrRenderer::draw` 对绘制命令采用写时复制，会为目标生成新的 GPU 纹理并替换纹理表中的旧值。

因此，局部合成不能只给 C++ 增加区域参数：必须同时解决以下两个问题：

1. 目标纹理需要跨帧保持，才能保留未受影响区域。
2. Rust 合成命令必须只写目标脏区，并保留目标其余区域。

## 当前实现进度

- Rust `operation = 7` 按 source rect 到 destination rect 执行原地 GPU copy；命令与绘制在同一个 command encoder 中编码，目标纹理表项不变。
- `TVPBevyScene::Target` 在场景身份和尺寸不变时持久复用。`TVPBevyBuildScene` 将 `UpdateRegion` 包围盒裁剪到 primary layer 范围，只构建并复制目标脏区。
- 局部更新会跳过与 primary dirty bbox 完全不相交的子树及其合成命令。相交层在完整尺寸中间纹理上只填充、复制和合成 local dirty；活动转场、场景 dirty、目标变化和缺少 manager 走全屏路径。
- `RenderStats` 累积记录场景 copy、composite、draw 的像素面积及 texture create 命令。渲染集成测试已检查静态帧无 copy/composite，移动层与局部填充的 copy/composite 面积小于全屏。
- Rust 区域 copy 和 composite 子矩形 GPU 像素测试、端到端 layer 像素测试通过。C++ 子模块通过本机 Linux C++ 编译；尚未完成视频、transition 和跨平台构建验证。

## 设计

### 1. 脏区收集与生命周期

在 `TVPBevyBuildScene` 中：

1. 执行现有 `Prepare`，让 `onPaint` 等同步回调先产生更新。
2. 从 primary layer manager 读取 `UpdateRegion`。
3. 将复杂区域合并为包围盒，并裁剪到 primary layer 的 `[0, 0, width, height)`。
4. 将包围盒保存到本次 build 状态；决定开始重建后立即清空 manager 的 `UpdateRegion`。
5. 若构建失败，恢复全局 dirty，下一帧执行完整重建。

以下情况直接使用全屏脏区：

- 首次构建、窗口或 primary layer 指针变化、尺寸变化；
- 没有 manager；
- `TVPBevyInvalidateTexture` 无法关联到明确的层区域；
- 活动转场、视频帧或视频 bounds 变化；
- 脏区为空但场景状态要求重建。

删除层、移动层和可见性变化必须把旧位置和新位置都并入脏区，否则目标纹理会保留旧像素。第一版可以在这些更新无法可靠提供旧区域时退化为全屏脏区。

### 2. C++ 层树构建

给 `TVPBevyScene::Build` 增加 primary dirty bbox 和各层在 primary 坐标中的原点：

- 以子层位置和尺寸判断它是否与 primary dirty bbox 相交；
- 不相交时跳过该子树的递归构建和合成；
- 相交层仍使用完整尺寸中间纹理，但只填充 local dirty，只复制 `ImageLeft/ImageTop` 覆盖区域，并按 source/destination 子矩形合成子层；
- 透明层和 binder 层保留现有背景与合成语义。

第一版不尝试复用每层的中间结果或缩小中间纹理。若某个父层的变换、透明度或显示状态改变，继续重建该脏区内内容；只有明确不相交的分支才跳过。

### 3. 持久目标纹理

`TVPBevyScene::Target` 在尺寸不变时必须保持同一个 `iTVPTexture2D` 和 texture ID。每次局部更新前：

1. 确认目标纹理已经存在且尺寸匹配；尺寸变化时丢弃并重新创建。
2. 将本次构建结果写回目标纹理的对应区域。该结果包含脏区内的透明背景重建，覆盖即可清除透明层删除后的旧像素。

不能使用现有 `Assign` 或完整 `CopyRect` 作为目标复用协议，因为它们可能替换底层纹理对象。Rust 的专用 `operation = 7` 把源纹理区域复制到既有目标纹理的指定区域，目标纹理不从 renderer texture map 中替换。

### 4. Rust 合成接口

当前 `operation = 7` 是原地纹理区域 copy，按一次 copy 操作传递：

- source texture ID；
- persistent target texture ID；
- source rectangle；
- destination rectangle。

Layer blend 继续由 `operation = 5` 完成，随后 copy 到持久目标。`renderer.rs` 中：

- operation 5 在提供 source rectangle 时按该区域采样，未提供时沿用全 source 默认行为；
- GPU copy 和 draw 按同一 `CommandEncoder` 顺序编码；
- 临时源纹理由 renderer 的延迟回收池持有，避免编码后立即复用。

后续若引入需要多阶段计算的持久目标写入，应继续采用完整操作接口，不逐项暴露 wgpu API。

### 5. 转场与视频

- 活动转场第一版继续使用全屏脏区，先保证时间推进和末帧一致性。
- 视频帧更新继续触发 dirty；视频合成区域变化时至少清理旧 bounds 和新 bounds 的并集。
- 视频播放、停止、隐藏、暂停、seek、切换窗口和销毁仍然触发场景更新。
- 后续若 profiling 证明视频是主要成本，再单独设计视频 overlay 的局部合成，不与第一版脏层逻辑混合。

## 分阶段实施

### 阶段 0：基线和观测

- 为 `TVPBevyBuildScene`、C++ `Build`、Rust draw/composite 和 GPU copy 增加计数器或 trace。
- 记录脏区面积、目标尺寸、临时纹理数量和每帧合成命令数。
- 固定方案 A 的静态、层更新、转场和视频回归测试。

验收：静态场景没有 build；局部层更新能记录小于全屏的脏区；现有像素结果不变。

### 阶段 1：持久目标与原地 copy

基础实现已完成，阶段验收尚未完成。

- [x] 新增 Rust 原地 GPU copy 操作。
- [x] 让 C++ 目标纹理在尺寸不变时保持稳定 ID。
- [x] 实现全屏 fallback 和局部区域写回。
- [ ] 验证连续帧场景像素、材质绑定和尺寸变化。

验收：动态更新不再因目标纹理替换而强制重绑材质；连续帧像素与方案 A 一致；尺寸变化正确重建目标。

### 阶段 2：包围盒脏区

已接入基础区域路径；像素回归和工作量验收尚未完成。

- [x] 接入 `UpdateRegion` 包围盒和裁剪。
- [x] 为层树递归增加区域相交判断，跳过不相交子树。
- [x] 将相交层的填充、主图复制、子层合成限制到 local dirty，并只写回目标脏区。
- [x] 验证局部填充、移动层旧位置清除和隐藏组旧区域清除的像素结果。
- [ ] 验证透明度、连续更新、视频和 transition 像素结果。

验收：代表性的移动与填充更新已通过像素和面积断言；透明度、文本、视频、transition、多次连续更新仍需补充验证。

### 阶段 3：扩展与优化

- 根据 profiling 决定是否支持多个脏矩形，而不是默认增加复杂度。
- 评估转场是否可以按区域处理；没有明确收益时保持全屏路径。
- 评估视频 overlay 是否单独维护目标区域。
- 清理 trace 和仅用于实验的开关，保留必要的运行时诊断指标。

## 测试计划

测试按子系统分批运行：

- C++/Rust 接口：目标尺寸变化、原地 copy、纹理池生命周期和错误回退；
- Layer：局部填充、文本绘制、层移动、隐藏/删除、透明度、子层顺序；
- 转场：起始帧、中间帧、结束帧和 callback/selfupdate；
- 视频：播放帧、暂停、停止、bounds 变化和窗口切换；
- 宿主：连续静态帧 texture ID 稳定，场景切换后不复用错误目标。

像素断言之外，至少记录并检查：

- 脏区包围盒是否被正确裁剪；
- 脏区外像素是否保持不变；
- 删除或移动后的旧区域是否被清除；
- 纹理 ID、材质绑定和临时纹理数量是否符合预期。

## 风险与停止条件

- 坐标转换、透明层清除或转场区域处理出现任何可见残留时，立即退回全屏脏区，不继续扩大局部优化范围。
- 若原地目标写入需要改动 renderer 的资源所有权模型，先单独提交并验证，不把未验证的纹理池改动与 C++ 区域裁剪混在一个提交中。
- 若 profiling 显示大多数更新本身接近全屏，保留方案 A，停止继续实现多个离散矩形。
- 当前 dirty 标记假设场景构建与引擎更新在同一串行边界；若引入后台纹理更新，必须先增加 generation/pending 机制，不能静默丢失构建期间的更新。

每个阶段独立提交并通过对应测试后再进入下一阶段。方案 B 的最终验收以像素正确性、无旧像素残留和可测量的 GPU 工作量下降同时满足为准。
