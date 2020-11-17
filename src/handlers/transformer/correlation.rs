use hyper::{Request, Body, Response};
use crate::handlers::{HandlerResponse, Handler};
use crate::handlers::HandlerResponse::Continue;
use uuid::Uuid;
use hyper::header::{HeaderValue, HeaderName};
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct CorrelationIdTransformer {
    header_name: String
}

impl Handler for CorrelationIdTransformer {
    fn handle_req(&self, req: &mut Request<Body>) -> HandlerResponse {
        let req_headers = req.headers_mut();
        if req_headers.get(self.header_name.clone()).is_some() {
            Continue
        } else {
            let correlation: String = Uuid::new_v4().to_hyphenated().to_string();
            let name = self.header_name.as_str();
            req_headers.insert(HeaderName::from_str(name).unwrap(), HeaderValue::from_str(correlation.as_str()).unwrap());
            Continue
        }
    }

    fn handle_res(&self, _res: &mut Response<Body>) -> HandlerResponse {
        Continue
    }
}

#[cfg(test)]
mod tests {
    use hyper::service::{make_service_fn, service_fn};
    use std::net::SocketAddr;
    use hyper::{Server, StatusCode, Body, Response, Client, Uri, Request};
    use std::convert::Infallible;
    use crate::conf::api::Api;
    use crate::gateway::start_local_gateway;
    use crate::tests::{wait_for_gateway, unwrap_body_as_str};
    use std::str::FromStr;
    use uuid::Uuid;
    use crate::handlers::transformer::correlation::CorrelationIdTransformer;
    use hyper::header::HeaderValue;
    use log::*;

    async fn echo_correlation_server(header: &'static str, backend_port: u16) {
        let addr = SocketAddr::from(([127, 0, 0, 1], backend_port));
        let make_svc = make_service_fn(|_conn| {
            async move {
                Ok::<_, Infallible>(
                    service_fn(move |req| {
                        let headers = req.headers();
                        let correlation_header = headers.get(header).unwrap();
                        let correlation_id = correlation_header.to_str().unwrap().to_string();
                        async move {
                            Ok::<_, Infallible>(Response::builder()
                                .status(StatusCode::OK)
                                .body(Body::from(correlation_id))
                                .unwrap()
                            )
                        }
                    }))
            }
        });
        let server = Server::bind(&addr).serve(make_svc);
        info!("Mock server listening on http://{}", addr);
        if let Err(e) = server.await {
            error!("Server error: {}", e);
        }
    }

    #[tokio::test]
    async fn correlation_id_is_added_if_missing() {
        let gw_port = 7000;
        let backend_port = 7001;
        let prefix = "/correlation";
        let header = "X-Correlation-Id";
        tokio::spawn(async move {
            echo_correlation_server(header, backend_port).await
        });
        tokio::spawn(async move {
            let mut api = Api::http("127.0.0.1", backend_port, prefix.to_string()).unwrap();
            api.add_handler(Box::new(CorrelationIdTransformer { header_name: header.to_string() }));
            start_local_gateway(gw_port, vec![api]).await
        });
        wait_for_gateway(gw_port).await;
        let client = Client::new();
        let resp = client.get(Uri::from_str(format!("http://127.0.0.1:{}{}", gw_port, prefix).as_str()).unwrap()).await.unwrap();
        assert_eq!(200, resp.status());
        let body = unwrap_body_as_str(resp).await;
        assert_ne!("", body);
        assert!(Uuid::parse_str(body.as_str()).is_ok());
    }

    #[tokio::test]
    async fn correlation_id_is_forwarded_if_present() {
        let gw_port = 7002;
        let backend_port = 7003;
        let prefix = "/correlation";
        let header = "X-Correlation-Id";
        tokio::spawn(async move {
            echo_correlation_server(header, backend_port).await;
        });
        tokio::spawn(async move {
            let mut api = Api::http("127.0.0.1", backend_port, prefix.to_string()).unwrap();
            api.add_handler(Box::new(CorrelationIdTransformer { header_name: header.to_string() }));
            start_local_gateway(gw_port, vec![api]).await
        });
        wait_for_gateway(gw_port).await;
        let client = Client::new();
        let url = format!("http://127.0.0.1:{}{}", gw_port, prefix);
        let custom_correlation = "Custom-Generated-Correlation";
        let req = Request::builder()
            .method("GET")
            .uri(url)
            .header("X-Correlation-Id", HeaderValue::from_str(custom_correlation).unwrap())
            .body(Body::empty())
            .unwrap();
        let resp = client.request(req).await.unwrap();
        assert_eq!(200, resp.status());
        let body = unwrap_body_as_str(resp).await;
        assert_eq!(body, custom_correlation);
    }

}