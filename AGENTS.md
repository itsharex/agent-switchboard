# Agent Switchboard

## 项目定位

- 面向 Codex 与 Claude Code 的本地配置控制台，由 React/Vite 前端、Tauri 桌面壳和 Rust workspace 组成。
- 它管理供应商档案、切换预览和恢复；协议转换只服务本机回环，并非公共代理或多客户端产品。

## 工作路由

- `src/`：React 页面、组件和客户端 API；变更界面契约时同步检查 `src-tauri/` 命令及关联 `*.test.tsx`。
- `src-tauri/`：Tauri 命令、窗口和本机运行时；`tauri.conf.json` 拥有桌面打包配置。
- `crates/asb-core/`：共享核心、适配器和网站装配生成逻辑；`crates/asb-switch/`：配置切换、备份和恢复事务。
- `website/`：独立静态站，拥有自己的 `AGENTS.md`；仅在站点或生成产物相关改动时进入该目录。

## 本地约束

- 只支持 Codex 与 Claude Code；未经明确需求不得扩展为账户、云同步、遥测或通用代理。
- 对真实客户端配置的变更必须先展示脱敏预览并经确认；测试使用隔离路径，日志、文档和差异不得含密钥或私有端点。
- 协议转换仅监听 `127.0.0.1`；客户端配置只能得到本地端点和能力令牌。

## 验证与交付

- 前端改动运行 `npm run typecheck` 和 `npm test`；安装器发布脚本改动运行 `npm run test:updater-release`。
- 改动 Rust workspace、配置事务或 Tauri 命令时运行 `cargo test --workspace`。
- 需启动桌面应用时使用 `npm run dev:desktop`；Windows 打包只在明确的发布任务中运行 `npm run tauri:build:windows`。
