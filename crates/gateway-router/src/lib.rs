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
        })
    }
}

fn route_matches(route: &RouteConfig, request: &RequestContext) -> bool {
    listener_matches(route, request)
        && host_matches(route, request)
        && path_matches(route, request)
        && method_matches(route, request.method)
}

fn listener_matches(route: &RouteConfig, request: &RequestContext) -> bool {
    route.listener == request.listener
}

fn host_matches(route: &RouteConfig, request: &RequestContext) -> bool {
    route.hosts.is_empty() || route.hosts.iter().any(|item| item == &request.host)
}

fn path_matches(route: &RouteConfig, request: &RequestContext) -> bool {
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
                },
                RouteConfig {
                    name: "api".into(),
                    listener: "edge".into(),
                    hosts: vec!["api.example.com".into()],
                    path_prefixes: vec!["/v1/".into()],
                    methods: vec![],
                    upstream: "api-cluster".into(),
                    filters: vec!["request-id".into()],
                },
            ],
            upstreams: vec![UpstreamConfig {
                name: "api-cluster".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
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
