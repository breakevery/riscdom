[English](audit-v2.md) | 中文

# audit v2 —— 主链与它的临时段

**状态** v1.0 规范（M5-1a）｜ **日期** 2026-09-30 ｜ **面向读者** 内核开发者，以及实现 M5 的人。

## 1. 本文是什么

[roadmap §7](roadmap-v1.0.zh-CN.md) 把审计链的*语义*扩展为「主链 + 临时段」，并说明段首带一个**跨段引用**、作为**元数据**附加、哈希公式不变 —— 而 [decisions §127](decisions.zh-CN.md) 就是 owner 对这次扩展的授权。本文把该扩展**写下来**：段是什么、段首如何指向它所承接的链、以及哪些问题仍被有意留着。

**它是审计子系统的规范，不是连接层的。** [connection.md §7](connection.zh-CN.md) 是链的 digest 的*传输* —— 对链上某一点的一份承诺，按定时器向上报告 —— 而它指向本文来解释段是什么。两者相接之处：本文说链**意味着**什么，connection.md §7 说它的 digest 如何**上路**。

**M5-1a 落的就是本文这一批**：**schema**（`segments` 表与 `audit_events.segment_id` 列）与这份规范。它不写任何段、也不开任何段 —— 那是 M5-1b。

## 2. 主链与临时段

**[已定]** **每设备一链，加临时段。** 这个切分是**语义的，不是第二个公式**：设备的链是一条线性、可验证的事件序列，而一个**段**是这条序列里的**一段区间** —— 真中心无法联系时、由临时中心代行期间写下的事件。变的是*一个事件属于哪一段*，绝不是*一个事件怎么被哈希*。

**[已定]** **段首携带一个跨段引用** —— 元数据，位于该区间之首，说明这个段从链的哪个位置承接下去。它**不是** [`compute_hash`](../audit/src/hash.rs) 的输入（公式只是 `prev_hash | ts | actor | action | detail_json`，别无其它），所以链的验证与从前一模一样（decisions §127 第 2 条）。

**[已定]** **标就是 `audit_events.segment_id`。** `NULL` 意为**主链** —— 这正是这一列存在之前写下的每一个事件的值，所以老日志**零改写**即读对。非 `NULL` 的值指向 `segments` 表里的一行（§3）。

**[已定]** **`segments` 表不是哈希链的一部分。** 与 `runs` 一样，它记录链自身的行所蕴含的东西；段行在不在，链照样验证。

**临时段自己那条链的物理形状，[已在 §2 定为 (b)](#2-主链与临时段)。** 它一份链一份 SQLite 文件，放在主链旁边；真中心回来时，其事件以**转录**方式写入主链（M5-2）。因此主链始终是一条线性、可验证的序列，公式与 `verify_chain` 都待在原地。[§8](#8-临时段住在哪里) 说文件住在哪，[§9](#9-每条链如何被验证) 说各自如何被验证。

两个被掂量过、但**不是**本项目形状的候选：**（a）一份文件、一条链，段只是标** —— 临时中心直接以它的 `segment_id` 追加进主 `audit_events`。这对一个在*真中心不可达时*代行的中心不成立：两者是**不同的机器**，事件在相遇之前不可能在同一份文件里，而把一个尚未被核实的中心的行写进受信的链，正与 `provisional` 的用意相反。**（c）一份文件内的另一条链** —— 那会迫使 `verify_chain` 长出**分段感知**模式，也就是在红线 4 的验证面上动刀。

## 3. 段记录

`segments` 表（M5-1a 建、M5-1b 写）：

| 列 | 类型 | 含义 |
|---|---|---|
| `segment_id` | `TEXT` PRIMARY KEY | 段自己的名字 —— 事件在 `audit_events.segment_id` 里携带的那个值。 |
| `kind` | `TEXT` NOT NULL | `main` 或 `temporary`（[`audit::SegmentKind`](../audit/src/segment.rs)）。 |
| `head_hash` | `TEXT` | 段有事件后的最后一个哈希；在那之前为 `NULL`。 |
| `head_prev_chain` | `TEXT` | **跨段引用**（§4）：段开启时主链的头。 |
| `opened_at_ms` | `INTEGER` | 开启时间。 |
| `closed_at_ms` | `INTEGER` | 关闭时间；开着时为 `NULL`。 |
| `state` | `TEXT` NOT NULL | `open` / `closed` / `folded` / `forked`（[`audit::SegmentState`](../audit/src/segment.rs)）。 |
| `note` | `TEXT` | 人类注记。自由形式，与这里其它东西一样存在链旁边。 |

`main` / `temporary`、`open` / `closed` / `folded` / `forked` 是**落盘的字**；读回者是 [`SegmentKind::parse`](../audit/src/segment.rs) / [`SegmentState::parse`](../audit/src/segment.rs)——它们对不认识的单词答 `None`，而不是去猜。

**[已定]** **M5-1b 开与关一个段。** `AuditStore::open_segment(kind)` 写下该行 —— `head_prev_chain` 取**那一刻**链的头，在任何追加**之前**读出 —— 然后把开启事件（§6）追加到**主链**；若那次追加失败，行被移除，所以一个段行总是带着它的开启事件。`AuditStore::close_segment(&segment_id)` 置 `state = closed` 与 `closed_at_ms`，然后追加关闭事件，若那次追加失败就把行放回 `open`。`segments` 表**没有 append-only 触发器** —— 它是坐在链*旁边*的记录，与 `runs` 一样 —— 所以它自己的行可以被更新；链的行只被追加。

## 4. 跨段引用

**[已定]** **段首记录它从何处承接，而那份记录是元数据。** 字段是 `head_prev_chain`：段开启时主链的头哈希。同时握着段与链的验证者，可以检查该段确实接在它自称的位置上，**而无需以任何不同方式重算哈希** —— 引用坐在事件旁边，[`compute_hash`](../audit/src/hash.rs) 从不读它。

**为什么不把它折进哈希。** 任何加进 `compute_hash` 的输入，都会改变它之后的每一个哈希，也就是另一个公式 —— 那正是 decisions §127 第 2 条所禁止的（「哈希公式不变」）。把引用放进一列，正是让链的保证仍是它自己的保证。

## 5. 仍开着的问题

[roadmap §7](roadmap-v1.0.zh-CN.md) 留着三个问题，本文**不**替它们收口 —— 而是把它们记下来，使实现有意地去面对：

1. **汇总链是否需要它自己的 `prev_hash`。**
2. **跨链引用怎么验证** —— 按摘要、按区间，还是两者都要。
3. 当两个段都声称同一个动作时，**冲突怎么判定**。

三者都在 roadmap §7 标为 `[open]`；其中（2）与（3）是并入阶段（M5-2）需要的输入；（1）关乎在 §2 已把物理形状定为 (b) 之后，段首如何成形。

## 6. 段事件（预留）

**[已定]** **生命周期事件在主链上**，由 M5-1b 写下：

- `host.audit.segment_opened` —— 一个段开启。detail：`{ "segment_id": …, "kind": "main" | "temporary", "head_prev_chain": <哈希或 null> }`。
- `host.audit.segment_closed` —— 一个段关闭。detail：`{ "segment_id": …, "closed_at_ms": <epoch ms> }`。
- `host.audit.segment_merged` —— 一个段的事件被写入主链。**预留**：§2 把形状定为转录，而**实现它的**是 **M5-2**；名字在此记录，好让并入有一个可用的名字。

两者都以 `segment_id IS NULL` 写下（记录段的*生命*不是段自己的事件），两者都是普通追加，且都携带 §3 所存的那个字（`kind` 是 `SegmentKind::as_str`）。

它们属 **`host.audit.`** 族，那是审计子系统自己的族（连接层的事实留在 `host.connection.*`：`key_minted`、`data_too_new`、`peer_offline`、`peer_recovered`）。它们是**审计事件名**，不是流名：`control-plane-events.md` 的二十个名字是另一套词汇（`agent:*`、`vm:*`、`m:*`……），本文不碰它们。

## 7. 本文没有什么

- **`provisional` 与 `fork`** —— 标记、折入与冲突规则 —— 是 **M5-2**，且需要先回答 §5 的问题。
- **临时中心** —— 谁接管、三层抑制、以及回归 —— 是 **M5-3**。
- **关键事件的即时推送**（一次逐出、一次 fork、一次接管）是 **M4e-2**；它旁边那份分批的 digest 由 [connection.md §7](connection.zh-CN.md) 覆盖。
- **段自己那条链的物理形状**已**定为 (b)** —— 见 §8。
- **它不是 digest 的传输。** digest 是什么、怎么上路，是 [connection.md §7](connection.zh-CN.md)。

## 8. 临时段住在哪里

**[已定]** **主链**是节点一直有的那份储存 —— 审计目录里的 `audit.db`（**backup** 已经在搬的那一份，`<workspace>/.riscdom/audit.db`）。它的行就是 `segment_id IS NULL` 的行，关于它的一切不变。

**[已定]** **一个临时段就是它自己的 SQLite 文件**：**同一个审计目录**里的 `audit-segments/<segment_id>.db`。一段一份，由 [`segment_db_path_in`](../audit/src/store.rs) 拼出，由 [`AuditStore::open_segment_store`](../audit/src/store.rs) 打开（并创建目录与文件）。该文件是一份**普通储存**：同一套 `audit_events` schema、同一批 append-only 触发器，以及**它自己的创世**（首批的 `prev_hash` 是 [`GENESIS_PREV_HASH`](../audit/src/hash.rs)）。

基准目录是调用者的 —— **审计目录**，不是 workspace —— 所以 `audit` 不拼任何宿主布局；它只拼 `audit-segments/<id>.db` 这一段。

**[已定]** **两者由 *主* 储存里的 `segments` 行系在一起。** 开启一个段（§3）就在那里登记它，`head_prev_chain` = 那一刻的主链头。段自己的储存装着它自己的事件；主储存装的是该段的**记录**，不是它的行。

**并入即转录（M5-2）。** 真中心回来时，一批读出该段的事件、把它们作为**新事件**追加到主链，然后追加并入事件（§6）。什么都不被改写：主链只会生长，而 `segments` 行转为 `folded`（无冲突）或 `forked`（有冲突）—— 由 §5 的第 3 个问题决定是哪一个。

## 9. 每条链如何被验证

**[已定]** **主链：`verify_chain`，未改。** 它按 id 顺序走一份储存，检查 `prev_hash` 链接并重算每个哈希 —— 而由于临时段是**另一个文件**，它读的主储存里只有主链的行，别无其它。分段感知模式不存在，也不需要存在。

**[已定]** **临时段：同一个函数，另一份储存。** `verify_chain(&segment_store)` 就是全部：段文件是一份有自己创世的普通储存，所以这一个验证者回答「这个段是否完好」，与它为主链作答的方式完全相同。没有第二个验证者，也没有第二个公式。

**跨链验证是 M6。** `head_prev_chain`（§4）是后一批据此核对两者的**锚点**；§5 的第 2 个问题 —— 按摘要、按区间，还是两者都要 —— 决定它的形状。在那之前，两条链各自可被验证，而它们的*关系*是已记录的元数据。
