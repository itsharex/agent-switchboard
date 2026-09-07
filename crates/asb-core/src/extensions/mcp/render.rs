use std::collections::BTreeMap;

use crate::extensions::contracts::{CodexServerOptions, McpDefinition, SecretValue};

/// Errors from projection. Document-level failures carry the adapter error
/// with its line information.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ProjectionError {
    /// The definition cannot be projected to the requested client.
    #[error("{0}")]
    Unsupported(String),
    /// A secret reference could not be resolved while preparing the render.
    #[error("凭据引用 {0} 无法解析；请在扩展库中补充该凭据")]
    SecretUnavailable(String),
    /// Codex's `bearer_token_env_var` accepts an environment variable name
    /// only; a stored secret cannot be written there.
    #[error(
        "Codex 的 bearer_token_env_var 只接受环境变量名；请改用环境变量引用或改选 Claude 目标"
    )]
    BearerNeedsEnvName,
    #[error(transparent)]
    Document(#[from] crate::adapter::AdapterError),
}

/// The concrete Codex render of one server. Values are final; secret
/// references were resolved by the caller.
#[derive(Debug, Clone, PartialEq)]
pub enum CodexServerRender {
    Stdio {
        command: String,
        args: Vec<String>,
        env: BTreeMap<String, String>,
        /// Host environment variable names passed through by name.
        env_vars: Vec<String>,
        options: Option<CodexServerOptions>,
        enabled: bool,
    },
    Http {
        url: String,
        bearer_token_env_var: Option<String>,
        http_headers: BTreeMap<String, String>,
        env_http_headers: BTreeMap<String, String>,
        enabled: bool,
    },
}

/// The concrete Claude render of one server.
#[derive(Debug, Clone, PartialEq)]
pub enum ClaudeServerRender {
    Stdio {
        command: String,
        args: Vec<String>,
        env: BTreeMap<String, String>,
    },
    Http {
        url: String,
        headers: BTreeMap<String, String>,
    },
    Sse {
        url: String,
        headers: BTreeMap<String, String>,
    },
    Ws {
        url: String,
        headers: BTreeMap<String, String>,
    },
}

/// Resolves a stored secret reference to its concrete value, or `None`.
pub type SecretResolve<'a> = &'a dyn Fn(&str) -> Option<String>;

fn resolve_value(
    value: &SecretValue,
    resolve: SecretResolve,
) -> Result<Option<String>, ProjectionError> {
    match value {
        SecretValue::EnvRef { name } => Ok(Some(format!("${{{name}}}"))),
        SecretValue::Plain { value } => Ok(Some(value.clone())),
        SecretValue::SecretRef { reference } => resolve(reference)
            .map(Some)
            .ok_or_else(|| ProjectionError::SecretUnavailable(reference.clone())),
    }
}

/// Projects one definition to the Codex shape. Environment references become
/// `env_vars` / `env_http_headers` / `bearer_token_env_var` names; literals
/// and resolved secrets become values. Claude-only transports are refused.
pub fn render_codex(
    definition: &McpDefinition,
    enabled: bool,
    resolve: SecretResolve,
) -> Result<CodexServerRender, ProjectionError> {
    match definition {
        McpDefinition::Stdio {
            command,
            args,
            env,
            codex_options,
        } => {
            let mut literal_env = BTreeMap::new();
            let mut env_vars = Vec::new();
            for (name, secret) in env {
                match secret {
                    SecretValue::EnvRef { name: variable } => env_vars.push(variable.clone()),
                    other => {
                        if let Some(resolved) = resolve_value(other, resolve)? {
                            literal_env.insert(name.clone(), resolved);
                        }
                    }
                }
            }
            env_vars.sort();
            Ok(CodexServerRender::Stdio {
                command: command.clone(),
                args: args.clone(),
                env: literal_env,
                env_vars,
                options: codex_options.clone(),
                enabled,
            })
        }
        McpDefinition::Http {
            url,
            headers,
            bearer,
        } => {
            let mut http_headers = BTreeMap::new();
            let mut env_http_headers = BTreeMap::new();
            for (name, secret) in headers {
                match secret {
                    SecretValue::EnvRef { name: variable } => {
                        env_http_headers.insert(name.clone(), variable.clone());
                    }
                    other => {
                        if let Some(resolved) = resolve_value(other, resolve)? {
                            http_headers.insert(name.clone(), resolved);
                        }
                    }
                }
            }
            let bearer_token_env_var = match bearer {
                None => None,
                Some(SecretValue::EnvRef { name }) => Some(name.clone()),
                Some(SecretValue::SecretRef { .. }) | Some(SecretValue::Plain { .. }) => {
                    return Err(ProjectionError::BearerNeedsEnvName);
                }
            };
            Ok(CodexServerRender::Http {
                url: url.clone(),
                bearer_token_env_var,
                http_headers,
                env_http_headers,
                enabled,
            })
        }
        McpDefinition::ClaudeSse { .. } | McpDefinition::ClaudeWs { .. } => Err(
            ProjectionError::Unsupported("Claude 专用传输不能投影到 Codex".to_string()),
        ),
    }
}

/// Projects one definition to the Claude shape. Environment references render
/// as the native `${NAME}` expression; stored secrets render as literals and
/// every target that receives one is reported by the plan preview.
pub fn render_claude(
    definition: &McpDefinition,
    resolve: SecretResolve,
) -> Result<ClaudeServerRender, ProjectionError> {
    let render_headers = |headers: &BTreeMap<String, SecretValue>,
                          bearer: Option<&SecretValue>|
     -> Result<BTreeMap<String, String>, ProjectionError> {
        let mut rendered = BTreeMap::new();
        for (name, secret) in headers {
            if let Some(resolved) = resolve_value(secret, resolve)? {
                rendered.insert(name.clone(), resolved);
            }
        }
        if let Some(bearer) = bearer {
            let resolved = resolve_value(bearer, resolve)?;
            if let Some(token) = resolved {
                rendered.insert("Authorization".to_string(), format!("Bearer {token}"));
            }
        }
        Ok(rendered)
    };
    match definition {
        McpDefinition::Stdio {
            command, args, env, ..
        } => {
            let mut rendered_env = BTreeMap::new();
            for (name, secret) in env {
                if let Some(resolved) = resolve_value(secret, resolve)? {
                    rendered_env.insert(name.clone(), resolved);
                }
            }
            Ok(ClaudeServerRender::Stdio {
                command: command.clone(),
                args: args.clone(),
                env: rendered_env,
            })
        }
        McpDefinition::Http {
            url,
            headers,
            bearer,
        } => Ok(ClaudeServerRender::Http {
            url: url.clone(),
            headers: render_headers(headers, bearer.as_ref())?,
        }),
        McpDefinition::ClaudeSse { url, headers } => Ok(ClaudeServerRender::Sse {
            url: url.clone(),
            headers: render_headers(headers, None)?,
        }),
        McpDefinition::ClaudeWs { url, headers } => Ok(ClaudeServerRender::Ws {
            url: url.clone(),
            headers: render_headers(headers, None)?,
        }),
    }
}
