# 插件迁移清单

## 实现参考
参考 `~/Workspace/krkrsdl3` 中的插件实现

## 编写规范

- 只写列表，不添加说明性文字。
- 按实际状态填写，不把已实现项列入待实现。
- 不啰嗦。
- 不要在本文档新增和删除章节。

## 已实现（已测试）

## 已实现（未测试）

## 部分实现

## 下一批实现

| 插件 | 难度 | 作用 | 迁移要点 |
| --- | --- | --- | --- |
| `windowEx.dll` | 低 | Window/MenuItem/Pad/System 的 Win32 兼容面，大部分为空实现 | 补 Bevy 窗口尺寸与光标/显示器信息映射 |

## 待实现

| 插件 | 难度 | 作用 | 迁移要点 |
| --- | --- | --- | --- |
| `gfxFire.dll` / `gfxEffect.dll` | 低 | 注入 `gfxFire` TJS 类 | 两个文件除模块名外相同，二选一注册 |
| `motionplayer.dll` | 低 | 加载 `emoteplayer.dll` 的别名模块 | 本身只有模块依赖，需先有 `emoteplayer` |
| `PackinOne.dll` | 低 | 批量加载 layerExBTOA/Image/Raster/shrinkCopy/addFont/scriptsEx | 内部模块已自动注册，主要保留模块入口 |
| `json.dll` | 低 | JSON 解析与序列化 TJS 类 | 纯逻辑，走 Storage 文本流，补独立样例 |
| `win32dialog.dll`（完整版） | 低 | 自绘 Win32 对话框控件模型 | 替换现有 15 行 stub，接入宿主消息框与输入框 |
| `layerExBTOA.dll` | 低 | 6 个 Layer CPU 像素函数（Alpha 搬运/填充） | 走 `mainImageBuffer(ForWrite)`，上传由现有桥完成 |
| `shrinkCopy.dll` | 低 | `Layer.shrinkCopy` / `shrinkCopyFast` CPU 缩放拷贝 | 同 CPU 像素桥；对照裁剪与 pitch 语义 |
| `layerExAreaAverage.dll` | 低 | `stretchCopyAA` CPU 面积平均缩放 | 同 CPU 像素桥；逐像素对照验证 |
| `psbfile.dll` | 中 | PSB/MDF 解析、`PSBFile` 与 `psb` 存储 media | 当前未链接 zlib，需加依赖或改走 `krkr_zlib_*`；解除 SNBK 启动阻塞 |
| `textrender.dll` | 中 | `TextRenderBase` 文本排版、链接与滚动度量 | 纯布局不绘制；需补 `Layer_FetchImageSize`，与 KAG 文本行为对照 |
| `sqlite3.dll` | 中 | SQLite TJS 绑定与 XP3 VFS | 用 `cc` 编译 vendored `sqlite3.c`，VFS 接 Storage |
| `wuffmpeg.dll` | 中 | FFmpeg 音频解码 creator | 接入现有 ffmpeg-next，处理与 `video-ffmpeg` 的依赖共享 |
| `ExtKAGParser.dll` | 中 | 替代全局 `KAGParser` 的扩展解析器与 `TJSAry`/`TJSDic` | 纯解析；需与核心 KAGParserEx 做特性差异确认 |
| `emoteplayer.dll` | 高 | emote/PSB 动画引擎（物理、时间轴、网格、蒙版） | 依赖 `iTVPRenderBackend`，需在 Bevy GPU 实现渲染目标、网格、蒙版与读回语义 |
| `drawdeviceD3D.dll` | 高 | `iTVPDrawDevice` 与合成器（前后平面、转场、软件 DrawBuffer 上传） | 重做合成路径为 GPU 命令，避免逐帧 CPU 读回与上传 |
| `layerExDraw.dll`（完整 API） | 高 | GDI+ 兼容的矢量、文字与图像绘制 | 补齐路径、渐变、矩阵、图像与记录接口；当前仅文字子集 |
| `autotest.dll` | 高 | 离屏自动化（帧推进、抓帧、输入注入） | 需要 Bevy 侧 headless 测试宿主与抓帧/输入注入 |
| `kirikiroid2.dll` 的 BPG/JXR/PVR 分支 | 高 | 图像加载器 | 新增解码库或格式实现；7z/ZIP/TAR/PNG/JPEG/WEBP 已完成 |
