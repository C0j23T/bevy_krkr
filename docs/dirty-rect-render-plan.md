# 脏矩形渲染与空转验收

## 场景合成

`krkr_scene_build` 每次返回当前场景的 GPU 纹理 ID 和尺寸。窗口、primary layer 和尺寸稳定时，`TVPBevyScene` 保留同一张目标纹理。没有待处理的 `onPaint` 或活动转场时，静止帧通过计数跳过层树检查。没有脏区时，场景构建直接复用目标，暂停的视频也复用已经合成的画面。

普通 Layer 更新通过 `tTVPLayerManager::UpdateRegion` 收集。构建将其包围盒裁剪到 primary layer，把与脏区相交的子树合成到按区域尺寸分配的临时纹理。完全覆盖叶子层的主图可以直接作为区域采样来源。按区域分配也把 Rust 写时复制产生的中间拷贝限制在临时纹理的尺寸内。最终通过原地 GPU copy 写回持久目标的对应区域，保留区域外的像素。移动层依赖 manager 同时登记旧位置和新位置。

首次构建、场景身份或尺寸变化、缺少 manager、活动转场和全局失效都会触发全屏合成。视频帧、播放状态和 bounds 变化通过全局失效触发更新，视频 overlay 在层树写回后合成。构建失败会标记下一帧重新构建。常规合成和展示均在 GPU 上执行。

## Bevy 纹理与窗口

`show_scene` 在 GPU 纹理和采样器身份稳定时复用已绑定的 `GpuImage`；窗口尺寸变化只调整展示布局。新的场景尺寸创建对应的 Bevy 图像资产，底层 GPU 纹理变化会更新绑定。同尺寸的普通 Layer 更新原地写入目标，窗口帧仍由 Bevy 正常渲染和提交。

1000 FPS 对应 1 ms 的窗口帧预算。验收测量 Bevy 的窗口循环，使用同尺寸的纯 Bevy 静态场景作为基线。GPU、交换链和宿主调度共同影响结果。

## 验证

```sh
cargo test --test render --features test-render-stats
cargo test --test video --test window --test restart_memory --features test-render-stats

export CARGO_PROFILE_RELEASE_OPT_LEVEL=3
export CARGO_PROFILE_RELEASE_LTO=false
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16
WGPU_BACKEND=vulkan cargo run --release --example empty --features test-render-stats -- --measure
WGPU_BACKEND=vulkan cargo run --release --example empty --features test-render-stats -- --krkr --measure
```

`empty` 使用 800×600 窗口、连续更新、`AutoNoVsync` 和关闭 MSAA 的 2D 相机。先预热 2 秒，再按至少 5 秒的实际帧数计算平均 FPS。给 Kirikiri 测速加上 `--features test-render-stats` 后，会校验测量期间的绘制、合成、写回、逻辑纹理创建和 Bevy 图像更新计数保持不变。

局部填充用 64×48 的场景验证。2×2 更新只合成并写回 4 个像素，同时验证写时复制面积低于全屏。像素回归还覆盖移动前的位置清除、隐藏层、嵌套透明度、`onPaint`、文本、转场和场景尺寸变化。

## RTX 4060 实测

NVIDIA GeForce RTX 4060 Laptop GPU，驱动 620.70.06，Linux Wayland，Vulkan。以 800×600 实体窗口运行上面的优化构建。

| 场景 | 第一次 | 第二次 |
| --- | ---: | ---: |
| 纯 Bevy | 1,075.1 FPS | 1,054.6 FPS |
| Kirikiri | 1,024.9 FPS | 1,248.3 FPS |

两次 Kirikiri 采样分别记录了 5,125 和 6,242 帧，均达到平均 1000 FPS。采样期间场景绘制、合成、写回、逻辑纹理创建和 `GpuImage` 更新计数增量均为 0。
