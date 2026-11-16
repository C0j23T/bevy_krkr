# 视频纹理导入进展

## 目标

硬件解码帧优先交给 Bevy 使用的 wgpu 设备处理，避免整帧读回 CPU；能力探测或单帧导入失败时，继续使用 CPU RGBA 上传。视频的合成与色彩控制仍通过项目自建 renderer 的 `VideoColor` 操作完成。

## 当前数据路径

硬件解码开始时，worker 只读取一帧原生帧用于探测，不生成 CPU RGBA。GPU 导入成功后确认共享路径并恢复最多四帧预取；某一帧导入失败时丢弃该帧并清理已排队的原生帧。Android 随后重开软件解码器，其他支持硬件帧回读的平台使用 `av_hwframe_transfer_data`，再经 `sws` 生成 CPU RGBA。帧交接结构 `Frame` 同时携带可选 CPU 数据和可选原生帧。

渲染入口位于 `crates/krkr_plugin/src/render/video.rs`。CPU 路径创建 RGBA wgpu texture 并用 `RenderQueue::write_texture` 上传。视频绘制复用 `crates/krkr_plugin/src/render/renderer.rs` 中的 `VideoColor` 操作，而不是另建 Bevy 图像管线。

## 平台状态

| 平台 | 当前状态 | 条件与限制 |
| --- | --- | --- |
| Linux | 通过 `wgpu_external_frame` 实现 Vulkan DMA-BUF 导入尝试 | FFmpeg 帧必须能映射为 DRM_PRIME，且 DRM layer 必须是单平面打包 32 位格式。项目接受库支持的 ARGB/XRGB/ABGR/XBGR fourcc；X 格式会在 `VideoColor` 中强制 alpha 为不透明。库在 GPU 内复制到 wgpu 自有纹理并完成同步，导入失败会走 CPU 上传。NV12/多平面不受当前库的 DMA-BUF 帧接口支持。尚未在实际 VAAPI 设备上验证。 |
| iOS | VideoToolbox → `CVPixelBuffer` → IOSurface → wgpu Metal 已实现 | 导入双平面 YCbCr 纹理并在 GPU 转为 RGBA；原生帧引用随 GPU 纹理生命周期保留。仅支持 VideoToolbox 导出的 IOSurface 双平面 4:2:0；格式或导入不支持时回退 CPU。需要 Metal 后端。 |
| Android 8.0+ | MediaCodec → ImageReader → AHardwareBuffer → wgpu 已实现，Adreno 750 真机导入与画面已验证 | Vulkan 1.1、所需外部内存/同步扩展及 samplerYcbcrConversion 可用时启用。ImageReader 请求 YUV_420_888；RGBA 导入可直接采样，YCbCr 平面在 GPU 按驱动报告的 BT.601/709/2020 与 full/video range 转为 RGBA。初始化、尺寸、格式或导入不支持时回退 CPU。实际导入与回退通过逐播放器计数日志确认。 |

能力探测在渲染初始化时写入日志。探测结果是后端/feature 级筛选，不代表具体帧一定能导出或导入；Linux 的格式及句柄条件仍逐帧验证。

Android 启用硬件导入时，将 KrkrVideoPlugin 加在 DefaultPlugins 之前，使 Vulkan 扩展回调早于设备创建。晚注册或手工创建的设备缺少扩展时只使用 CPU 路径，不重新创建 Bevy 设备。

Android ImageReader 返回的 acquire fence 由 importer 在 GPU 上等待，不在 CPU poll。lease 同时保留 Image 与 ImageReader，直到 importer 确认 GPU 使用结束。YUV 平面转换复用缓存的 compute pipeline 与矩阵 uniform，转换结果通过既有 VideoColor 合成。只有成功创建 ImageReader 输出 Surface 后才启用 MediaCodec，初始化失败时使用软件解码。切换 CPU 时丢弃导入纹理并创建可上传的新纹理；Surface 解码无法直接读回时重开软件解码器。seek 会排空 Surface 队列中的旧图像，并丢弃目标时间之前的解码帧。

ImageReader 帧保留 `AImage_getCropRect`，允许底层 AHardwareBuffer 尺寸大于可见视频尺寸，例如 800×600 视频使用 800×608 缓冲区。YCbCr 转换在 GPU 上应用裁剪偏移并只输出可见区域；RGBA 别名需要裁剪时使用 GPU 区域复制。无效裁剪会记录尺寸信息后回退。`vendor/video/wgpu-external-frame` 保留 0.4.0 源码，并增加 Qualcomm 8 位 NV12 UBWC（`0x7fa30c06`）的 external-format 导入支持，仍拒绝未知格式和 10 位格式，补丁说明见该目录的 `PATCHES.md`。

## 生命周期与风险

Linux 使用 `wgpu_external_frame::dma_buf::DmaBufImporter::copy_to_texture`：DMA-BUF 被导入后在 GPU 内复制到 wgpu 自有纹理，调用会等待复制提交完成后再释放源 buffer。这个路径避免 CPU 回读，但有一次 GPU 内复制，因此不是同一原生纹理的零拷贝采样。

目前 Linux 覆盖该库支持的四种单平面打包 32 位 DRM fourcc。常见 NV12 输出需要库提供多平面导入，或另行实现 Vulkan 多平面导入与 GPU YUV 转换，再交给现有 `VideoColor` 合成。FFmpeg 输出不能映射为 DRM_PRIME、错误句柄、modifier 或驱动不支持的组合仍会回退 CPU。

CPU RGBA 只在软件解码或 GPU 导入失败后的下一帧生成。Linux 导入成功时不发生硬件帧回读、颜色转换或 CPU texture 上传；探测失败帧和回退切换时已经排队的原生帧按要求丢弃。

## 验证

- `cargo test -p krkr_plugin --features video-ffmpeg --lib video --offline`：9 项通过，包括实际 GPU 像素对照、BT.601/709/2020 的 full/video range、非完整 workgroup 边界、解码流切换和音频时间线连续性。裁剪修正后重跑 2 项受影响的渲染测试，包含非零裁剪偏移和缓冲区填充的 GPU 像素对照，均通过。
- `cargo check -p krkr_plugin --features video-ffmpeg --offline`：通过。
- `cargo test -p krkr_plugin --features video-ffmpeg --lib video::decoder::tests::stream_selection_changes_decoded_video_and_audio --offline`：通过。
- `git diff --check`：通过。
- Android `aarch64-linux-android` 交叉检查：通过。启用 `video-ffmpeg,bevy/android-native-activity`，使用本地缓存的 Android FFmpeg 构建产物及 NDK API 26；该检查覆盖 Android 分支的 Rust 类型检查，不代表 APK 链接或实机播放验证。
- iOS 交叉编译检查在依赖构建阶段因缺少 Apple SDK 和 `xcrun` 中止，尚未检查项目的 iOS 分支；IOSurface 导入和 VideoToolbox 播放尚未在 Apple 平台验证。
- Linux DMA-BUF 实际导入、渲染输出及 GPU 同步：尚未在 VAAPI 硬件上验证。

## 后续工作

1. Linux：以真实 VAAPI 帧验证四种格式、modifier 与 GPU 同步。NV12 多平面需等待依赖支持或增加专用 Vulkan/YUV 路径。
2. Android：继续验证其他厂商 Surface 格式、分辨率变化及 GPU 同步压力；realme RMX3888 已覆盖 UBWC、对齐缓冲区裁剪、seek 和循环播放。
3. iOS：在实际 iOS 设备验证 VideoToolbox 解码输出、像素对照、seek/换流和连续播放压力；当前适配使用 IOSurface YUV 平面导入，异常格式回退 CPU。
4. 为共享路径增加平台目标编译、实际帧像素对照、分辨率变化、seek/stream 切换和连续播放压力验证。

## Android 独立视频 APK

`examples/android_video.rs` 使用内置的 `fd_op01.mpg`，不需要共享存储权限或游戏目录。原始媒体由 `tools/android/extract-video.py` 从本机 `test_games/Fate hollow ataraxia/data.xp3` 提取到忽略的 `target/android-video-test/media`，不加入版本控制。视频为 800×600 H.264/AAC，约 122 秒。

使用 Android 构建环境执行：

```sh
CARGO_TARGET_DIR="$PWD/target/android-release" cargo ndk -t arm64-v8a -P 26 build --release \
  --no-default-features --features real_ffi,video-bundled --example android_video
tools/android/package-video.sh
adb install --no-incremental -r target/android-video-test/fate-video-arm64.apk
adb shell am start -n org.bevykrkr.video.fate/android.app.NativeActivity
```

打包脚本使用本机 SDK 的 build-tools 36.0.0、android-37.0 平台以及 NDK r30，并打入 `libc++_shared.so`。APK 使用本地测试密钥签名。界面提供播放、暂停、前后 10 秒 seek、重新开始及硬件/软件解码切换。

模拟器验证使用 Android 35 x86_64 系统镜像的 arm64 转译；当前 `-gpu host` 下 Surface 只显示 Bevy 清屏背景，普通 Sprite 和视频控制面板也不可见，因此该环境无法验证视频画面。启动日志显示 MediaCodec 已创建并持续解码，Vulkan AHardwareBuffer 导入器初始化成功，但不能据此认为视频纹理已呈现。需在真实 arm64 设备继续验证。暂停 seek 预览取目标时间后的首帧，避免目标时间位于两帧之间时一直保留旧画面。

真实设备已确认 APK 可以启动并正常显示视频。realme RMX3888（Android 15、Adreno 750）实际选择 `c2.qti.avc.decoder`，使用 800×608 UBWC 缓冲区、800×600 裁剪；连续播放、seek 和循环播放累计导入超过 5100 帧，CPU 上传为 0。模拟器的清屏现象不代表真实设备的显示结果。不同 GPU 兼容性及严格音画同步仍需验证。

视频 PCM 由后台解码线程直接送入混音队列，不再等待主线程取走视频帧。seek/换流使用序号隔离旧 PCM；混音游标按实际输出样本数连续推进，不在每次音频回调时按墙钟重新定位。回调抖动、seek 隔离与主线程未取视频帧时的 PCM 供给由视频子系统测试覆盖。

音频重采样输出时间扣除 libswresample 缓存的延迟，并按输出样本数连接相邻帧，消除 44.1 kHz 转 48 kHz 时的周期性短缺口。混音器在设备采样率转换时跨帧线性插值，不再因队列锁短暂占用而跳过整个回调。44.1 kHz 测试媒体覆盖重采样后时间连续性，跨帧斜坡信号覆盖设备采样率转换。音频 click 是否完全消失仍需真机听测。

两个 Android 示例的 Activity 均使用 `Theme.Material.NoActionBar.Fullscreen`，在应用显示时隐藏顶部状态栏。新版 APK 已通过 arm64 release 构建和签名校验；模拟器截图与窗口 `FULLSCREEN` 状态确认顶部状态栏隐藏，底部导航栏保持系统默认行为。

所有平台的屏幕呈现由 `render.rs` 统一设置：相机覆盖窗口物理分辨率，`KirikiriScreen` 保留游戏图像尺寸，按窗口与图像宽高比中的较小值设置 X/Y 共用的 `Transform.scale`，并居中显示；窗口尺寸变化时同步更新。

## 真机路径确认

先清除旧日志并重新启动 APK，播放至少 15 秒：

```sh
adb logcat -c
adb shell am force-stop org.bevykrkr.video.fate
adb shell am start -n org.bevykrkr.video.fate/android.app.NativeActivity
adb logcat -v threadtime > fate-video-path.log
```

停止采集后筛选：

```sh
rg 'video path:|video codec:|video: .*via|MediaCodec started successfully|AHardwareBuffer import' fate-video-path.log
```

`video: ... via mediacodec` 表示 FFmpeg 使用 MediaCodec Surface 解码；还需查看 `MediaCodec started successfully: codec = ...` 的组件名。MediaCodec 本身不等于专用硬件加速，系统也有软件组件。测试 APK 在 Android 10+ 启动时使用 `MediaCodecInfo.isHardwareAccelerated()` / `isSoftwareOnly()` 记录视频解码组件的分类，见 [Android API 文档](https://developer.android.com/reference/android/media/MediaCodecInfo)。把 FFmpeg 实际选择的名称与 `video codec: name=... hardware_accelerated=true software_only=false` 对应，才能确认该组件由系统标为硬件加速。单独列出的硬件组件仅代表可用，不代表已被选择。Android 8/9 没有该分类 API，名称推断不足以严格证明硬件加速。

`AHardwareBuffer -> wgpu Vulkan import succeeded` 只在实际帧成功导入后记录。随后 `current=Imported imported_frames=... cpu_upload_frames=0` 的导入计数持续增长，表示这些已送入合成器的帧使用 GPU 导入，并未执行 CPU 像素上传。计数每个播放器独立，从打开视频起累计，首帧、路径切换和每 300 个合成帧记录一次；不是屏幕呈现完成的 GPU fence 计数。

`RGBA alias` 表示直接别名导入 RGBA，`YCbCr420 + GPU conversion` 表示导入后在 GPU 转为 RGBA 再合成，后者也没有 CPU 像素回读，但包含 GPU 转换过程。不能把它描述为完全没有复制或转换。出现 `requesting CPU fallback`、`current=Cpu` 或递增的 `cpu_upload_frames` 表示已经使用 CPU 回退。初始化时的 `import available` 和界面上的硬解复选框只表示能力或请求，不证明实际路径。

诊断 APK 已通过 Android arm64 release 构建、签名和模拟器日志验证：导入模式累计 1800 帧时 CPU 上传为 0；关闭硬件选项后出现 `via software` 与 `current=Cpu`，CPU 上传累计 600 帧。证据保存在 `target/android-video-test/import-diagnostics.log`。这验证日志能够区分两条路径，不替代真机画面或硬件性能验证。

真机首帧导入后的 `AHardwareBuffer: GraphicBuffer(w=4, h=4, lc=1) failed` 与视频的 800×608 缓冲区不同。该设备初始化时记录 4 条，持续播放和 seek 时没有继续产生。独立 Vulkan 小程序不解码视频，仅调用 `vkGetPhysicalDeviceSurfaceFormatsKHR` 也能复现：驱动探测不支持的 gralloc 格式 59（`AHARDWAREBUFFER_FORMAT_R10G10B10A10_UNORM`）。表面格式查询仍成功返回 5 种格式。这类探测日志不代表视频导入失败；需结合 `current=Imported` 与 CPU 上传计数判断。证据保存在 `target/android-video-test/real-device-fixed-start.log`、`real-device-final.log` 和 `real-device-format-probe.log`。
