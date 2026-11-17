# 工作快照

- 截止：2026-11-17 北京时间 21:30；用户可随时叫停。根 `AGENTS.md` 约定：主代理调度、审查、提交并维护本快照与 `plugin-migration.md`；实现由全新上下文子代理完成；仅跑受影响测试。
- 今日任务1已完成：LayerExMovie 文件归档。Kirikiroid2 `958accc`，根 gitlink `303e454`。
- LayerExDraw 尚未完成：`clear(argb)` GPU 子项已提交（Kirikiroid2 `770d780`，根 `e9dc4da`），Vulkan 像素测试通过；矢量 API 子代理进行中。插件整体仍缺路径、图像、记录与 EMF+。
- Emote 子代理正在补 mesh GPU 绘制闭环。PSB 打开文件已验证两遍，禁止重复；`test_games/atri` 与 `test_games/snbk` 都有 Emote 资源。之后按时间运行 snbk 收尾测试。
- 必须按当前实现更新 `docs/plugin-migration.md`。上一项 LayerExDraw 集成测试另有 morphing 最后一帧 RGB 各差 1 的失败，不能误报为通过。
- 保留现存用户改动：`external_plugins/psdfile` 内 `psdclass.cpp` 与未跟踪 `psdfile.so`；Emote 代理正在改 `crates/krkr_plugin/src/render/renderer.rs`。
