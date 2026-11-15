# 视频纹理导入进展

## 目标

硬件解码帧优先直接交给 Bevy 使用的 wgpu 设备采样，避免把整帧读回 CPU；能力探测或单帧导入失败时，继续使用 CPU RGBA 上传。视频的合成与色彩控制仍通过项目自建 renderer 的 `VideoColor` 操作完成。解码、时间戳、缩放和颜色空间转换逻辑保持现状。

## 当前数据路径

`Decoder::next` 仍为每个视频帧生成 CPU RGBA，作为可靠回退。同时尝试保留原生硬件帧：Linux 上调用 `av_hwframe_map` 请求 DRM_PRIME 映射；iOS 上通过 `av_frame_ref` 保留 VideoToolbox 帧。帧交接结构 `Frame` 同时携带 RGBA 和可选原生帧。

渲染入口位于 `crates/krkr_plugin/src/render/video.rs`。CPU 路径创建 RGBA wgpu texture 并用 `RenderQueue::write_texture` 上传。视频绘制复用 `crates/krkr_plugin/src/render/renderer.rs` 中的 `VideoColor` 操作，而不是另建 Bevy 图像管线。

## 平台状态

| 平台 | 当前状态 | 条件与限制 |
| --- | --- | --- |
| Linux | 已实现 DMA-BUF 导入尝试 | 要求 Vulkan adapter/device 支持并启用 `VULKAN_EXTERNAL_MEMORY_DMA_BUF`，FFmpeg 帧必须能映射为 DRM_PRIME，且 DRM layer 必须是单平面 ABGR8888（内存字节序 RGBA）。导入失败、NV12/多平面或其他格式会走 CPU 上传。尚未在实际 VAAPI 设备上验证。 |
| iOS | CPU 回退 | 已保留 VideoToolbox 原生帧引用；`CVPixelBuffer` / `CVMetalTextureCache` 到 wgpu Metal texture 的导入尚未实现。探测不会把 Metal 后端存在误报为视频共享可用。 |
| Android 8.0+ | CPU 回退 | 当前 wgpu-hal 集成没有 AHardwareBuffer 导入路径；MediaCodec 输出与 Vulkan/wgpu 的 buffer 桥接尚未实现。 |

能力探测在渲染初始化时写入日志。探测结果是后端/feature 级筛选，不代表具体帧一定能导出或导入；Linux 的格式及句柄条件仍逐帧验证。

## 生命周期与风险

Linux 每帧导入 FFmpeg 导出的 DMA-BUF，并在 `VideoTexture` 中暂存最近四个原生帧引用，降低解码器过早复用底层 buffer 的风险。这是临时生命周期保护，不是 GPU 完成信号；在高延迟或不同步的驱动上仍需改成基于队列完成通知的回收机制，并验证 VAAPI 与 Vulkan 的外部同步约定。

目前 Linux 只处理一层一平面的 RGBA 类帧。常见 NV12 输出需要导入多个平面并增加 GPU YUV 转换，再交给现有 `VideoColor` 合成。多对象、多层、不同 DRM modifier 和颜色元数据也尚未覆盖。

硬件解码帧的 CPU RGBA 备用数据仍每帧生成，因此 Linux 导入成功时虽然跳过 RGBA texture 上传，解码侧的硬件帧回读和颜色转换开销目前仍存在。要真正消除该开销，需在导入成功时避免 `av_hwframe_transfer_data` 与 `sws` 转换，或将 CPU 回退延迟到 GPU 导入失败后按需生成。

## 验证

- `cargo check -p krkr_plugin --features video-ffmpeg --offline`：通过。
- `git diff --check`：通过。
- Android `aarch64-linux-android` 交叉检查：未完成，构建环境缺少该目标的 FFmpeg 库及交叉编译 pkg-config 配置。
- iOS 编译与设备验证：当前环境没有已安装的 iOS Rust target，且原生纹理导入尚未实现。
- Linux DMA-BUF 实际导入、渲染输出及 GPU 同步：尚未在 VAAPI 硬件上验证。

## 后续工作

1. Linux：支持 NV12 多平面 DMA-BUF 导入和 GPU YUV 转换；以真实 VAAPI 帧验证 modifier、plane layout、颜色范围及同步，并用 GPU 完成事件替换固定长度帧保留。
2. iOS：从 VideoToolbox `CVPixelBuffer` 创建 `CVMetalTexture`，包装进当前 wgpu Metal device，管理 CoreVideo 对象与 GPU 使用完成之间的生命周期；导入失败继续走 CPU 上传。
3. Android：确定 MediaCodec 输出到 `AHardwareBuffer` 的 FFmpeg 接口路径，实现 Vulkan import 并按设备、格式和使用标志运行时探测；不支持时维持 CPU 回退。
4. 为共享路径增加平台目标编译、实际帧像素对照、分辨率变化、seek/stream 切换和连续播放压力验证。
