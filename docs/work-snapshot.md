# 工作快照

- 截止：2026-11-17 北京时间 21:30；用户可随时叫停。根 `AGENTS.md` 约定：主代理调度、审查、提交并维护本快照与 `plugin-migration.md`；实现由全新上下文子代理完成；仅跑受影响测试。
- 今日任务1已完成：LayerExMovie 文件归档。Kirikiroid2 `958accc`，根 gitlink `303e454`。
- Fate `util.dll` unlink 已修复并获用户实机确认；focused 测试1/1。Kirikiroid2 `8bcb9c8`，根 `d778425`；不要再代跑 Fate。
- Emote GPU/真实sd001.mtn `21fd1f8`；时间轴/物理/hit/reload/两轮restart 3项通过 `d39c58a`。`emote_mesh_inheritance`补4层GPU曲面oracle；PSB打开已测两遍禁止重复，atri/snbk资源可用。
- XP3 资源需参考 xp3filter 解密；已通知 emote_mesh_test。负责人定位 data720.xp3 内33个 .mtn，正在用项目 Storage 验证。
- LayerExDraw Vello GPU首检查点`28d19f4`（K2 ABI`615d5f8`），同设备路径/alpha/clip/transform/gradient/stroke/image/crop/self/resize专项通过。lead继续C++对象/图像/文字/记录，GPU代理继续Hatch/PathGradient/pen；见layerexdraw-task.md与layer-ex-draw-ffi.md。未整体完成。
- LayerExDraw C++对象/Path/Matrix/图像/文字轮廓端到端已提交`64999fd`（K2`a980b6c`）；仍在推进EMF+与复杂画笔。Emote mode6 FFI跳过已修`98b51c1`，待继承代理专项回归。
- LayerExDraw Vello/C++当前未提交工作：GPU `layer_ex_draw.rs` Hatch/PathGradient/图像裁剪与 resize；K2 `LayerExDraw*.inc/.h/.cpp` 复杂对象/记录/文字改动；Rust `tests/layer_ex_draw.rs` 与 `layer_ex_draw_pixels.rs`。先跑 focused 测试并审查，再分提交。用户预存删除 `docs/ida-mcp-repair.md`、`docs/psd-plugin-identification.md` 不得恢复或提交；`external_plugins/psdfile/.gitignore` dirty 保留。
- Emote 最新已提交 `d39c58a`（fixture、时间轴/物理/hit/reload/restart 3项通过）；`emote_mesh_inheritance` 正在 mesh_pixels 增加4层 root/warp/icon oracle、blend 6/1/4 测试，未提交。PSB打开已验证两遍，禁止重复；XP3资源须经 xp3filter。
- 当前代理：`layerexdraw_lead`（K2复杂对象/记录/EMF+）、`layerexdraw_gpu`（Vello Hatch/PathGradient/复杂画笔）、`emote_mesh_inheritance`（多层GPU oracle）。主代理只审查、运行受影响测试、分批提交、维护清单/快照。
- 重要已提交点：Fate ignored unlink `d778425`/K2`8bcb9c8`；Emote GPU `21fd1f8`；Emote验收 `d39c58a`；Vello ABI/GPU `28d19f4`/K2`615d5f8`；LayerExDraw C++ `64999fd`/K2`a980b6c`；mode6 `98b51c1`；PSD BMP `298e8f6`/PSD`0f8a684`。
- Fate 与 Emote 验收后，按时间运行 `test_games/snbk` 收尾测试；必须按当前实现更新 `docs/plugin-migration.md`。
- 旧morphing末帧失败已定位：nagano.wesl权重255/256；完成帧应直接返回src2，尚未修复，不能误报通过。
- PSD源码已按用户要求存档：根`298e8f6`，PSD`0f8a684`；保留其`.gitignore`工作树改动。用户另删除了两份docs文档，勿恢复或夹带提交。
