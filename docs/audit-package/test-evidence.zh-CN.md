[English](test-evidence.md) | 中文

# RiscDom —— 测试证据

> **数字从哪来。** 一次真实的 `scripts/gate.sh` 运行，跑在
> [ENVIRONMENT.md](../../ENVIRONMENT.md) 那台机器上（Windows，`PATH` 上有 QEMU 11.1.0 与
> xPack RISC-V GCC 15.2.0），对象是提交 `54f1f09` 加本包自己的文档。这里没有一条引自更早的
> 运行。gate 是「绿」的**唯一**清单：CI 逐字跑同一个文件。

## 1. 总览

| 项 | 值 |
|---|---|
| gate 结果 | **`gate: OK`**（26 步，步步为绿） |
| gate 所跑测试 | **1018 passed；0 failed** |
| 报告结果的测试二进制 / 套件 | **146** 行 `test result:`（单元测试、集成测试与 doc-tests） |
| gate 所用命令 | `cargo test --no-fail-fast -- --include-ignored --skip real_deepseek_writes_and_runs_hello_world --skip real_api_streams_content_deltas --skip os_keyring_persists_to_credential_manager` |
| 按名跳过的测试 | **3**（它们需要模型 API key 或操作系统凭据库） |
| 盘上的 `#[ignore]` 属性 | **63** 个，分布于 **38** 个文件 |
| 双语配对 | 本节运行时 **142** 个文件；本包补齐后 **144**（本文档是最后一对） |

gate 用了 **`--include-ignored`**，因为本机有访客工具，所以平时被 `#[ignore]` 的测试**确实跑
了** —— 除了上面点名的三条。这就是日志里处处 `ignored: 0` 的原因。

## 2. gate 实际跑的东西（26 步，按序）

1. `cargo fmt --all -- --check`
2. `cargo clippy -p audit -p sandbox -p agent --all-targets -- -D warnings`
3. `cargo check -p audit -p sandbox -p agent`
4. `cargo clippy -p cli -p server -p host-core -p host-tauri -p worker -p net -p riscdom-backup -p riscdom-sdk --all-targets --no-deps -- -D warnings`
5. `cargo clippy --manifest-path ui/src-tauri/Cargo.toml --all-targets -- -D warnings`
6. `cargo check --manifest-path ui/src-tauri/Cargo.toml`
7. `cargo test`（上面那行命令）
8. `npm run build`（前端）
9. **16 个 UI probe** —— scroll、width、runs、snapshot、dialog、preflight、theme、i18n、api、login、sse、read-only、node panel、network tab、LAN、executor picker（最后一个是 `probe-ui-remote`）
10. `node --test sdk/typescript/test/*.test.ts`
11. `node scripts/check-mirrored-constants.mjs`
12. `python scripts/scan-encoding.py --check`（乱码 + BOM）
13. `node scripts/check-tool-schema.mjs`
14. `python examples/python/dispatch.py --self-test`
15. `python examples/python/supervisor.py --self-test`
16. `cargo run -q -p worker --example remote_executor -- --self-test`
17. `cargo run -q -p net --example identity -- --self-test`
18. `cargo run -q -p net --example sign -- --self-test`
19. `cargo run -q -p net --example transport -- --self-test`
20. `cargo run -q -p net --example discovery -- --self-test`
21. `cargo run -q -p net --example rooms -- --self-test`
22. `cargo run -q -p net --example relay -- --self-test`
23. `node scripts/check-wix-version.mjs`
24. `node scripts/check-ui-strings.mjs`
25. `sh -n scripts/pack.sh`（打包脚本语法）
26. `sh scripts/check-bilingual.sh` —— 双语配对

（平台跑不了的，gate 会**打印一条带原因的 skip** —— 例如没有 Python 的机器上的两条 Python 自测与
编码扫描。没有静默跳过。）

## 3. 真 QEMU 与 mock

RiscDom 的测试有两种形态，这个区别对评审者很重要：

- **真 QEMU 访客。** sandbox 的测试，以及 host-core 的端到端测试，会真的启动
  `qemu-system-riscv64`、启动一个裸机镜像、读回串口输出。这些是证明产品可用的测试，也是无 QEMU 的
  机器跑不了的测试。
- **mock 模型，真访客。** `host-core` 多数端到端测试用 **mock LLM**（因此无需 key、无需网络），
  但仍启动真访客。点名的那个是 `golden_path.rs`；同样的形状在 `dispatch`、`session_integration`、
  `vm_*`、`qemu_*` 与快照测试里反复出现。
- **完全没有访客。** 解析、存储、签名、路由、路由表与能力词汇表的单元测试在任何地方都能跑。

按每个带 `#[ignore]` 的**文件**所触及的东西分类（据其内容做 `qemu` / `gcc` / `mock` / `key`
标记扫描，不是逐条测试的审计，所以当图看，别当普查）：

| 组 | 文件（含 `#[ignore]` 测试） |
|---|---|
| 启动真访客 | `sandbox/tests/{smoke,snapshot,snapshot_real,qemu_discover,serial_observer,port_race}.rs`；`host-core/tests/{qemu_commands,qemu_injection,qemu_path_snapshot,vm_lifecycle,vm_poweroff,vm_status,run_diagnosis,run_provenance,golden_path,dispatch,e2e_ui,session_integration,snapshot_commands,serial_subscription,sandbox_request_tools,stream_forwarding}.rs`；`agent/tests/{vm_injection,zig_vm,read_serial_continuous,read_serial_empty,serial_subscribe,e2e_mock,stream_real}.rs` |
| 真的编译 C（需要 RISC-V GCC） | `agent/tests/{compiler_cleanup,compiler_discovery,compiler_parallel,tools}.rs` |
| 需要模型 key 或 OS 凭据库 | `agent/tests/{real_api,stream_real}.rs`；`host-core/tests/{keyring_os,settings,preflight,toolchain_commands}.rs` |

`agent` 有两个单元测试真的编译 C，没有 GCC 时打印 skip；三条按名跳过的，是需要 key 或凭据库的。

## 4. `#[ignore]` 测试，按 crate 分组

**63** 个 `#[ignore]` 属性分布在 **38** 个文件：**agent 15**、**host-core 40**、**sandbox 8**。
它们被 `#[ignore]` 是因为需要真访客、真工具链或真 key —— 具备前两者的机器会跑掉除三条外的全部
（本机就是如此）。

- **`agent`（15）**：`compiler_cleanup`(1)、`compiler_discovery`(1)、`compiler_parallel`(1)、
  `e2e_mock`(1)、`read_serial_continuous`(1)、`read_serial_empty`(1)、`real_api`(1)、
  `serial_subscribe`(3)、`stream_real`(1)、`tools`(1)、`vm_injection`(2)、`zig_vm`(1)。
- **`host-core`（40）**：`session_integration`(6)、`run_provenance`(4)、`toolchain_commands`(4)、
  `preflight`(3)、`qemu_commands`(3)、`run_diagnosis`(2)、`sandbox_request_tools`(2)、
  `serial_subscription`(2)、`snapshot_commands`(2)、`vm_status`(2)、`dispatch`(1)、`e2e_ui`(1)、
  `golden_path`(1)、`keyring_os`(1)、`qemu_injection`(1)、`qemu_path_snapshot`(1)、`settings`(1)、
  `stream_forwarding`(1)、`vm_lifecycle`(1)、`vm_poweroff`(1)。
- **`sandbox`（8）**：`serial_observer`(2)、`snapshot_real`(2)、`port_race`(1)、
  `qemu_discover`(1)、`smoke`(1)、`snapshot`(1)。

## 5. CI 跑什么 —— 与不跑什么

CI 只有**一个** gate 作业，它跑**同一个 `scripts/gate.sh`**。刻意的差异（记录在
[`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) 顶部）：

- **启动访客的测试不在 CI 跑。** 标准 runner 没有 `qemu-system-riscv64`、也没有 RISC-V 裸机
  GCC，所以 gate 检测到这点后改跑**可移植**的 `cargo test --workspace`。
- **CI 从不跑 `--ignored` 测试** —— 这是设计；它们由人工运行。
- **Linux 会装 gate 需要的系统库**（`keyring` 需要 `libdbus-1-dev`；Tauri 系 crate 需要
  WebKitGTK 及其伙伴）。

> **信任绿徽章之前先读这句。** **CI 绿不等于 QEMU 测试绿。** CI 证明的是可移植代码能编译、
> lint 通过、测试通过，以及前端能构建 —— 它**不**启动访客。启动访客的证据是 §1 那次本机 gate
> 运行，跑在有访客工具的机器上。

## 6. 关于 flake，直说

仓库最老的未结项是 **QEMU 与 gate 的并行 flake** —— QMP `10054` 家族与 `net` 回环测试里的端口
竞争（[roadmap §12](../roadmap-v1.0.md)，以及 [known-issues.md](known-issues.md) §2）。它们每次
上报、从不掩盖。在本包背后的几次运行里，`net` 的 transport/relay 测试有一两次走了直连路径（而非
期望的 relay），下一次运行即通过 —— 同族。**没有任何测试被 `#[ignore]` 来掩盖 flake**；这些
flake 是夹具里的时序竞争，处理方式是重跑 gate，而不是削弱它。
