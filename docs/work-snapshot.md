# 工作快照

- **工作方式**：主代理只调度/审查/提交/维护清单，不实现；每项实现委派 `fork_turns=none` 新上下文。读根 AGENTS.md；只测受影响子系统。截止2026-11-17北京时间21:30，用户可随时叫停；未完成不能宣称完成。
- **保护工作树**：用户删除 `docs/ida-mcp-repair.md`、`docs/psd-plugin-identification.md`，PSD子模块`.gitignore`脏改动，均不恢复、不夹带提交。PSD BMP源码已提交根`298e8f6`/PSD`0f8a684`。
- **已完成任务1**：LayerExMovie归档，根`303e454`/K2`958accc`。Fate ignored unlink修复根`d778425`/K2`8bcb9c8`，测试1项且用户实机确认，禁止再代跑Fate。
- **Emote存档**：GPU与真实sd001.mtn`21fd1f8`；时间轴/序列化/变量/物理/hit/reload/两轮restart三项`d39c58a`；mode6跳过`98b51c1`；四层曲面8种mask+C++ evaluator oracle`0629c45`。mesh专项通过。PSB打开已测两遍禁止重复；atri/snbk资源经xp3filter+Storage，不能用7z失败判无资源。
- **LayerExDraw存档**：Vello依赖/同Bevy设备队列GPU后端`28d19f4`/K2 ABI`615d5f8`；C++对象/Path/Matrix/形状/图像/文字轮廓及TJS测试`64999fd`/K2`a980b6c`。GPU与TJS专项通过，整体仍未完成。
- **当前代理/文件边界**：`layerexdraw_lead`改K2 `plugins/LayerExDraw*`、build.rs/tests/layer_ex_draw.rs，补字体/metadata与零宽GPU专项；`layerexdraw_record`按layer-ex-draw-record-task.md实现真实EMF+，与lead协调ABI；`snbk_acceptance`修StorageImpl.cpp插件归档路径并实际启动snbk。代理若收到空payload，把任务写共享docs再通知读取。
- **后续存档**：高级paint `fcbdca0`/K2 ABI`30fe229`（Paint392）；内部record/saveImage/color-scan `b8a41c5`/K2`630b944`；53Hatch/cosmetic1px+弧线/DPI/文字模式 `e4070e9`/K2`c34b57e`。最新TJS与GPU专项均过；文档现状`86a6c2d`。
- **未提交**：lead继续字体/metadata/pen，record代理新增Rust EMF+ format/ABI，snbk代理诊断菜单问题。Vello负责coverage，普通帧不读回；save/loadRecord必须真实EMF+。更多契约见layerexdraw-task.md、layer-ex-draw-ffi.md与record-task文档。
- **snbk现状**：Vulkan窗口1280x720、9/9归档挂载；textrender归档路径修复/测试已提交`e3aa474`/K2`a2139c5`。部署.so后实际启动进入KAGMainWindow，但override.tjs菜单查询/anonymous afterinit回调报INVALIDTYPE；代理正在Scripts.dump诊断（DebugIntf空gateway需输出），尚无VM bug证据。tests/snbk.rs为未提交诊断草稿。二进制/游戏样本不入仓；事实docs/snbk-acceptance.md。
- **待修回归**：extra_plugins的morphing末帧差1，已定位nagano.wesl权重255/256；完成帧应直接返回src2，尚未修复。`transition_regression`可恢复接修，别改断言掩盖。
- **下一步**：检查代理结果→只暂存对应路径→先提交子模块再根gitlink→更新docs/plugin-migration.md实际状态及本快照。snbk收尾要记录startup/首屏/动画/输入，未通过就保留具体阻塞，不标完成。用户离开电脑无需等确认，持续至完成或截止。
