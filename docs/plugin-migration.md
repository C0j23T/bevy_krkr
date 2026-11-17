# 插件迁移清单

## 实现参考
参考 `~/Workspace/krkrsdl3` 中的插件实现

- [外部插件 GPU 初始化与 WESL 接口](gpu-plugin-interface.md)：随机唯一 `u32 > 100`；独立 tp_stub 插件真实 Vulkan 注册、执行、卸载与重载已验证。

## 编写规范

- 只写列表，不添加说明性文字。
- 按实际状态填写，不把已实现项列入待实现。
- 不啰嗦。
- 不要在本文档新增和删除章节。
- 调查部分写对应列表插件实现方式的探索，插件移走了也删除或移动相关内容。

## 已实现（已测试）

| 插件 | 范围 |
| --- | --- |

| `windowEx.dll` | 窗口矩形取宿主画面纹理；显示器为单一假显示器；光标、系统度量、环境变量接宿主；Win32 专属接口为空实现 |
| `PackinOne.dll` | 批量加载 `layerExBTOA`/`layerExImage`/`layerExRaster`/`shrinkCopy`/`addFont`/`yz_scripts_ext`/`fstat`/`saveStruct` |
| `layerExBTOA.dll` | 6 个 Layer 函数走 GPU；蓝色/alpha 通道、province 阈值与填色、区域 alpha；真实 Vulkan 像素验证 |
| `shrinkCopy.dll` | GPU compute 面积平均；`shrinkCopyFast` 保留分轴截断、尾部采样与不透明输出；真实 Vulkan 像素验证 |
| `layerExAreaAverage.dll` | `Layer.stretchCopyAA` 走 GPU compute；保留透明色权重与边界语义；真实 Vulkan 像素验证 |
| `json.dll` | 独立外部 submodule；标准 `tp_stub.h`；JSON 解析、序列化、Storage 文本流、Unicode、卸载重载已验证；见 [外部插件](external-plugins.md) |
| `psdfile.dll` / `psd.dll` | 原仓库移入 submodule；IDA 确认 snbk 同插件家族；双名混合加载共用注册；图层 RGBA、Storage、卸载重载与重启已验证；见 [比对证据](psd-plugin-identification.md) |
| `psbfile.dll` | 独立 tp_stub 外部 submodule；PSB v1–v3、MDF、路径/octet、`psb://` 资源、重载与损坏输入；真实 snbk `window.pimg` 已验证 |
| `sqlite3.dll` | 独立 tp_stub 外部 submodule；SQL 绑定、事务、Unicode/blob、Storage/XP3 只读 VFS、卸载重载与关闭已验证 |
| `ExtKAGParser.dll` | 独立 tp_stub 外部 submodule；扩展宏/循环/局部变量、Unicode、状态恢复与赋值；卸载恢复原 KAGParser、重载与关闭已验证 |
| `textrender.dll` | 独立 tp_stub 外部 submodule；TextRenderBase 布局、Unicode、回调、链接、滚动与公开图像尺寸协议已验证；不绘制文字 |

## 已实现（未测试）

## 部分实现

| 插件 | 范围 |
| --- | --- |
| `win32dialog.dll` | 独立 tp_stub 外部 submodule；控件模型、Unicode、公开 System 消息/输入协议、卸载已测试；宿主实际 UI、GDI、原生资源模板等未实现 |
| `layerExDraw.dll` | 文字、字体度量、简化 Appearance、GPU clear 与矩形/椭圆的不透明实色整数子集；专项像素测试通过；描边、半透明形状、路径、图像、变换与记录未完成 |

### 调查

- `layerExDraw.dll`：[Vello GPU 后端评估](layer-ex-draw-vello.md)；同设备纹理、预乘 alpha、提交顺序与 EMF+ 兼容仍需验证。

## 下一批实现

| 插件 | 难度 | 作用 | 迁移要点 |
| --- | --- | --- | --- |
| `emoteplayer.dll` | 高 | emote/PSB 动画引擎（物理、时间轴、网格、蒙版） | 依赖 `iTVPRenderBackend`，需在 Bevy GPU 实现渲染目标、网格、蒙版与读回语义 |
| `motionplayer.dll` | 低 | 加载 `emoteplayer.dll` 的别名模块 | 本身只有模块依赖，需先有 `emoteplayer` |

### 调查

见 [emote-integration.md](emote-integration.md)

## 待实现

| 插件 | 难度 | 作用 | 迁移要点 |
| --- | --- | --- | --- |
| `wuffmpeg.dll` | 中 | FFmpeg 音频解码 creator | 接入现有 ffmpeg-next，处理与 `video-ffmpeg` 的依赖共享 |
| `drawdeviceD3D.dll` | 高 | `iTVPDrawDevice` 与合成器（前后平面、转场、软件 DrawBuffer 上传） | 需宿主 LayerManager、窗口呈现与网格契约；WESL 像素接口可承载混合核，不能独立实现整个插件 |
| `autotest.dll` | 高 | 离屏自动化（帧推进、抓帧、输入注入） | 需要 Bevy 侧 headless 测试宿主与抓帧/输入注入 |
| `kirikiroid2.dll` 的 BPG/JXR/PVR 分支 | 高 | 图像加载器 | 新增解码库或格式实现；7z/ZIP/TAR/PNG/JPEG/WEBP 已完成 |

### 调查
