[English](README.md) | 中文

# RiscDom —— 审计包

**受审版本：`v1.0.0`** —— 本仓在提交 `891c2375faef1f7ac1a08ff9ade62c843441ef6b` 处打的发布
tag（轻量 tag，`git cat-file -t v1.0.0` 返回 `commit`）。本包全部内容只描述该提交，不含任何
更晚的改动。

> **这是什么，不是什么。** 本包**由项目方**为外部评审者撰写，**不是审计报告**：没有独立第三方
> 验证过这些说法，其中也没有任何认证结论。凡是在本机上无法核查的说法，这里会明说，而不是四舍
> 五入成一个漂亮结论。

目标：让一个从未见过 RiscDom 的评审者，在约 **30 分钟**内看懂它是什么、今天到底能做什么、已知
哪里有问题、以及如何亲眼看到它跑起来。下面八项就是这条路径，按顺序读。

## 八项

| # | 文档 | 回答 |
|---|---|---|
| 1 | *本页* + 发布 tag | **到底在审哪个版本？** `v1.0.0`，tag `891c237`。 |
| 2 | [architecture.md](architecture.md) — [中文](architecture.zh-CN.md) | **它由什么构成、数据怎么流？** 十个 crate 与一条 task 的路径。 |
| 3 | [capabilities.md](capabilities.md) — [中文](capabilities.zh-CN.md) | **今天真能跑什么？** 只列已落盘且被验证的功能。 |
| 4 | [known-issues.md](known-issues.md) — [中文](known-issues.zh-CN.md) | **哪里坏了、哪里 flaky、哪里没验？** 从项目自有记录汇总。 |
| 5 | [test-evidence.md](test-evidence.md) — [中文](test-evidence.zh-CN.md) | **测试证据长什么样？** 一次真实 gate 运行与数字。 |
| 6 | [demo.md](demo.md) — [中文](demo.zh-CN.md) | **怎么看到它work？** 七步，外加两个驱动脚本。 |
| 7 | [dependencies.md](dependencies.md) — [中文](dependencies.zh-CN.md) | **它站在什么之上？** 工具与 crate，以及哪些是可信依赖。 |
| 8 | [concerns.md](concerns.md) — [中文](concerns.zh-CN.md) | **项目 owner 最担心什么？** *（留给 owner 填写。）* |

## 如何复现受审版本

```sh
git clone https://github.com/breakevery/riscdom.git
cd riscdom
git checkout v1.0.0
git rev-parse HEAD          # 891c2375faef1f7ac1a08ff9ade62c843441ef6b
```

`v1.0.0` 是**第一个**冻结版本：控制平面的 HTTP 协议、客户端可观察到的东西、以及各 host crate 的
公开面，自该 tag 起冻结（[api-compatibility.md](../api-compatibility.md) §1）。API 路径前缀是
**`/v0/`** —— 它不在 v1.0 处改变；前缀只在下一次破坏协议的变更时才移动
（[api-compatibility.md](../api-compatibility.md) §5）。

## 范围与阅读约定

- **双语**：本目录每份文档都有 `.zh-CN.md` 对应件，与 `docs/` 其余部分的规则一致。
- **「今天能跑」= gate 会跑到它。** 第 3 项不列仅停留在设计层的东西；
  [roadmap-v1.0.md](../roadmap-v1.0.md) 中标为 `[default]` 或 `[open]` 的，不算功能。
- **证据优先于形容词。** 第 5 项的数字来自本机一次真实的 `scripts/gate.sh` 运行；第 4 项的问题
  来自项目自己的 handoff、roadmap 与跨链记录，而非临时意见。
