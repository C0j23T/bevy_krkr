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

## PSB

```sh
tools/external-plugins/build.sh psbfile
cargo test --test external_psb -- --ignored
```

`PSBFile` 支持 PSB v1–v3、普通 zlib MDF、Storage 路径与 octet 输入，以及
`psb://容器/资源键` 只读媒体。MDF 资源从解压后的容器读取；重复加载替换旧状态。
不支持加密 PSB、v4、MFL、写入或媒体目录枚举。标准测试使用合成容器，
设置 `PSB_SAMPLE` 可额外验证一个本地真实容器，不运行游戏。
