# LayerExDraw GPU 委派

- 负责人 `layerexdraw_gpu`：读取 AGENTS.md、layerexdraw-task.md、layer-ex-draw-ffi.md 与 `Kirikiroid2/src/core/environ/bevy/BevyLayerExDraw.h`。
- 用户明确采用调查文档的 `vello_gpu 0.4.0`；实现 Rust FFI、Cargo依赖与同BevyGPU绘图后端，不能停在调研或自造光栅器。
- `layerexdraw_lead`负责C++对象/Path/Matrix/Appearance与TJS/Storage，Rust代理不改C++，FFI改动先协调；全新上下文，不提交或改迁移清单/快照。
- 先让完整操作 `krkr_layer_ex_draw` 的最小路径真GPU像素测试通过，再扩展solid/gradient/stroke/clip/transform/GPU图像。遵守同设备/队列、编码提交顺序、预乘alpha适配、纹理生命周期与surface_sync锁；日常绘制不读回。
- 负责文件：`crates/krkr_shim/src/layer_ex_draw.rs`、`crates/krkr_plugin/src/render/layer_ex_draw.rs`、相关module/Cargo/renderer接入和独立测试。Emote目前已提交GPU闭环，验收代理只改Emote相关C++/tests/emote.rs；共享文件如有冲突先汇报。
- 截止2026-11-17北京时间21:30；目标完整插件涉及功能测试通过，不能把结构字段存在当实现完成。仅跑受影响测试；汇报精确能力、测试与剩余缺口，持续推进。
