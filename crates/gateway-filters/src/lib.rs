use std::future::Future;
use std::pin::Pin;

use gateway_types::{RequestContext, ResponseContext, Result};

pub type FilterFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

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
