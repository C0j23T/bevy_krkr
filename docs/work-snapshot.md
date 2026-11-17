# 工作快照

- 截止：2026-11-17 北京时间 21:30（启动时北京时间 08:39）；用户可随时叫停。
- 每完成一个可验证部分提交 Git；主代理串行提交，子代理全新上下文。
- 规范：根 `AGENTS.md`；仅测受影响子系统；GPU 复用纹理，避免逐帧 CPU 回读上传。
- 初始用户改动：`Kirikiroid2/src/core/base/win32/PluginImpl.cpp` 从 ignoredPlugins 删除 `lzfs.dll`，保留。
- 进度以 `plugin-migration.md` 与 Git 记录为准；只在本文件维护当前工作和阻塞。

| 负责人 | 当前任务 | 主要范围 |
| --- | --- | --- |
| `linux_video`（完成） | Linux FFmpeg / wgpu 修复：`e9dd3f4`；硬件4项通过 | 已释放槽位 |
| `gpu_lead`（阶段停） | GPU接口核心/真实GPU测试完成未提交；owner hooks已接，待独立插件验证/文档 | render operations/composer/renderer、shim/render、三插件 C++ |
| `external_lead`（阶段停） | loader/json已提交 `06883b7`；组员 `logic_plugins` 跟踪 sqlite3/ExtKAGParser验证 | 外部加载、构建、`external_plugins` |
| `move_psdfile` | 用户追加：真实移动既有PSD仓库并登记submodule；构建通过，待external_classes | psdfile仓库、构建脚本PSD段、docs/测试 |
| `ida_compat` | 用户追加：IDA MCP更新与IDA Pro版本兼容修复 | 本机IDA/MCP环境、专项修复文档 |
| 主代理 | 提交整合、清单、恢复排期 | docs、验收 |

- 通道：GPU数组/原地/区域测试已过；C++接入随FFI编译通过。外部测试依赖loader首批完成。
- task6目录整理已提交 `b46e09c`；JSON子仓库已登记submodule，宿主集成测试通过。
- GPU owner hooks已接回，接口真实GPU注册/回滚/卸载测试通过；外部Extrans完整像素与生命周期1项通过。
- sqlite/KAG测试日志 `/tmp/krkr-external-logic-batch.log`；PSD日志 `/tmp/bevy-krkr-psd-external-classes.log`。
- 新外部仓库统一主代理提交；PSD已有origin/history保持，预存dirty `psdclass.cpp`与untracked `psdfile.so`保留，迁移commit不含它们。
- `external_plugins/psbfile`独立构建通过但尚未验收；继续查MDF资源offset/缓存/损坏数据边界。textrender/wuffmpeg/win32dialog尚未做。
- GPU 组与外部组共享边界：tp_stub GPU 注册导出；插件目录移动须协调三插件路径。
- 任务7待空槽分新上下文做Vello调查；任务8由external_lead排期，逻辑留C++/Kirikiroid2。Emote文档已按新约束精简提交 `cbc63e2`。
