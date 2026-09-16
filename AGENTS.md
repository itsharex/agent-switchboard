# Agent Switchboard

## 项目定位

- Agent Switchboard 是面向 Codex 与 Claude Code 的本地配置控制台：管理供应商档案、预览差异、执行可恢复切换并展示本机使用状态。
- 不新增其他客户端、云同步、遥测、账户体系、自动代理、提供商故障转移、渲染器注入或安装器补丁，除非产品范围明确改变。
- 产品与设计事实分别由 `README.md` 和 `DESIGN.md` 拥有；不维护任务流水文档。

## 工作路由与契约

- React 前端位于 `src/`；Tauri 边界位于 `src-tauri/`；共享 Rust 逻辑位于 `crates/asb-core/` 和 `crates/asb-switch/`。
- UI 只能请求类型化预览或切换操作；真实 Codex/Claude Code 配置写入只能由切换执行器完成。
- 切换保留不归该档案所有的键，只变更被选中覆盖层拥有的字段。真实写入必须可观察、备份、校验并可恢复。
- UI 改动读取相关 `src/components/`、`src/styles/` 与 `DESIGN.md`；配置行为改动检查受影响适配器、执行器、契约和测试。

## 验证

- 除非用户在当前请求中明确要求，不运行 `npm run typecheck`、`npm run test`、`cargo test`、`cargo check`、`cargo fmt` 或其他编译、类型检查与自动化测试。测试代码和脚本仅供用户明确要求时按最小相关范围手动执行。
- 用户明确要求执行写配置测试时，只使用隔离临时目录；不得触碰用户真实配置、凭据缓存或环境。
