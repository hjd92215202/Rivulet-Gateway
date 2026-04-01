use std::net::SocketAddr;

use gateway_filters::FilterRegistry;
use gateway_router::Router;
use gateway_types::{GatewayError, HttpMethod, RequestContext, ResponseContext, Result};
use gateway_upstream::UpstreamRegistry;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct ProxyService {
    router: Router,
    filters: FilterRegistry,
    upstreams: UpstreamRegistry,
}

impl ProxyService {
    pub fn new(router: Router, filters: FilterRegistry, upstreams: UpstreamRegistry) -> Self {
        Self {
            router,
            filters,
            upstreams,
        }
    }

    pub async fn handle(&self, mut request: RequestContext) -> Result<ResponseContext> {
        let route = self.router.resolve(&request)?;
        self.filters
            .run_before(&route.filter_names, &mut request)
            .await?;

        let selected = self
            .upstreams
            .cluster(&route.upstream_name)?
            .next_endpoint()?;

        let mut response = ResponseContext::new(200);
        response.upstream = Some(selected.address);

        self.filters
            .run_after(&route.filter_names, &mut response)
            .await?;

        Ok(response)
    }

    pub async fn handle_connection(
        &self,
        listener_name: &str,
        downstream: &mut TcpStream,
        client_addr: SocketAddr,
    ) -> Result<()> {
        let request = HttpRequest::read_from(downstream).await?;
        let mut request_context = RequestContext::new(
            listener_name,
            request.host_header(),
            request.path_for_route(),
            request.method,
        );
        request_context.client_addr = Some(client_addr);

        let route = self.router.resolve(&request_context)?;
        self.filters
            .run_before(&route.filter_names, &mut request_context)
            .await?;

        let endpoint = self
            .upstreams
            .cluster(&route.upstream_name)?
            .next_endpoint()?;

        let mut upstream = TcpStream::connect(&endpoint.address).await.map_err(|err| {
            GatewayError::Io(format!("connect upstream {}: {}", endpoint.address, err))
        })?;

        request
            .write_to_upstream(&mut upstream, &request_context)
            .await?;

        let mut upstream_bytes = Vec::new();
        upstream
            .read_to_end(&mut upstream_bytes)
            .await
            .map_err(|err| GatewayError::Io(format!("read upstream response: {}", err)))?;

        if upstream_bytes.is_empty() {
            return Err(GatewayError::Protocol(
                "upstream returned an empty response".into(),
            ));
        }

        downstream
            .write_all(&upstream_bytes)
            .await
            .map_err(|err| GatewayError::Io(format!("write downstream response: {}", err)))?;
        downstream
            .flush()
            .await
            .map_err(|err| GatewayError::Io(format!("flush downstream response: {}", err)))?;

        let status_code = parse_status_code(&upstream_bytes).unwrap_or(200);
        let mut response = ResponseContext::new(status_code);
        response.upstream = Some(endpoint.address);

        self.filters
            .run_after(&route.filter_names, &mut response)
            .await?;

        Ok(())
    }
}

#[derive(Clone, Debug)]
struct HttpRequest {
    method: HttpMethod,
    target: String,
    version: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl HttpRequest {
    async fn read_from(stream: &mut TcpStream) -> Result<Self> {
        const MAX_HEADER_BYTES: usize = 64 * 1024;
        let mut buffer = Vec::with_capacity(2048);
        let mut temp = [0_u8; 2048];

        let header_end = loop {
            if let Some(position) = find_header_end(&buffer) {
                break position;
            }
            if buffer.len() >= MAX_HEADER_BYTES {
                return Err(GatewayError::Protocol(
                    "request headers exceed maximum size".into(),
                ));
            }
            let read = stream
                .read(&mut temp)
                .await
                .map_err(|err| GatewayError::Io(format!("read downstream request: {}", err)))?;
            if read == 0 {
                return Err(GatewayError::Protocol(
                    "connection closed before request completed".into(),
                ));
            }
            buffer.extend_from_slice(&temp[..read]);
        };

        let head = String::from_utf8(buffer[..header_end].to_vec())
            .map_err(|_| GatewayError::Protocol("request head is not valid utf-8".into()))?;
        let mut lines = head.split("\r\n");
        let request_line = lines
            .next()
            .ok_or_else(|| GatewayError::Protocol("missing request line".into()))?;
        let mut request_parts = request_line.split_whitespace();
        let method = request_parts
            .next()
            .ok_or_else(|| GatewayError::Protocol("missing request method".into()))
            .and_then(HttpMethod::try_from)?;
        let target = request_parts
            .next()
            .ok_or_else(|| GatewayError::Protocol("missing request target".into()))?
            .to_string();
        let version = request_parts
            .next()
            .ok_or_else(|| GatewayError::Protocol("missing request version".into()))?
            .to_string();

        if version != "HTTP/1.1" {
            return Err(GatewayError::Unsupported(format!(
                "http version {}",
                version
            )));
        }

        let mut headers = Vec::new();
        for line in lines {
            if line.is_empty() {
                continue;
            }
            let (name, value) = line
                .split_once(':')
                .ok_or_else(|| GatewayError::Protocol(format!("invalid header line {}", line)))?;
            headers.push((name.trim().to_string(), value.trim().to_string()));
        }

        let content_length = parse_content_length(&headers)?;
        let mut body = buffer[(header_end + 4)..].to_vec();

        if header_has_transfer_encoding(&headers) {
            return Err(GatewayError::Unsupported(
                "transfer-encoding is not supported in the first kernel cut".into(),
            ));
        }

        if body.len() < content_length {
            let remaining = content_length - body.len();
            let mut extra = vec![0_u8; remaining];
            stream
                .read_exact(&mut extra)
                .await
                .map_err(|err| GatewayError::Io(format!("read request body: {}", err)))?;
            body.extend_from_slice(&extra);
        } else if body.len() > content_length {
            body.truncate(content_length);
        }

        Ok(Self {
            method,
            target,
            version,
            headers,
            body,
        })
    }

    fn host_header(&self) -> String {
        self.headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("host"))
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    }

    fn path_for_route(&self) -> String {
        self.target
            .split('?')
            .next()
            .unwrap_or(self.target.as_str())
            .to_string()
    }

    async fn write_to_upstream(
        &self,
        upstream: &mut TcpStream,
        request_context: &RequestContext,
    ) -> Result<()> {
        let mut request_bytes = Vec::with_capacity(1024 + self.body.len());
        request_bytes.extend_from_slice(
            format!(
                "{} {} {}\r\n",
                request_context.method, self.target, self.version
            )
            .as_bytes(),
        );

        let mut has_host = false;
        let mut has_x_forwarded_for = false;

        for (name, value) in &self.headers {
            if is_hop_by_hop_header(name) {
                continue;
            }
            if name.eq_ignore_ascii_case("host") {
                has_host = true;
            }
            if name.eq_ignore_ascii_case("x-forwarded-for") {
                has_x_forwarded_for = true;
            }
            request_bytes.extend_from_slice(format!("{}: {}\r\n", name, value).as_bytes());
        }

        if !has_host {
            request_bytes
                .extend_from_slice(format!("Host: {}\r\n", request_context.host).as_bytes());
        }
        if !has_x_forwarded_for {
            if let Some(client_addr) = request_context.client_addr {
                request_bytes.extend_from_slice(
                    format!("X-Forwarded-For: {}\r\n", client_addr.ip()).as_bytes(),
                );
            }
        }

        request_bytes.extend_from_slice(b"Connection: close\r\n");
        request_bytes
            .extend_from_slice(format!("Content-Length: {}\r\n\r\n", self.body.len()).as_bytes());
        request_bytes.extend_from_slice(&self.body);

        upstream
            .write_all(&request_bytes)
            .await
            .map_err(|err| GatewayError::Io(format!("write upstream request: {}", err)))?;
        upstream
            .flush()
            .await
            .map_err(|err| GatewayError::Io(format!("flush upstream request: {}", err)))?;
        Ok(())
    }
}

fn parse_content_length(headers: &[(String, String)]) -> Result<usize> {
    let mut content_length = None;
    for (name, value) in headers {
        if name.eq_ignore_ascii_case("content-length") {
            let parsed = value.parse::<usize>().map_err(|_| {
                GatewayError::Protocol(format!("invalid content-length header value {}", value))
            })?;
            content_length = Some(parsed);
        }
    }
    Ok(content_length.unwrap_or(0))
}

fn header_has_transfer_encoding(headers: &[(String, String)]) -> bool {
    headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("transfer-encoding"))
}

fn is_hop_by_hop_header(name: &str) -> bool {
    name.eq_ignore_ascii_case("connection")
        || name.eq_ignore_ascii_case("keep-alive")
        || name.eq_ignore_ascii_case("proxy-connection")
        || name.eq_ignore_ascii_case("transfer-encoding")
        || name.eq_ignore_ascii_case("upgrade")
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_status_code(bytes: &[u8]) -> Option<u16> {
    let head = bytes.split(|byte| *byte == b'\n').next()?;
    let line = String::from_utf8_lossy(head);
    let mut parts = line.split_whitespace();
    let _version = parts.next()?;
    let code = parts.next()?;
    code.parse::<u16>().ok()
}

pub fn error_response(error: &GatewayError) -> Vec<u8> {
    let (status, reason) = match error {
        GatewayError::RouteNotMatched => (404_u16, "Not Found"),
        GatewayError::Unsupported(_) => (501_u16, "Not Implemented"),
        GatewayError::Protocol(_)
        | GatewayError::InvalidConfig(_)
        | GatewayError::FilterRejected(_) => (400_u16, "Bad Request"),
        GatewayError::NoHealthyUpstream(_) | GatewayError::NotFound(_) => {
            (503_u16, "Service Unavailable")
        }
        GatewayError::Io(_) => (502_u16, "Bad Gateway"),
    };

    let body = format!("{} {}\n", status, reason);
    format!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\nContent-Type: text/plain\r\n\r\n{}",
        status,
        reason,
        body.len(),
        body
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gateway_config::{
        EndpointConfig, GatewayConfigFile, ListenerConfig, LoadBalanceConfig, ProtocolConfig,
        RouteConfig, RuntimeConfig, UpstreamConfig,
    };
    use tokio::net::{TcpListener, TcpStream};

    #[tokio::test]
    async fn proxies_http_request_to_upstream() {
        let backend = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind backend");
        let backend_addr = backend.local_addr().expect("backend addr");

        tokio::spawn(async move {
            let (mut stream, _) = backend.accept().await.expect("accept backend");
            let request = HttpRequest::read_from(&mut stream)
                .await
                .expect("read backend request");
            assert_eq!(request.path_for_route(), "/hello");
            assert_eq!(request.host_header(), "example.test");

            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello",
                )
                .await
                .expect("write backend response");
        });

        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "127.0.0.1:0".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![RouteConfig {
                name: "default".into(),
                listener: "edge".into(),
                hosts: vec!["example.test".into()],
                path_prefixes: vec!["/".into()],
                methods: vec![],
                upstream: "api".into(),
                filters: vec!["request-id".into()],
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                endpoints: vec![EndpointConfig {
                    address: backend_addr.to_string(),
                    weight: 1,
                }],
            }],
        };

        let service = ProxyService::new(
            Router::from_config(&config),
            FilterRegistry::with_defaults(),
            UpstreamRegistry::from_config(&config),
        );

        let gateway = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway");
        let gateway_addr = gateway.local_addr().expect("gateway addr");

        let server = tokio::spawn(async move {
            let (mut downstream, client_addr) = gateway.accept().await.expect("accept gateway");
            service
                .handle_connection("edge", &mut downstream, client_addr)
                .await
                .expect("proxy request");
        });

        let mut client = TcpStream::connect(gateway_addr)
            .await
            .expect("connect gateway");
        client
            .write_all(b"GET /hello HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write request");

        let mut response = Vec::new();
        client
            .read_to_end(&mut response)
            .await
            .expect("read response");

        server.await.expect("gateway task");

        let text = String::from_utf8(response).expect("utf-8 response");
        assert!(text.starts_with("HTTP/1.1 200 OK"));
        assert!(text.ends_with("hello"));
    }
}
