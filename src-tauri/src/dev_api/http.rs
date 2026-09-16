use super::dispatch::dispatch;
use super::DEV_API_HEALTH_STATUS;
use crate::commands::error::CommandError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::AppHandle;
use tiny_http::{Header, Method, Response, StatusCode};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InvokeRequest {
    pub(super) command: String,
    #[serde(default = "empty_object")]
    pub(super) args: Value,
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum InvokeResponse {
    Success { result: Value },
    Failure { error: CommandError },
}

pub(super) fn handle_request(
    request: &mut tiny_http::Request,
    app: &AppHandle,
    development_origin: &str,
) -> Response<std::io::Cursor<Vec<u8>>> {
    if request.method() == &Method::Options && request.url() == "/invoke" {
        if !has_development_origin(request, development_origin) {
            return error_response(
                403,
                "web-origin-rejected",
                "开发后端只接受本机 Vite 页面请求",
            );
        }
        return cors_response(
            Response::from_data(Vec::new()).with_status_code(StatusCode(DEV_API_HEALTH_STATUS)),
            development_origin,
        );
    }
    if is_health_request(request.method(), request.url()) {
        if !has_development_origin(request, development_origin) {
            return error_response(
                403,
                "web-origin-rejected",
                "开发后端只接受本机 Vite 页面请求",
            );
        }
        return cors_response(
            Response::from_data(Vec::new()).with_status_code(StatusCode(DEV_API_HEALTH_STATUS)),
            development_origin,
        );
    }
    if request.method() != &Method::Post || request.url() != "/invoke" {
        return error_response(404, "web-command-not-found", "开发后端不存在该接口");
    }
    if !has_development_origin(request, development_origin) {
        return error_response(
            403,
            "web-origin-rejected",
            "开发后端只接受本机 Vite 页面请求",
        );
    }
    if !request.headers().iter().any(|header| {
        header.field.equiv("Content-Type") && header.value.as_str().starts_with("application/json")
    }) {
        return cors_response(
            error_response(415, "web-content-type-invalid", "开发后端请求必须使用 JSON"),
            development_origin,
        );
    }

    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        return cors_response(
            error_response(400, "web-request-unreadable", "无法读取开发后端请求"),
            development_origin,
        );
    }
    let request = match serde_json::from_str::<InvokeRequest>(&body) {
        Ok(request) => request,
        Err(_) => return cors_response(
            error_response(400, "web-request-invalid", "开发后端请求格式无效"),
            development_origin,
        ),
    };
    match dispatch(app, request) {
        Ok(result) => cors_response(
            json_response(200, &InvokeResponse::Success { result }),
            development_origin,
        ),
        Err(error) => cors_response(
            json_response(200, &InvokeResponse::Failure { error }),
            development_origin,
        ),
    }
}

fn cors_response(
    response: Response<std::io::Cursor<Vec<u8>>>,
    development_origin: &str,
) -> Response<std::io::Cursor<Vec<u8>>> {
    response
        .with_header(
            Header::from_bytes("Access-Control-Allow-Origin", development_origin)
                .expect("development origin is a valid response header"),
        )
        .with_header(
            Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
                .expect("static CORS methods header is valid"),
        )
        .with_header(
            Header::from_bytes("Access-Control-Allow-Headers", "Content-Type")
                .expect("static CORS headers header is valid"),
        )
}

fn has_development_origin(request: &tiny_http::Request, development_origin: &str) -> bool {
    request.headers().iter().any(|header| {
        header.field.equiv("Origin") && origin_is_allowed(header.value.as_str(), development_origin)
    })
}

pub(super) fn origin_is_allowed(request_origin: &str, development_origin: &str) -> bool {
    request_origin == development_origin
}

pub(super) fn is_health_request(method: &Method, url: &str) -> bool {
    method == &Method::Get && url == "/health"
}

fn error_response(
    status: u16,
    code: &'static str,
    message: &'static str,
) -> Response<std::io::Cursor<Vec<u8>>> {
    json_response(
        status,
        &InvokeResponse::Failure {
            error: CommandError::new(code, message),
        },
    )
}

fn json_response<T: Serialize>(status: u16, value: &T) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_string(value).unwrap_or_else(|_| {
        "{\"kind\":\"failure\",\"error\":{\"code\":\"web-response-invalid\",\"message\":\"开发后端响应无法序列化\"}}".to_string()
    });
    Response::from_string(body)
        .with_status_code(StatusCode(status))
        .with_header(
            Header::from_bytes("Content-Type", "application/json; charset=utf-8")
                .expect("static development response header is valid"),
        )
}

pub(super) fn argument<T: DeserializeOwned>(args: &Value, name: &str) -> Result<T, CommandError> {
    let value = args
        .get(name)
        .cloned()
        .ok_or_else(|| CommandError::new("web-argument-missing", format!("缺少参数：{name}")))?;
    serde_json::from_value(value)
        .map_err(|_| CommandError::new("web-argument-invalid", format!("参数无效：{name}")))
}

pub(super) fn optional_argument<T: DeserializeOwned>(
    args: &Value,
    name: &str,
) -> Result<Option<T>, CommandError> {
    args.get(name)
        .cloned()
        .map(|value| {
            serde_json::from_value(value)
                .map_err(|_| CommandError::new("web-argument-invalid", format!("参数无效：{name}")))
        })
        .transpose()
}

pub(super) fn as_json<T: Serialize>(
    result: Result<T, CommandError>,
) -> Result<Value, CommandError> {
    result.and_then(|value| {
        serde_json::to_value(value)
            .map_err(|_| CommandError::new("web-response-invalid", "开发后端响应无法序列化"))
    })
}
