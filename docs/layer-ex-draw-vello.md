# LayerExDraw 的 Vello GPU 后端

LayerExDraw 使用 `vello_gpu 0.4.0` 和 Bevy 的同一 wgpu 设备、队列与纹理。C++ 负责 TJS 对象、Storage、输入流及 FreeType 字形轮廓。Rust 消费完整绘图操作，负责字体整形、画笔及 GPU 提交、记录和位图编解码。

## 绘图与合成

- 路径、填充、描边和几何变换通过 Vello Scene 光栅化。Layer 矩形裁剪和记录路径裁剪作用于绘图形状。
- 原生 Vello paint 绘制实色和可直接表达的线性渐变。二维平铺渐变、路径渐变及部分图片裁剪使用 WESL 生成 GPU paint 纹理。53 种 Hatch 由 8×8 覆盖图案着色后缓存。
- 外部图片绑定池化纹理，GPU 转换为 Vello 使用的预乘 alpha。池化 `Rgba8Unorm` 存储 Vello 的绘图结果。WESL 将结果合成到普通直通 alpha、加算 alpha 或不透明 Layer。EMF+ SourceCopy 使用独立的几何覆盖纹理处理透明像素和抗锯齿边缘。
- 普通绘图与记录重放留在 GPU。保存图片、颜色区域扫描和导出记录中的位图需要显式读回。局部 Layer 更新使用经过裁剪和图片偏移换算的绘图边界。

## 提交与资源

Vello render 前提交既有 encoder。渲染及队列提交位于 renderer mutex 与 surface_sync 锁内，并在下一次 render 前提交。资源在命令提交前保持有效，路径及裁剪由同一个绘图操作消费。Vello Scene 以 `u16` 表示目标尺寸，纹理同时受设备限制。[Vello render 契约](https://docs.rs/vello_gpu/0.4.0/vello_gpu/struct.Renderer.html#method.render)

[绘图操作契约](layer-ex-draw-ffi.md)列出 FFI 字段与画笔支持范围。[EMF+ 记录](layer-ex-draw-record-task.md)描述记录编解码。[专项测试](layerexdraw-task.md)记录已验证的 GPU、TJS 和 Wine 样本，以及待实测的平台。
