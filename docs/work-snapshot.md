# 工作快照

- 截止：2026-11-17 北京时间 21:30；用户可随时叫停。根 `AGENTS.md` 约定：主代理调度、审查、提交并维护本快照与 `plugin-migration.md`；实现由全新上下文子代理完成；仅跑受影响测试。
- 今日任务1已完成：LayerExMovie 文件归档。Kirikiroid2 `958accc`，根 gitlink `303e454`。
- 新优先项：Fate 启动遇到 `Not loaded plugin util.dll`。新代理核对发现消息由 `TVPUnloadPlugin` 抛出；需验证 XP3 中 link/unlink 顺序并补回归。Fate 包：`test_games/Fate hollow ataraxia/data.xp3`。
- Emote 子代理正在补 mesh GPU 绘制闭环。PSB 打开文件已验证两遍，禁止重复；`test_games/atri` 与 `test_games/snbk` 都有 Emote 资源。
- LayerExDraw 尚未完成：`clear(argb)` GPU 子项已提交（Kirikiroid2 `770d780`，根 `e9dc4da`），Vulkan 像素测试通过。几何 API 子代理暂缓，工作树保留未验收改动；插件仍缺路径、图像、记录与 EMF+。
- Fate 与 Emote 验收后，按时间运行 `test_games/snbk` 收尾测试；必须按当前实现更新 `docs/plugin-migration.md`。
- LayerExDraw 旧集成测试仍在 morphing 最后一帧有 RGB 各差 1 的失败，不能误报为通过。
- 保留现存用户改动：`external_plugins/psdfile` 内 `psdclass.cpp` 与未跟踪 `psdfile.so`；Emote 代理正在改 `crates/krkr_plugin/src/render/renderer.rs`。
