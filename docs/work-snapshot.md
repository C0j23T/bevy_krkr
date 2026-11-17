# 工作快照

- 截止：2026-11-17 北京时间 21:30；用户可随时叫停。根 `AGENTS.md` 约定：主代理调度、审查、提交并维护本快照与 `plugin-migration.md`；实现由全新上下文子代理完成；仅跑受影响测试。
- 今日任务1已完成：LayerExMovie 文件归档。Kirikiroid2 `958accc`，根 gitlink `303e454`。
- Fate `util.dll` unlink 已修复并获用户实机确认；focused 测试1/1。Kirikiroid2 `8bcb9c8`，根 `d778425`；不要再代跑 Fate。
- Emote GPU闭环与真实sd001.mtn绘制已通过；提交见最新git log。`emote_validation`继续物理/时间轴/继承/hit/重启验收。PSB打开已测两遍禁止重复；atri/snbk资源可用。
- XP3 资源需参考 xp3filter 解密；已通知 emote_mesh_test。负责人定位 data720.xp3 内33个 .mtn，正在用项目 Storage 验证。
- LayerExDraw：`layerexdraw_lead`写C++对象/Path/Matrix/图像/文字/记录，`layerexdraw_gpu`写vello_gpu依赖与Rust后端；任务契约见layerexdraw-task.md/layer-ex-draw-ffi.md。均在工作树未验收，勿误报完成。
- Fate 与 Emote 验收后，按时间运行 `test_games/snbk` 收尾测试；必须按当前实现更新 `docs/plugin-migration.md`。
- 旧morphing末帧失败已定位：nagano.wesl权重255/256；完成帧应直接返回src2，尚未修复，不能误报通过。
- PSD源码已按用户要求存档：根`298e8f6`，PSD`0f8a684`；保留其`.gitignore`工作树改动。用户另删除了两份docs文档，勿恢复或夹带提交。
