//! 可观测性层目前先做两件事：
//! 1. 维护最基础的运行时指标
//! 2. 生成结构化 access log

use std::sync::atomic::{AtomicU64, Ordering};
use std::{env, sync::OnceLock};

use gateway_types::{RequestContext, ResponseContext};

#[derive(Debug, Default)]
pub struct RuntimeStats {
    total_requests: AtomicU64,
    completed_requests: AtomicU64,
    active_connections: AtomicU64,
    successful_responses: AtomicU64,
    client_error_responses: AtomicU64,
    server_error_responses: AtomicU64,
    upstream_retries: AtomicU64,
}

impl RuntimeStats {
    /// 接收到连接后立刻计数，便于观察瞬时连接压力。
    pub fn record_connection_opened(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// 连接关闭时把活动连接数减回去。
    pub fn record_connection_closed(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    /// 只要开始进入请求处理链，就认为有一个请求进入了网关。
    pub fn record_request_started(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// 响应完成后统一记录状态码分桶和完成数。
    pub fn record_request_completed(&self, status_code: u16) {
        self.completed_requests.fetch_add(1, Ordering::Relaxed);

        match status_code {
            200..=399 => {
                self.successful_responses.fetch_add(1, Ordering::Relaxed);
            }
            400..=499 => {
                self.client_error_responses.fetch_add(1, Ordering::Relaxed);
            }
            _ => {
                self.server_error_responses.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// 重试次数按“额外尝试”累计，便于观察上游抖动。
    pub fn record_retries(&self, retries: usize) {
        self.upstream_retries
            .fetch_add(retries as u64, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> RuntimeStatsSnapshot {
        RuntimeStatsSnapshot {
            total_requests: self.total_requests.load(Ordering::Relaxed),
            completed_requests: self.completed_requests.load(Ordering::Relaxed),
            active_connections: self.active_connections.load(Ordering::Relaxed),
            successful_responses: self.successful_responses.load(Ordering::Relaxed),
            client_error_responses: self.client_error_responses.load(Ordering::Relaxed),
            server_error_responses: self.server_error_responses.load(Ordering::Relaxed),
            upstream_retries: self.upstream_retries.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RuntimeStatsSnapshot {
    pub total_requests: u64,
    pub completed_requests: u64,
    pub active_connections: u64,
    pub successful_responses: u64,
    pub client_error_responses: u64,
    pub server_error_responses: u64,
    pub upstream_retries: u64,
}

/// access log 先收口成稳定结构，输出方式后续可以再替换成文件、stdout 或 tracing。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccessLogRecord {
    pub listener: String,
    pub request_id: Option<String>,
    pub method: Option<String>,
    pub host: Option<String>,
    pub path: Option<String>,
    pub share_id: Option<String>,
    pub share_scope: Option<String>,
    pub status_code: u16,
    pub upstream: Option<String>,
    pub duration_ms: u128,
    pub retries: usize,
    pub error: Option<String>,
}

impl AccessLogRecord {
    pub fn success(
        request: &RequestContext,
        response: &ResponseContext,
        duration_ms: u128,
        retries: usize,
    ) -> Self {
        Self {
            listener: request.listener.clone(),
            request_id: request.request_id.clone(),
            method: Some(request.method.to_string()),
            host: Some(request.host.clone()),
            path: Some(request.path.clone()),
            share_id: request.share_id.clone(),
            share_scope: request.share_scope.clone(),
            status_code: response.status_code,
            upstream: response.upstream.clone(),
            duration_ms,
            retries,
            error: None,
        }
    }

    pub fn failure(
        listener: impl Into<String>,
        request: Option<&RequestContext>,
        status_code: u16,
        duration_ms: u128,
        retries: usize,
        error: impl Into<String>,
    ) -> Self {
        Self {
            listener: listener.into(),
            request_id: request.and_then(|value| value.request_id.clone()),
            method: request.map(|value| value.method.to_string()),
            host: request.map(|value| value.host.clone()),
            path: request.map(|value| value.path.clone()),
            share_id: request.and_then(|value| value.share_id.clone()),
            share_scope: request.and_then(|value| value.share_scope.clone()),
            status_code,
            upstream: None,
            duration_ms,
            retries,
            error: Some(error.into()),
        }
    }

    pub fn to_json_line(&self) -> String {
        format!(
            concat!(
                "{{",
                "\"listener\":\"{}\",",
                "\"request_id\":{},",
                "\"method\":{},",
                "\"host\":{},",
                "\"path\":{},",
                "\"share_id\":{},",
                "\"share_scope\":{},",
                "\"status_code\":{},",
                "\"upstream\":{},",
                "\"duration_ms\":{},",
                "\"retries\":{},",
                "\"error\":{}",
                "}}"
            ),
            escape_json(&self.listener),
            json_string_or_null(self.request_id.as_deref()),
            json_string_or_null(self.method.as_deref()),
            json_string_or_null(self.host.as_deref()),
            json_string_or_null(self.path.as_deref()),
            json_string_or_null(self.share_id.as_deref()),
            json_string_or_null(self.share_scope.as_deref()),
            self.status_code,
            json_string_or_null(self.upstream.as_deref()),
            self.duration_ms,
            self.retries,
            json_string_or_null(self.error.as_deref()),
        )
    }
}

pub fn emit_access_log(record: &AccessLogRecord) {
    // 压测场景下如果每个请求都直打 stdout，
    // 日志系统本身会反过来成为主要瓶颈，所以这里提供一个显式关闭开关。
    if !access_log_enabled() {
        return;
    }
    println!("{}", record.to_json_line());
}

fn access_log_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();

    *ENABLED.get_or_init(|| {
        !matches!(
            env::var("GATEWAY_DISABLE_ACCESS_LOG"),
            Ok(value) if value == "1" || value.eq_ignore_ascii_case("true")
        )
    })
}

fn json_string_or_null(value: Option<&str>) -> String {
    match value {
        Some(value) => format!("\"{}\"", escape_json(value)),
        None => "null".into(),
    }
}

fn escape_json(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use gateway_types::{HttpMethod, RequestContext, ResponseContext};

    #[test]
    fn runtime_stats_snapshot_reflects_recorded_values() {
        let stats = RuntimeStats::default();

        stats.record_connection_opened();
        stats.record_request_started();
        stats.record_retries(2);
        stats.record_request_completed(502);
        stats.record_connection_closed();

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.total_requests, 1);
        assert_eq!(snapshot.completed_requests, 1);
        assert_eq!(snapshot.active_connections, 0);
        assert_eq!(snapshot.server_error_responses, 1);
        assert_eq!(snapshot.upstream_retries, 2);
    }

    #[test]
    fn access_log_record_serializes_to_json() {
        let mut request =
            RequestContext::new("edge", "example.test", "/v1/orders", HttpMethod::Post);
        request.request_id = Some("req-1".into());
        request.share_id = Some("share-01".into());
        request.share_scope = Some("preview".into());
        let mut response = ResponseContext::new(200);
        response.upstream = Some("127.0.0.1:9000".into());

        let line = AccessLogRecord::success(&request, &response, 12, 1).to_json_line();

        assert!(line.contains("\"listener\":\"edge\""));
        assert!(line.contains("\"request_id\":\"req-1\""));
        assert!(line.contains("\"share_id\":\"share-01\""));
        assert!(line.contains("\"share_scope\":\"preview\""));
        assert!(line.contains("\"status_code\":200"));
        assert!(line.contains("\"retries\":1"));
    }
}
