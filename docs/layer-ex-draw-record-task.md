# LayerExDraw EMF+ 记录

C++ 负责 TJS 记录状态及 Storage 读写。Rust 按不透明句柄保存绘图操作和资源。查询结构为 64 字节，包含尺寸、背景、操作数、边界和 DPI。位图导入、绘图操作附带的图片以及其克隆都使用 GPU 快照。重放组合记录变换和 viewTransform，绘制为嵌套图片时再应用源矩形裁剪。保存 EMF+ 时才需要从 GPU 读回所引用的图片。

导出写入 EMF 容器和 EMF+ 绘图记录。几何及字形轮廓写成路径，线宽使用世界坐标，零宽画笔使用设备像素单位。导出保持标准虚线样式和合成模式，裁剪通过成对图形状态保存和恢复限定作用范围。读入支持帧和 DPI、物理页单位、变换和状态栈、区域组合、几何、DrawString、DrawDriverString、DrawImage 及 DrawImagePoints。图像对象支持压缩图片、不同位深的原始位图和可递归读取的 EMF+ 图片。重放所需的格式对象在记录解码成功后整体发布。

格式兼容边界包括纯 EMF/GDI 绘图、画笔变换、任意自定义线帽以及部分高级 StringFormat 布局选项。DrawString 目前使用 Parley 对齐和布局，输出字形轮廓，支持换行、下划线及布局裁剪。原生 GDI+ 字体选择、裁剪边缘和个别画笔组合的像素一致性仍需扩大外部样本验证。

[专项测试](layerexdraw-task.md)记录已验证的范围和运行命令。[MS-EMFPLUS 规范](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-emfplus/) · [MS-EMF 规范](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-emf/)
