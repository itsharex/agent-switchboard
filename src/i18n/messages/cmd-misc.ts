import type { MessageEntry } from "../types";

/** Backend command errors translated on the renderer side. Keys are emitted
 * by desktop commands via `CommandError::keyed/localized` (see the matching
 * src-tauri area); `message` stays the scrubbed raw diagnostic fallback. */
export const messages = {
  // commands/discovery.rs
  "errors.misc.noReadableCodexConfig": ["当前没有可读取的 Codex 配置", "No readable Codex configuration found"],
  "errors.misc.noImportableCodexProviders": ["当前配置没有可导入的 Codex 供应商", "The current configuration has no importable Codex providers"],
  "errors.misc.gatewayRouteActiveBlockImport": ["本机协议网关正在使用供应商；请先切换到直连或官方登录后再导入供应商", "The local protocol gateway is routing through a provider; switch to direct connection or official login before importing providers"],
  "errors.misc.noExportableProviders": ["没有可导出的供应商档案", "No exportable provider profiles"],
  "errors.misc.noExportableProvidersWithSkipped": ["没有可导出的供应商档案；{count} 项无法导出", "No exportable provider profiles; {count} items cannot be exported"],
  "errors.misc.exportTargetIsDirectory": ["导出目标是一个目录，请提供文件路径", "The export target is a directory; provide a file path"],
  "errors.misc.exportParentDirMissing": ["导出目标的父目录不存在", "The parent directory of the export target does not exist"],
  "errors.misc.exportWriteFailed": ["无法写入导出文件：{error}", "Failed to write the export file: {error}"],
  // commands/official_login.rs
  "errors.misc.officialLoginInProgress": ["已有进行中的官方登录，请先完成或取消", "An official login is already in progress; finish or cancel it first"],
  "errors.misc.officialLoginNotStarted": ["尚未开始官方登录", "The official login has not started"],
  // commands/provider_endpoints.rs
  "errors.misc.writeGateNotInitialized": ["写入闸门尚未初始化", "The write gate has not been initialized"],
  "errors.misc.officialProviderNoCustomEndpoints": ["官方登录供应商不支持自定义服务端点", "Official-login providers do not support custom endpoints"],
  "errors.misc.activeProviderEndpointChangeBlocked": ["当前 Claude 供应商正在使用，请重新应用后再修改服务端点", "The current Claude provider is in use; re-apply it before modifying endpoints"],
  // commands/quota_refresh.rs
  "errors.misc.providerNotFound": ["供应商不存在", "Provider not found"],
  "errors.misc.profileNotCodexOfficial": ["此档案不是 Codex 官方登录", "This profile is not a Codex official login"],
  "errors.misc.quotaLockUnavailable": ["官方额度查询锁不可用", "The official quota query lock is unavailable"],
  // commands/runtime_log.rs
  "errors.misc.logDirCreateFailed": ["无法创建应用日志目录", "Failed to create the app log directory"],
  "errors.misc.logDirOpenFailed": ["无法打开应用日志文件夹", "Failed to open the app log folder"],
  // commands/codex_probe.rs
  "errors.misc.probeInvalidCount": ["检测次数必须在 1 到 {max} 之间", "The probe run count must be between 1 and {max}"],
  "errors.misc.probeUnknownStatus": ["未知的检测状态：{status}", "Unknown probe status: {status}"],
  // commands/window.rs
  "errors.misc.mainWindowConfigMissing": ["缺少主窗口配置", "The main window configuration is missing"],
  "errors.misc.mainWindowMinWidthMissing": ["缺少主窗口最小宽度", "The main window minimum width is missing"],
  "errors.misc.mainWindowMinHeightMissing": ["缺少主窗口最小高度", "The main window minimum height is missing"],
  "errors.misc.directoryPickerNoResult": ["目录选择对话框未返回结果", "The directory picker dialog returned no result"],
  "errors.misc.filePickerNoResult": ["文件选择对话框未返回结果", "The file picker dialog returned no result"],
  // provider_request/claude.rs
  "errors.misc.claudeNativeSdkOnly": ["此 Claude 档案使用原生云 SDK，请通过 Claude Code 验证，不能发送普通 HTTP 测试请求", "This Claude profile uses the native cloud SDK; verify through Claude Code instead of sending a plain HTTP test request"],
  "errors.misc.claudeAccountChanged": ["Claude 托管账号或目标已变化，请重新准备请求", "The Claude managed account or target has changed; prepare the request again"],
  // provider_request/connection.rs
  "errors.misc.providerRequestCustomOnly": ["真实请求仅适用于自定义供应商（API 密钥或托管账号）", "Real requests apply only to custom providers (API key or managed account)"],
  "errors.misc.endpointUrlCredentialsForbidden": ["真实请求要求服务地址不含 URL 凭据、片段或控制字符", "Real requests require a service URL without URL credentials, fragments, or control characters"],
  "errors.misc.providerRequestConnectionInvalid": ["请检查服务地址、API 密钥、API 格式和 Responses 请求模式", "Check the service URL, API key, API format, and Responses request mode"],
  // provider_request/transport.rs
  "errors.misc.requestBodyOverrideInvalid": ["请求 body 覆盖无效", "The request body override is invalid"],
  "errors.misc.requestBuildFailed": ["无法构造请求，请检查供应商服务地址与 API 密钥格式", "Failed to build the request; check the provider service URL and API key format"],
  // provider_request/mod.rs
  "errors.misc.providerRequestModelInvalid": ["请填写有效的模型 ID（1–256 个字符，不含控制字符）", "Enter a valid model ID (1–256 characters, no control characters)"],
  "errors.misc.providerRequestProfileChanged": ["供应商档案已变化，请重新准备并核对请求目标后再发送", "The provider profile has changed; prepare again and verify the request target before sending"],
  "errors.misc.providerRequestInterrupted": ["真实请求任务中断，请重试", "The real request task was interrupted; try again"],
  // provider_request/registry.rs
  "errors.misc.providerRequestLimitReached": ["待处理请求过多，请关闭其他请求面板后重试", "Too many pending requests; close other request panels and retry"],
  "errors.misc.providerRequestPreparationUnavailable": ["请求准备已失效、已取消或已经使用，请重新准备后发送", "The request preparation has expired, was cancelled, or was already used; prepare again before sending"],
  "errors.misc.providerRequestStateUnavailable": ["请求状态不可用，请重新打开应用后重试", "Request state is unavailable; reopen the app and retry"],
  // dev_api/http.rs
  "errors.misc.webOriginRejected": ["开发后端只接受本机 Vite 页面请求", "The development backend only accepts local Vite page requests"],
  "errors.misc.webCommandNotFound": ["开发后端不存在该接口", "The development backend has no such endpoint"],
  "errors.misc.webContentTypeInvalid": ["开发后端请求必须使用 JSON", "Development backend requests must use JSON"],
  "errors.misc.webRequestUnreadable": ["无法读取开发后端请求", "Failed to read the development backend request"],
  "errors.misc.webRequestInvalid": ["开发后端请求格式无效", "The development backend request format is invalid"],
  "errors.misc.webArgumentMissing": ["缺少参数：{name}", "Missing argument: {name}"],
  "errors.misc.webArgumentInvalid": ["参数无效：{name}", "Invalid argument: {name}"],
  "errors.misc.webResponseSerializeFailed": ["开发后端响应无法序列化", "The development backend response could not be serialized"],
  "errors.misc.webCommandUnavailable": ["浏览器开发环境不支持该原生窗口命令", "The browser development environment does not support this native window command"],
} as const satisfies Record<string, MessageEntry>;
