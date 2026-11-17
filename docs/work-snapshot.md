# 工作快照

- 截止：2026-11-17 北京时间 21:30；用户可随时叫停。根 `AGENTS.md` 约定：主代理调度、审查、提交并维护本快照与 `plugin-migration.md`；实现由全新上下文子代理完成；仅跑受影响测试。
- 今日任务1已完成：LayerExMovie 文件归档。Kirikiroid2 `958accc`，根 gitlink `303e454`。
- Fate `util.dll` unlink 已修复并获用户实机确认；focused 测试1/1。Kirikiroid2 `8bcb9c8`，根 `d778425`；不要再代跑 Fate。
- Emote GPU闭环与真实sd001.mtn绘制已通过；提交见最新git log。`emote_validation`继续物理/时间轴/继承/hit/重启验收。PSB打开已测两遍禁止重复；atri/snbk资源可用。
- XP3 资源需参考 xp3filter 解密；已通知 emote_mesh_test。负责人定位 data720.xp3 内33个 .mtn，正在用项目 Storage 验证。
- LayerExDraw clear与真实矩形/椭圆不透明整数子集已存档（根`688c895`，K2`8b24735`）；`layerexdraw_lead`按`docs/layerexdraw-task.md`组织Vello后端及完整对象适配。现有依赖还没有vello_gpu。
- Fate 与 Emote 验收后，按时间运行 `test_games/snbk` 收尾测试；必须按当前实现更新 `docs/plugin-migration.md`。
- LayerExDraw 旧集成测试仍在 morphing 最后一帧有 RGB 各差 1 的失败，不能误报为通过。
- 保留现存用户改动：`external_plugins/psdfile` 内 `psdclass.cpp` 与未跟踪 `psdfile.so`；Emote 代理正在改 `crates/krkr_plugin/src/render/renderer.rs`。
