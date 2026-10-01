[English](multi-repo.md) | 中文

# 跨三个仓库工作

**状态** v1.0 规范（M7i）｜ **日期** 2026-09-29 ｜ **面向读者** 贡献者，以及筹备第二仓的人。

**本文是什么。** [roadmap §11](roadmap-v1.0.zh-CN.md) 说 v1.0 需要「第二个仓库需要的 **CONTRIBUTING** 增补」，
但没有列出它们。本文就是那份增补：其余仓库 —— 管理程序 `riscdom-adminapp` 与控制平面程序 `riscdom-server` ——
如何与本仓并立、如何取得内核、继承什么、又必须自己拥有什么。它是拆仓的**规格**；拆仓本身是
[M7a](roadmap-v1.0.zh-CN.md)，它在 v1.0 之后执行（§7）。

## 1. 三个仓库、一个内核

- **本仓是内核**：沙箱、审计链、agent 循环、网络层与宿主核心 —— `host-core`、`sandbox`、`audit`、`agent`、
  `worker`、`net`、`backup` 与 `sdk/rust`。它还留着 **`cli`**（`riscdom`，一个**纯客户端**：那条在进程内
  起控制平面的 `serve` 本地模式归 `riscdom-server`，§7）与连接层的 relay bin
  （`net/src/bin/riscdom-relay.rs`，§7）。**控制平面不在这里**：`server` 属 `riscdom-server`；在拆仓落地
  之前它仍待在 workspace 里，时机由 §7 记录。
- **`riscdom-adminapp` 是管理程序**：Tauri 外壳与它的前端（`host-tauri` 与 `ui`，后者取 `server`，供手机
  接入控制平面）。[RELEASE_NOTES.md](../RELEASE_NOTES.md) 说得明白 —— 该程序 v0.9 **在本仓内**交付、
  **v1.0 成为自己的仓**（即 v1.0 发布之后，§7）；[decisions §9](decisions.zh-CN.md) 的影响段同此。
- **`riscdom-server` 是作为程序的控制平面**：`server` crate（HTTP + SSE 控制平面）**以及那条 `serve` 本地
  模式**（`riscdom` CLI 从前在进程内跑的），成为自己的仓，好让一台宿主无需桌面程序即可安装并运行它。该
  程序所服务的前端是 `riscdom-adminapp` 的构建（来源见 §7）。它眼下仍留在本仓（§7）。
- **这层关系是内核的，不是 fork 的。** [architecture-evolution §12](architecture-evolution.zh-CN.md) 称拆出的仓
  是「独立仓库，**由同一份源码维护**（像 Linux 的 coreutils / iproute2）」。每个程序都是**内核级工具**：它是
  内核的一个消费者，不拥有该内核，随内核特性同步推进 —— 这也是 §12 之所以说每条内核能力都必须有管理 API、
  而程序够不着的能力就是装饰。
- **拆仓等 v1.0。** 两个程序都以**钉在 tag 上**的依赖消费内核（§2），而本仓还没有 v1.0 tag，所以拆仓在那次
  发布之后执行；§7 记下时机与顺序。

## 2. 第二仓如何取得内核

**git 依赖、钉在 tag 上。** 第二仓的 `Cargo.toml` 只列它**直接使用**的 crate，每个都写同一个
`{ git = …, tag = … }`：

```toml
[dependencies]
# 管理程序真正用到的 crate，一个一行，不多。
host-tauri = { git = "https://github.com/breakevery/riscdom", tag = "v1.0.0" }
server     = { git = "https://github.com/breakevery/riscdom", tag = "v1.0.0" }
```

- **这个 tag 在 v1.0 时才打，今天只有 v0.9.x 的。** 上面的 `v1.0.0` 是本仓在 v1.0 发布时会打的 tag（§7）；
  在它存在之前，没有东西可钉，所以**拆仓不在那之前执行**。今天要钉只能钉 `v0.9.9`，而它早于连接层、
  备份工具与 SDK —— 一个比消费它的程序更旧的内核。清单是每个 crate 一行：`riscdom-adminapp` 点名
  `host-tauri` 与 `server`（它内嵌控制平面）；`riscdom-server` 只点名 `server`。
- **间接 crate 会跟着一起来，且在同一 revision。** 一个内核 crate 自己的依赖是本仓内的 `path` 依赖
  （`host-tauri` → `host-core` / `net`；`ui` → `host-tauri` / `server`），而 Cargo 会把「住在同一个 git 仓内的
  path 依赖」对着**那同一份 checkout** 解析。于是当第二仓按 `tag = "v1.0.0"` 点名 `host-tauri` 时，它同时拿到
  `v1.0.0` 所钉的 `host-core` 与 `net` —— 一个 tag、一个 revision，没有第二份要对齐的清单。
- **`Cargo.lock` 要提交，它才是真正的钉。** 一条锁记录写下的是解析出的 commit、不只是 tag：

  ```text
  [[package]]
  name = "host-core"
  version = "0.9.9"
  source = "git+https://github.com/breakevery/riscdom?tag=v1.0.0#<commit>"
  ```

  tag 可以被移动；提交进仓的锁不刻意改动就不会变，所以即便 tag 后来指向别处，构建仍停在被测过的那个
  revision。**提交锁** —— 那才让「钉」成为「钉」。
- **tag 在本仓发版时打。** tag 形如 `vX.Y.Z`（今天是 `v0.9.9`），在本仓发版时创建；第二仓在自己选择时移到新
  tag。它**永不跟随 `main`**：产品底下跑着一个不稳定内核，正是 tag 存在的理由。
- **为何暂不 crates.io。** 发布到 registry 是一种承诺（一个名字、一份稳定性保证、一套 yank 说辞），而拆仓
  **并不需要**它。保留这个选项零成本、且是加法式的：日后从 `{ git = …, tag = … }` 换成带版本的依赖，每
  crate 只改一行、别的都不动。该决定推迟到 v1.0 商业化启动时（[decisions §9](decisions.zh-CN.md) 把管理程序
  定为商业化那一侧，所以内核的发布是一个商业问题）。
- **为何不用 vendored subtree。** 把内核拷进第二仓会破坏 §12 的「由同一份源码维护」：拷贝会漂移，而两个
  冒充成一个的内核，正是拆仓所要避免的状态。

## 3. 第二仓继承什么

「继承」指：同一条纪律适用，而它自己的工具必须被造出来以维持它。

| 本仓的规矩 | 适用于第二仓吗？ |
|---|---|
| **双语文档**（[CONTRIBUTING §文档规范](../CONTRIBUTING.zh-CN.md)）—— 每个 `X.md` 配一个 `X.zh-CN.md`，首行有语言开关 | **适用，作为要采纳的惯例。** 这条规矩存在是因为本项目发布两种语言；第二仓的文档是它自己的，所以它必须维持同样的成对与开关，并需要它自己的双语检查。 |
| **gate** —— 单一真源、`scripts/gate.sh`，且没有任何东西只在 CI 里被检查 | **形状上适用，脚本上不适用。** 它跑不了本仓的 gate（那 gate 检查的是本 workspace）；它必须有自己的 gate，镜像同一条纪律 —— 一份命令清单，CI 跑同一份。 |
| **提交纪律** —— 受门禁保护的提交、ASCII 提交信息（Windows 下 `-m` 会丢字符） | **适用，且它需要自己的脚本。** 这些是本仓所带 `scripts/commit.*` 与 `scripts/preflight.*` 的惯例；第二仓复刻它们，而不是越过仓库边界去够。 |
| **编码规则** —— 绝不用 PowerShell 读写源文件；UTF-8 无 BOM | **适用。** 这条规矩要防的损伤是 Windows PowerShell 的属性，不是本仓的属性。 |
| **秘密扫描** —— 一个扫全历史的 CI job | **适用。** 它自己的仓需要自己的 `secrets` job；一段共享的历史不会被某个仓的 job 扫到。 |

**它*不*继承：** 本仓的 `Cargo.lock`、`Cargo.toml` workspace、`.gitignore`、以文件形式存在的许可、以及本仓
gate 的 crate 清单。内核是作为**依赖**被消费的，内核由**本**仓的 gate 检查 —— 第二仓检查它自己的 crate，
不检查内核的一份拷贝。

## 4. 第二仓必须自己拥有什么

1. **它自己的 gate** —— 一份 `gate.sh`（若仍以 Windows 为已验证平台，再加一个 `.ps1` 孪生），一份命令
   清单，由 CI 的 `gate` job 以同样方式调用。
2. **它自己的构建与 bundle** —— 本仓 CI `bundle` job 已经示范的 Tauri 打包（macOS 出 dmg；Linux 出
   deb / rpm / AppImage；由版本 tag 或手动触发导入）。
3. **它自己的发布流程** —— 它自行发布管理程序，按自己的节奏，对着一个被钉住的内核 tag（§2）。
4. **它自己的 CLA 供给** —— 见 §5。
5. **它自己的文档地图** —— 一份属于它自己的 `docs/README.md`，描述它自己的文档；§3 的开关与成对规则
   适用于它。
6. **它自己的秘密扫描 job**（§3）。

## 5. CLA 跨仓

**CLA 是为一个仓写的，这一点保持不变。** [CLA.md](../CLA.md) §1 把「Project」定义为「**the RiscDom
repository** and the work distributed from it」—— 单数 —— 而签名库
（[`signatures/version1/cla.json`](../signatures/version1/cla.json)）与 CLA Assistant workflow
（[`.github/workflows/cla.yml`](../.github/workflows/cla.yml)）都住在本仓。[CONTRIBUTING.md](../CONTRIBUTING.zh-CN.md)
已经把这一点说出口：贡献「may be taken in somewhere other than this repository in the future, so this
section speaks only for the flow that exists here today」。

**每个仓自带自己那一套。** 已定的答案（决策 §163）：**每个新仓都装它自己的 CLA Assistant 与自己的签名
库**；三个仓之间不共享任何东西，每个仓的 `CLA.md` 只为自己说话。这与 §3 给 secret scan 定下的形状相同 ——
共享的历史不被另一个仓的作业覆盖 —— 而共享签名库需要一个本项目没有理由建立的跨仓授权。**对本仓的贡献
仍由本仓的 CLA 覆盖**，本文不把一份签名延伸到任何别的仓。

## 6. 本文不是什么

- **它不是拆仓操作手册。** 创建仓库、搬 crate、打第一个 tag、接上 CI 是 [M7a](roadmap-v1.0.zh-CN.md)，
  它在 v1.0 之后执行（§7），并需要自己的授权（它要写新的远端）。**该授权是逐批授予的，不是一次性的**
  （决策 §163）：拆仓以 M8-4a…M8-4d 执行（§7），其中每一个写到新远端或从本仓移除目录的动作，都各自被授权。
- **它不是发布机制。** 本仓的 server 与包如何发布、第二仓的又如何发布，是 [M7b](roadmap-v1.0.zh-CN.md)
  与发行说明的事，不是本文。
- **它不是 SDK 契约。** 第三方消费者看到的类型是 [M7c / M7d](roadmap-v1.0.zh-CN.md) 的事；§2 的 git 依赖
  是内核自己的 crate，不是一份已发布的接口。
- **它不复述 CLA 或 CONTRIBUTING。** 那些文档已经说了的地方，本文指向它们而不复制它们，好让改动只有
  一处。

## 7. 三个仓库，以及它们何时拆

**M7a —— 拆仓 —— 在 v1.0 之后发生。** [roadmap §11](roadmap-v1.0.zh-CN.md) 已定：管理程序在 v1.0 迁往
自己的仓库；而 [M8](roadmap-v1.0.zh-CN.md) 是「API 冻结，并发布」：宣布冻结、v1.0 发布。**`v1.0.0` 已打**
（2026-10-01），而那正是唯一的条件（§2：每个新仓都要钉一个内核 tag）。顺序是**先 v1.0 发布，再
`riscdom-server`，最后 `riscdom-adminapp`** —— 在冻结之前拆出的程序，会被钉在一个仍在它脚下移动的内核上。

**它以四批执行**（决策 §163），每批各自被授权（§6）：

- **M8-4a —— `riscdom-server` 拆出。** `server` crate、它自己的 `Cargo.toml`（以 `tag = "v1.0.0"` 命名内核
  crate）、它的 `gate` 与 `server-bundle` 作业，以及一份 README。它带走 **`serve` 本地模式**：`riscdom`
  不再在进程内起控制平面，收缩为**纯客户端**（§1）。
- **M8-4b —— `riscdom-adminapp` 拆出。** `host-tauri` 与 `ui`、它们自己的 workspace、`bundle` 作业与 UI 探针。
- **M8-4c —— 本仓收尾。** `Cargo.toml` 的 `members`、`gate`、`scripts`、`.gitignore` 与各文档不再点名已拆出的东西。
- **M8-4d —— 三仓对账。** 每个新仓的 lock 对着 `v1.0.0` 提交，且内核自己的 gate 在那些 crate 离去后仍绿。

**`riscdom-server` 的包所服务的前端从哪来。** 该包的 `web/` 是构建好的前端
（[server-distribution.md](server-distribution.zh-CN.md)、`scripts/pack.*`），而那份构建属于
`riscdom-adminapp`。已定的答案（决策 §163）是：`riscdom-server` **从 `riscdom-adminapp` 的 release 取它**
—— 打包器下载那个钉定版本已发布的前端制品，而不是从第二份源码副本去构建 UI。

每个新仓是干什么的（在本文记录；两者都**不在 v1.0 实现**）：

- **`riscdom-adminapp` —— 管理程序。** 今天是**桌面**应用（Tauri + React + Vite，`host-tauri` + `ui`）。它
  之后长向**移动端（Android / iOS）**并保留**浏览器**模式；三者都连 **RiscDom 节点**与 **`riscdom-server`**
  两端。
- **`riscdom-server` —— 作为程序的控制平面。** 今天是**命令行**（`riscdom-server`，加上连接层的
  `riscdom-relay`）。它会有一张 **web 状态页**，并为 **Windows 与 Linux** 发布。它**只服务 RiscDom**：
  不是通用服务器管理面板。

**relay 留在这里。** `net/src/bin/riscdom-relay.rs` 只依赖 `net` —— 它只 import `net` 的类型、不碰 `server` ——
而 `net` 留在本仓，所以 relay bin 不随任何一个新仓拆出：它是连接层的程序，不是控制平面的。
