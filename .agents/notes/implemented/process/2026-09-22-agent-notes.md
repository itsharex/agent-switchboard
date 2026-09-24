# Agent Note: 引入 DeepSeek 式决策笔记（Agent Notes）

Status: implemented

## Problem

本仓库由 AI 多会话密集开发，代码只表达「系统现在怎么跑」，为什么这样定、否过哪些路线散落在会话对话里，会话结束即丢失。`AGENTS.md` 管住了行为准则，但没有决策留痕机制：被否掉的方案会被新会话重提，为避坑故意写的代码会被当作坏味道「优化」掉。

## Decision

采用 DeepSeek Harness 的 Agent Notes 方法（上游 `czm15053/write-notes-like-deepseek`，MIT）。技能装在 `.agents/skills/write-notes-like-deepseek/`，笔记树在 `.agents/notes/{proposed,implemented,rejected,archived}/{feature,bug-fix,simplification,architecture,process,testing}/yyyy-mm-dd-topic.md`，目录即状态、不建 INDEX.md。三条校验脚本在 `scripts/`，经 `npm run verify-notes` 串跑作为机械门禁；归档由 `npm run archive-note` 封印进 `manifest.json`（SHA-256，只增不改）。非平凡改动先写 `proposed/`，落地与代码同批转 `implemented/`。

## Alternatives considered

- **只靠 git history 与 AGENTS.md 散文约定** — 零额外成本，但 commit message 检索「为什么」效率低，散文约定对 AI 无强制力（DeepSeek Harness 实测：agent 遵守机械门禁的可靠性远高于散文式约定）。
- **传统 `docs/adr/` 目录** — 社区成熟方案，但缺少生命周期目录树、归档封印与死链检查，也没有面向 AI 会话的触发协议和现成校验脚本，等于自己重造整套门禁。

## Consequences

- **收益**：跨会话决策可检索可审计；被否路线有据可防重提；归档笔记篡改可被哈希校验发现。
- **代价与已知上限**：每笔非平凡改动多一篇笔记的维护开销，笔记与代码漏同步会腐化；校验脚本运行依赖 `npx tsx`（Node ≥ 18，首次联网拉取）；结构由脚本把关，笔记内容质量仍靠写时自检。

## Verification

`npm run verify-notes` 三线校验通过；本篇为笔记树首篇（`implemented/process/`）。
