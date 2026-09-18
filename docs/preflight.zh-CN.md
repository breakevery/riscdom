[English](preflight.md) | 中文

# 环境预检

RiscDom 检查你配置的工具链与 QEMU **真的能配合工作**——方式是**跑一遍**，而不是比对版本号。四步是：

1. 所配置的 RISC-V GCC 能启动并回答 `--version`；
2. 它能**在真实路径上**编译一个四行的 guest；
3. 所配置的 QEMU 能启动并回答 `--version`；
4. 它能启动该 guest，且 guest 的 banner 出现在串口上。

四步是**快速失败**的：后一步依赖前一步，所以失败会直接指出第一个没走通的环节。

## 为什么不做版本矩阵

本仓库没有任何 QEMU × GCC 兼容矩阵，也没有去猜一个：版本规则只能靠编，而**错的规则比没有规则更糟**
（见 `PROJECT_CONSTITUTION.md` §10 的 v0.4 第 5 条）。把真实的一对跑一遍，回答的才是真问题——"这套
配置在本机能跑吗"——并且能盖住用户真正会踩的坑：工具链在过长或含空格的路径下、QEMU 起不来、能编译
但永远启动不了。

## 什么时候跑

- 你设置了手动工具链 / QEMU 路径之后；
- 配置变化后的**首次运行**——缓存结果与配置指纹绑定，配置一变即失效；
- 你点 **设置 → 工具链 → 环境预检 → 重新预检** 时。

它不会每次启动都跑，也不会阻塞界面：面板按 `preflight:progress` 逐步显示。

## 它不做什么

- **绝不写入审计链**：预检是环境检查，不是一次 run。
- **不会让 run 失败**：预检失败只是一条告警，附上失败步骤的原始输出与建议；是否继续由 run 自己决定。
- **不碰宿主持有的 VM**：预检 guest 用**自己的端口**单独启动，马上停掉。

## 逃生阀

预检失败时你可以**仍要继续**。该选择会按配置记进 `settings.json`，于是这条告警在你下次改配置之前不再
出现。接受**不改变事实**：面板照旧显示哪一步失败，记录里也写明这是你的选择。

## 实现在哪

- `host/src/preflight.rs`：步骤词表、缓存结构、banner 等待。
- `host/src/state.rs`：`preflight_status` / `ensure_preflight` / `acknowledge_preflight`（执行器）。
- `host/src/commands.rs`：`preflight_status` / `run_preflight` / `acknowledge_preflight`，以及路径变更
  时触发的后台预检。
- `ui/src/lib/preflightView.ts`：设置页显示的文字。
