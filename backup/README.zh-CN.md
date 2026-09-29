[English](README.md) | 中文

# backup

`riscdom-backup` —— **可移植性工具**（v1.0 M7e），依 [docs/backup.md](../docs/backup.md) 冻结的样子。
[decisions §19](../docs/decisions.zh-CN.md) 定下了它为什么存在：*能走，才是敢留的前提* —— 它写出的那个
包**就是**可移植性的单位。

**今天这里有什么：整个节点，封进一个文件（批 AX/AY，AV-1 + AV-2）。** `export` 读
[backup.md](../docs/backup.zh-CN.md) §1 的两个根，把它们封进**一个加密的包**：

- **数据目录** —— `settings.json`、`sessions.db`、`token`、`node.key`、`peers.json`、`rooms.json`
  （§1.1）；
- **workspace 的 `.riscdom/`** —— **审计存储**与**快照**（§1.2）。审计存储经 **SQLite 自己的一致性路径**
  （`VACUUM INTO`）取出，绝不逐字节拷贝：`audit.db` 是 WAL 且多进程，单对文件拷贝会漏掉仍在 `-wal` 里的帧。
  快照整棵遍历、连同树结构一起带走；
- **keyring 的凭据**（§1.4），**反推**而非枚举 —— OS keyring 无法被列出（`keyring` v3 按 service 与
  account 查找凭据），所以账户名来自 `settings.json`：`llm-api-key:<executor_id>:<provider_id>`、legacy 的
  `llm-api-key:<provider_id>`、以及取自 `network.remote_url` 的 `remote-token:<host>`。

**没有任何东西被悄悄丢掉。** 凡是点不出名的 —— 一个 `settings.json` 暗示、keyring 却没有的账户，或一个
本 build 读不了的 `settings.json` —— 都进清单的 **`not_derived`** 列表，命令也会把这些行打印出来。列表里
还有一条长期声明：keyring 无法被列出，所以一个其 executor 或 host 已从 `settings.json` 消失的凭据
**找不到**，而 [backup.md](../docs/backup.zh-CN.md) §1.4 称这是这个包唯一声明的包外依赖。

**归档是封口的**，用 **AES-256-GCM**、密钥由运维者口令经 **PBKDF2-HMAC-SHA256** 推得；包头（magic、
salt、nonce、轮数）是 AEAD 的附加数据，所以改它会让包读不了、而不只是读错。口令错与被改文件给出同一个
答案，因为认证加密分不开它们。

**口令绝不走错路。** 它不从命令行接受、绝不被本工具写到盘上、绝不被打印。它来自
`--passphrase-from-env <VAR>`，或来自**作为管道时的 stdin**；终端提示是最后手段，会打印一句警告，因为
终端会回显你输入的东西。

## 用法

```
riscdom-backup export --data-dir <dir> --workspace <dir> --output <path> [--passphrase-from-env <VAR>] [--force]
```

- `--data-dir` —— 节点存放其文件的目录（`riscdom-server --data-dir` 被赋予的那个，或桌面的 app-data
  目录）。
- `--workspace` —— 节点所工作的目录；它的 `.riscdom/` 存放审计存储与快照。两个根都必填：只备一半不是
  §19 定义的那个单位。
- `--output` —— 包写到哪里，惯例是 `riscdom-backup-<node_id>-<timestamp>.rdbak`。没有 `--force` 时它
  拒绝替换已存在的文件。
- `--passphrase-from-env <VAR>` —— 不回显地把口令交出去的方式。管道到 stdin 也行：
  `riscdom-backup export … < passphrase-file`。

退出码：`0` 成功、`2` 用法错误、`1` 失败（目录缺失、`--output` 已存在、SQLite 拒绝导出的审计存储、读取时
口令错）。命令打印文件数、输出路径，以及每一行 `not carried:` —— 绝不打印口令、绝不打印任何文件的内容。

## 布局

- `src/lib.rs` —— 导出：收集两个根、读每个文件的标记、反推凭据、构建 tar、gzip、封口。`export`、
  `export_with`（注入 keyring）、`decrypt` 与 `read_manifest` 是公开表面。
- `src/main.rs` —— 命令行，包括口令的各个来源。
- `tests/export.rs` —— 端到端视角：一个真数据目录与 workspace、一个封好的包、把清单再读出来。

本 crate 依赖 `host-core`（它的数据目录路径、设置类型、keyring）与 `agent`（节点的设备名，也就是它的
`node_id`）。审计存储需要 `rusqlite`，它已在 `Cargo.lock` 里，所以没有新增包。它**不**依赖 `net`、
`server` 或 `cli`，所以它从那些 crate 复述的文件名就写在它们定义的旁边。

**还没有的：** `import`（恢复一个包）是后续批次。
