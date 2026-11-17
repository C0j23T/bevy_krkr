# 工作快照

- **已停止**：用户2026-11-17北京时间12:00要求立即停工。三个子代理已interrupt；本任务cargo测试及关联编译进程已终止。除保存本快照，不再实现/测试。需用户重新授权恢复，21:30原截止不构成恢复授权。
- **停止后存档授权**：用户随后要求提交全部未提交改动；当前半成品、诊断测试、用户两份文档删除及PSD`.gitignore`一并存档。K2检查点`4f40b58`，PSD`696a0cd`，根提交见最新git log。以下“未提交”描述均指该停工检查点中的未完成工作，验证状态不变；不表示可恢复作业。
- **工作方式**：主代理只调度/审查/提交/维护文档，不实现；实现均委派`fork_turns=none`全新上下文。读根AGENTS.md，仅测受影响子系统。插件全部功能测试通过才算完成，目前LayerExDraw与snbk均未完成。
- **用户原有改动**：两份文档删除及PSD`.gitignore`已按最新“提交未提交改动”授权存档，不得恢复。PSD BMP源码已存档根`298e8f6`/PSD`0f8a684`。
- **已完成**：任务1 LayerExMovie归档`303e454`/K2`958accc`；Fate ignored unlink `d778425`/K2`8bcb9c8`，用户实机确认，禁止再代跑Fate。PSB打开已验证两遍禁止重复；atri/snbk资源经xp3filter+Storage，不以7z失败判无资源。
- **Emote存档**：GPU/真实sd001.mtn `21fd1f8`；时间轴/变量/序列化/物理/hit/reload/两轮restart三项通过`d39c58a`；mode6 `98b51c1`；四层8种mask+C++ evaluator oracle `0629c45`。完整游戏脚本尚未验收。
- **LayerExDraw存档**：Vello同Bevy设备队列`28d19f4`；C++对象/Path/Matrix/形状/图像/文字`64999fd`；高级paint `fcbdca0`/K2 ABI`30fe229`（Paint392）；内部record/saveImage/color-scan `b8a41c5`；53Hatch/cosmetic1px/弧线/DPI/文字模式`e4070e9`/K2`c34b57e`，相关GPU/TJS专项通过。现状文档`86a6c2d`。
- **未提交EMF+**：Rust`render/layer_ex_draw_record{,_format}.rs`、fixtures/layer_ex_draw、renderer/layer_ex_draw/shim接入，K2 BevyLayerExDraw.h与State/Image接入。格式5项通过，官方bytes+Wine原创fixture解码、独立Wine播放通过；TJS save/load及2倍Metafile绘制1项通过。发现record持有快照误retire入池bug，代理已局部修复并新加`record_snapshot_survives_replay_and_texture_pool_reuse`，该回归被叫停未获结果。不能直接把全部当前diff当已验收。更多缺口/契约见docs/layer-ex-draw-record-task.md；未知EMF+记录明确拒绝，不自定义格式冒充。
- **未提交C++**：LayerExDraw.cpp/Geometry.h/Text.inc/Image.inc/State.inc及新增Font.inc（私有字体进行中，未完成验证）。`layerexdraw_lead`负责；勿误提交未测字体。`layerexdraw_record`负责Rust codec/opaque生命周期。两代理均已interrupt，不可自动恢复。
- **snbk存档/现场**：外部插件归档项目目录修复`e3aa474`/K2`a2139c5`，textrender专项2项通过；本地game软链接部署`lib*.so`，不入仓。`examples/snbk.rs` 启动前扫描同目录与`plugin/`的`lib*.so`并attach；实际Vulkan窗口1280x720、9/9归档挂载，4个插件预加载，`exec_startup`完成；未到标题/交互验收。
- **snbk/scriptsEx**：K2 `ScriptsExt.cpp` 已按 krkrsdl3 参考补齐（`scriptsEx.dll`，`scriptsExt.dll`别名），`tests/scripts_ext.rs` 专项通过；PackinOne 改为组合 `scriptsEx.dll`。`ScriptMgnIntf.cpp` 的首次 startup 异常保留，`DebugIntf.cpp` 提供 `Scripts.dump`。`tests/snbk.rs` 仍为诊断草稿。
- **待修**：morphing末帧RGB差1，nagano.wesl权重255/256；完成帧应直接返回src2，尚未修复。LayerExDraw仍缺完整pen/字体/EMF+记录组合与Windows参考一致性，不能标整体完成。
- **恢复后顺序**：先git status核对停工检查点→新上下文分工→复验record资源池bug→推进snbk首因→分子系统测试→先子模块commit再根gitlink→更新plugin-migration.md/本快照。
