# sandbox

智芯城（RiscDom）的 **QEMU RISC-V 沙箱层**。

在 QEMU `virt` 机器上启动 RISC-V 裸机 ELF，通过 QMP 控制，捕获串口输出，
并提供（MVP 降级的）快照/回滚能力。所有对外操作都会写入审计。

## 模块

- `vm` — `RiscVVirtualMachine` / `VMConfig`：QEMU 生命周期、串口捕获、快照
- `platform` — `QmpEndpoint` / `SerialEndpoint`：平台端点抽象（端点 → QEMU 参数）
- `qmp` — `QmpClient`：最小 QMP 客户端（greeting / `qmp_capabilities` / `stop` / `cont` / `quit`）
- `error` — `SandboxError`

## 审计（audit crate）

审计实现来自 **`audit` crate**（append-only SQLite + SHA-256 hash chain）。
依赖方向为 `sandbox → audit`；`sandbox` 不自定义审计类型。

- 构造函数要求 `Arc<Mutex<dyn AuditSink>>`（`AuditSink` 的 `record` 为 `&mut self`）。
- 直接写 SQLite（append-only），不再是 JSONL 占位。
- 事件 `actor` 统一为 `"sandbox"`，当前类型：
  `vm.start` / `vm.stop` / `vm.snapshot.save` / `vm.snapshot.load` / `serial.read` / `serial.write`。

`audit::FileAuditSink` 保留在 `audit` crate 中**仅作示例**，生产请用 `audit::SqliteAuditSink`。

## 平台限制

1. **Windows：全部走 TCP。** QMP 用 `tcp:host:port,server=on,wait=off`，串口用
   `tcp:host:port,server=on,wait=on`。不使用 Unix domain socket，不使用 `mon:stdio`。
2. **Unix：** `QmpEndpoint::UnixSocket` 已在类型层预留（`#[cfg(unix)]`），但 MVP 尚未实现，
   连接时会返回 `SandboxError::Unsupported`。当前只在 Windows 上测试通过。
3. **串口 `wait=on` 的意义：** QEMU 会阻塞到宿主机连上串口 socket 才开始运行 guest，
   从而保证开机早期输出不被丢失（这是捕获线程与 guest 之间的竞态解法）。
4. **`-bios none` 是必需的：** 否则默认 OpenSBI 固件会占用 `0x80000000`。
   guest ELF 的入口必须链接在镜像最前（见 `tests/fixtures/link.ld` 的 `.text.start`），
   因为 `-bios none` 下 QEMU 从 `0x80000000` 起跳。
5. 需要宿主安装 `qemu-system-riscv64`。可用环境变量 `RISCDOM_QEMU` 指定其绝对路径；
   否则按常见安装路径 / `PATH` 解析。

## MVP 快照降级方案

`save_snapshot` / `load_snapshot` **不是真正的虚拟机状态快照**：

- `save_snapshot(name)`：把 `(kernel path, memory_mb, qemu args, timestamp)` 序列化为
  JSON，写入 `<snapshot_dir>/<name>.json`，`mode` 字段标记为 `"mvp-reboot"`。
- `load_snapshot(name)`：停止当前 VM，读取 JSON，用相同参数重启。

也就是说，MVP 的“回滚”等于“用相同参数重启”。设备状态与内存不会被保存。

## 测试

```text
cargo test -p sandbox
```

- `tests/smoke.rs`（3a）：启动 QEMU → 捕获 `HELLO RISCV` → 停止 → 校验审计链 Intact
- `tests/snapshot.rs`（3b）：启动 → 保存快照 → 停止 → 加载 → 再启动成功
- `tests/fixtures/`：最小 RISC-V 裸机 guest（`hello.c` + `link.ld`）

需要 `riscv64-unknown-elf-gcc` 编译 fixture（可用 `RISCDOM_RISCV_GCC` 指定路径）。

## 示例

```text
cargo run -p sandbox --example run_hello
```

启动 guest、打印串口输出，并把审计写入 SQLite 后调用 `verify_chain` 打印结果。

可再用独立工具复核：

```text
cargo run -p audit --bin audit-verify -- <path-to-db>
```

## v0.2 TODO

- QEMU `savevm` / `loadvm` 真实快照（替换 MVP 重启式降级方案）
- Unix socket 支持（macOS / Linux，`QmpEndpoint::UnixSocket`）
- virtio 设备（块设备 / 网络）
- sandbox 主动回调串口（替代 host 轮询）
