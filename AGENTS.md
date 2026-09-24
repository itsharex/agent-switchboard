# Agent Switchboard

## 项目定位

- Agent Switchboard 是面向 Codex 与 Claude Code 的本地配置控制台：管理供应商档案、预览差异、执行可恢复切换并展示本机使用状态。
- 不新增其他客户端、云同步、遥测、账户体系、自动代理、隐式提供商切换、渲染器注入或安装器补丁，除非产品范围明确改变。
- 产品与设计事实分别由 `README.md` 和 `DESIGN.md` 拥有；不维护任务流水文档。

## 工作路由与契约

- React 前端位于 `src/`；Tauri 边界位于 `src-tauri/`；共享 Rust 逻辑位于 `crates/asb-core/` 和 `crates/asb-switch/`。
- UI 只能请求类型化预览或切换操作；真实 Codex/Claude Code 配置写入只能由切换执行器完成。
- 切换保留不归该档案所有的键，只变更被选中覆盖层拥有的字段。真实写入必须可观察、备份、校验并可恢复。
- UI 改动读取相关 `src/components/`、`src/styles/` 与 `DESIGN.md`；配置行为改动检查受影响适配器、执行器、契约和测试。

## 决策笔记（Agent Notes）

- 非平凡改动（改了行为、架构、跨文件契约、流程与工具链、测试策略、落盘/网络/配置格式）前，遵循 `.agents/skills/write-notes-like-deepseek/SKILL.md` 写或更新 `.agents/notes/` 笔记；机械性小改（样式、格式化、打标、不改行为的补丁）直接提交代码。
- 写之前先检索 `.agents/notes/` 同主题旧笔记：有归属就地更新事实；新想法先放 `proposed/`，落地随同代码改动转 `implemented/`；新方案彻底取代旧决策时，同批归档旧篇并标明被谁取代。
- 被放弃的方案先写它最强的理由，再解释为什么不用。
- 决策笔记记录决定与取舍，不属于「项目定位」中禁止的任务流水文档；`README.md` 与 `DESIGN.md` 的事实归属不变。
- 笔记落盘后跑 `npm run verify-notes`（只校验笔记结构，不涉及编译与项目测试），红了先修再交。

## 验证

- 除非用户在当前请求中明确要求，不运行 `npm run typecheck`、`npm run test`、`cargo test`、`cargo check`、`cargo fmt` 或其他编译、类型检查与自动化测试。测试代码和脚本仅供用户明确要求时按最小相关范围手动执行。
- 用户明确要求执行写配置测试时，只使用隔离临时目录；不得触碰用户真实配置、凭据缓存或环境。
