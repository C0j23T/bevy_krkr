# LayerExDraw 当日任务

- 负责人：`layerexdraw_lead`；完整插件涉及功能全部可测试通过才算完成，截止2026-11-17北京时间21:30。主代理只调度、审查、提交、维护迁移清单和快照；负责人组织全新上下文代理实现。
- 必须采用 `docs/layer-ex-draw-vello.md` 评估的 `vello_gpu` 后端，不从零实现光栅器。当前 clear/矩形/椭圆整数实色子集仅复用 FillARGB，不能作为完整后端。
- 以 `layerExDraw/main.cpp`、`LayerExDraw.cpp` 真实上游注册和参数为准。C++负责GDI+对象/TJS/Storage；Rust负责完整GPU绘图操作，不逐项导出Vello API。
- 先完成同Bevy wgpu设备/队列的Vello GPU最小验证，再实现Path/Matrix/Appearance/绘图、图像/文字/记录所需契约。落实预乘alpha适配、命令提交顺序、clip与资源生命周期；普通绘制不读回。
- 当前可用名额用完；转场代理结束后可分给全新上下文子代理。负责人先定义并落盘FFI操作与文件边界，后把对象适配和GPU后端拆开。不要改 `docs/plugin-migration.md` 或 `work-snapshot.md`，不提交；汇报真实完成边界与测试证据。
- 其他任务：`emote_validation`处理Emote时间轴/物理/hit/重启验收，`transition_regression`排查morphing末帧差1。共享renderer.rs/shader.rs接入须先协调，可先建LayerExDraw专属模块。
- 实现完成后删除或精简本任务文件，避免陈旧交接信息。仅运行受影响测试，不全量/example默认跑。
