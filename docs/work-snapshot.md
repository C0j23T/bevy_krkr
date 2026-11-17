# 工作快照

- 截止：2026-11-17 北京时间 21:30（启动时北京时间 08:39）；用户可随时叫停。
- 每完成一个可验证部分提交 Git；主代理串行提交，子代理全新上下文。
- 规范：根 `AGENTS.md`；仅测受影响子系统；GPU 复用纹理，避免逐帧 CPU 回读上传。
- 初始用户改动：`Kirikiroid2/src/core/base/win32/PluginImpl.cpp` 从 ignoredPlugins 删除 `lzfs.dll`，保留。
- 进度以 `plugin-migration.md` 与 Git 记录为准；只在本文件维护当前工作和阻塞。

| 负责人 | 当前任务 | 主要范围 |
| --- | --- | --- |
| `linux_video` | Linux FFmpeg / wgpu 共享修复 | 视频 Rust、`vendor/video` |
| `gpu_lead` | 三插件 GPU 加速，随后外部 WESL 注册与适用插件 | render operations/composer/renderer、shim/render、三插件 C++ |
| `external_lead` | 非渲染插件独立 submodule、krkrz 加载对齐、插件目录整理 | 外部加载、构建、`external_plugins` |
| 主代理 | 清单、提交整合、通道转换接入、Vello 调查 | docs、BevyRender.cpp、ExternalTransitions.cpp |

- 通道转换：已找到 ExternalTransitions.cpp 的 BGRA→RGBA CPU 循环；GPU 组添加 `ChannelSwizzle`，主代理接入。
- GPU 组与外部组共享边界：tp_stub GPU 注册导出；插件目录移动须协调三插件路径。
- 任务 8：有余力后安排；逻辑留 C++ / Kirikiroid2，禁止照旧文档重写 Rust。
