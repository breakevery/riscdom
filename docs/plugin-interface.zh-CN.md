[English](plugin-interface.md) | 中文

# 沙箱插件接口

**状态** v1.0 规范（里程碑 [M3](roadmap-v1.0.zh-CN.md)）｜ **日期** 2026-09-28 ｜ **基线**
v0.9.9（`3365970`）｜ **读者** 插件作者 —— 写一个本内核能跑的沙箱的人。

**本文是什么。** 把 [roadmap §8](roadmap-v1.0.zh-CN.md) 描述的那个接口写下来，好让它能被**冻结** ——
[decisions §3](decisions.zh-CN.md) 要求这次冻结早于内核 API 的冻结，因为一个在跨设备工作做完之后被重塑
的接口，等于同一件事做两遍。它是一份规范：内核自己对这一规范的实现是 v1.x 的工作
（[decisions §3](decisions.zh-CN.md)），本文标为「未冻结」的每一处也一样。

**它的同伴。** [decisions §3](decisions.zh-CN.md)（沙箱是插件）、[§4](decisions.zh-CN.md)（预置环境）与
[§5](decisions.zh-CN.md)（插件仓库）是它背后的决策；[security-model.md](security-model.zh-CN.md) §4 说
了一个 capability 值多少；[error-model.md](error-model.zh-CN.md) 是错误行走时用的词汇；
[api-compatibility.md](api-compatibility.zh-CN.md) 说明「冻结」在这个表面上意味着什么。

下面每一节都以「**冻结**」与「**未冻结**」收尾。

## 1. 冻结了什么，没冻结什么

- **[已定]** **本文冻结**：传输（§2）、机制层四操作及其帧语法（§3）、语义层两操作（§4）、capability
  声明的**框架**（§5）、manifest 的必备键（§6）、错误与版本规则（§7）、架构无关这条约束（§8）与信任
  模型（§9）。
- **[待定]** **本文不冻结**：**capability 声明格式的细节**（§5 —— 刻意留作 draft，是
  [roadmap §8](roadmap-v1.0.zh-CN.md) 说「应当最后冻结」的那一块）、**架构抽象**（§8 —— 要求冻结了，
  trait 没做设计），以及插件自定义负载所携带的任何**名字**。

**冻结**：上面那份清单。**未冻结**：标出的那两项，以及本文没有一字一句写明的任何形状 —— 沉默不是承诺。

## 2. 传输

- **[已定]** **进程外、走 stdio、一行一个 JSON 对象** —— [`worker`](../worker/README.zh-CN.md) 已经在用的
  形状，也正是 [decisions §3](decisions.zh-CN.md) 称插件「与 `worker` 同构」的原因。没有进程内插件、没有
  动态库、没有 ABI。
- **[已定]** **stdout 是协议通道。** 每个**请求**行恰好对应一个**应答**行，上面不写别的。往 stdout 写了
  别的东西的插件，已经破坏了协议。
- **[已定]** **事件走 stderr**，用项目其余部分已经在用的信封
  （`{"version", "kind": "event", "event", "agent_id", "task_id", "ts", "payload"}` ——
  [control-plane-events.md](control-plane-events.zh-CN.md)）。插件自己的事件用 `plugin:` 前缀，正如
  `worker` 用 `worker:` 给自己打前缀。事件是诊断性的：监督者用一种方式解析所有行，不认识的就忽略。
- **[已定]** **一次一个请求。** 内核发一个请求、等它的应答，再发下一个；应答带着它所答请求的 `id`。
- **[已定]** **进程活一个会话。** 与 `worker`（一个 task、然后退出）不同，插件被启动一次、应答请求直到
  内核关闭 stdin。收到 stdin EOF 时它释放手里的一切并以 `0` 退出。在 EOF 之前退出或死掉的插件，对内核
  来说是一次**崩溃**（[§7](#7-错误与版本协商)）。

**冻结**：通道分工、一行一消息、事件信封、串行派发、会话生命周期。**未冻结**：内核**如何**监管一个插件
（重启策略、资源上限）—— 那是内核的事，不是插件作者要实现的东西。

## 3. 机制层

**[已定]** 四个操作是**必含**的：答不出全部四个的插件，就不是沙箱插件。每个帧都是一行；`v` 是协议版本
（[§7](#7-错误与版本协商)），`id` 是请求的身份。

```text
request   {"v":1,"id":"<string>","op":"<name>","args":{...}}
response  {"v":1,"id":"<same string>","ok":true,"result":{...}}
          {"v":1,"id":"<same string>","ok":false,"error":{"kind":"<string>","message":"<string>","retryable":<bool>}}
```

### 3.1 `start`
- **args** `{"instance":"<id>","definition":"<name|absent>","workspace":"<path>","data_dir":"<path>","instance_dir":"<path>","shared_dir":"<path>"}` —— 那两个目录就是 [decisions §4](decisions.zh-CN.md) 的持久目录，交给插件，因为**挂载什么**是插件的决定。
- **result** `{"state":"running","since_ms":<int>}`
- **已经在跑** 是 `ok:false`、`error.kind:"refused"` —— 被拒绝，绝不在调用方背后排队或重启（VM 已经在守这条规矩）。

### 3.2 `stop`
- **args** `{"force":<bool|absent>}`
- **result** `{"state":"stopped","since_ms":<int>}`
- **什么都没在跑** 是 `ok:true`：停两次不是错误。

### 3.3 `execute`
- **args** `{"input":"<string>","timeout_ms":<int|absent>}`
- **result** `{"outcome":<any JSON>,"output_ref":<string|absent>}` —— guest 的应答。它的形状是**插件的**；内核把 `outcome` 当作不透明 JSON。`output_ref` 存在时是一个句柄，内核可以把它交回 `output`（§3.4），于是大应答不必走两遍。
- **超时**由插件自己执行。超过它是 `ok:false`、`error.kind:"network"`，`error.message` 点出那个时限（[error-model.md §4](error-model.zh-CN.md) —— 超时的重试判定）。
- **execute 不隐含 start**：什么都没在跑时调用它是 `ok:false`、`error.kind:"invalid"`，而内核的应答是调用方自己的问题，不是一次静默启动。

### 3.4 `output`
- **args** `{"since":<int|absent>,"ref":<string|absent>}` —— `since` 是插件上次返回的索引；`ref` 是来自 `execute` 的 `output_ref`。
- **result** `{"lines":["<string>",...],"next":<int>}` —— 累积的 console 输出，按序。来自「未来」的索引、或插件不认识的 ref，是 `ok:false`、`error.kind:"invalid"`。

**冻结**：四个操作名、请求/应答语法、§3.1–§3.4 的参数与结果键，以及「第二次 `start` 被拒、第二次 `stop`
不算错」这两条规矩。**未冻结**：`input` 与 `outcome` 的**内容**（按设计就是架构相关的，§8），以及插件
如何跑一个 guest 的任何细节。

## 4. 语义层

**[已定]** 两个操作是**可选**的。插件**声明**自己有哪些（[§5](#5-capability-声明)）；调用一个插件没有声明的
操作是 `ok:false`、`error.kind:"invalid"` —— 内核不推断，也不回落到另一个插件。

```text
snapshot      {"name":"<string>"}          -> {"name":"<string>","mode":"<string>"}
fingerprint   {}                           -> {"fingerprint":"<string>","schema":"<string>"}
```

- **[已定]** `snapshot` 的 `mode` 是插件自己对「快照怎么打的」的说法；内核原样带过而不解释（现存的那两个 mode 是 QEMU 的，不属于本接口）。
- **[已定]** `fingerprint` 答的是与 run 指纹同一类东西（[run-provenance.md](run-provenance.zh-CN.md)）：一个稳定的字符串加上产生它的 schema，于是两个节点可以比较**一份配置是什么**，而不必比较不透明字节。

**冻结**：两个操作名、它们的帧、以及「未声明的操作被拒」这条规矩。**未冻结**：`mode` 的词汇，以及
指纹是拿什么算出来的。

## 5. capability 声明

**[默认]** —— **这是整个接口里刻意留作 draft 的唯一一块**，因为它是插件作者最先实现的部分，也是在一个真实
插件存在之后最可能需要修一次的部分（[roadmap §8](roadmap-v1.0.zh-CN.md)；此前格式被说成「在 §8」，而 §8
说「在 §14.11」—— 那是一个循环，本节通过写明框架、把其余标为 draft 来打破它）。

**框架 —— 冻结：**

- **[已定]** **写在哪**：manifest（[§6](#6-manifest)）的 `capabilities` 键，一个 **字符串列表**。
- **[已定]** **谁校验**：**内核**，绝不是插件。
- **[已定]** **何时校验**：**注册时**（插件被读取、它的声明在被启动之前就被校验）以及**请求时**（一个其 capability 未被声明的操作会被拒，即使插件提供它）。
- **[已定]** **未知名字被拒，不是被忽略。** 内核校验不了的 capability，就是没人授予的 capability；带这种名字的声明是 `error.kind:"invalid"`，插件不加载。名字来自内核自己的词汇表（`Capability` 列表，[control-plane-api.md §3](control-plane-api.zh-CN.md)）—— 不是来自插件。
- **[已定]** **空声明不等于「全部」。** 什么都没声明的插件什么都做不了；默认拒绝，与其他任何 actor 完全一样（[security-model.md](security-model.zh-CN.md) §4）。

**Draft —— 未冻结：** 一条声明条目的确切拼法（裸名字，还是名字带 scope）、机制层与语义层的操作是否各自
需要一个**具名** capability（例如一个 `sandbox.execute` 形状的名字），还是由内核已有的那些来授予，以及
声明能否按实例收窄。**这些在 v1.x 实现任何东西之前定下来**；在那之前，插件作者应当把本节当作那个文件
的形状，并预期拼法还会变。

## 6. manifest

- **[已定]** **manifest 是插件旁边的一个 JSON 文件**，`plugin.json`。它是内核读的第一样东西；解析不了的 manifest 是一次拒绝，绝不是一次默认。
- **[已定]** **必备键**：`name`、`version`、`protocol`（本插件说的 `v`）、`entry`（`{"program":"<path|name>","args":["<string>",...]}` —— 内核要启动的命令）、`capabilities`（[§5](#5-capability-声明)）。
- **[已定]** **可选键**：`description`、`persistent_dirs`（`{"instance":<bool>,"shared":<bool>}` —— 本插件想要 [decisions §4](decisions.zh-CN.md) 那两个目录里的哪几个）。
- **[已定]** **manifest 来源是合并的，不是互斥的** —— 插件声明、内核扫描、开发者手写，恰好就是 [decisions §4](decisions.zh-CN.md) 点名的三种；冲突会被报告，绝不按优先级静默解决。
- **[已定]** **预置内容带哈希、启动时校验**（[decisions §4](decisions.zh-CN.md)）；内容与安装时不符的插件被拒。

**冻结**：文件名、上面那些必备与可选键，以及两条规矩（没有静默默认、没有静默优先级）。**未冻结**：
插件是从**哪里**装进来的（仓库格式是 [decisions §5](decisions.zh-CN.md)，v1.x 建），以及签名的载体 ——
包复用凭据规范，而那个规范的形状不归本文陈述。

## 7. 错误与版本协商

- **[已定]** **错误是一个应答，不是一次崩溃。** 做不了被要求之事的插件答 `ok:false`，`error.kind` 取自 [error-model.md](error-model.zh-CN.md) 的分类（`network`、`refused`、`crashed`、`partial`、`invalid`），带一句人能读的 `message`，以及该模型赋予的 `retryable` 判定。内核把它映射到自己的 `DispatchError` —— 它不会发明第六个分类。
- **[已定]** **死掉的进程就是一次崩溃**：内核以 `crashed` 报告它并带上退出状态，对同一个 task 是 `retryable: false`（[error-model.md](error-model.zh-CN.md)）。
- **[已定]** **`v` 要双向检查。** 内核拒绝使用一个 `protocol` 大版本它不会说的插件；而见到**更新** `v` 的请求时，插件答 `ok:false`、`error.kind:"invalid"`，并且不去服务它听不懂的任何东西。这就是把 [api-compatibility.md](api-compatibility.zh-CN.md) §6 的「老 reader 绝不读新格式」从文件搬到进程上。
- **[已定]** **同一大版本内只做增量**：一个新的 `op`、一个新的可选键、一种新事件 —— 都可以在小版本里到来。删除一个操作、或改变某个操作的含义，需要新的一个大版本（[api-compatibility.md](api-compatibility.zh-CN.md) §2、§3）。

**冻结**：错误帧、它借用的分类词汇、崩溃规矩、双向版本规矩。**未冻结**：崩溃之后内核的重启策略，以及
除了那条拒绝规矩之外、「更新的内核」如何与「更旧的插件」说话。

## 8. 架构无关

- **[已定]** **内核不得假设 guest 是 RISC-V。** 插件内容不受架构约束，而本接口里没有任何东西 —— 没有帧、没有键、没有操作 —— 点名某台机器、某个指令集或某个模拟器（[roadmap §8](roadmap-v1.0.zh-CN.md)、[decisions §2](decisions.zh-CN.md)）。RISC-V 仍是基质与默认实现；它不是本接口的要求。
- **[已定]** **证据就在这些帧里**：`execute` 的 `input` 是字符串、`outcome` 是不透明 JSON（§3.3），`snapshot` 的 `mode` 是插件自己的说法（§4）。一个无法通过这四个操作跑非 RISC-V 插件的内核，就是加了一条本文禁止的假设。
- **[待定]** **抽象层的 trait 不在本文设计。** 内核*如何*持有这样一个它不能做假设的插件 —— 这些操作背后的进程内缝 —— 是 v1.x 的工作；在一个插件都还不存在时就冻结一个 trait，会重犯 §5 正在避免的那个错误。

**冻结**：那条约束（不做架构假设，RISC-V 是默认而非要求），以及让它可以被检验的帧形状。**未冻结**：
trait、模块，以及将来实现引入的任何进程内类型。

## 9. 信任模型

- **[已定]** **插件的权力 = 它声明的 capabilities** —— 按其他任何 capability 被校验的方式校验，且在同一套模型里（[decisions §3](decisions.zh-CN.md)、[security-model.md](security-model.zh-CN.md) §4）。本接口不新增第二套权限系统。
- **[已定]** **进程外是一道边界，不是形式。** 插件不在内核的地址空间里；一个想要它没声明之物的插件，只能通过一个请求去要，而内核能在那里拒绝它。
- **[已定]** **内核替插件做的事在链上。** 一次 start、一次 stop、一次内核引发的 execute，都是与其他行为一样的审计行 —— 接口自己绝不写链（[security-model.md](security-model.zh-CN.md) §7），也绝不能变成第二本账。
- **[已定]** **插件不被托付内核的机密。** 没有钥匙、token 或凭据会走进请求帧或事件负载；§3.1 的 `start` 里那些目录是唯一交出去的状态。

**冻结**：受 capability 约束的信任模型、进程边界、「链是内核写的、不是插件写的」、以及无机密这条规矩。
**未冻结**：对插件进程本身的沙箱化（它在宿主机上能碰什么）—— 那是一个部署与操作系统的关注点，本接口
不做决定。

## 10. 四条红线

[roadmap §1](roadmap-v1.0.zh-CN.md) 写下四条约束，并说它们是每个里程碑的工作都得熬过的测试。本接口如何
熬过它们如下；那四条约束本身不在此复述，只作引用。

- **非通用沙箱。** 本接口的存在，就是为了让沙箱**可插拔**：§8 禁止任何帧点名某台机器，而 RISC-V 是插件**可以**替换的默认，绝不是插件**必须**满足的要求。读者因此可以把本文当作这条约束仍然成立的理由。
- **无内置 supervisor。** 这里的每一个帧都是调用方驱动的**机制**；没有一个帧作任何决定。没有「做正确的事」这种操作，§3.3 也拒绝隐式启动 —— 决定不是本接口能做的东西。
- **无官方运营服务。** 插件是调用方自己机器上的一个文件加一个进程；§6 说它从哪里被读取、校验什么，并没有说出任何**提供**它的一方。
- **审计不变量不动。** §9 说链是内核写的、不是插件写的，而本接口**没有**定义任何审计事件、任何哈希输入 —— 它借用已有的词汇，而不是往里加。

**冻结**：本文任何一处都不得以「会让四条之一失败」的方式被修正。**未冻结**：上面那些话是那场**测试**，
不是一份可以改动四条含义的许可。
