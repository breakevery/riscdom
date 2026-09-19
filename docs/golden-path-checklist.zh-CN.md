[English](golden-path-checklist.md) | 中文

# 黄金路径 —— 人工走查清单（第 1、2 步）

> **为什么要人工。** 第 1–2 步是机器装配：装 QEMU、解析 RISC-V 编译器、配置服务商、跑预检。CI 没有
> QEMU、没有 GUI、也没有真实 API key，无法诚实自动化这两步（见 [golden-path.zh-CN.md](golden-path.zh-CN.md) §6）。
> 因此由人在一台干净机器上每版走一遍，并**在这里记录下来** —— 即
> [golden-path.zh-CN.md](golden-path.zh-CN.md) §8 的决定 6：*没人写下来的行走，没人能复核。*
>
> 第 3–7 步由 `cargo test -p host --test golden_path -- --ignored` 覆盖（mock LLM + 真实 QEMU guest；
> 见 [golden-path.zh-CN.md](golden-path.zh-CN.md) §6a）。本清单负责它覆盖不到的部分。

## 怎么用

1. 把下面的模板复制到 `docs/walks/<日期>-<版本>.md`（没有 `docs/walks/` 就新建），或直接粘进发布说明。
   *若把记录放进仓库，它必须满足 `scripts/check-bilingual.sh`：仓库里每个 `*.md` 都要有对应语言的版本 ——
   所以要么写双语，要么把记录留在发布说明里。*
2. 在一台从未跑过本项目的机器上走第 1、2 步。
3. **每个字段都填**。`unknown` 也是答案；空白不是。
4. 出问题时不要只写「失败」：点名是哪一步、把原始错误原样粘上、并说明是否可恢复、怎么恢复。
5. 把这份记录随发布一起留下。

## 模板（复制这段）

~~~markdown
### 黄金路径走查 —— <版本> —— <日期>

**机器与日期**
- 机器：<主机名 / 型号>，<CPU / 内存>
- 日期：<YYYY-MM-DD> —— 操作人：<姓名>

**操作系统**
- <版本> —— 内部版本 <build>

**QEMU（第 1 步）**
- `qemu-system-riscv64 --version` → <版本>
- 来源：<winget / 官方安装包 / 手动下载>
- 「设置 → 工具链」里的路径：<路径，或「自动探测」>

**RISC-V GCC（第 1 步）**
- `<gcc> --version` → <版本>
- 来源：<自动探测 / 手动指定 / 应用内 xPack 下载>
- 「设置 → 工具链」里的路径：<路径>

**服务商与模型（第 2 步）**
- 服务商：<deepseek / …> —— 模型：<…>
- Base URL：<…>
- key：<存进操作系统钥匙串 / 未存储>

**预检（第 2 步 —— 设置 → 工具链 → 环境预检）**
- 第 1 步 <名称>：<通过 / 失败> —— <明细>
- 第 2 步 <名称>：<通过 / 失败> —— <明细>
- 第 3 步 <名称>：<通过 / 失败> —— <明细>
- 第 4 步 <名称>：<通过 / 失败> —— <明细>
- 结论：<全绿 / 已跳过：原因>

**两次 run（证明走查确实走到了路径上）**
- run A 短指纹：<16 位十六进制>
- run B 短指纹：<16 位十六进制>
- 两者不同：<是 —— 改动的是哪个字段 / 否>

**导出的审计记录**
- 文件：<路径>
- `audit-verify <路径-to-db> --runs` → <Intact { length: N } + RunIndex { findings: 0 } / Broken { … }>

**失败记录**
- 出在哪一步：<1 / 2 / 无>
- 原始错误：<原样粘贴>
- 是否可恢复：<是 —— 怎么修的 / 否 —— 阻塞>
~~~

## 填写示例

```markdown
### 黄金路径走查 —— v0.5.0 —— 2026-09-19

**机器与日期**
- 机器：Z0624145651262（x64），16 GB 内存
- 日期：2026-09-19 —— 操作人：项目所有者

**操作系统**
- Windows 11 专业版 —— 内部版本 22631

**QEMU（第 1 步）**
- `qemu-system-riscv64 --version` → QEMU emulator version 10.1.0
- 来源：winget
- 「设置 → 工具链」里的路径：C:\Program Files\qemu\qemu-system-riscv64.exe

**RISC-V GCC（第 1 步）**
- `riscv-none-elf-gcc --version` → xPack 15.2.0-1
- 来源：应用内 xPack 下载
- 「设置 → 工具链」里的路径：%APPDATA%\…\toolchains\xpack-riscv-none-elf-gcc-15.2.0-1\bin\riscv-none-elf-gcc.exe

**服务商与模型（第 2 步）**
- 服务商：deepseek —— 模型：deepseek-chat
- Base URL：https://api.deepseek.com
- key：存进操作系统钥匙串

**预检（第 2 步 —— 设置 → 工具链 → 环境预检）**
- 第 1 步 toolchain_runs：通过 —— `--version` 有应答
- 第 2 步 compile：通过 —— 四行 guest 编译成功
- 第 3 步 qemu_runs：通过 —— QEMU 应答 `--version`
- 第 4 步 boot：通过 —— guest 打印了 banner
- 结论：全绿

**两次 run（证明走查确实走到了路径上）**
- run A 短指纹：933566304b8632a7
- run B 短指纹：4b0f0d2c81aa7e93
- 两者不同：是 —— `llm.model`（deepseek-chat → deepseek-reasoner）

**导出的审计记录**
- 文件：<工作区>\exports\run_01a0b812….jsonl
- `audit-verify <路径-to-db> --runs` → Intact { length: 41 } + RunIndex { findings: 0 }

**失败记录**
- 出在哪一步：1
- 原始错误：error: could not find `qemu-system-riscv64` on PATH (searched: PATH, RISCDOM_QEMU, C:\Program Files\qemu)
- 是否可恢复：是 —— 用 `winget install SoftwareFreedomConservancy.QEMU` 装好，重开应用，自动探测到了
```

## 说明

- 上面的两个指纹，是「同一请求、只改一个配置字段」的两次 run（第 7 步）。把它们记下来，才让「这次环境被
  捕获了」变成可复核的事实，而不是一句断言。
- 导出的 run 区间是链上的一个**切片**：`audit-verify` 针对该文件所属的数据库运行，或针对「该 run 之前的链
  重建 + 导出文件」的数据库运行。接不回链上的导出是一个**发现**，不是格式细节。
- 判为 `Broken` 或 `findings > 0` 是发布的**停止项**：那份记录已经不能自解释了。
