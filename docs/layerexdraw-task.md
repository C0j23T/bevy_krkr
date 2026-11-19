# LayerExDraw 专项验证

在 Linux、RTX 4060 Laptop GPU、Vulkan 上通过以下定向测试。

| 子系统 | 已验证范围 |
| --- | --- |
| EMF+ | 外部 GDI+ 和规范样本、帧与 DPI、物理页单位、状态栈、区域与裁剪、DrawString 和 DrawDriverString、压缩图片、索引色、RGB565、负 stride、对象续块、截断和重载 |
| GPU | 三种 Layer alpha、透明 SourceCopy 与子像素覆盖、路径与图像仿射裁剪、线性渐变与 gamma、53 种 Hatch、端帽与复合描边、源图变更、目标 resize、纹理池复用及多次重放 |
| 场景更新 | 裁剪与负图像偏移下，局部绘制的场景复制区域为 4 个像素 |
| 字体 | 首选字体、组合字符回退、双向字形顺序、私有字体的渲染和度量 |
| 图片 | PNG、BMP、GIF、TIFF、JPEG 的通道与透明度，带物理分辨率的 PNG、BMP、TIFF、JFIF，BMP 行序、截图旋转、GIF 版本与交错、TIFF 位深与压缩、JPEG 渐进与基线 |
| TJS | 创建几何与字体对象、绘制与测量、记录重绘和保存后载入的全图 RGBA 对照、嵌套记录、错误加载后继续重绘原记录 |
| Wine GDI+ 11.22 | 原始路径文件与 Rust 导出文件在 Wine 回放时全部 3,072 个像素一致，导出文件的 18 条记录均播放成功。重新生成的混合样本与仓库夹具逐字节一致，TJS GPU 测试核对其渐变、排除裁剪、文字和位图。 |

仍待验证 Windows 原生的逐像素对照，以及 Android Adreno、Mali 真机的像素和性能。扩展格式的兼容边界见 [EMF+ 记录](layer-ex-draw-record-task.md)。

## 定向命令

```sh
cargo test -p krkr_plugin --lib layer_ex_draw_record -- --test-threads=1
cargo test -p krkr_plugin --test layer_ex_draw_pixels
cargo test -p krkr_shim --lib drawing_bitmap
cargo test -p krkr_shim --lib font::
cargo test --test layer_ex_draw
cargo test --features test-render-stats --test extra_plugins layer_ex_draw_
cargo test -p krkr_plugin --lib wine_gdiplus_plays_exported_metafile -- --ignored --nocapture
```

外部样本的来源和生成命令见 [夹具说明](../crates/krkr_plugin/tests/fixtures/layer_ex_draw/README.md)。
