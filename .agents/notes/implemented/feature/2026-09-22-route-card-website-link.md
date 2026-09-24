# Agent Note: 连接卡官网列与跨客户端档案查找契约

Status: implemented

## Problem

供应商页顶部连接卡需要展示当前供应商的官网并可点击打开。直接实现暴露了一个既有 wiring 缺口：`useConfigSnapshot.profiles` 只含 Claude 档案（`records` 过滤了 `app === "claude"`），而 `DualRelay` 在两个客户端页面都渲染两张卡。Codex 卡片的供应商名此前靠 `route.providerName` 回退才没有暴露这个缺口，但官网地址是档案本地字段、从不写入客户端配置，没有任何回退来源——如果只接 `snapshot.profiles`，Codex 卡片的官网永远显示「未设置」。

## Decision

连接卡新增第四列「官网」，事实来源是**当前匹配档案**的 `websiteUrl`，经 `@tauri-apps/plugin-opener` 打开系统浏览器；未匹配档案或未填写时如实显示「未设置」。不读取服务地址猜测官网。

契约按三层单一所有者落地：

- **匹配规则**归 `src/lib/current-provider-name.ts`：新导出 `ActiveProfileRef`（`{ app, id, name, websiteUrl }` 最小形状），`currentProviderProfile` 以 `app + activeProfileId` 匹配，`currentProviderName` 复用同一规则。两个档案存储的记录结构都满足该形状（结构类型兼容，诊断页 `ConfigStatusPanel` 无需改动）。
- **全量清单组装**归 `useConfigSnapshot`：派生 `relayProfiles` 扁平化三个 store——Claude 档案、Codex 官方档案（官网在 profile 级）、Codex 第三方档案（官网在 **record 级**，组装时映射 `{ ...profile, app: "codex", websiteUrl: record.websiteUrl }`）。
- **消费**归 `ProviderWorkspaceShell` / `DualRelay`：shell 的 `profiles` prop 类型收窄为 `readonly ActiveProfileRef[]`，两个页面统一传 `snapshot.relayProfiles`。`CodexProvidersPage` 原先的 `profiles` prop（实为 claude-only 错误数据、仅喂 relay）整条删除。

链接样式 `asb-row-host` 改名 `asb-provider-link`：该样式被供应商行与连接卡共用，名字不再绑死行上下文。打开行为与列表行官网链接复用同一 `ProviderEndpoint` 组件。

## Alternatives considered

- **在 snapshot 上暴露 `activeWebsiteUrl(app)` 解析器**：与 `currentProviderName` 形成两处「谁是当前档案」的判定规则，规则复制即债，否。改为规则归 lib、组装归 snapshot、各自单所有者。
- **用 `route.baseUrl` 的 host 渲染成链接**：无需任何档案数据、实现最小，但服务地址不等于官网，虚构了卡片事实，违反「连接卡只展示从真实配置读取的状态」的设计契约，否。
- **`DualRelay` 按 app 拆两组 props（claude 列表 + codex 列表）**：每页传参不同、app 匹配逻辑外泄到调用方，否。单一 `relayProfiles` 列表 + 既有 app 匹配规则更小。

## Consequences

Codex 卡片的供应商名现在优先走档案匹配（命中档案显示档案名，未命中才回退 `route.providerName`）——顺手修正了 Codex 页 relay 输入数据错误的问题。诊断页 `ConfigStatusPanel` 维持既有 claude-only 输入与回退行为，结构兼容未动。`relayProfiles` 依赖三个 store 数组的 memo，随快照刷新重建，频率低。验证为人工检查（项目规则禁跑 typecheck/测试）；`npm run dev` 下建议目检 Codex 第三方、Codex 官方、Claude 三类档案的官网列取值。
