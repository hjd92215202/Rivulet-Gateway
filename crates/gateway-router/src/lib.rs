//! 路由层只负责“这次请求该走哪条规则”，不负责真正的转发。
//! 这样后续即便代理策略变复杂，路由匹配逻辑也能保持稳定。

use gateway_config::{GatewayConfigFile, RouteConfig};
use gateway_types::{GatewayError, HttpMethod, RequestContext, Result, RouteMatch};

#[derive(Clone, Debug)]
pub struct Router {
    routes: Vec<RouteConfig>,
}

impl Router {
    pub fn from_config(config: &GatewayConfigFile) -> Self {
        Self {
            routes: config.routes.clone(),
        }
    }

    /// 第一阶段采用“按配置顺序取首个匹配”的简单策略。
    /// 这让行为足够直观，也给后面引入显式优先级留了空间。
    pub fn resolve(&self, request: &RequestContext) -> Result<RouteMatch> {
        let route = self
            .routes
            .iter()
            .find(|route| route_matches(route, request))
            .ok_or(GatewayError::RouteNotMatched)?;

        Ok(RouteMatch {
            route_name: route.name.clone(),
            upstream_name: route.upstream.clone(),
            filter_names: route.filters.clone(),
            // 璺敱鍛戒腑鏃跺氨鎶婅矾鐢卞眰鐨勭瓥鐣ユ惡甯︿笅鍘伙紝
            // 杩欐牱浠ｇ悊涓婚摼璺笉鐢ㄥ啀鍥炲埌閰嶇疆鏁存爲閲嶆柊鏌ユ壘銆?
            proxy_policy: route.policy.to_overrides(),
            auth_policy: route.auth.to_policy(),
        })
    }
}

fn route_matches(route: &RouteConfig, request: &RequestContext) -> bool {
    listener_matches(route, request)
        && host_matches(route, request)
        && path_matches(route, request)
        && method_matches(route, request.method)
}

/// listener 维度的隔离很关键，它决定了同一路径能否在不同入口上复用不同策略。
fn listener_matches(route: &RouteConfig, request: &RequestContext) -> bool {
    route.listener == request.listener
}

fn host_matches(route: &RouteConfig, request: &RequestContext) -> bool {
    route.hosts.is_empty() || route.hosts.iter().any(|item| item == &request.host)
}

fn path_matches(route: &RouteConfig, request: &RequestContext) -> bool {
    // 目前先使用前缀匹配，足够支撑最常见的网关路径路由。
    route.path_prefixes.is_empty()
        || route
            .path_prefixes
            .iter()
            .any(|prefix| request.path.starts_with(prefix))
}

fn method_matches(route: &RouteConfig, method: HttpMethod) -> bool {
    route.methods.is_empty()
        || route
            .methods
            .iter()
            .any(|item| HttpMethod::from(*item) == method)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gateway_config::{
        GatewayConfigFile, ListenerConfig, LoadBalanceConfig, ProtocolConfig, RouteConfig,
        UpstreamConfig,
    };

    fn router_config() -> GatewayConfigFile {
        GatewayConfigFile {
            runtime: Default::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "0.0.0.0:8080".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![
                RouteConfig {
                    name: "write-api".into(),
                    listener: "edge".into(),
                    hosts: vec!["api.example.com".into()],
                    path_prefixes: vec!["/v1/items".into()],
                    methods: vec![gateway_config::HttpMethodConfig::POST],
                    upstream: "api-cluster".into(),
                    filters: vec![],
                    policy: Default::default(),
                    auth: Default::default(),
                },
                RouteConfig {
                    name: "api".into(),
                    listener: "edge".into(),
                    hosts: vec!["api.example.com".into()],
                    path_prefixes: vec!["/v1/".into()],
                    methods: vec![],
                    upstream: "api-cluster".into(),
                    filters: vec!["request-id".into()],
                    policy: gateway_config::ProxyPolicyConfig {
                        connect_timeout_ms: None,
                        read_timeout_ms: Some(1500),
                        retry_attempts: Some(3),
                    },
                    auth: Default::default(),
                },
            ],
            upstreams: vec![UpstreamConfig {
                name: "api-cluster".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
                endpoints: vec![gateway_config::EndpointConfig {
                    address: "127.0.0.1:9000".into(),
                    weight: 1,
                }],
            }],
        }
    }

    #[test]
    fn resolves_matching_route() {
        let config = router_config();

        let router = Router::from_config(&config);
        let request = RequestContext::new("edge", "api.example.com", "/v1/users", HttpMethod::Get);

        let matched = router.resolve(&request).expect("route should match");
        assert_eq!(matched.route_name, "api");
        assert_eq!(matched.upstream_name, "api-cluster");
        assert_eq!(matched.proxy_policy.upstream_retry_attempts, Some(3));
        assert!(!matched.auth_policy.is_enabled());
    }

    #[test]
    fn resolve_rejects_request_with_wrong_listener() {
        let config = router_config();
        let router = Router::from_config(&config);
        let request =
            RequestContext::new("internal", "api.example.com", "/v1/users", HttpMethod::Get);

        let error = router
            .resolve(&request)
            .expect_err("route should not match");
        assert!(matches!(error, GatewayError::RouteNotMatched));
    }

    #[test]
    fn resolve_rejects_request_with_wrong_host() {
        let config = router_config();
        let router = Router::from_config(&config);
        let request = RequestContext::new("edge", "www.example.com", "/v1/users", HttpMethod::Get);

        let error = router
            .resolve(&request)
            .expect_err("route should not match");
        assert!(matches!(error, GatewayError::RouteNotMatched));
    }

    #[test]
    fn resolve_honors_method_specific_route() {
        let config = router_config();
        let router = Router::from_config(&config);
        let request = RequestContext::new("edge", "api.example.com", "/v1/items", HttpMethod::Post);

        let matched = router.resolve(&request).expect("method route should match");
        assert_eq!(matched.route_name, "write-api");
    }
}
