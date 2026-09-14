[English](snapshot-experiment.md) | 中文

# 快照可行性实验（阶段 18a）

> 结论先行：**路径 A（QMP `migrate` 到 `file:` + `-incoming file:`）在当前环境不可用。**
> 迁移引擎本身可用（TCP 传输可成功完成），失败点仅在 **Windows 构建下的 file/exec 传输通道**。
> 因此本步保留「重启式降级」快照，真实快照另议（见文末候选方案）。

## 环境

- OS：Windows（10.0.22631）
- QEMU：11.1.0（`v11.1.0-12130-ge470268ff4`），`C:\Program Files\qemu\qemu-system-riscv64.exe`
- 运行方式与 sandbox 一致：`-machine virt -cpu rv64 -m 128M -bios none -display none -kernel hello.elf`
  - 串口：实验中用 `-serial file:<log>`（是否为 tcp 不影响迁移语义）
  - QMP：`tcp:127.0.0.1:<port>,server=on,wait=off`

## 实验 1：`migrate` → `file:`

```text
uri: file:D:/.../snap1
reply: {"return": {}}
query-migrate: {"return": {"status": "failed",
                "error-desc": "Failed to set FD nonblocking: Input/output error"}}
快照文件：0 字节
```
源 VM 的串口正常输出 `HELLO RISCV`（说明启动与串口链路本身没问题）。

## 实验 2：变体

| 变体 | URI | 结果 |
| --- | --- | --- |
| `exec:`（cat） | `exec:cat > D:/.../snap2` | `Failed to execute helper program (No such file or directory)`（Windows 无 `cat`） |
| `exec:`（cmd） | `exec:cmd.exe /c more > D:/.../snap2` | 同上（QEMU 未做 PATH 解析 / 无 shell 重定向） |
| `file:`（前导斜杠） | `file:/D:/.../snap2` | `Could not create '/D:/.../snap2': Invalid argument` |
| `fd:` | `fd:<n>` | **无法测试**：需要把打开的文件句柄以可继承方式传给 QEMU；PowerShell 无法构造，需原生启动器 |

## 实验 3：对照组——`migrate` → `tcp:`（隔离失败点）

在另一端先起一个 `-incoming tcp:127.0.0.1:<port>` 的 QEMU，再从源 VM 迁移：

```text
reply:        {"return": {}}
query-migrate: {"return": {"status": "setup"}}
event:         {"event": "STOP"}
query-migrate: {"return": {"status": "completed", "total-time": 66,
                 "ram": {"transferred": 495403, "mbps": 60.97, ...}}}
目的端 query-status: {"return": {"status": "running", "running": true}}
```

**迁移成功**。说明：

- QEMU 的迁移引擎、QMP 控制、RAM 传输在本环境完全可用；
- 失败可精确定位到 **Windows 构建把迁移流写到 file/exec 通道**这一步（`Failed to set FD
  nonblocking`），而 TCP 通道不受影响。

## 结论

1. 路径 A 的「迁移到文件」在当前环境**不可用**；`exec:` 变体同样不可用；`fd:` 无法在本环境验证。
2. 迁移到 **TCP** 可用，但 state 落在对端 QEMU 内存里，**不会落盘**，因此不构成「文件快照」。
3. 按预先约定：**不硬上路径 B**（virtio-blk + qcow2 需重做启动链）。本步保留「重启式降级」快照，
   并在 `sandbox/README.md` 与 `PROJECT_CONSTITUTION.md` 标注 `[BLOCKED]`。

## 候选后续方案（供决策，本步未实现）

- **方案 A′（推荐，成本低）**：迁移改走 TCP，由 host 侧一个**本地文件中继**落盘——
  迁移时接受 QEMU 的连接并把字节流写文件；恢复时起一个 `-incoming tcp:` 的 QEMU，
  由中继把文件内容喂给它。不引入新依赖（`std::net` + `std::fs`），**不改 `-kernel` 启动路径**
  （仅恢复时多加 `-incoming tcp:`）。
- **方案 B（成本高）**：引入 virtio-blk + qcow2，改用 QEMU `savevm`/`loadvm`；需重做启动链与镜像管理。

## 方案 A′ 实施结果（阶段 19b/19c）

**已实现并验证通过。** 实现方式与上面“方案 A′”略有不同（实测修正）：

- **保存**：host/sandbox 起一个本地 TCP 监听（`MigrationRelay`），发 `migrate` 到该地址；
  QEMU 作为客户端接入，我们把它写出的字节流落入 `<snapshot_dir>/<name>.mig`。
  QEMU 迁移完成后并不总是关闭套接字，因此中继带**停止信号**：QMP 报 `completed` 即收尾。
- **恢复**：`-incoming tcp:<addr>` 是**目的端监听**，所以改由我们的线程**主动连接** QEMU
  并推送文件（`relay::send_file_to`），随后用 QMP `query-status` 等到 `running` 才返回。
- 实测（`sandbox/tests/snapshot_real.rs`，fixture `hello_phases.c`）：快照 ~3.5 MB；
  恢复后串口在迁移点之后继续输出 `PHASE2`；串口/审计链均正常。
- 同名快照**拒绝覆盖**（返回错误），不会静默替换旧快照。

剩余限制：host 当前不持有常驻 VM，因此 UI 只能**列出/删除**快照，“保存/恢复”留待
v0.3（需启用 `AppState.vm` 槽与跨 run 常驻）。

## 复现

实验脚本（未入库，位于会话工作区）：`experiment.ps1` / `variants.ps1` / `tcp-variant.ps1`。
