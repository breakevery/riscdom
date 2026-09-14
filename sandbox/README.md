# sandbox

智芯城（RiscDom）的 **QEMU RISC-V 沙箱层**。

在 QEMU `virt` 机器上启动 RISC-V 裸机 ELF，通过 QMP 控制，捕获串口输出，
并提供（MVP 降级的）快照/回滚能力。所有对外操作都会写入 `AuditSink`。

## 模块

- `vm` — `RiscVVirtualMachine` / `VMConfig`：QEMU 生命周期、串口捕获、快照
- `platform` — `QmpEndpoint` / `SerialEndpoint`：平台端点抽象（端点 → QEMU 参数）
- `qmp` — `QmpClient`：最小 QMP 客户端（greeting / `qmp_capabilities` / `stop` / `cont` / `quit`）
- `audit_sink` — `AuditSink` trait + `FileAuditSink`（占位实现）
- `error` — `SandboxError`

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

## AuditSink 是占位接口

`audit_sink` 模块中的 `AuditSink` trait 与 `FileAuditSink` 只是**占位实现**，
用于让每个对外操作从第一天就产生审计事件（JSONL 追加）。

真正的 append-only + hash chain 实现属于 `audit` crate，落地后会替换 `FileAuditSink`。
按项目宪法：审计日志在 AI 之外、append-only、不可关闭。

`actor` 统一为 `"sandbox"`；当前事件类型：

- `vm.start` / `vm.stop`
- `vm.snapshot.save` / `vm.snapshot.load`
- `serial.read` / `serial.write`

## 测试

```text
cargo test -p sandbox
```

- `tests/smoke.rs`（3a）：启动 QEMU → 捕获 `HELLO RISCV` → 停止 → 校验审计
- `tests/snapshot.rs`（3b）：启动 → 保存快照 → 停止 → 加载 → 再启动成功
- `tests/fixtures/`：最小 RISC-V 裸机 guest（`hello.c` + `link.ld`）

需要 `riscv64-unknown-elf-gcc` 编译 fixture（可用 `RISCDOM_RISCV_GCC` 指定路径）。

## 示例

```text
cargo run -p sandbox --example run_hello
```

## v0.2 待办

- 用 QEMU `savevm` / `loadvm` 替换 MVP 快照降级方案（真实内存 + 设备状态）
- 接入 `gdbstub`（TCP）做调试
- virtio 设备（块设备 / 网络）
- Unix socket 端点实现（`QmpEndpoint::UnixSocket`）
- 用真正的 `audit` crate 替换 `FileAuditSink`
