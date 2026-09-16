# Agent Switchboard 网站

## 项目定位

- 这是与桌面应用工程分离的 Vite、React、TypeScript 静态介绍站。
- 配置积木台展示来自桌面端适配器生成的静态证据；部署只读取站点产物，不在站点构建中执行 Rust。

## 工作路由

- `src/content/site-content.ts`：站点文案与链接内容的唯一所有者。
- `src/styles/tokens.css`：运行时视觉令牌的唯一所有者；页面组件只消费它定义的令牌。
- `src/generated/configuration-assembly.ts`：桌面端生成的展示产物；核心渲染规则变动时与生成器一起核对。
- `public/`：静态资源；`vite.config.ts`：站点构建行为。

## 本地约束

- 保持站点与桌面应用的工程边界，不在此目录复制桌面端运行逻辑。
- 下载入口固定指向 GitHub Releases 最新页，不在站点内维护安装包文件名或版本号。

## 验证与交付

- 除非用户在当前请求中明确要求，不运行 `npm run typecheck`、`npm run build`、`npm run verify:assembly` 或任何自动化测试。相关脚本仅在用户明确要求时按最小范围手动执行。
- 当前 `package.json` 未定义部署脚本；部署设置变动时阅读本目录 `README.md`。
