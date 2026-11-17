# snbk 实机验收

本地样本位于 `test_games/snbk`，游戏资源与插件二进制不入仓。
资源由引擎 Storage 与 xp3filter 读取；不能直接以通用归档工具读取加密 XP3
来判断资源是否存在。

归档与 patch 调查、按游戏引用功能分类的插件支持情况，以及 krkrsdl3 源码对照，见
[snbk 插件支持报告](snbk-plugin-support.md)。

## 运行条件

构建本机外部插件，将产物放到游戏目录或 `plugin` 子目录：

```sh
tools/external-plugins/build.sh textrender
tools/external-plugins/build.sh psbfile
cp target/external-plugins/textrender/libtextrender.so test_games/snbk/plugin/
cp target/external-plugins/psbfile/libpsbfile.so test_games/snbk/plugin/
cargo run --example snbk -- test_games/snbk/data.xp3 --dump-log /tmp/snbk-startup.log
```

`--dump-log` 保存启动脚本结束时的引擎日志；启动后的异步错误应同时从宿主
标准输出保存。Linux 验收环境为 NVIDIA RTX 4060 / Vulkan，桌面显示服务可用。

## 已观察结果

2026-11-17：实际 Bevy 示例成功创建 1280×720 窗口并挂载 9/9 个归档。
首次启动在 `initialize.tjs` 的 `KAGLoadScript` 报
`Cannot load Plugin textrender.dll`，发生在 `exec_startup` 内。
该错误涉及两项前提：游戏 `plugin` 目录原为空；插件查找把归档项目
`data.xp3` 当作目录。插件查找现对文件项目使用父目录。

专项回归 `archive_project_loads_plugin_from_game_directory` 使用自制空 XP3，
已验证游戏同级 `plugin/libtextrender.so` 能被加载、执行布局并卸载（1/1）。
原有目录项目布局、回调、滚动与卸载重载回归也通过（1/1）。

```sh
cargo test --test external_textrender archive_project_loads_plugin_from_game_directory -- --ignored
```
