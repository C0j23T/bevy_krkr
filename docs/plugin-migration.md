# 插件迁移清单

## 实现参考
参考 `~/Workspace/krkrsdl3` 中的插件实现

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
| `psdfile.dll` | 原仓库移入 `external_plugins/psdfile` submodule；PSD 图层 RGBA、Storage BMP、卸载重载与宿主重启已验证 |

## 已实现（未测试）

## 部分实现

| 插件 | 范围 |
| --- | --- |

### 调查

## 下一批实现

| 插件 | 难度 | 作用 | 迁移要点 |
| --- | --- | --- | --- |
| `psbfile.dll` | 中 | PSB/MDF 解析、`PSBFile` 与 `psb` 存储 media | 当前未链接 zlib，需加依赖或改走 `krkr_zlib_*`；解除 SNBK 启动阻塞 |
| `emoteplayer.dll` | 高 | emote/PSB 动画引擎（物理、时间轴、网格、蒙版） | 依赖 `iTVPRenderBackend`，需在 Bevy GPU 实现渲染目标、网格、蒙版与读回语义 |
| `motionplayer.dll` | 低 | 加载 `emoteplayer.dll` 的别名模块 | 本身只有模块依赖，需先有 `emoteplayer` |

### 调查

见 [emote-integration.md](emote-integration.md)

## 待实现

| 插件 | 难度 | 作用 | 迁移要点 |
| --- | --- | --- | --- |
| `win32dialog.dll`（完整版） | 低 | 自绘 Win32 对话框控件模型 | 替换现有 15 行 stub，接入宿主消息框与输入框 |
| `textrender.dll` | 中 | `TextRenderBase` 文本排版、链接与滚动度量 | 纯布局不绘制；需补 `Layer_FetchImageSize`，与 KAG 文本行为对照 |
| `sqlite3.dll` | 中 | SQLite TJS 绑定与 XP3 VFS | 用 `cc` 编译 vendored `sqlite3.c`，VFS 接 Storage |
| `wuffmpeg.dll` | 中 | FFmpeg 音频解码 creator | 接入现有 ffmpeg-next，处理与 `video-ffmpeg` 的依赖共享 |
| `ExtKAGParser.dll` | 中 | 替代全局 `KAGParser` 的扩展解析器与 `TJSAry`/`TJSDic` | 纯解析；需与核心 KAGParserEx 做特性差异确认 |
| `drawdeviceD3D.dll` | 高 | `iTVPDrawDevice` 与合成器（前后平面、转场、软件 DrawBuffer 上传） | 重做合成路径为 GPU 命令，避免逐帧 CPU 读回与上传 |
| `layerExDraw.dll`（完整 API） | 高 | GDI+ 兼容的矢量、文字与图像绘制 | 补齐路径、渐变、矩阵、图像与记录接口；当前仅文字子集 |
| `autotest.dll` | 高 | 离屏自动化（帧推进、抓帧、输入注入） | 需要 Bevy 侧 headless 测试宿主与抓帧/输入注入 |
| `kirikiroid2.dll` 的 BPG/JXR/PVR 分支 | 高 | 图像加载器 | 新增解码库或格式实现；7z/ZIP/TAR/PNG/JPEG/WEBP 已完成 |

### 调查
