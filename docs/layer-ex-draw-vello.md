# LayerExDraw 的 Vello 后端评估

核对日期：2026-11-17。结论：**有条件合适，优先验证 `vello_gpu 0.4.0`**。它能承担路径、画笔、图像和字形的 GPU 光栅化；GDI+ 对象行为、字体布局、记录文件兼容仍需项目实现。本次仅调研，未添加依赖或实现后端。

## 版本与选择

本项目锁定 Bevy 0.20.1 / wgpu 30.0.1。锁文件中的 `vello_cpu` / `vello_common` 0.1.0 是字体依赖，不代表已有 GPU Vello。

| 候选 | 已发布依赖 | 接入判断 |
| --- | --- | --- |
| `vello_gpu 0.4.0` | wgpu `^30.0.0`，Rust ≥1.89 | 首选；CPU 处理路径，GPU 光栅化与合成，接受宿主 encoder 和外部纹理绑定 |
| `vello 0.12.0` | wgpu `^30.0.0`，Rust ≥1.89 | 可共用设备，但计算着色器路线仍属实验实现；公开渲染入口自行提交命令，外部图像需复制进 atlas |
| `vello_hybrid 0.2.0` | wgpu `^29.0.3` | 不直接接入；与 wgpu 30 的设备、纹理类型不兼容 |

`vello_gpu` 是 `vello_hybrid` 的新包名，0.4.0 已发布；不要依据旧名称选择旧依赖。依据：[0.4.0 发布源 Cargo.toml](https://github.com/linebender/vello/blob/b7e8f523b45c07a5edb806bb7241ef5afe34a882/vello_gpu/Cargo.toml)、[包名与功能](https://docs.rs/vello_gpu/latest/vello_gpu/#package-rename)、[计算型 vello 依赖](https://docs.rs/crate/vello/0.12.0)、[旧 hybrid 依赖](https://docs.rs/crate/vello_hybrid/0.2.0)。

## 与现有接口的对应

当前 [C++ 插件](../Kirikiroid2/src/core/environ/bevy/plugins/LayerExDraw.cpp) 注册 `clear`、`drawRectangle` / `drawEllipse`、文字绘制/度量和简化 Appearance。形状使用 `(appearance,x,y,width,height)`，只支持不透明实色笔刷与整数坐标/偏移；拒绝 pen、渐变、半透明和小数，不支持变换或抗锯齿；返回范围为字典。形状和 `clear` 复用 GPU `FillARGB`，椭圆按裁剪后的扫描线派发；文字先生成 FreeType 遮罩，再用 `LayerExTextPaint` 合成。完整需求以 [上游注册表](../layerExDraw/main.cpp) 和 [实现](../layerExDraw/LayerExDraw.cpp) 为准。

| LayerExDraw 能力 | Vello GPU 对应及额外工作 |
| --- | --- |
| clear | `FillARGB` 写入宿主 Layer 纹理；验证 ARGB 到 RGBA 和 alpha 清除语义 |
| Path、形状、填充、描边 | `BezPath`、`fill_path`、`stroke_path`；弧线和 GDI+ cardinal spline 转曲线；核对填充规则、端帽、连接、虚线和 inset pen |
| Matrix、viewTransform | `Affine` / `set_transform`；自行保持两套矩阵，验证 prepend/append、角度方向和 Appearance 偏移顺序 |
| 裁剪、透明度 | `push_clip_rect/path` 与 layer；沿用 Layer 当前 clip，保持栈配对；不能把 mask layer 当成已实现功能 |
| 实色、线性/径向渐变 | `set_paint`、独立 paint transform；Tile/FlipX/FlipY/FlipXY 要按二维坐标映射，不能统一当作 Reflect |
| Hatch、PathGradient | Hatch 可生成可缓存的小纹理；GDI+ 任意边界的 PathGradient 不等于径向渐变，需单独实现或明确拒绝 |
| Image、裁剪、缩放、仿射 | 图像 paint + 几何 + paint transform；外部 `TextureBindings` 采样 GPU 纹理；Storage、解码、尺寸/DPI/RotateFlip 行为另行适配 |
| drawString、drawPathString | `glyph_run` 接受字体与已定位字形；布局、回退、度量、私有字体、下划线/删除线仍由宿主负责；路径文字需真实轮廓描边，不能用遮罩膨胀代替 |
| record、redrawRecord | 保留项目自己的绘图操作与资源快照，再重建 Scene；Scene 的视口相关 strips 不是可直接缩放的通用记录格式 |
| saveRecord、loadRecord | 上游是 EMF+ Metafile 字节流；Vello 不提供 EMF+ 编解码，自定义记录格式不能声明兼容旧文件 |
| saveImage、getColorRegionRects | 仅显式调用时同步读回/编码或扫描；普通绘图不读回 |

API 依据：[Scene](https://docs.rs/vello_gpu/latest/vello_gpu/struct.Scene.html)、[外部纹理约束](https://docs.rs/vello_gpu/latest/vello_gpu/struct.TextureBindings.html)。上游还暴露 smoothingMode、textRenderingHint、updateWhenDraw，必须逐项定义可观察行为；Vello 的抗锯齿与字体栅格器不能保证逐像素复刻 GDI+。

## GPU 接入方案

1. C++ 保留 ncbind/TJS 对象、参数校验、Storage/输入流适配；向 Rust 提交包含目标纹理 ID、clip、矩阵、路径、Appearance、图像/字体资源句柄的完整绘图操作。Rust 持有资源与后端，不逐项导出 Vello API，不跨 FFI 保存 TJS 指针。
2. 在现有 [KrkrRenderer](../crates/krkr_plugin/src/render/renderer.rs) 内保存 renderer、Resources、Scene 与缓存。使用 `RenderDevice::wgpu_device()` 和同一个 RenderQueue；不用 Vello 的独立窗口/设备工具。两个候选的 wgpu 30 版本范围都允许统一到现有 30.0.1，接入时用依赖树确认仅一套 wgpu。
3. `vello_gpu::Renderer::render` 写入宿主 `CommandEncoder`，随后由现有提交路径提交。每次 render 的 encoder 必须在下一次 render 前提交，不能先连续编码多个目标再统一提交；先按每个连续绘图批次提交，之后再测优化空间。[提交约束](https://docs.rs/vello_gpu/latest/vello_gpu/struct.Renderer.html#method.render)
4. 复用现有 renderer mutex，按既有顺序取得 [surface_sync](../crates/krkr_plugin/src/render/surface_sync.rs) 锁；Vello text atlas 路径也可能内部 `submit`，调用 render 时须纳入此锁。切换后端前先提交已有 encoder，保持脚本调用顺序；不以 GPU 完成等待代替队列排序。资源保持有效至相关命令提交，resize/销毁时清理绑定与缓存。
5. 首版用池化 `Rgba8Unorm` 临时纹理渲染透明结果，增加 `RENDER_ATTACHMENT`，保留 `TEXTURE_BINDING | COPY_SRC | COPY_DST`；深度附件先设 `None`。现有 Layer 纹理没有 `RENDER_ATTACHMENT`，且 alpha 语义不同，不能直接把其 view 作为 Vello 目标。每批显式透明清除；同源同目标的绘图先做 GPU 快照，避免 attachment 与采样别名。
6. Vello 输出按预乘 alpha 合成；现有 Layer 普通 alpha 路径保存直通 RGB。新增一次 GPU 合成，将 Vello 预乘结果按 Layer 类型写回：普通 alpha 用 `(S.rgb + D.rgb·D.a·(1−S.a))/A`，`A=S.a+D.a·(1−S.a)`，零 alpha 写零；加算 alpha 层另走既有语义。该步骤覆盖 clip 和 dirty rect，随后调用现有纹理失效与 Layer 更新。
7. 外部图像绑定要求预乘 RGBA；对现有直通 alpha 源先用 GPU 转换并按纹理版本缓存。颜色和图像均以 sRGB 编码值进入 `Rgba8Unorm` 路径，避免使用 `_Srgb` view 意外解码；保留现有交给 Bevy 展示时使用 sRGB view 的边界。渐变显式选择插值空间，并与 GDI+ 样例比较，不能把线性光插值视为自动兼容。

alpha 依据：[TextureBinding](https://docs.rs/vello_gpu/latest/vello_gpu/enum.TextureBinding.html)、[发布版预乘混合管线](https://github.com/linebender/vello/blob/b7e8f523b45c07a5edb806bb7241ef5afe34a882/vello_gpu/src/render/wgpu/passes/strip.rs)、[sRGB paint 类型](https://github.com/linebender/vello/blob/b7e8f523b45c07a5edb806bb7241ef5afe34a882/vello_common/src/paint.rs)。这里的转换与缓存是拟议实现，尚未实测。

计算型 `vello` 的不同约束：输出必须为 `Rgba8Unorm + STORAGE_BINDING`，其 fine shader最终写回直通 alpha；`register_texture` 接受直通 alpha 源但需要 `COPY_SRC`，修改后标记 dirty；公开入口内部提交。因此不能与上述 GPU 版共用 alpha 假设。[Renderer](https://docs.rs/vello/latest/vello/struct.Renderer.html)、[输出 shader](https://github.com/linebender/vello/blob/b7e8f523b45c07a5edb806bb7241ef5afe34a882/research/vello_shaders/shader/fine.wgsl)

## 平台、许可与边界

- Android 首选复用 Bevy 的 Vulkan 设备。Vello GPU 不要求 compute；wgpu 的 Android OpenGL ES 3.0 路线属于尽力支持，而且本项目合成器本身使用 compute，不能据此承诺整个引擎支持 GLES 3.0。需在目标 Adreno、Mali 真机验证。[Vello 架构](https://github.com/linebender/vello/blob/84ffc24a117708a45de069a598538e4bcc35a243/ARCHITECTURE.md)、[wgpu 平台表](https://github.com/gfx-rs/wgpu#supported-platforms)
- Scene 有可变状态且不是 Sync；所有 Scene/Resources/renderer 的修改放在既有渲染串行边界。CPU 路径准备可以另做任务分批，但资源归属、提交和图层顺序必须保持确定。Scene/目标尺寸为 `u16`，须在 FFI 前结合设备纹理上限校验尺寸、有限坐标与资源大小。[尺寸契约](https://docs.rs/vello_gpu/latest/vello_gpu/struct.RenderTargetConfig.html)
- 上游明确列出 mask layer、复杂 filter graph、部分非隔离混合会 panic，字形缓存也有实验接口；FFI 前拒绝这些未支持组合，不能依赖跨 FFI 捕获 panic。[已知限制](https://docs.rs/vello_gpu/latest/vello_gpu/#current-state)
- Vello 为 `Apache-2.0 OR MIT`；接入建议选用 MIT 并补第三方许可文本/版权声明。示例 assets 有各自许可，不随依赖引入。[许可原文](https://github.com/linebender/vello/blob/b7e8f523b45c07a5edb806bb7241ef5afe34a882/LICENSE-MIT)、[assets 说明](https://github.com/linebender/vello#license)

## 接入前的最小验证门槛

1. 单独小型验证目标：只引入候选依赖，关闭默认后端特性，启用 `wgpu`、所需 image/text 特性；证实同一个 Bevy wgpu 30 设备、队列与既有纹理可用，无第二个设备，无逐帧 readback。
2. GPU 验证图：透明/半透明层、普通/加算 alpha、嵌套 clip、填充孔洞、非均匀缩放描边、虚线、矩阵顺序、二维翻转渐变、图像裁剪与同纹理绘制；对照 GDI+ 生成固定参考图，单独界定抗锯齿边缘容差。
3. 字体与记录：中日文/组合字符/回退、outline 与 measure 的边界一致；record 后改 viewTransform/尺寸能重放；选定真实 EMF+ 文件验证。如果旧 EMF+ 读写是硬要求，Vello 单独作为完整替代后端不合适，须先确定独立编解码方案。
4. 顺序与生命周期：交错现有绘图/Vello/CPU 读取、连续多图层、resize、窗口表面重配、字体/图像释放；开启 wgpu validation，验证无过期 atlas、死锁、漏提交和资源泄漏。
5. Linux 桌面与 Android Adreno/Mali 分别记录 1080p 混合场景的 CPU/GPU p50/p95、提交次数与峰值显存；区分首次编译、内容更新和静态帧。目标设备更新帧 p95 ≤16.7ms，静态帧不重建 Scene、不重新上传像素；门槛未过前不替换当前文字路径。

本阶段只核对源码、发布依赖和官方文档；未进行 GPU、Android 或性能验证。后续按上述门槛分子系统验证，不运行全量测试或 example。
