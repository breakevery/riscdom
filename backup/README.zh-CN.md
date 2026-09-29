[English](README.md) | 中文

# backup

`riscdom-backup` —— **可移植性工具**（v1.0 M7e），依 [docs/backup.md](../docs/backup.md) 冻结的样子。
[decisions §19](../docs/decisions.zh-CN.md) 定下了它为什么存在：*能走，才是敢留的前提* —— 它写出的那个
包**就是**可移植性的单位。

**今天这里有什么：数据目录，封进一个文件（批 AX / AV-1）。** `export` 读一个节点的数据目录 ——
`settings.json`、`sessions.db`、`token`、`node.key`、`peers.json` 与 `rooms.json`
（[backup.md](../docs/backup.zh-CN.md) §1.1）—— 写一份**清单**，逐条记下每个文件的大小、SHA-256 与它格式
的标记，再把这一整批封进**一个加密的包**：一个 gzip 过的 tar（`manifest.json` 加 `data-dir/*`），其下是
**AES-256-GCM**，密钥由运维者口令经 **PBKDF2-HMAC-SHA256** 推得。口令错与文件被改给出同一个答案，因为
认证加密分不开这两者、也不假装能分开。

**口令绝不走错路。** 它不从命令行接受、绝不被本工具写到盘上、绝不被打印。它来自
`--passphrase-from-env <VAR>`，或来自**作为管道时的 stdin**；终端提示是最后手段，会打印一句警告，因为
终端会回显你输入的东西。

**还没有的（AV-2）：审计存储、快照、以及 keyring。** `audit.db` 与 `<workspace>/.riscdom/snapshots/`
是**第二个根**（[backup.md](../docs/backup.zh-CN.md) §1.2），而 keyring 条目是 §1.4 里那部分必须
**从 `settings.json` 反推**（OS keyring 没有枚举 API）、反推不出来时**被报告**的东西。AV-2 落地之前，
`export` 不带历史、不带快照、也**不带任何凭据** —— 而清单会说出来，在它的 `not_derived` 列表里，而不是
让读者去假设这个包是整个节点。

## 用法

```
riscdom-backup export --data-dir <dir> --output <path> [--passphrase-from-env <VAR>] [--force]
```

- `--data-dir` —— 节点存放其文件的目录（`riscdom-server --data-dir` 被赋予的那个，或桌面的 app-data
  目录）。
- `--output` —— 包写到哪里，惯例是 `riscdom-backup-<node_id>-<timestamp>.rdbak`。没有 `--force` 时它
  拒绝替换已存在的文件。
- `--passphrase-from-env <VAR>` —— 不回显地把口令交出去的方式。管道到 stdin 也行：
  `riscdom-backup export … < passphrase-file`。

退出码：`0` 成功、`2` 用法错误、`1` 失败（`--data-dir` 缺失、`--output` 已存在、读取时口令错）。命令打印
文件数与输出路径 —— 绝不打印口令、绝不打印任何文件的内容。

## 布局

- `src/lib.rs` —— 导出：收集数据目录、读每个文件的标记、构建 tar、gzip、封口。`export`、`decrypt` 与
  `read_manifest` 是公开表面。
- `src/main.rs` —— 命令行，包括口令的三个来源。
- `tests/export.rs` —— 端到端视角：一个真数据目录、一个封好的包、把清单再读出来。

本 crate 依赖 `host-core`（它的数据目录路径）与 `agent`（节点的设备名，也就是它的 `node_id`），此外不依赖
本 workspace 的任何东西。它**不**依赖 `net`、`server` 或 `cli`，所以它从那些 crate 复述的文件名就写在它们
定义的旁边。
