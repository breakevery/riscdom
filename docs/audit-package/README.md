[中文](README.zh-CN.md) | English

# RiscDom — audit package

**Audited version: `v1.0.0`** — the release tag cut in this repository at commit
`891c2375faef1f7ac1a08ff9ade62c843441ef6b` (a lightweight tag, `git cat-file -t v1.0.0`
answers `commit`). Everything in this package describes that commit and no later one.

> **What this is, and what it is not.** This package is written **by the project** for an
> outside reviewer. It is **not an audit report**: no independent party has verified these
> claims, and nothing here is a certification. Where a claim could not be checked on this
> machine it says so rather than rounding it up.

The goal is a reviewer who has never seen RiscDom to understand, in about **30 minutes**,
what it is, what it can actually do today, what is known to be wrong, and how to see it
run. The eight items below are that path; read them in order.

## The eight items

| # | Document | Answers |
|---|---|---|
| 1 | *(this page)* + the release tag | **What exactly is being reviewed?** `v1.0.0`, tag `891c237`. |
| 2 | [architecture.md](architecture.md) — [中文](architecture.zh-CN.md) | **What is it made of, and how does data move?** The ten crates and one task's path. |
| 3 | [capabilities.md](capabilities.md) — [中文](capabilities.zh-CN.md) | **What really runs today?** Only functions that are on disk and exercised. |
| 4 | [known-issues.md](known-issues.md) — [中文](known-issues.zh-CN.md) | **What is broken, flaky, or unverified?** Summarised from the project's own records. |
| 5 | [test-evidence.md](test-evidence.md) — [中文](test-evidence.zh-CN.md) | **What does the test evidence look like?** A real gate run, with the numbers. |
| 6 | [demo.md](demo.md) — [中文](demo.zh-CN.md) | **How do I see it work?** Seven steps, and two scripts to drive them. |
| 7 | [dependencies.md](dependencies.md) — [中文](dependencies.zh-CN.md) | **What does it stand on?** The tools and crates, and which are the trusted ones. |
| 8 | [concerns.md](concerns.md) — [中文](concerns.zh-CN.md) | **What worries the project owner most?** *(left for the owner to write.)* |

## How to reproduce the version under review

```sh
git clone https://github.com/breakevery/riscdom.git
cd riscdom
git checkout v1.0.0
git rev-parse HEAD          # 891c2375faef1f7ac1a08ff9ade62c843441ef6b
```

`v1.0.0` is the **first** frozen release: the control plane's HTTP protocol, what a client
can observe, and the host crates' public surface are frozen as of this tag
([api-compatibility.md](../api-compatibility.md) §1). The API path prefix is **`/v0/`** —
it does not change at v1.0; the prefix moves only at the next protocol-breaking change
([api-compatibility.md](../api-compatibility.md) §5).

## Scope and reading conventions

- **Bilingual**: every document here has a `.zh-CN.md` counterpart, the same rule the rest
  of `docs/` follows.
- **"Runs today" means the gate exercises it.** Item 3 does not list anything that is only
  designed; items marked `[default]` or `[open]` in
  [roadmap-v1.0.md](../roadmap-v1.0.md) are not capabilities.
- **Evidence over adjectives.** Item 5's numbers come from a real `scripts/gate.sh` run on
  this machine; item 4's problems come from the project's own handoff, roadmap and
  cross-chain records, not from a fresh opinion.
