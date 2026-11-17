# 外部插件 GPU 接口

外部插件在标准 `V2Link(iTVPFunctionExporter *)` 中查询 GPU 扩展并注册方法。
插件只需 krkrz 的 `tp_stub.h`，不包含 Kirikiroid2 或 Bevy 私有头；普通插件的
加载流程和 tp_stub ABI 保持一致。GPU 扩展需要宿主已初始化 Bevy renderer。

通过 exporter 查询下面两个完整签名；也可在 `TVPInitImportStub` 后使用
`TVPGetImportFuncPtr`：

```cpp
tjs_uint32 ::TVPInitializeGPU(const tjs_char *,const char *)
void ::TVPApplyGPU(const tjs_char *,iTJSDispatch2 *,iTJSDispatch2 *,const tTVPRect &,const tTVPRect &,const float *,tjs_uint)
```

对应的函数指针类型：

```cpp
using Initialize = tjs_uint32 (TJS_INTF_METHOD *)(const tjs_char *name, const char *wesl);
using Apply = void (TJS_INTF_METHOD *)(const tjs_char *name,
    iTJSDispatch2 *target, iTJSDispatch2 *source,
    const tTVPRect &rect, const tTVPRect &sourceRect,
    const float *parameters, tjs_uint count);
```

`Initialize` 只在 `V2Link` 期间可调用。方法名全局唯一，不能覆盖内建方法；WESL
源码在调用期间复制。每个方法获得随机、不重复且大于 100 的 `u32` opcode，
返回值仅供诊断；执行按方法名查找。已分配的 opcode 在当前 renderer 生命周期内
不复用。内部以原始位传递 opcode，支持完整 u32 范围。

`Apply` 接受目标和源 Layer；源为空时使用目标。矩形为左上包含、右下排除的图像
坐标，源矩形映射到目标矩形。目标纹理边界裁剪保留原映射。最多 16 个 f32 参数，
`count` 为零时允许空指针，其余参数补零。宿主负责目标图像独立化、纹理同步、
GPU 命令提交和 Layer 脏区域更新；源与目标可以相同。插件无需映射像素缓冲区。

WESL 模块必须导出：

```wesl
import package::krkr::operations::{p, source, coordinate, sample};

public fn render(at: vec2<f32>, xy: vec2<i32>) -> vec4<f32> {
    let color = sample(source, coordinate(0u, at));
    return vec4(color.bg, p[2].y, color.a);
}
```

`at` 是目标像素中心，`xy` 是整数目标坐标。输出为线性通道值的 RGBA8 unorm；
与 Layer 纹理现有操作相同，不在接口内另做 sRGB 转换。每次调用读取执行前的
纹理快照，返回值写入目标像素，目标矩形外保留原像素。

可从 `package::krkr::operations` 导入以下资源和辅助函数：

| 名称 | 含义 |
| --- | --- |
| `source` | 源 Layer 纹理 |
| `destination` / `previous` | 执行前的目标纹理 |
| `p[0]` | 目标矩形 `[left, top, right, bottom]` |
| `p[2]` 到 `p[5]` | 16 个用户参数，每项 vec4；全部槽位保留 |
| `p[6]` | 源矩形 |
| `p[7]` | 目标矩形 |
| `coordinate(0u, at)` | 映射到源纹理的坐标 |
| `fetch(texture, xy)` | 整数读取，限制在纹理边界 |
| `sample(texture, at)` | 最近邻读取，限制在源矩形 |

`ShaderComposer` 为各模块生成 import 和 switch 分支，由同一 Bevy ShaderCache
处理 WESL 依赖，再创建共享布局的 compute pipeline。注册时同步验证；语法、
类型或 pipeline 验证失败会抛出 TJS 异常，已有方法和 pipeline 保持可用。
注册适合加载时完成，不宜逐帧重编译。

方法归属于当前加载的外部插件。成功卸载或链接抛出异常时，宿主移除该插件的
全部方法并重编译其余方法。已提交的 GPU 命令持有所需 GPU 资源，不调用插件
代码。插件的 `V2Unlink` 仍负责检查 TJS 对象引用并移除自己注册的脚本成员。

独立示例与真实 GPU 验证：

```sh
tools/external-plugins/build.sh gpu_fixture
cargo test --test external_gpu -- --ignored --nocapture
cargo test -p krkr_plugin --test gpu_registration -- --nocapture
```

示例源码为 `tools/external-plugins/gpu-fixture/plugin.cpp`，只包含 `tp_stub.h`。
集成测试覆盖注册、源目标映射、局部像素保护、原地操作、引用阻止卸载与重载。
注册测试覆盖多个插件、重复名、内建名、编译错误回滚、卸载和 opcode 不复用。

## 渲染插件适用性

该接口适合由源纹理、目标纹理、少量参数描述的逐像素或邻域 compute 操作。
面积平均、通道变换、图像滤镜和转场像素核均可采用同一 ShaderComposer 路径。
它不提供窗口、网格、额外存储缓冲区、自定义绑定或宿主帧调度。

| 当前待实现项 | 判断与所需能力 |
| --- | --- |
| `drawdeviceD3D.dll` | 单像素核不足。参考实现还管理 `iTVPDrawDevice`、LayerManager、窗口呈现、独立 D3DLayer/D3DPicture、前后平面、转场快照、输入与生命周期；需要通用网格及宿主合成契约。现有接口可承载其混合/转场核，不能据此宣称整个插件完成。 |
| `autotest.dll` | 依赖宿主离屏运行、帧推进、输入注入与显式抓帧；像素核不能代替这些宿主能力。 |
| `kirikiroid2.dll` 的 BPG/JXR/PVR | 依赖格式解析与解码器，输入是压缩字节而非现有 Layer 纹理；该接口不能补齐解码器。 |
| `textrender.dll` | 文本排版、链接与滚动度量为 CPU 布局逻辑；无需 GPU 像素核。 |
| `wuffmpeg.dll` | 音频解码 creator，属于音频宿主能力。 |
| `win32dialog.dll` | 对话框控件、事件与宿主 UI 交互，需对应宿主模型。 |

当前待实现列表中没有可仅靠该像素接口完整迁移的渲染插件，因此本次提供并验证
通用 GPU 能力，不新增不完整的插件替代。`LayerExDraw` 和 emote/PSB 不在本次
候选迁移范围。参考边界为 `~/Workspace/krkrsdl3/plugins/DrawDeviceD3D` 和当前
`docs/plugin-migration.md` 中各插件职责。
