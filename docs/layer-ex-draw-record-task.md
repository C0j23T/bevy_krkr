# LayerExDraw EMF+ 接入任务

当前内部记录在 C++ `plugins/LayerExDrawState.inc`：`RecordedDraw { Path path; Appearance appearance; Matrix transform; int smoothingMode; }`；`DrawState` 保存 `recordBackground`、`records`、`recording/replaying`，记录绘图时拷贝 path/appearance，并对 image 使用 `tTVPBaseTexture` 的独立写入快照。重放时组合已记录 transform 与当前 viewTransform，Layer 当前 clip 生效。修改 viewTransform 会重放；clear 清空记录并记背景色。`getRecordImage` 当前为 GPU 位图快照，尚非上游可缩放 Metafile 对象。

`saveRecord/loadRecord` 上游处理真实 EMF+。当前不支持，不能保存自定义格式或将 roundtrip 当作外部格式兼容性证据。前端 Storage 由 C++ 提供；编解码应放 Rust，消费和产生完整操作，而非暴露底层 EMF+ API。

现有绘图 ABI 在 `Kirikiroid2/src/core/environ/bevy/BevyLayerExDraw.h`，完整 operation 144 字节，Paint 392 字节；`crates/krkr_shim/src/layer_ex_draw.rs` 拥有复制后的 `DrawRequest`。当前 Paint 扩展包含真实 gradient 边界点、环绕色、center/focus、二维tile、blend/gamma、compound 和 arrow cap，不代表每字段已经完整实现。

建议新增记录接口（独立结构；实现代理先与 C++ 负责人确认再落盘）：

```cpp
struct TVPBevyRecordedDraw {
    TVPBevyLayerExDraw operation;
    uint32_t smoothing_mode;
    uint32_t reserved;
};
struct TVPBevyRecord {
    const TVPBevyRecordedDraw *operations;
    uint32_t operation_count;
    uint32_t background_argb;
    uint32_t width,height;
};
extern "C" int krkr_layer_ex_record_encode(const TVPBevyRecord *,void **bytes,size_t *length);
extern "C" void krkr_layer_ex_record_free(void *,size_t);
```

编码前 Rust 深拷贝每项完整 operation，忽略 target/clip 与回写 bounds；transform 只含绘图 transform，不含 viewTransform。image 纹理导出仅 saveRecord 显式操作允许读回。编码必须产生普通 EMF 外壳（EMR_HEADER、EMR_COMMENT_EMFPLUS、EMR_EOF）和真实 EmfPlusHeader/Object/FillPath/DrawPath/DrawImage 等记录。须核对brush/pen对象完整参数与图像颜色/预乘约定。

解码建议返回 Rust 持有的 opaque record 句柄：一次 decode(bytes,length)->handle；query header；replay(handle,target,clip,viewTransform,alphaMode) 在GPU完成；release。C++ 将 opaque record 纳入 DrawState，在新绘图前允许同一记录继续追加。不能返回悬空指针或跨FFI保存TJS指针。具体生命周期与已记录 DrawRequest 的集成需在实现前确定。

官方规范是 Microsoft `[MS-EMFPLUS] - v20240423`，已下载到 `/tmp/layerexdraw-emfplus.pdf`，纯文本 `/tmp/layerexdraw-emfplus.txt`。发布PDF：[MS-EMFPLUS](https://winprotocoldoc.z19.web.core.windows.net/MS-EMFPLUS/%5BMS-EMFPLUS%5D.pdf)，外壳：[MS-EMF](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-emf/)。重点：EmfPlusPath 2.2.1.6、Pen 2.2.1.7、LinearGradientBrushData 2.2.2.24、Header 2.3.3.3、FillPath 2.3.4.17、Object 2.3.5.1。

调研候选 `metafile-emfplus 1.0.1` 只支持解析/播放且存在功能拒绝；其发布源 `/tmp/layerexdraw-metafile/metafile-emfplus-1.0.1`，许可仅 Apache-2.0。未经项目许可审查不添加依赖，不复制其代码。当前项目许可 GPL-2.0-or-later，第三方依赖的GPL版本约束需一起评估。Wine/libgdiplus 可用于独立对照验证，但不能把其bitmap光栅器引入普通绘制路径。

测试按子系统：

1. 编码结构与外部解析器核对、官方或Windows GDI+真实参考文件解码；涵盖曲线、多figure孔洞、brush/pen/transform/clip、图像/文字、continued object、畸形长度和资源上限。
2. 在现有 `tests/layer_ex_draw.rs` 上验证 record 后清空Appearance/修改源图不影响重放；改viewTransform/Layer尺寸后仍为矢量；getRecordImage再绘到新尺寸保持语义。
3. saveRecord->loadRecord->GPU像素与原图比较，同时至少一份外部GDI+生成的EMF+参考，不以仅内部roundtrip结论替代兼容验证。
4. 正常绘图与record/redraw没有GPU读回；只有显式saveRecord导出图像资源可能读回。验证交错命令、destroy/restart与资源释放。
