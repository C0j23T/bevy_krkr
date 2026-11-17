# 工作快照

- **工作方式**：主代理只调度/审查/提交/维护清单，不实现；每项实现委派 `fork_turns=none` 新上下文。读根 AGENTS.md；只测受影响子系统。截止2026-11-17北京时间21:30，用户可随时叫停；未完成不能宣称完成。
- **保护工作树**：用户删除 `docs/ida-mcp-repair.md`、`docs/psd-plugin-identification.md`，PSD子模块`.gitignore`脏改动，均不恢复、不夹带提交。PSD BMP源码已提交根`298e8f6`/PSD`0f8a684`。
- **已完成任务1**：LayerExMovie归档，根`303e454`/K2`958accc`。Fate ignored unlink修复根`d778425`/K2`8bcb9c8`，测试1项且用户实机确认，禁止再代跑Fate。
- **Emote存档**：GPU与真实sd001.mtn`21fd1f8`；时间轴/序列化/变量/物理/hit/reload/两轮restart三项`d39c58a`；mode6跳过`98b51c1`；四层曲面8种mask+C++ evaluator oracle`0629c45`。mesh专项通过。PSB打开已测两遍禁止重复；atri/snbk资源经xp3filter+Storage，不能用7z失败判无资源。
- **LayerExDraw存档**：Vello依赖/同Bevy设备队列GPU后端`28d19f4`/K2 ABI`615d5f8`；C++对象/Path/Matrix/形状/图像/文字轮廓及TJS测试`64999fd`/K2`a980b6c`。GPU与TJS专项通过，整体仍未完成。
- **当前代理/文件边界**：`layerexdraw_lead`改K2 `plugins/LayerExDraw*`、build.rs与tests/layer_ex_draw.rs；继续记录/saveImage/color-scan/EMF+/字体。`layerexdraw_gpu`改Rust `layer_ex_draw*`、shim ABI与GPU像素测试；继续53Hatch/PathGradient/复杂pen，当前Paint ABI392与C++同步，第二阶段未提交。`snbk_acceptance`运行实际snbk宿主并修真实阻塞，写简短验收事实。代理若收到空payload，把任务写共享docs再通知读取。
- **未提交/验收**：LayerExDraw复杂paint第二阶段像素已过（15独立Hatch/任意polygon PathGradient/二维gradient），等待稳定检查点后分批提交；不要提交未用代码。Vello负责coverage，不从零光栅；普通帧无GPU读回。save/loadRecord必须EMF+，不能自定义格式冒充。更多契约见layerexdraw-task.md、layer-ex-draw-ffi.md。
- **待修回归**：extra_plugins的morphing末帧差1，已定位nagano.wesl权重255/256；完成帧应直接返回src2，尚未修复。`transition_regression`可恢复接修，别改断言掩盖。
- **下一步**：检查代理结果→只暂存对应路径→先提交子模块再根gitlink→更新docs/plugin-migration.md实际状态及本快照。snbk收尾要记录startup/首屏/动画/输入，未通过就保留具体阻塞，不标完成。用户离开电脑无需等确认，持续至完成或截止。
