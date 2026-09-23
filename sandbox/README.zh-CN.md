[English](README.md) | 中文

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

### 真实快照：TCP 中继方案（方案 A′），已于 v0.2 实现

真实快照**已实现**：sandbox 通过 QMP `migrate` 迁移到本地 TCP 中继，由中继把迁移流落盘为
`<snapshot_dir>/<name>.mig`；恢复时把该文件喂给以 `-incoming tcp:` 启动的 QEMU。

- 为什么不用文件 URI：`migrate` → `file:` 在 Windows + QEMU 11.1.0 不可用
  （`Failed to set FD nonblocking`），`exec:` / `fd:` 变体同样不可用，而 `tcp:` 通道正常。
  当时的探索过程作为**历史记录**保留在
  [`docs/snapshot-experiment.md`](docs/snapshot-experiment.md)。
- 方案 B（virtio-blk + qcow2 + `savevm`/`loadvm`）未采用。
- 旧的重启式降级（`.json`）保留兼容。
- 残留限制：同名快照**拒绝覆盖**；VM 归 host 持有（`AppState::vm_slot`），UI 可保存/恢复
  （阶段 20b–20d）。

#### 端口租约的契约

QMP 与串口端口来自进程级租约（`relay::lease_local_ports`），不是每个调用方各自 `bind(0)`。它确切承诺的是：

- **两个同时存活的租约绝不携带同一个号码。** 检查与登记在同一个锁作用域内是一次操作（`relay::reserve`），
  且租约在 `hand_off` 之前一直持着监听器，所以 OS 也无法把该端口给别人。
- **已释放的号码会回到池子里。** 租约被销毁（或已移交）即释放该号码，OS 可以再把它发出去——发给本进程，也发给别的进程。
  正因如此，调用方用新端口重试，而不是假定端口在 QEMU 起来之前一直归自己（`port_race.rs` 走的就是这个跨进程窗口，默认忽略）。
- 因此它**不是**「进程存活期间某号码绝不二次发出」：对已释放号码做隔离（quarantine）被考虑并否决（会无界增长，
  且改变一个没人需要的契约）。

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

- ~~真实快照~~ **已完成**（方案 A′：TCP 迁移 + 本地文件中继；阶段 19b/20c）
- Unix socket 支持（macOS / Linux，`QmpEndpoint::UnixSocket`）
- virtio 设备（块设备 / 网络）
- ~~sandbox 主动回调串口~~ ✅ 已完成（阶段 15a）
