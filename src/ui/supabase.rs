use std::sync::{Mutex, OnceLock};
use url::Url;
use waki::bindings::wasi::http::{outgoing_handler, types as http_types};
use waki::bindings::wasi::io::streams::StreamError;

const SUPABASE_URL: Option<&str> = option_env!("SUPABASE_URL");
const SUPABASE_PUBLISHABLE_KEY: Option<&str> = option_env!("SUPABASE_PUBLISHABLE_KEY");
const SUPABASE_SOURCE_AB_PLUGIN_V2: &str = "ab_plugin_v2";

static SUPABASE_ACCESS_TOKEN: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn supabase_access_token() -> &'static Mutex<Option<String>> {
    SUPABASE_ACCESS_TOKEN.get_or_init(|| Mutex::new(None))
}

pub fn report_device_to_supabase(device_addr: &str, device_name: &str) -> Result<(), String> {
    let Some(supabase_url) = SUPABASE_URL else {
        return Err("SUPABASE_URL 未配置".to_string());
    };
    let Some(supabase_key) = SUPABASE_PUBLISHABLE_KEY else {
        return Err("SUPABASE_PUBLISHABLE_KEY 未配置".to_string());
    };

    let addr = device_addr.trim();
    if addr.is_empty() {
        return Err("设备地址为空".to_string());
    }

    let name = normalize_device_name(device_name)
        .ok_or_else(|| "设备名称清洗后为空，已取消上报".to_string())?;

    let access_token = match get_supabase_access_token(supabase_url, supabase_key) {
        Ok(token) => token,
        Err(e) => {
            return Err(format!("匿名登录失败: {}", e));
        }
    };

    let endpoint = format!(
        "{}/rest/v1/user_devices?on_conflict=device_id",
        supabase_url.trim_end_matches('/')
    );

    let payload = serde_json::json!([{
        "device_id": addr,
        "device_name": name,
        "source": SUPABASE_SOURCE_AB_PLUGIN_V2
    }]);

    let body = payload.to_string();
    match http_post_json(&endpoint, &body, supabase_key, &access_token) {
        Ok((status, resp)) => {
            if status == 200 || status == 201 || status == 204 {
                tracing::info!(
                    "supabase report ok: status={}, source={}",
                    status,
                    SUPABASE_SOURCE_AB_PLUGIN_V2
                );
                Ok(())
            } else {
                if status == 401 || status == 403 {
                    let mut slot = supabase_access_token()
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    *slot = None;
                }
                let _ = resp;
                Err(format!("status={}", status))
            }
        }
        Err(e) => Err(e),
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

fn get_supabase_access_token(supabase_url: &str, apikey: &str) -> Result<String, String> {
    {
        let slot = supabase_access_token()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(token) = slot.as_ref() {
            return Ok(token.clone());
        }
    }

    let auth_base = supabase_url.trim_end_matches('/');
    let token_endpoint = format!("{}/auth/v1/token?grant_type=anonymous", auth_base);
    let signup_endpoint = format!("{}/auth/v1/signup", auth_base);

    let mut last_error = String::new();
    for (endpoint, body) in [
        (token_endpoint.as_str(), "{}"),
        (signup_endpoint.as_str(), r#"{"data":{}}"#),
    ] {
        match http_post_json(endpoint, body, apikey, apikey) {
            Ok((status, resp)) => {
                if status < 200 || status >= 300 {
                    let _ = resp;
                    last_error = format!("auth status={}", status);
                    continue;
                }
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&resp) {
                    let token = json
                        .get("access_token")
                        .and_then(|v| v.as_str())
                        .or_else(|| {
                            json.get("session")
                                .and_then(|v| v.get("access_token"))
                                .and_then(|v| v.as_str())
                        });
                    if let Some(token) = token {
                        let mut slot = supabase_access_token()
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        *slot = Some(token.to_string());
                        return Ok(token.to_string());
                    }
                    last_error = "auth response missing access_token".to_string();
                } else {
                    last_error = "auth response is not valid json".to_string();
                }
            }
            Err(e) => {
                last_error = e;
            }
        }
    }

    Err(last_error)
}

fn http_post_json(
    url: &str,
    body: &str,
    apikey: &str,
    bearer_token: &str,
) -> Result<(u16, String), String> {
    let auth_header = format!("Bearer {}", bearer_token);
    let headers = vec![
        ("content-type".to_string(), "application/json".to_string()),
        ("apikey".to_string(), apikey.to_string()),
        ("authorization".to_string(), auth_header),
        (
            "prefer".to_string(),
            "resolution=merge-duplicates,return=minimal".to_string(),
        ),
    ];
    let (status, bytes) = http_request_bytes("POST", url, &headers, Some(body.as_bytes()))?;
    let text = String::from_utf8_lossy(&bytes).to_string();
    Ok((status, text))
}

fn http_request_bytes(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Option<&[u8]>,
) -> Result<(u16, Vec<u8>), String> {
    tracing::info!("supabase http_request_bytes method={}", method);
    let url = Url::parse(url).map_err(|e| e.to_string())?;
    let header_entries: Vec<(String, Vec<u8>)> = headers
        .iter()
        .map(|(k, v)| (k.clone(), v.as_bytes().to_vec()))
        .collect();
    let headers = http_types::Headers::from_list(&header_entries).map_err(|e| format!("{:?}", e))?;
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

    let future_response = outgoing_handler::handle(req, Some(options)).map_err(|e| format!("{:?}", e))?;
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
    let incoming_body = incoming_response
        .consume()
        .map_err(|_| "missing body".to_string())?;
    let input_stream = incoming_body
        .stream()
        .map_err(|_| "failed to open body stream".to_string())?;

    let mut bytes = Vec::new();
    loop {
        match input_stream.blocking_read(1024 * 64) {
            Ok(chunk) => {
                if chunk.is_empty() {
                    break;
                }
                bytes.extend_from_slice(&chunk);
            }
            Err(StreamError::Closed) => break,
            Err(e) => return Err(format!("read body failed: {:?}", e)),
        }
    }
    Ok((status, bytes))
}
