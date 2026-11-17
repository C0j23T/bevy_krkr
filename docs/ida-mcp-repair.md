# IDA MCP 环境修复

本机 IDA Pro 9.4 满足要求。故障来自两侧依赖不同步：Codex 的
`ida-mcp 20261028.0.3` 使用 `ida-nexus 0.14.2`，IDA 侧插件仍为
`20261008.0.1`、Nexus 为 `0.13.4`，启动 `idat` 报
`No module named 'ida_nexus._session'`。

通过官方命令修复：

```sh
uvx --from ida-hcli ida-hcli --disable-updates plugin upgrade ida-mcp
```

当前 IDA 插件位于 `~/.idapro/plugins/ida-mcp`，版本 `20261028.0.3`。
IDA Python 为 `/opt/miniconda3/envs/py314`（3.14.8），Nexus 为 `0.14.2`；
升级前后 `pip freeze` 仅 Nexus 发生变化，`ida-domain 0.5.1` 保持一致。

已用实际 MCP 完成 `open_database → reference → execute_python →
list_databases → close_database`：`/usr/bin/true` 的临时副本通过 `idat`
打开，查询得到 IDA 9.4、Nexus 0.14.2、126 个函数，stderr 为空。
最终租约已释放，数据库列表为空。临时测试文件在
`/tmp/ida-mcp-compat-check`。未验证 GUI 窗口连接。
当前 Codex MCP 连接无需重启；已运行的 IDA GUI 需重新启动以加载新包。

备份目录：`~/.local/state/ida-mcp-repair-20261117`，含旧插件、Nexus、
Domain 包及升级前后包清单。需要回退时，先关闭 IDA 和 MCP worker，再执行：

```sh
/opt/miniconda3/envs/py314/bin/python -m pip install 'ida-nexus==0.13.4'
mv ~/.idapro/plugins/ida-mcp ~/.local/state/ida-mcp-repair-20261117/ida-mcp-upgraded
cp -a ~/.local/state/ida-mcp-repair-20261117/ida-mcp ~/.idapro/plugins/ida-mcp
```

回退会重新引入此次兼容问题，应与 MCP 端版本一起评估。
安装行为依据 [Hex-Rays 官方说明](https://github.com/HexRaysSA/ida-mcp#installation)。
