# 外部 extrans 构建与展示

```sh
tools/extrans/build.sh
cargo run --example extrans_showcase
```

构建使用 `~/Workspace/extrans` 的算法源码和 `~/Workspace/krkrz/tp_stub` 的动态插件 stub，输出 `~/Workspace/extrans/extrans.so`。三个可选参数依次覆盖源码目录、stub 目录和输出路径，不修改上游源码。

`simplebinder.hpp` 仅提供 extrans 所需的入口宏，`v2link.cpp` 初始化 import stub 并调用源码原有的注册/注销函数。插件不链接宿主静态库。

独立示例使用原 showcase 的 `test_games/extrans_showcase/a1080.png` 和 `b1080.png`，其入口脚本独立放在 `examples/extrans_showcase.tjs`。可指定插件路径，并用 `--once` 跑完七种效果后退出：

```sh
cargo run --example extrans_showcase -- ~/Workspace/extrans/extrans.so --once
EXTRANS_PLUGIN="$HOME/Workspace/extrans/extrans.so" cargo test --test external_extrans -- --ignored
```

兼容范围和验证结果见 [外部插件兼容状态](../../docs/external-plugin-compatibility-status.md)。
