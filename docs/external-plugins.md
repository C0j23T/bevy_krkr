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
