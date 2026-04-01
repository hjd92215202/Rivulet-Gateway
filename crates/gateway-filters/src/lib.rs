//! 过滤器层是后续扩展能力的主入口。
//! 第一阶段先把接口定稳，具体能力只保留最小集合。

use std::future::Future;
use std::pin::Pin;

use gateway_types::{RequestContext, ResponseContext, Result};

pub type FilterFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

/// 过滤器分成请求前和响应后两个阶段，便于后面串接鉴权、限流和日志等能力。
pub trait HttpFilter: Send + Sync {
    fn name(&self) -> &'static str;

    fn before<'a>(&'a self, _request: &'a mut RequestContext) -> FilterFuture<'a> {
        Box::pin(async { Ok(()) })
    }

    fn after<'a>(&'a self, _response: &'a mut ResponseContext) -> FilterFuture<'a> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Default)]
pub struct RequestIdFilter;

impl HttpFilter for RequestIdFilter {
    fn name(&self) -> &'static str {
        "request-id"
    }

    fn before<'a>(&'a self, request: &'a mut RequestContext) -> FilterFuture<'a> {
        Box::pin(async move {
            // 第一阶段先用稳定可读的字符串拼一个 request id，
            // 方便测试和排障；后面再替换成真正的随机或雪花算法。
            if request.request_id.is_none() {
                request.request_id = Some(format!(
                    "{}:{}:{}",
                    request.listener, request.host, request.path
                ));
            }
            Ok(())
        })
    }
}

#[derive(Default)]
pub struct AccessLogFilter;

impl HttpFilter for AccessLogFilter {
    fn name(&self) -> &'static str {
        "access-log"
    }
}

pub struct FilterRegistry {
    filters: Vec<Box<dyn HttpFilter>>,
}

impl FilterRegistry {
    /// 默认注册表只放核心过滤器。
    /// 未知过滤器当前会被忽略，后面再根据控制面需求决定是否改成严格失败。
    pub fn with_defaults() -> Self {
        Self {
            filters: vec![Box::new(RequestIdFilter), Box::new(AccessLogFilter)],
        }
    }

    pub async fn run_before(
        &self,
        filter_names: &[String],
        request: &mut RequestContext,
    ) -> Result<()> {
        for filter_name in filter_names {
            if let Some(filter) = self.find(filter_name) {
                filter.before(request).await?;
            }
        }
        Ok(())
    }

    pub async fn run_after(
        &self,
        filter_names: &[String],
        response: &mut ResponseContext,
    ) -> Result<()> {
        for filter_name in filter_names {
            if let Some(filter) = self.find(filter_name) {
                filter.after(response).await?;
            }
        }
        Ok(())
    }

    fn find(&self, filter_name: &str) -> Option<&dyn HttpFilter> {
        self.filters
            .iter()
            .find(|filter| filter.name() == filter_name)
            .map(|filter| filter.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gateway_types::{HttpMethod, RequestContext, ResponseContext};

    #[tokio::test]
    async fn request_id_filter_populates_missing_request_id() {
        let registry = FilterRegistry::with_defaults();
        let mut request = RequestContext::new("edge", "example.test", "/v1/hello", HttpMethod::Get);

        registry
            .run_before(&["request-id".into()], &mut request)
            .await
            .expect("filter should run");

        assert_eq!(
            request.request_id.as_deref(),
            Some("edge:example.test:/v1/hello")
        );
    }

    #[tokio::test]
    async fn unknown_filter_is_ignored() {
        let registry = FilterRegistry::with_defaults();
        let mut request = RequestContext::new("edge", "example.test", "/", HttpMethod::Get);
        let mut response = ResponseContext::new(200);

        registry
            .run_before(&["missing-filter".into()], &mut request)
            .await
            .expect("missing filter should be ignored");
        registry
            .run_after(&["missing-filter".into()], &mut response)
            .await
            .expect("missing filter should be ignored");

        assert!(request.request_id.is_none());
        assert_eq!(response.status_code, 200);
    }
}
