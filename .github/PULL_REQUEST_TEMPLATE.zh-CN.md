[English](PULL_REQUEST_TEMPLATE.md) | 中文

<!-- 感谢你的贡献。见 `CONTRIBUTING.zh-CN.md`：gate 是「绿」的唯一定义，提交一律走包装脚本
     （`scripts\commit.ps1` / `./scripts/commit.sh`），不要直接 `git commit`。文件名写成代码，
     因为这段文字会被插进 PR 正文，相对链接在那里不会像在文件里那样解析。
     填写时请把这些注释删掉。 -->

## 改了什么

<!-- 一段话。点名动到的 crate 或文档，以及**行为**的变化，不要复述 diff。 -->

## 关联 issue

<!-- 「Closes #NN」或「Refs #NN」；没有 issue 就写 none。 -->

none

## 怎么验证的

<!-- 你跑的命令与它的相关输出。跑不了就说明你改用了什么办法。
     每个 PR 都必须在 CI 里过 gate。 -->

## 检查清单

- [ ] 本地 gate 全绿（Windows 用 `scripts\gate.ps1`，其它平台用 `sh scripts/gate.sh`）
- [ ] 这里改动的文档是双语的（`.md` + `.zh-CN.md`，首行有语言切换行）
- [ ] 没有在任何地方新增 API Key、token 或凭据——代码、日志、`Debug` 输出、前端都不行
- [ ] 改动是收敛的；没有碰无关文件
- [ ] 已签署 CLA（见 `CLA.zh-CN.md`；轻微修正按 §9 豁免）
