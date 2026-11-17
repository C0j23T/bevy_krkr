# snbk 的 psd.dll 与 psdfile

`test_games/snbk/psd.dll` 与 `external_plugins/psdfile` 属于同一 PSD 插件实现家族，原有 TJS 接口一致，可以使用 `psd.dll` 作为加载名。二者不是同一版本或同一二进制；当前源码另有 `loadOctet`、编辑和合成功能，不能据此宣称全部行为逐字节相同。

对照源码为 `wamsoft/psdfile` 的 `7edea2b`。目标为 PE32 / Intel i386 DLL，SHA-256 为 `4dae356621df5b3e96dc5618a07ee3b959afc479b65412b69a5d0b21372fc301`。使用 IDA 9.4 / Hex-Rays，通过 IDA MCP 在 `/tmp/bevy-krkr-psd-ida/psd.dll.i64` 分析副本；原 DLL 未修改，分析会话已关闭。以下为首选基址 `0x10000000` 下的虚拟地址。

| IDA 位置 | 反编译及交叉引用证据 | 源码对应 |
| --- | --- | --- |
| `0x10001000` | 导出 `V2Link` / `_V2Link@4`，保存 exporter，遍历 ncbind 注册回调，记录全局引用基线 | `ncbind.cpp` 的标准插件入口 |
| `0x10009340` | ncbind 类登记函数；`load`、六个整数属性、`getLayerType/Name/Info`、`getLayerData/Raw/Mask`、`getSlices`、`getGuides`、`getBlend`、`getLayerComp`、`clearStorageCache` 的字符串引用均落到该函数；颜色、混合模式、层类型常量的名称、数值、顺序一致 | `psdclass.cpp` 的 `NCB_REGISTER_CLASS(PSD)` |
| `0x1000C7A0` | 登记 `load` 的适配器 RTTI 为 `InvokeCommand<PSD, bool (PSD::*)(tTJSString)>`，目标函数 `0x10005090` 返回加载状态 | `bool PSD::load(ttstr)` |
| `0x1000C830` / `0x1000C8C0` / `0x1000C950` | RTTI 分别为 `int (PSD::*)(int)`、`tTJSString (PSD::*)(int)`、`tTJSVariant (PSD::*)(int)` | `getLayerType`、`getLayerName`、`getLayerInfo` |
| `0x1000C9E0` | 三个层像素方法共用 `void (PSD::*)(tTJSVariant, int)` 适配器 | `getLayerData`、`getLayerDataRaw`、`getLayerDataMask` |
| `0x10007640` / `0x100076B0` / `0x10007720` | 三个方法转调同一函数 `0x10007290`，图像模式依次为 `2`、`0`、`1` | `_getLayerData(..., IMAGE_MODE_MASKEDIMAGE/IMAGE/MASK)` |
| `0x1000CB00` / `0x1000CA70` / `0x1000CB90` | RTTI 分别确认 `bool (PSD::*)(tTJSVariant)`、`tTJSVariant (PSD::*)()`、静态 `void (*)()` | `getBlend`、元数据方法、`clearStorageCache` |

这组证据来自实际登记函数和方法体反编译、UTF-16 方法名的交叉引用以及 ncbind 模板 RTTI，不以 DLL 文件名或孤立字符串作为判定依据。

运行 `tools/external-plugins/build.sh psdfile` 会构建 `target/external-plugins/psdfile/libpsdfile.so`，并生成同目录的相对符号链接 `libpsd.so -> libpsdfile.so`。部署时保留该链接；标准库名转换使 `Plugins.link('psd.dll')` 找到 `libpsd.so`，不需要修改宿主的文件搜索规则。

`tools/external-plugins/psdfile` 的构建适配只通过 `tp_stub` 的标准 `V2Link` / `V2Unlink` 管理两种加载名的引用。第一次加载执行原 ncbind 登记，最后一次成功卸载才注销；如果原插件拒绝卸载，引用数保持不变。两个名字指向同一共享库，重复加载不会替换 `PSD` 类。此适配不修改外部子模块源码，也不依赖宿主提供额外别名 API。

验证：`cargo test --test external_psd_alias -- --ignored --nocapture` 覆盖两种名称顺序、重复 link、类对象身份、卸载其中一个名字后继续读取 PSD/Storage、活实例拒绝最后卸载、清理后注销和重新加载。
