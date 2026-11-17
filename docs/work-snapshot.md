# 工作快照

- 截止：2026-11-17 北京时间 21:30；用户可随时叫停。每完成部分 Git commit；全新上下文代理，主代理串行提交。
- 规范见根 `AGENTS.md`；仅测受影响子系统，GPU不逐帧CPU往返；清单只由主代理维护。
- 保留用户预存改动：K2 `src/core/base/win32/PluginImpl.cpp` 删除 ignored `lzfs.dll`（提交时从index版本补回，工作树继续保留删除）；PSD `psdclass.cpp` 两行BMP对齐与未跟踪 `psdfile.so`。

| 已完成 | 根提交 / 验收 |
| --- | --- |
| Linux视频DMA-BUF | `e9dd3f4`；NVIDIA VAAPI连续10帧全Imported，视频专项4通过 |
| 三插件GPU加速 | `8e7ef5f`；BTOA/AreaAverage/Shrink真实Vulkan像素1通过 |
| 通道数组转换 | `6c4b985`；GPU数组1通过，外部Extrans1通过，AlphaMovie+PSD1通过 |
| 外部JSON/krkrz加载 | `ea1cb1b` / `06883b7`；标准tp_stub、Unicode/Storage/卸载重载1通过 |
| 多文件插件目录整理 | `b46e09c`；extrans/layerExSave/xp3filter及所有构建引用 |
| 外部WESL GPU接口 | `ebf4b87`；随机唯一>100、统一编译、失败回滚、卸载；注册/独立插件各1通过；见gpu-plugin-interface.md |
| PSD仓库真实移动 | `d63cbde`；保留历史remote/用户dirty，CMake+外部classes测试通过 |
| IDA MCP版本修复 | `61e505c`；IDA9.4/Nexus0.14.2/插件20261028.0.3，实际MCP打开查询关闭通过；见ida-mcp-repair.md |

| 当前负责人 | 未完成部分 |
| --- | --- |
| `external_lead` | psbfile纯tp_stub草稿已构建，修MDF资源/缓存/边界并验证；随后win32dialog/wuffmpeg/textrender及任务8 C++ Emote排期 |
| `external_lead/logic_plugins` | sqlite3/ExtKAGParser独立构建通过；集成暴露宿主UTF8/UTF16代理对bug，修CharacterSet.cpp/tjsConfig.cpp并跑unicode+两插件专项；日志 `/tmp/krkr-unicode-logic-tests.log` |
| `vello_assessment` | 任务7官方当前Vello/wgpu兼容评估，写docs/layer-ex-draw-vello.md；不改依赖 |
| 主代理 | 审查、分批提交、迁移清单、快照 |

- 未提交宿主标准exports/内外插件去重在 `PluginImpl.cpp`、`ExternalStorage.cpp`、`InternalPlugins.cpp`、`ncbind.hpp`；属逻辑插件批次。
- `external_plugins/{sqlite3,ExtKAGParser,psbfile}` 尚未init/gitlink；完成验收后主代理各自init+commit+submodule add+absorbgitdirs。JSON本地绝对URL未发布；PSD保留上游remote。
- task8逻辑必须留Kirikiroid2/C++；GPU网格变形须内建Composer。docs/emote-integration.md已按该约束重写（`cbc63e2`）。
- 外部GPU候选drawdevice/autotest/压缩图像解码均需额外宿主能力，不能只靠pixel kernel完整迁移，仍列待实现。
