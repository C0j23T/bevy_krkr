# 外部插件

插件独立构建，只通过 krkrz 的 `tp_stub.h` 访问宿主。`TPSTUB_DIR` 指向
`krkrz/tp_stub`，`NCBIND_DIR` 指向 upstream ncbind，不包含 Kirikiroid2 私有头。

```sh
tools/external-plugins/build.sh json
cargo test --test external_logic -- --ignored
```

将生成的 `target/external-plugins/json/libjson.so` 放到游戏目录或其 `plugin`
目录，脚本调用 `Plugins.link("json.dll")`。

动态加载遵循 krkrz generic 的命名规则：Linux 下小写 `.dll` 替换为 `.so`，
没有 `lib` 前缀时补上。其他后缀保持原样，只补 `lib` 前缀。按转换后的名称先通过
Storage 自动搜索路径查找，再查找宿主应用目录与 `plugin` 目录。Bevy 宿主的
`ExePath()` 对应当前游戏项目目录。归档内的插件由 Storage 临时文件持有器解包，
持有器存活到插件卸载。

加载去重使用传入名称的精确匹配；存在 `V2Link` 时调用，不以其返回值判定失败。
卸载前触发 TJS 垃圾回收，`V2Unlink` 负返回值阻止卸载。退出时按加载顺序逆序卸载。
外部转场仍在使用时拒绝卸载，保护在 GPU 提交中引用的插件对象。
支持 `RTLD_NODELETE` 的平台与 krkrz 一样保留库映射，直到进程退出，确保 TJS
关闭时残留的暂存对象仍能调用有效代码；卸载仍移除插件成员和 GPU 方法。

PSD 插件源码位于子模块 `external_plugins/psdfile`。构建脚本会应用
`tools/external-plugins/psdfile-bmp-header.patch`，确保非 Windows 平台的 BMP
文件头采用 2 字节对齐；源码中的兼容补丁保持为子模块工作区修改。

```sh
git submodule update --init --recursive external_plugins/psdfile
tools/external-plugins/build.sh psdfile
ALPHA_MOVIE_PLUGIN=$HOME/Workspace/AlphaMovie/AlphaMovie.so \
  cargo test --test external_classes -- --ignored --nocapture
```

PSD 构建产物为 `target/external-plugins/psdfile/libpsdfile.so`，测试默认使用该路径；
`PSD_FILE_PLUGIN` 可指定其他插件二进制。测试覆盖 PSD 图层像素、Storage BMP
读取以及 AlphaMovie 的 JPEG/zlib 帧与透明度。

IDA 对照确认 snbk 的 `psd.dll` 属于相同插件家族，见
[比对证据](psd-plugin-identification.md)。构建同时生成相对链接
`libpsd.so -> libpsdfile.so`，部署时保留链接，即可使用 `Plugins.link("psd.dll")`。
两名称混用只注册一次；最后一个名称卸载时才注销，活实例仍阻止最后卸载。

## PSB

```sh
tools/external-plugins/build.sh psbfile
cargo test --test external_psb -- --ignored
```

`PSBFile` 支持 PSB v1–v3、普通 zlib MDF、Storage 路径与 octet 输入，以及
`psb://容器/资源键` 只读媒体。MDF 资源从解压后的容器读取；重复加载替换旧状态。
不支持加密 PSB、v4、MFL、写入或媒体目录枚举。标准测试使用合成容器，
设置 `PSB_SAMPLE` 可额外验证一个本地真实容器，不运行游戏。

## SQLite 与扩展 KAG

```sh
tools/external-plugins/build.sh sqlite3
tools/external-plugins/build.sh ExtKAGParser
cargo test --test external_sqlite --test external_kag -- --ignored --test-threads=1
```

`sqlite3.dll` 提供 `Sqlite` / `SqliteStatement`、绑定、事务、Unicode/blob，以及
只读 Storage / XP3 VFS；写入数据库使用标准本地路径。`ExtKAGParser.dll`
加载时替换全局 `KAGParser`，卸载时恢复原值；覆盖宏、循环、局部变量、参数宏、
跨行标签、状态保存恢复和赋值。两个独立仓库均只依赖标准 tp_stub 与 ncbind，
有活实例时拒绝卸载，已验证卸载重载和宿主关闭。

## 对话框模型

```sh
tools/external-plugins/build.sh win32dialog
cargo test --test external_dialog -- --ignored
```

`win32dialog.dll` 已替代内置 stub，提供模板、控件状态与消息处理，通过公开
`System.inform` / `System.inputString` 调用宿主。模型与调用协议已测试；当前
宿主仅记录请求并返回取消，没有交互 UI。原生资源模板、GDI、property sheet
及 modeless 窗口仍不支持，不能作为 Win32 完整对话框实现。
