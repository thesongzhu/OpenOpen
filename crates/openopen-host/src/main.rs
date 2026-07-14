use openopen_protocol::{RpcError, RpcRequest, RpcResponse};
use serde_json::json;
use std::io::{self, BufRead, Write};

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if let Some(response) = handle_line(&line) {
            serde_json::to_writer(&mut stdout, &response)?;
            writeln!(stdout)?;
            stdout.flush()?;
        }
    }
    Ok(())
}

fn handle_line(line: &str) -> Option<RpcResponse> {
    let value = match serde_json::from_str::<serde_json::Value>(line) {
        Ok(value) => value,
        Err(error) => {
            return Some(RpcResponse::failure(
                None,
                RpcError {
                    code: -32_700,
                    message: error.to_string(),
                    data: None,
                },
            ));
        }
    };
    match serde_json::from_value::<RpcRequest>(value.clone()) {
        Ok(request) if params_are_structured(&request.params) => Some(dispatch(&request)),
        Ok(request) => Some(RpcResponse::failure(
            Some(request.id),
            RpcError {
                code: -32_602,
                message: "Params must be an object, array, or null".into(),
                data: None,
            },
        )),
        Err(_) if is_valid_notification(&value) => None,
        Err(_) => Some(RpcResponse::failure(
            None,
            RpcError {
                code: -32_600,
                message: "Invalid JSON-RPC request".into(),
                data: None,
            },
        )),
    }
}

fn is_valid_notification(value: &serde_json::Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object.get("jsonrpc").and_then(serde_json::Value::as_str) == Some("2.0")
        && object
            .get("method")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|method| !method.is_empty())
        && !object.contains_key("id")
        && object.get("params").is_none_or(params_are_structured)
}

fn params_are_structured(params: &serde_json::Value) -> bool {
    params.is_object() || params.is_array() || params.is_null()
}

fn dispatch(request: &RpcRequest) -> RpcResponse {
    if request.jsonrpc != "2.0" {
        return RpcResponse::failure(
            Some(request.id),
            RpcError {
                code: -32_600,
                message: "jsonrpc must be exactly 2.0".into(),
                data: None,
            },
        );
    }
    match request.method.as_str() {
        "account.read" => RpcResponse::success(
            request.id,
            json!({
                "state": "notConnected",
                "reason": "Codex App Server is not connected yet"
            }),
        ),
        method if is_public_family(method) => RpcResponse::failure(
            Some(request.id),
            RpcError {
                code: -32_001,
                message: format!("{method} is not connected to its real implementation"),
                data: Some(json!({ "state": "notReady" })),
            },
        ),
        _ => RpcResponse::failure(
            Some(request.id),
            RpcError {
                code: -32_601,
                message: "Unknown OpenOpen RPC method".into(),
                data: None,
            },
        ),
    }
}

fn is_public_family(method: &str) -> bool {
    [
        "account.",
        "outcome.",
        "mission.",
        "channel.",
        "receipt.",
        "workflow.",
        "skill.",
    ]
    .iter()
    .any(|prefix| method.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::{dispatch, handle_line};
    use openopen_protocol::RpcRequest;
    use serde_json::json;

    #[test]
    fn unfinished_public_route_is_explicitly_not_ready() {
        let response = dispatch(&RpcRequest {
            jsonrpc: "2.0".into(),
            id: 1,
            method: "channel.send".into(),
            params: json!({}),
        });
        assert_eq!(response.error.unwrap().code, -32_001);
    }

    #[test]
    fn connected_account_is_never_fabricated() {
        let response = dispatch(&RpcRequest {
            jsonrpc: "2.0".into(),
            id: 1,
            method: "account.read".into(),
            params: json!({}),
        });
        let result = response.result.unwrap();
        assert_eq!(result["state"], "notConnected");
    }

    #[test]
    fn parse_error_uses_json_rpc_null_id() {
        let response = handle_line("not-json").unwrap();
        assert_eq!(response.id, None);
        assert_eq!(response.error.unwrap().code, -32_700);
    }

    #[test]
    fn valid_json_with_invalid_request_shape_uses_invalid_request() {
        let response = handle_line("{}").unwrap();
        assert_eq!(response.id, None);
        assert_eq!(response.error.unwrap().code, -32_600);
    }

    #[test]
    fn valid_notification_produces_no_response() {
        assert!(handle_line(r#"{"jsonrpc":"2.0","method":"account.read"}"#).is_none());
    }

    #[test]
    fn primitive_params_are_rejected() {
        let response =
            handle_line(r#"{"jsonrpc":"2.0","id":7,"method":"account.read","params":"invalid"}"#)
                .unwrap();
        assert_eq!(response.id, Some(7));
        assert_eq!(response.error.unwrap().code, -32_602);
    }
}
