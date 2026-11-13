# 文件宿主层

`krkr_storage` 提供同步文件系统 trait、dummy 与 Linux 实现及 C ABI。C++ 保留 TJS、Storage 名称归一化、AutoPath、归档与 `tTJSBinaryStream` 适配；Rust 拥有文件句柄、目录访问、元数据与文件修改。资源读取不依赖 Bevy ECS，调用期间不等待下一帧。

## 接口与平台

`FileSystem` 提供打开、元数据、目录枚举、创建目录、删除文件/空目录、复制、移动、整文件替换、本地路径解析和可写根目录。`FileStream` 提供读取、写入、64 位定位、大小、截断及刷新。打开模式区分读取、覆盖写入、追加（不存在时创建）、更新（要求存在）。截断改变文件长度，不等同于移动游标。

Linux 后端复用 Rust 标准库，独占重命名使用 `libc::renameat2`；本地路径按 Unix 原始字节传递，保留非 UTF-8 文件名。路径大小写恢复使用 ASCII 不区分大小写匹配，未存在的尾部仍保留，供新文件创建。枚举保留真实名称，Storage 在 C++ 转换为规范名。

Storage 的空路径存在性检查返回 false；资源搜索未找到字体等可选文件时，不会把空路径解释成媒体名。`BevyKrkrPlugin` 在初始化前提供 Bevy 内置的 Fira Mono 字节；未找到其他字体时，FreeType 直接从内存注册该后备字体，无需临时文件。Fira Mono 子集不覆盖中文，游戏应自行提供中文字体。直接使用 FFI 的宿主可通过 `krkr_sys::font::set_fallback_font` 在启动前设置静态字体字节。

Dummy 对文件访问返回 `Unsupported`，不会伪造读写成功。Linux 默认使用 Linux 后端，其他目标默认使用 dummy；宿主可在启动引擎前安装自己的 `Arc<dyn FileSystem>`。已打开的流独立拥有生命周期，不因更换后端失效。尚未实现 Android/iOS 后端、URI 授权或 Bevy AssetReader 适配，这些目标不能据此视为已可运行。

## ABI 与错误

文件、目录快照使用不透明句柄，由对应 close 释放。字符串由 Rust 分配，由 `krkr_fs_string_free` 释放。目录项名称由快照拥有，有效期到快照关闭；C++ 在 Rust 调用返回后执行自己的回调，异常不会穿过 Rust 栈。空缓冲允许空指针，非空缓冲必须有效，句柄不得重复释放或并发使用。

状态函数以 0 表示成功、-1 表示失败；字节数用 `SIZE_MAX`、64 位返回值用 `UINT64_MAX` 表示失败，句柄与字符串用空指针表示失败。线程局部 `krkr_fs_last_error` 提供最近失败信息，Rust panic 在 ABI 边界转换为失败。

整文件替换在目标同目录独占创建临时文件，写入、刷新成功后 rename，失败时清理临时文件；这保证 Linux 上名称替换的原子性，不承诺断电持久性。Linux 的“不覆盖”移动使用 `renameat2(RENAME_NOREPLACE)`，支持文件和目录，目标已存在时失败，不支持跨文件系统移动。路径存在性与修改不是跨调用事务。

## 接入范围

当前 Bevy 编译路径的 Storage 本地流与枚举、BevyPlatform 文件函数、FileUtils、Fstat、Debug 文件日志、生成的项目/归档入口和配置文件经此层访问。外部插件加载仍由平台动态库 API 执行，文件检查经宿主层；纯路径拼接可留在 C++。未编译的上游平台实现不改写。

`set_modified` 供现有平台时间设置入口使用。Linux metadata 的 changed 表示 inode 状态变更时间，不承诺文件出生时间。

内存查询通过 `krkr_shim` 的 `sysinfo` 实现，属于独立 Rust 宿主能力，不属于文件系统 trait。Debug 的 TJS 类、回调、内存日志和宿主转发保留；本次不自动按 release 移除 Debug 行为。

## 验证

分批运行 `cargo test -p krkr_storage`、`cargo test -p krkr_shim --test config`、`cargo test -p krkr_codegen`、`cargo test --test storage`，以及引擎的 plugins、scripts_ext、restart 测试。覆盖流模式、短读/EOF、seek/截断、Unicode/大小写路径、目录、覆盖策略、替换失败、dummy、ABI 所有权与错误。目录版和 XP3 的 NVL 验收按 `docs/nvl-maker-compatibility.md` 执行。

宿主可以通过 `bevy_krkr::storage::set_filesystem` 在启动前选择后端；此选择是进程级配置，不支持在引擎运行期间切换。

本次 Linux 验证：存储后端/ABI 4 项、配置与内存 7 项、codegen 5 项、目录/XP3 集成 1 项、scripts_ext 1 项、plugins 1 项、kag_parser 1 项、restart 1 项通过。kag_parser 和 restart 在可访问 Vulkan 驱动的沙箱外运行。未运行全量测试、NVL 模板验收或移动端构建。

Fira Mono 后备字体已通过目录/XP3 的默认字体断言和 GPU 文字像素测试。`video-ffmpeg` release 的 snbk 示例运行 extrans_showcase 时完成启动脚本并创建 1920×1080 场景；验证进程在 18 秒后由超时结束。
