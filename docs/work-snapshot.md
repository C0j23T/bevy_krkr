# 工作快照

- 截止：2026-11-17 北京时间 21:30（启动时北京时间 08:39）；用户可随时叫停。
- 每完成一个可验证部分提交 Git；主代理串行提交，子代理全新上下文。
- 规范：根 `AGENTS.md`；仅测受影响子系统；GPU 复用纹理，避免逐帧 CPU 回读上传。
- 初始用户改动：`Kirikiroid2/src/core/base/win32/PluginImpl.cpp` 从 ignoredPlugins 删除 `lzfs.dll`，保留。
- 进度以 `plugin-migration.md` 与 Git 记录为准；只在本文件维护当前工作和阻塞。

| 负责人 | 当前任务 | 主要范围 |
| --- | --- | --- |
| `linux_video`（完成） | Linux FFmpeg / wgpu 修复：`e9dd3f4`；硬件4项通过 | 已释放槽位 |
| `gpu_lead` | 三插件已提交 `8e7ef5f`；当前实现外部 WESL 注册与适用插件 | render operations/composer/renderer、shim/render、三插件 C++ |
| `external_lead` | loader/json已验证待提交；组员 `logic_plugins` 做 sqlite3/ExtKAGParser | 外部加载、构建、`external_plugins` |
| 主代理 | 清单、提交整合、通道转换接入、Vello 调查 | docs、BevyRender.cpp、ExternalTransitions.cpp |

- 通道：GPU数组/原地/区域测试已过；C++接入随FFI编译通过。外部测试依赖loader首批完成。
- 外部loader GPU owner hooks曾提前加入导致链接失败；external_lead先撤并提交loader/json，GPU接口完成后再接线。
- 新外部子仓库提交、submodule登记统一由主代理做；尚未发布远程仓库。
- GPU 组与外部组共享边界：tp_stub GPU 注册导出；插件目录移动须协调三插件路径。
- 任务7待空槽分新上下文做Vello调查；任务8由external_lead排期，逻辑留C++/Kirikiroid2。Emote文档已按新约束精简提交 `cbc63e2`。
