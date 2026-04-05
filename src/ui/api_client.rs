use flate2::read::GzDecoder;
use std::io::Read;
use url::Url;
use waki::bindings::wasi::http::{outgoing_handler, types as http_types};
use waki::bindings::wasi::io::streams::StreamError;

use super::state;

pub fn auth_headers(include_content_type: bool) -> Result<Vec<(String, String)>, String> {
    let client_type = state::server_api_client_type()?;
    let api_key = state::server_api_key()?;

    let mut headers = vec![
        ("X-Client-Type".to_string(), client_type.to_string()),
        ("X-API-Key".to_string(), api_key.to_string()),
    ];

    if include_content_type {
        headers.push(("Content-Type".to_string(), "application/json".to_string()));
    }

    Ok(headers)
}

pub fn get_json(url: &str) -> Result<serde_json::Value, String> {
    let headers = auth_headers(false)?;
    let (status, body) = request_bytes("GET", url, &headers, None)?;
    tracing::info!("api_get_json status={}, body_len={}", status, body.len());
    log_body_preview("api_get_json", &body);
    parse_http_json_response(status, body)
}

pub fn post_json(url: &str, payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let body = serde_json::to_vec(payload).map_err(|e| format!("请求序列化失败: {}", e))?;
    let headers = auth_headers(true)?;
    let (status, response_body) = request_bytes("POST", url, &headers, Some(&body))?;
    tracing::info!(
        "api_post_json status={}, body_len={}",
        status,
        response_body.len()
    );
    log_body_preview("api_post_json", &response_body);
    parse_http_json_response(status, response_body)
}

pub fn report_device(device_id: &str, device_name: &str) -> Result<(), String> {
    let base = state::server_api_base()?;
    let endpoint = format!("{}/api/device/report", base.trim_end_matches('/'));
    let device_name = normalize_device_name(device_name)
        .ok_or_else(|| "设备名称清洗后为空，已取消上报".to_string())?;
    let payload = serde_json::json!({
        "deviceId": device_id,
        "deviceName": device_name,
    });
    let response = post_json(&endpoint, &payload)?;
    if response.get("code").and_then(|v| v.as_str()) == Some("200") {
        tracing::info!("device report ok: device_id={}", device_id);
        Ok(())
    } else {
        Err(response
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("设备上报失败")
            .to_string())
    }
}

fn normalize_device_name(device_name: &str) -> Option<String> {
    let mut name = device_name.trim().to_string();

    if let Some((prefix, suffix)) = name.rsplit_once(' ') {
        if suffix.len() == 4 && suffix.chars().all(|c| c.is_ascii_alphanumeric()) {
            name = prefix.trim_end().to_string();
        }
    }

    if name.is_empty() { None } else { Some(name) }
}

fn parse_http_json_response(status: u16, body: Vec<u8>) -> Result<serde_json::Value, String> {
    let body = maybe_decompress(body)?;
    if body.is_empty() {
        return Err("Empty response".to_string());
    }

    let json: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| format!("响应解析失败: {}", e))?;

    if status == 200 {
        return Ok(json);
    }

    if let Some(message) = json.get("message").and_then(|v| v.as_str()) {
        return Err(message.to_string());
    }
    if let Some(code) = json.get("code").and_then(|v| v.as_str()) {
        return Err(format!("接口错误: {}", code));
    }

    Err(format!("HTTP {}", status))
}

fn request_bytes(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Option<&[u8]>,
) -> Result<(u16, Vec<u8>), String> {
    tracing::info!("api_request_bytes method={}", method);
    let url = Url::parse(url).map_err(|e| e.to_string())?;
    let header_entries: Vec<(String, Vec<u8>)> = headers
        .iter()
        .map(|(k, v)| (k.clone(), v.as_bytes().to_vec()))
        .collect();
    let headers =
        http_types::Headers::from_list(&header_entries).map_err(|e| format!("{:?}", e))?;
    let req = http_types::OutgoingRequest::new(headers);

    let http_method = match method {
        "POST" => http_types::Method::Post,
        "GET" => http_types::Method::Get,
        _ => return Err(format!("unsupported method: {}", method)),
    };

    req.set_method(&http_method)
        .map_err(|()| "failed to set method".to_string())?;

    let scheme = match url.scheme() {
        "https" => http_types::Scheme::Https,
        _ => http_types::Scheme::Http,
    };
    req.set_scheme(Some(&scheme))
        .map_err(|()| "failed to set scheme".to_string())?;

    let authority = url.authority();
    req.set_authority(Some(authority))
        .map_err(|()| "failed to set authority".to_string())?;

    let path = match url.query() {
        Some(q) => format!("{}?{}", url.path(), q),
        None => url.path().to_string(),
    };
    req.set_path_with_query(Some(&path))
        .map_err(|()| "failed to set path".to_string())?;

    let options = http_types::RequestOptions::new();
    let outgoing_body = req
        .body()
        .map_err(|_| "outgoing request write failed".to_string())?;
    let maybe_stream = if let Some(body) = body {
        let stream = outgoing_body
            .write()
            .map_err(|_| "open body writer failed".to_string())?;
        stream
            .blocking_write_and_flush(body)
            .map_err(|e| format!("write body failed: {:?}", e))?;
        drop(stream);
        None
    } else {
        None
    };
    http_types::OutgoingBody::finish(outgoing_body, maybe_stream)
        .map_err(|_| "finish body failed".to_string())?;

    let future_response =
        outgoing_handler::handle(req, Some(options)).map_err(|e| format!("{:?}", e))?;
    let incoming_response = match future_response.get() {
        Some(result) => result.map_err(|()| "response already taken".to_string())?,
        None => {
            let pollable = future_response.subscribe();
            pollable.block();
            future_response
                .get()
                .ok_or_else(|| "response not available".to_string())?
                .map_err(|()| "response already taken".to_string())?
        }
    }
    .map_err(|e| format!("{:?}", e))?;

    let status = incoming_response.status();
    tracing::info!("api_request_bytes status={}", status);
    let incoming_body = incoming_response
        .consume()
        .map_err(|_| "missing body".to_string())?;
    let input_stream = incoming_body
        .stream()
        .map_err(|_| "failed to open body stream".to_string())?;

    let mut body = Vec::new();
    loop {
        match input_stream.blocking_read(1024 * 64) {
            Ok(chunk) => {
                if chunk.is_empty() {
                    break;
                }
                body.extend_from_slice(&chunk);
            }
            Err(StreamError::Closed) => break,
            Err(e) => return Err(format!("read body failed: {:?}", e)),
        }
    }

    Ok((status, body))
}

fn log_body_preview(tag: &str, body: &[u8]) {
    if body.is_empty() {
        tracing::info!("{} body_preview: <empty>", tag);
        return;
    }
    let preview_len = body.len().min(400);
    let preview = String::from_utf8_lossy(&body[..preview_len]);
    tracing::info!("{} body_preview_utf8: {}", tag, preview);
}

fn maybe_decompress(body: Vec<u8>) -> Result<Vec<u8>, String> {
    if body.len() >= 2 && body[0] == 0x1f && body[1] == 0x8b {
        tracing::info!("detected gzip body, decompressing...");
        let mut decoder = GzDecoder::new(&body[..]);
        let mut out = Vec::new();
        decoder
            .read_to_end(&mut out)
            .map_err(|e| format!("gzip decompress failed: {}", e))?;
        return Ok(out);
    }

    Ok(body)
}
