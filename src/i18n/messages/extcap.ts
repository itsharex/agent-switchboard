import type { MessageEntry } from "../types";

/** Extension capability catalog text, keyed by the stable capability `code`
 * and client: the Rust producer emits `extcap.<client>.<code>.resource` and
 * `extcap.<client>.<code>.verification`; the panel resolves them via
 * `catalogText`. */
export const messages = {
  "extcap.codex.skillUser.resource": ["用户级 Skills：~/.agents/skills/<name>/SKILL.md（原生共享目录，其他工具也可能读取）", "User Skills: ~/.agents/skills/<name>/SKILL.md (native shared directory; other tools may also read it)"],
  "extcap.codex.skillUser.verification": ["以选定 Codex 发行版在隔离目录实测清单加载", "Manifest loading is measured in an isolated directory against the selected Codex build"],
  "extcap.codex.skillProject.resource": ["项目 Skills：<project>/.agents/skills/<name>/SKILL.md（含 CWD 相关发现层级）", "Project Skills: <project>/.agents/skills/<name>/SKILL.md (includes CWD-dependent discovery levels)"],
  "extcap.codex.skillProject.verification": ["以选定 Codex 发行版实测仓库层级发现", "Repository-level discovery is measured against the selected Codex build"],
  "extcap.codex.skillToggle.resource": ["Skill 停用：config.toml 的 [[skills.config]] 路径规则（canonical document path 匹配）", "Skill disabling: config.toml [[skills.config]] path rules (canonical document path matching)"],
  "extcap.codex.skillToggle.verification": ["以选定 Codex 发行版实测停用后不可调用", "Post-disable non-invocability is measured against the selected Codex build"],
  "extcap.codex.mcpUser.resource": ["用户级 MCP：$CODEX_HOME/config.toml 的 mcp_servers.<key>", "User MCP: mcp_servers.<key> in $CODEX_HOME/config.toml"],
  "extcap.codex.mcpUser.verification": ["以选定 Codex 发行版实测服务加载", "Server loading is measured against the selected Codex build"],
  "extcap.codex.mcpProjectShared.resource": ["项目共享 MCP：<project>/.codex/config.toml 的 mcp_servers.<key>", "Project-shared MCP: mcp_servers.<key> in <project>/.codex/config.toml"],
  "extcap.codex.mcpProjectShared.verification": ["以选定 Codex 发行版实测项目信任与服务加载", "Project trust and server loading are measured against the selected Codex build"],
  "extcap.codex.mcpDisable.resource": ["MCP 停用：服务条目内的 enabled = false", "MCP disabling: enabled = false inside the server entry"],
  "extcap.codex.mcpDisable.verification": ["以选定 Codex 发行版实测停用效果", "The disable effect is measured against the selected Codex build"],
  "extcap.codex.mcpProjectPrivate.resource": ["项目私有 MCP：Codex 没有私有项目文件格式，不支持", "Project-private MCP: Codex has no private project file format; unsupported"],
  "extcap.codex.mcpProjectPrivate.verification": ["Codex 官方提供私有项目文档后重新评估", "Re-evaluate once Codex officially documents private project files"],
  "extcap.claude.skillUser.resource": ["用户级 Skills：~/.claude/skills/<name>/SKILL.md", "User Skills: ~/.claude/skills/<name>/SKILL.md"],
  "extcap.claude.skillUser.verification": ["以选定 Claude Code 发行版在隔离目录实测清单加载", "Manifest loading is measured in an isolated directory against the selected Claude Code build"],
  "extcap.claude.skillProject.resource": ["项目 Skills：<project>/.claude/skills/<name>/SKILL.md（含父级与嵌套发现规则）", "Project Skills: <project>/.claude/skills/<name>/SKILL.md (including parent and nested discovery rules)"],
  "extcap.claude.skillProject.verification": ["以选定 Claude Code 发行版实测发现层级", "Discovery levels are measured against the selected Claude Code build"],
  "extcap.claude.skillToggle.resource": ["Skill 可见性：settings 的 skillOverrides.<name>（on / name-only / user-invocable-only / off）", "Skill visibility: settings skillOverrides.<name> (on / name-only / user-invocable-only / off)"],
  "extcap.claude.skillToggle.verification": ["以选定 Claude Code 发行版实测四态语义", "The four-state semantics are measured against the selected Claude Code build"],
  "extcap.claude.mcpUser.resource": ["用户级 MCP：~/.claude.json 的 mcpServers（自定义 CLAUDE_CONFIG_DIR 下状态文件位置未核实）", "User MCP: mcpServers in ~/.claude.json (state-file location unverified under a custom CLAUDE_CONFIG_DIR)"],
  "extcap.claude.mcpUser.verification": ["以官方 CLI 在隔离 CLAUDE_CONFIG_DIR 中创建无凭据假服务核实状态文件位置", "Verify the state-file location by creating a credential-free fake server in an isolated CLAUDE_CONFIG_DIR with the official CLI"],
  "extcap.claude.mcpUserStandard.resource": ["用户级 MCP：~/.claude.json 的 mcpServers.<key>", "User MCP: mcpServers.<key> in ~/.claude.json"],
  "extcap.claude.mcpUserStandard.verification": ["以选定 Claude Code 发行版实测服务加载", "Server loading is measured against the selected Claude Code build"],
  "extcap.claude.mcpProjectShared.resource": ["项目共享 MCP：<project>/.mcp.json 的 mcpServers.<key>（受项目信任限制）", "Project-shared MCP: mcpServers.<key> in <project>/.mcp.json (subject to project trust)"],
  "extcap.claude.mcpProjectShared.verification": ["以选定 Claude Code 发行版实测项目信任交互", "Project-trust interaction is measured against the selected Claude Code build"],
  "extcap.claude.mcpProjectPrivate.resource": ["项目私有 MCP：~/.claude.json 的 projects[<绝对路径>].mcpServers.<key>", "Project-private MCP: projects[<absolute path>].mcpServers.<key> in ~/.claude.json"],
  "extcap.claude.mcpProjectPrivate.verification": ["以选定 Claude Code 发行版实测私有作用域优先级", "Private-scope precedence is measured against the selected Claude Code build"],
  "extcap.claude.mcpDisable.resource": ["项目级停用：projects[<路径>].disabledMcpServers 成员（用户级停用为撤下条目）", "Project-level disabling: projects[<path>].disabledMcpServers membership (user-level disabling removes the entry)"],
  "extcap.claude.mcpDisable.verification": ["以选定 Claude Code 发行版实测 /mcp 停用状态对应字段", "The /mcp disabled-state field is measured against the selected Claude Code build"],
} as const satisfies Record<string, MessageEntry>;
