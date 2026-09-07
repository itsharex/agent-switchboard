use crate::contracts::AppKind;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ValidationError {
    #[error("供应商名称不能为空")]
    EmptyName,
    #[error("供应商标识不能为空")]
    EmptyId,
    #[error("自定义供应商必须填写服务地址")]
    CustomRequiresBaseUrl,
    #[error("base_url 必须是 http(s) URL，当前值为 {0:?}；请填写包含协议头的完整地址")]
    BadBaseUrl(String),
    #[error("必须填写 API 密钥")]
    EmptyApiKey,
    #[error("自定义供应商必须选择上游 API 格式")]
    CustomRequiresProtocol,
    #[error("Codex 转换到 Anthropic Messages 时必须配置正整数的最大输出 token 数")]
    CodexAnthropicRequiresMaxOutputTokens,
    #[error("最大输出 token 数只适用于 Codex 转换到 Anthropic Messages 的供应商")]
    UnexpectedMaxOutputTokens,
    #[error("API 密钥最长 {0} 个字符")]
    ApiKeyTooLong(usize),
    #[error("官方登录不得携带服务地址、API 密钥、上游协议或模型覆盖；请先移除这些自定义路由字段")]
    OfficialRouteHasCustomFields,
    #[error("模型参数类型 {options_kind:?} 与供应商所属客户端 {app:?} 不一致")]
    ModelOptionsMismatch {
        options_kind: &'static str,
        app: AppKind,
    },
    #[error("键 {key} 不是 {app:?} 的通用设置参数；通用设置文件只能包含设置页列出的参数")]
    UnknownCommonKey { app: AppKind, key: String },
    #[error("通用设置缺少参数 {key:?} 的值；请重新加载后再保存")]
    MissingCommonKey { key: String },
    #[error("键 {key} 的值必须是 {allowed} 之一，当前值为 {value:?}")]
    BadCommonValue {
        key: String,
        value: String,
        allowed: String,
    },
    #[error("上下文窗口必须是正整数（token 数）")]
    BadContextWindow,
    #[error("availableModels 不能包含空行；请每行填写一个模型标识")]
    EmptyAvailableModel,
    #[error("{field} 不能包含 1M 标记；请通过 1M 上下文复选框设置")]
    InlineOneMMarker { field: &'static str },
    #[error("{field} 已启用 1M 上下文，但未填写模型")]
    OneMRequiresModel { field: &'static str },
    #[error("键 {key:?} 的数值必须为有限数")]
    NonFiniteNumber { key: String },
    #[error("官网地址必须是 http(s) URL，当前值为 {0:?}；请填写包含协议头的完整地址，或留空")]
    BadWebsiteUrl(String),
    #[error("备注最长 {0} 个字符")]
    NotesTooLong(usize),
    #[error("用量查询地址不能为空")]
    EmptyUsageQueryUrl,
    #[error("用量查询地址必须以 http(s) 地址或 {{baseUrl}} 开头")]
    BadUsageQueryUrl,
    #[error("用量查询至少要配置一个提取路径（余额 / 已用 / 总量）")]
    UsageQueryExtractsNothing,
    #[error("用量查询的 {field} 不能为空")]
    EmptyUsageQueryField { field: &'static str },
    #[error("用量查询脚本不能为空")]
    EmptyUsageQueryScript,
    #[error("用量查询脚本最长 {0} 个字符")]
    UsageQueryScriptTooLong(usize),
    #[error("自动刷新间隔须为 0–{0} 分钟，0 表示关闭")]
    UsageQueryRefreshIntervalTooLarge(u32),
    #[error("订阅额度自动刷新间隔只适用于 Codex 官方登录供应商")]
    QuotaIntervalRequiresOfficialCodex,
    #[error("订阅额度自动刷新间隔须为 1–{0} 分钟；未设置表示关闭")]
    OfficialQuotaRefreshIntervalOutOfRange(u32),
}

/// Metadata fields shared by draft and profile; kept short and local-only.
pub(super) const MAX_NOTES_LEN: usize = 500;
/// Upper bound of every auto-refresh interval contract, in whole minutes
/// (a day). Both the usage-query interval and the official Codex quota
/// interval consume this one bound.
pub const MAX_AUTO_REFRESH_INTERVAL_MINUTES: u32 = 1440;
