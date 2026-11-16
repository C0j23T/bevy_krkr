# 视频纹理导入进展

## 目标

硬件解码帧优先交给 Bevy 使用的 wgpu 设备处理，避免整帧读回 CPU；能力探测或单帧导入失败时，继续使用 CPU RGBA 上传。视频的合成与色彩控制仍通过项目自建 renderer 的 `VideoColor` 操作完成。

## 当前数据路径

硬件解码开始时，worker 只读取一帧原生帧用于探测，不生成 CPU RGBA。GPU 导入成功后确认共享路径并恢复最多四帧预取；某一帧导入失败时丢弃该帧，清理已排队的原生帧，并从下一帧开始执行 `av_hwframe_transfer_data` 和 `sws`。软件解码直接生成 CPU RGBA。帧交接结构 `Frame` 同时携带可选 CPU 数据和可选原生帧。

渲染入口位于 `crates/krkr_plugin/src/render/video.rs`。CPU 路径创建 RGBA wgpu texture 并用 `RenderQueue::write_texture` 上传。视频绘制复用 `crates/krkr_plugin/src/render/renderer.rs` 中的 `VideoColor` 操作，而不是另建 Bevy 图像管线。

## 平台状态

| 平台 | 当前状态 | 条件与限制 |
| --- | --- | --- |
| Linux | 通过 `wgpu_external_frame` 实现 Vulkan DMA-BUF 导入尝试 | FFmpeg 帧必须能映射为 DRM_PRIME，且 DRM layer 必须是单平面打包 32 位格式。项目接受库支持的 ARGB/XRGB/ABGR/XBGR fourcc；X 格式会在 `VideoColor` 中强制 alpha 为不透明。库在 GPU 内复制到 wgpu 自有纹理并完成同步，导入失败会走 CPU 上传。NV12/多平面不受当前库的 DMA-BUF 帧接口支持。尚未在实际 VAAPI 设备上验证。 |
| iOS | CPU 回退 | 已保留 VideoToolbox 原生帧引用；`CVPixelBuffer` / `CVMetalTextureCache` 到 wgpu Metal texture 的导入尚未实现。探测不会把 Metal 后端存在误报为视频共享可用。 |
| Android 8.0+ | CPU 回退 | `wgpu_external_frame` 提供 AHardwareBuffer 导入，但要求创建 wgpu Vulkan device 时启用额外 Vulkan 扩展；当前 Bevy device 创建配置没有接入这些要求。还需确认 FFmpeg MediaCodec 输出能否提供 `AHardwareBuffer`，再接入 importer。 |

能力探测在渲染初始化时写入日志。探测结果是后端/feature 级筛选，不代表具体帧一定能导出或导入；Linux 的格式及句柄条件仍逐帧验证。

## 生命周期与风险

Linux 使用 `wgpu_external_frame::dma_buf::DmaBufImporter::copy_to_texture`：DMA-BUF 被导入后在 GPU 内复制到 wgpu 自有纹理，调用会等待复制提交完成后再释放源 buffer。这个路径避免 CPU 回读，但有一次 GPU 内复制，因此不是同一原生纹理的零拷贝采样。

目前 Linux 覆盖该库支持的四种单平面打包 32 位 DRM fourcc。常见 NV12 输出需要库提供多平面导入，或另行实现 Vulkan 多平面导入与 GPU YUV 转换，再交给现有 `VideoColor` 合成。FFmpeg 输出不能映射为 DRM_PRIME、错误句柄、modifier 或驱动不支持的组合仍会回退 CPU。

CPU RGBA 只在软件解码或 GPU 导入失败后的下一帧生成。Linux 导入成功时不发生硬件帧回读、颜色转换或 CPU texture 上传；探测失败帧和回退切换时已经排队的原生帧按要求丢弃。

## 验证

- `cargo check -p krkr_plugin --features video-ffmpeg --offline`：通过。
- `cargo test -p krkr_plugin --features video-ffmpeg --lib video::decoder::tests::stream_selection_changes_decoded_video_and_audio --offline`：通过。
- `git diff --check`：通过。
- Android `aarch64-linux-android` 交叉检查：未完成，构建环境缺少该目标的 FFmpeg 库及交叉编译 pkg-config 配置。
- iOS 编译与设备验证：当前环境没有已安装的 iOS Rust target，且原生纹理导入尚未实现。
- Linux DMA-BUF 实际导入、渲染输出及 GPU 同步：尚未在 VAAPI 硬件上验证。

## 后续工作

1. Linux：把 RGBA 转换延迟到 DMA-BUF 导入失败后；以真实 VAAPI 帧验证四种格式、modifier 与 GPU 同步。NV12 多平面需等待依赖支持或增加专用 Vulkan/YUV 路径。
2. Android：为 Bevy Vulkan device 创建接入 `wgpu_external_frame::ahardware_buffer::DeviceRequirements`，确定 MediaCodec 输出 AHardwareBuffer 的 FFmpeg 接口路径，再实现逐帧导入与格式转换；失败时 CPU 回退。
3. iOS：从 VideoToolbox `CVPixelBuffer` 取得 IOSurface，使用 `io_surface` importer 导入 YUV 平面，管理 CoreVideo 对象和 GPU 使用生命周期；导入失败继续走 CPU 上传。
4. 为共享路径增加平台目标编译、实际帧像素对照、分辨率变化、seek/stream 切换和连续播放压力验证。
