[English](e2e-debugging.md) | 中文

# 调试一次端到端运行

端到端测试会驱动一次真实运行（mock LLM → 写源码 → 编译 → 启动 → 读串口），需要 QEMU 与 RISC-V
GCC。它是 `--ignored` 的，只有你显式要求才会跑：

```text
cargo test -p host-core --test e2e_ui -- --ignored --nocapture
```

它每次运行（通过或失败）都会打印一份**运行诊断**：

```text
--- run diagnosis (stage 5c-3) ---
outcome    : final after 6 iteration(s)
first failure: (none)
steps      : 4
  [ok ] write_source: wrote 353 bytes to hello.c
  [ok ] compile: compiled hello.c -> hello.elf (ok)
  [ok ] start_vm: VM started (qmp=54808, serial=54809)
  [ok ] read_serial: HELLO RISCV
serial     : 21 byte(s), tail "HELLO RISCV\n"
vm         : running
audit chain: Intact { length: 42 }
events     : agent:iteration x6, agent:tool_call x4, agent:tool_result x4, agent:final x1, serial:chunk x1, vm:state x2, preflight:progress x9
```

## 怎么读

1. **`first failure`** 直接回答"为什么失败"。判定顺序：宿主拒绝了这次运行（如 `qemu_missing`，附搜索
   诊断）→ 运行以非 `final` 结束（附原因）→ 第一个报错并返回的工具步骤。`(none)` 表示没有任何失败。
2. **`steps`** 按链序列出每一次工具执行，并给出其返回内容的首行。失败的那一步标 `[ERR]`；`compile`
   失败会原样引用编译器输出——通常这就是全部答案。
3. **`serial`** 报告 guest 打印了多少、尾部是什么。`0 byte(s)` 加 *"the guest never printed anything"*
   说明 guest 从未写过 UART（没启动起来，或内核不对）——而不是管道问题。
4. **`vm`** 与 **`audit chain`**：guest 是否仍在宿主槽里，以及日志链是否完整。链断裂意味着日志被改过，
   它从来不是运行失败的原因。
5. **`events`** 是宿主自身事件的计数；若 `start_vm` 报成功而 `serial:chunk x0`，问题在 guest，不在沙箱。

有一条值得记住：**运行可以是 `final` 而某一步失败了**（模型只是用自然语言把失败汇报出来）。outcome
的 kind 不是结论，`first failure` 才是。

## 实现在哪

- `host-core/tests/diagnosis/mod.rs`：配对工具调用与结果、判定第一个失败、渲染报告。
- `host-core/tests/e2e_ui.rs`：在断言之前/之后打印它（断言本身未动）。
- `host-core/tests/run_diagnosis.rs`：钉住措辞，用不需要 QEMU 的失败路径。
