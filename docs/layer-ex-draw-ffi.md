# LayerExDraw 操作契约

`BevyLayerExDraw.h` 定义同步完整绘图操作 `krkr_layer_ex_draw`。C++ 负责 TJS 对象、Storage、字体轮廓和曲线转换；Rust 复制操作、验证资源并用 `vello_gpu 0.4.0` 绘制。不存在跨调用保留的 TJS 指针。

- `target` 和画笔 `image` 为既有 Bevy 纹理 ID。`clip` 为 Layer 像素坐标 `[left,top,right,bottom]`。
- `transform`、`paint_transform` 均为 GDI+ 行向量顺序 `[m11,m12,m21,m22,dx,dy]`。坐标变换为 `(x*m11+y*m21+dx,x*m12+y*m22+dy)`。绘图矩阵是 `transform * viewTransform`；Appearance offset 在两者之前应用。
- path 的 `verb` 为 `0 Move`、`1 Line`、`2 Cubic`、`3 Close`；points 分别使用前 2、2、6、0 项。`fill_rule=0` 为 even-odd，`1` 为 nonzero。
- paints 按数组顺序依次绘制。`stroke=0` 为填充，`1` 为描边；brush_type 使用上游值：`0 solid`、`1 hatch`、`2 texture`、`3 path gradient`、`4 linear gradient`。ARGB 颜色保持原始编码，不做隐式线性光转换。
- linear 的 coordinates 是起止点 `[x1,y1,x2,y2]`；texture 是原图裁剪范围 `[x,y,width,height]`。paint_transform 在源 paint 空间到形状空间间映射。texture image 使用直通 alpha，进入 Vello 前由 GPU 预乘转换；同源同目标先做 GPU 快照。
- wrap、hatch、端帽、join、alignment 使用上游枚举值。width、miter_limit、dash_offset 使用逻辑坐标；dashes 使用笔宽倍数。stops 的 position 为 `[0,1]`，color 为 ARGB；空 stops 使用 colors 两端。offset 使用形状空间坐标。
- `alpha_mode=0` 普通直通 alpha，`1` 加算 alpha，`2` 不透明。Rust 临时 Vello 目标为 `Rgba8Unorm` 预乘 alpha，之后 GPU 合成至对应 Layer 语义。普通绘制不读回。
- `flags` bit0 禁用抗锯齿，其他位保留且必须为零；operation 当前为 144 字节。
- `bounds` 是成功时回写的未裁剪绘图边界 `[x,y,width,height]`。失败返回非零；C++ 以异常中止，不能将未支持 paint 默默降级。

所有维度、坐标、矩阵、stop 和 dash 在消费前验证有限值与上限。每次 Vello render 编码前先 flush 既有 encoder；render 与 queue submit 使用 renderer mutex 内的 surface_sync 顺序，并在下一次 render 前提交。

`TVPBevyLayerExDraw` 为 144 字节；`TVPBevyDrawPaint` 为 392 字节。Paint 在原 200 字节字段后追加以下完整画笔参数：

| 偏移 | 字段 | 含义 |
| --- | --- | --- |
| 200 | `tile[4]` | 线性渐变二维 tile 的 `[x,y,w,h]` |
| 232 / 248 | `center[2]` / `focus[2]` | PathGradient 中心与 X/Y focus scales |
| 264 / 268 | `brush_point_count` / `surround_count` | 独立画笔边界与周边颜色数量 |
| 272 / 280 | `brush_points` / `surround_colors` | double X/Y 点与 ARGB 数组 |
| 288 / 292 | `blend_count` / `brush_flags` | blend 数量；bit0 为 gamma correction |
| 296 / 304 | `blend_positions` / `blend_factors` | `[0,1]` 分段混合位置与强度 |
| 312 / 316 / 320 | `compound_count` / `reserved2` / `compound` | compound pen 的归一化横截面区间 |
| 328 / 360 | `start_arrow[4]` / `end_arrow[4]` | cap255 时的 width、height、middleInset、filled；尺寸以笔宽为单位 |

当前后端使用同一个 Bevy wgpu 30.0.1 设备和队列，路径覆盖由 Vello 生成。实色、sRGB 渐变与 stops、clip、仿射变换、外部 GPU 纹理和整数裁剪均有像素测试。线性渐变的五种 wrap 使用二维 tile 映射；PathGradient 用独立 polygon 边界、逐边周边颜色、center、focus scales 和 stops/blend 生成 GPU paint 纹理，再由 Vello 填充路径。Gamma 路径尚未对照 GDI+，procedural paint 当前使用 2.2 指数近似。

Hatch 使用独立算法生成并缓存 8×8 小纹理，支持 style 0–52；未复制 Wine 的图案表，图案像素尚未与 GDI+ 对照。Pen 支持居中描边、闭合路径 inset、flat/square/round 端帽、扩展形状端帽、可调箭头、flat/round/triangle dash cap 与对称 compound。零宽 pen 在目标像素空间生成一像素描边。MiterClipped join、非对称 compound 和开放路径 inset 明确拒绝，不能视为完整 GDI+ pen。

`cargo test -p krkr_plugin --test layer_ex_draw_pixels -- --nocapture` 在 RTX 4060 Vulkan 上验证三种 alpha、clip/变换、渐变、描边边界、外部纹理、自纹理绘制、整数裁剪、空 clip 与 target resize；扩展用例覆盖 53 种 Hatch、polygon PathGradient、斜向二维 FlipX、inset、对称 compound、自定义箭头和非均匀缩放/旋转/clip 下的一像素 cosmetic pen 与虚线。`cargo tree -p krkr_plugin -i wgpu --depth 1` 确认仅一份 wgpu 30.0.1。普通操作不读回像素。仍需 GDI+ 参考图、完整 pen 组合、Android 和性能验收。
