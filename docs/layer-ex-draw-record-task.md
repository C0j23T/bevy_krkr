# LayerExDraw EMF+ 接入任务

当前 C++ `plugins/LayerExDrawState.inc` 通过 Rust opaque record 句柄保存绘图操作；Storage 仍由 C++ 读写。精确 C ABI 在 `BevyLayerExDraw.h`，Rust 导出位于 `crates/krkr_plugin/src/render/layer_ex_draw_record.rs`。`create/append/clone/query/replay/draw/encode/decode/release/free` 以完整操作传递，`RecordInfo` 为48字节（四个u32与bounds[4] double），解码完整成功后才发布新句柄。

Rust registry 深拷贝每项完整 DrawRequest。append 中的 image 使用独立 GPU 纹理快照；clone 共享不可变纹理。replay 组合已记录 transform 与传入 view，沿用同设备同队列的 Vello 路径绘制；draw 还将源矩形映射为目标四边形裁剪。普通 append/replay/draw 没有 GPU 读回；encode 为显式导出读取图片资源。

格式实现在 `layer_ex_draw_record_format.rs`，依据 Microsoft `[MS-EMFPLUS] v20240423` 独立编写，未添加 parser 依赖或复制第三方解析器源码。当前编码生成真实 EMR_HEADER / EMR_COMMENT_EMFPLUS / EMR_EOF 外壳及 EMF+ Header、Clear、Object、FillPath、DrawPath、world transform、pixel page unit。Path 支持直线、cubic、多figure与close；brush 支持solid、hatch、texture的未压缩32位位图、polygon path gradient、linear gradient；pen 支持常规cap/join、dash、alignment、compound。解码实现对应对象，以及浮点/整数/相对path points、RLE point types、continued objects、save/restore/world transform。未知绘图记录、对象及未支持参数均明确失败。

当前检查点不代表完整兼容。尚未实现 DrawImage/DrawString 等非Path绘图记录、压缩或嵌套metafile图像、custom cap、pen transform、非pixel page unit、SourceCopy compositing、外部clip操作、全部brush组合；记录图片再绘到另一个record尚缺保留源裁剪的append-record完整操作。EMF frame与非zero bounds、全部渐变字段的外部语义、资源释放/重启、像素一致性及TJS全流程仍须验证。不能将内部roundtrip或Wine结构接受解释为全部功能完成。

可review证据：格式单测包含内部vector roundtrip、全部截断位置、长度overflow、continued对象total/type约束；外部fixture包含Wine GDI+独立生成的curve+solid fill+dashed pen metafile，以及官方§3.2.32.21的path record与§3.2.32.22的FillPath bytes。Rust重新编码的文件由Wine独立枚举并播放成功（16条记录，两条path draw），尚无Windows-native像素对照。fixture来源、许可与生成/验证命令见 `crates/krkr_plugin/tests/fixtures/layer_ex_draw/README.md`。

官方规范：[MS-EMFPLUS](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-emfplus/)、[MS-EMF](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-emf/)。本机PDF/文本位于`/tmp/layerexdraw-emfplus.pdf`和`/tmp/layerexdraw-emfplus.txt`。后续按格式、C++对象/Storage、GPU像素三个子系统验证，不默认运行全量测试或example。
