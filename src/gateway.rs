use hyper::service::{Service};
use hyper::{Error, Response, Server, StatusCode, Body, Request};
use futures::task::{Context, Poll};
use std::pin::Pin;
use std::future::Future;
use crate::conf::api::Api;
use log::info;
use std::sync::Arc;
use std::collections::HashMap;

type PinnedResponseFuture = Pin<Box<dyn Future<Output = Result<Response<Body>, Error>> + Send>>;
type PinnedGatewayFuture = Pin<Box<dyn Future<Output = Result<Gateway, Error>> + Send>>;

pub async fn start_local_gateway(port: u16, apis: Vec<Api>) -> Result<(), Error> {
    let gateway = MkGateway { apis: apis.into_iter().map(Arc::new).collect() };
    let in_addr = ([127, 0, 0, 1], port).into();
    let server = Server::bind(&in_addr).serve(gateway);
    info!("Listening on http://{}", in_addr);
    server.await
}

pub struct Gateway {
    by_path: HashMap<String, Arc<Api>>
}

impl Service<Request<Body>> for Gateway {
    type Response = Response<Body>;
    type Error = Error;
    type Future = PinnedResponseFuture;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let api: Option<Arc<Api>> = self.match_path(&req).cloned();
        Box::pin(
            async move {
                let path = req.uri().path();
                if path == "/health" {
                    return Ok(Response::builder().status(200).body(Body::empty()).unwrap())
                }
                match api {
                    Some(api) => {
                        let resp = api.proxy(req).await;
                        if resp.is_err() {
                            log::error!("{:?}", resp);
                            return Ok(Response::builder()
                                .status(StatusCode::BAD_GATEWAY)
                                .body(Body::empty()).unwrap())
                        }
                        resp
                    },
                    None =>
                        Ok(Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(Body::empty()).unwrap())
                }
            }
        )
    }

}

impl Gateway {
    fn new(apis: Vec<Arc<Api>>) -> Self {
        let mut map = HashMap::with_capacity(apis.len());
        for api in apis {
            map.insert(api.prefix.clone(), api);
        }
        Gateway { by_path: map }
    }
    fn match_path(&self, req: &Request<Body>) -> Option<&Arc<Api>> {
        let path = req.uri().path();
        let a = &path[1..].find('/');
        let id = a.map(|fst| &path[0..fst + 1]).unwrap_or(path);
        self.by_path.get(&id.to_string())
    }
}

pub struct MkGateway {
    pub apis: Vec<Arc<Api>>
}
impl <T> Service<T> for MkGateway {
    type Response = Gateway;
    type Error = Error;
    type Future = PinnedGatewayFuture;

    fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: T) -> Self::Future {
        let apis = self.apis.clone();
        let fut = async move { Ok(Gateway::new(apis)) };
        Box::pin(fut)
    }
}

#[cfg(test)]
mod tests {
    use hyper::{Response, Server, Body, Client, Uri, Request, StatusCode};
    use std::convert::Infallible;
    use hyper::service::{make_service_fn, service_fn};
    use std::net::SocketAddr;
    use crate::gateway::{start_local_gateway};
    use crate::conf::api::Api;
    use crate::tests::{test_server, wait_for_gateway, unwrap_body_as_str};
    use std::str::FromStr;
    use hyper::http::HeaderValue;
    use log::*;
    use serde_json::Value;

    async fn echo_path_server(port: u16) {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let make_svc = make_service_fn(|_conn| {
            async move {
                Ok::<_, Infallible>(
                    service_fn(move |req| {
                        let full_path: String = req.uri().path_and_query().map(|p| p.as_str()).unwrap_or("").to_string();
                        async move {
                            Ok::<_, Infallible>(Response::<Body>::new(full_path.into()))
                        }
                    }))
            }
        });
        let server = Server::bind(&addr).serve(make_svc);
        info!("Mock server listening on http://{}", addr);
        if let Err(e) = server.await {
            error!("server error: {}", e);
        }

    }

    async fn echo_body_server(port: u16) {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let make_svc = make_service_fn(|_conn| {
            async move {
                Ok::<_, Infallible>(
                    service_fn(move |req| {
                        let full_body: Body = req.into_body();
                        async move {
                            Ok::<_, Infallible>(Response::<Body>::new(full_body))
                        }
                    }))
            }
        });
        let server = Server::bind(&addr).serve(make_svc);
        info!("Mock server listening on http://{}", addr);
        if let Err(e) = server.await {
            error!("server error: {}", e);
        }

    }

    #[tokio::test]
    async fn test() {
        let gw_port = 3000;
        let backends= vec![(3001, "/first", "server1"), (3002, "/second", "server2")];
        let backend_3_port = 3003;
        let path_3 = "/third";
        for (port, _, payload) in backends.clone() {
            tokio::spawn(async move {
                test_server(payload, port).await
            });
        }
        tokio::spawn({
            let backends = backends.clone();
            async move {
                let mut apis: Vec<Api> = backends.iter().map(|(port, path, _)| {
                    Api::http("127.0.0.1", *port, path.to_string()).unwrap()
                }).collect();
                apis.push(Api::http("127.0.0.1", backend_3_port, path_3.to_string()).unwrap()); // <-- does not exist
                start_local_gateway(gw_port, apis).await
            }
        });
        wait_for_gateway(gw_port).await;

        let client = Client::new();
        let gw_url = format!("http://127.0.0.1:{}", gw_port);

        for (_, path, payload) in backends {
            let url = Uri::from_str(format!("{}{}", gw_url, path).as_str()).unwrap();
            let resp = client.get(url).await.unwrap();
            assert_eq!(StatusCode::OK, resp.status());
            assert_eq!(payload, unwrap_body_as_str(resp).await);
        }

        let url = Uri::from_str(format!("{}{}", gw_url, path_3).as_str()).unwrap();
        let resp = client.get(url).await.unwrap();
        assert_eq!(StatusCode::BAD_GATEWAY, resp.status());

        let url = Uri::from_str(format!("{}/fourth", gw_url).as_str()).unwrap();
        let resp = client.get(url).await.unwrap();
        assert_eq!(StatusCode::NOT_FOUND, resp.status());
    }

    #[tokio::test]
    async fn check_path() {
        let gw_port = 3010;
        let backend_port = 3011;
        let prefix = "/echo";
        let path = "/some/path?and_query=value";
        tokio::spawn(async move {
            echo_path_server(backend_port).await
        });
        tokio::spawn(async move {
            let apis = vec![
                Api::http("127.0.0.1", backend_port, prefix.to_string()).unwrap(),
            ];
            start_local_gateway(gw_port, apis).await
        });
        wait_for_gateway(gw_port).await;
        let client = Client::new();
        let gw_url = format!("http://127.0.0.1:{}", gw_port);
        let url = Uri::from_str(format!("{}{}{}", gw_url, prefix, path).as_str()).unwrap();
        let resp = client.get(url).await.unwrap();
        assert_eq!(StatusCode::OK, resp.status());
        let body_str = unwrap_body_as_str(resp).await;
        assert_eq!(path, body_str);
    }

    #[tokio::test]
    async fn check_forwarded_body() {
        let gw_port = 3020;
        let backend_port = 3021;
        let prefix = "/echo";
        tokio::spawn(async move {
            echo_body_server(backend_port).await
        });
        tokio::spawn(async move {
            let apis = vec![
                Api::http("127.0.0.1", backend_port, prefix.to_string()).unwrap(),
            ];
            start_local_gateway(gw_port, apis).await
        });
        wait_for_gateway(gw_port).await;
        let client = Client::new();
        let url = Uri::from_str(format!("http://127.0.0.1:{}{}", gw_port, prefix).as_str()).unwrap();
        let body = "the_body";

        let resp = client.request(
            Request::builder()
                .method("POST")
                .uri(url)
                .body(body.into())
                .unwrap()
        ).await.unwrap();
        assert_eq!(StatusCode::OK, resp.status());
        let body_str = unwrap_body_as_str(resp).await;
        assert_eq!(body, body_str);
    }

    #[tokio::test]
    async fn check_forwarded_headers() {
        let gw_port = 3030;
        let backend_port = 3031;
        let header = "X-Something-custom";
        let header_value = "some-value";
        let prefix = "/echo-header";
        tokio::spawn(async move {
            let addr = SocketAddr::from(([127, 0, 0, 1], backend_port));
            let make_svc = make_service_fn(|_conn| {
                async move {
                    Ok::<_, Infallible>(
                        service_fn(move |req| {
                            let original_header = req.headers().get(header).unwrap();
                            let new_value = format!("{}-forwarded", original_header.to_str().unwrap());
                            async move {
                                Ok::<_, Infallible>(Response::builder()
                                    .status(StatusCode::OK)
                                    .header(header, HeaderValue::from_str(new_value.as_str()).unwrap())
                                    .body(Body::empty())
                                    .unwrap()
                                )
                            }
                        }))
                }
            });
            let server = Server::bind(&addr).serve(make_svc);
            info!("Mock server listening on http://{}", addr);
            if let Err(e) = server.await {
                error!("server error: {}", e);
            }
        });
        tokio::spawn(async move {
            let apis = vec![
                Api::http("127.0.0.1", backend_port, prefix.to_string()).unwrap(),
            ];
            start_local_gateway(gw_port, apis).await
        });
        wait_for_gateway(gw_port).await;
        let client = Client::new();
        let url = Uri::from_str(format!("http://127.0.0.1:{}{}", gw_port, prefix).as_str()).unwrap();

        let resp = client.request(
            Request::builder()
                .uri(url)
                .header(header, HeaderValue::from_str(header_value).unwrap())
                .body(Body::empty())
                .unwrap()
        ).await.unwrap();
        assert_eq!(StatusCode::OK, resp.status());
        assert_eq!(resp.headers().get(header).unwrap(), format!("{}-forwarded", header_value).as_str());
    }

    #[tokio::test]
    async fn test_http_to_https_by_using_swapi() {
        let gw_port = 3040;
        let prefix = "/swapi";
        tokio::spawn(async move {
            start_local_gateway(
                gw_port,
                vec![Api::https("swapi.dev", prefix.to_string()).unwrap()]
            ).await.unwrap();
        });
        wait_for_gateway(gw_port).await;
        let client = Client::new();
        let url = Uri::from_str(format!("http://127.0.0.1:{}{}/api/people/1/", gw_port, prefix).as_str()).unwrap();
        let req = Request::builder()
            .method("GET")
            .uri(url)
            .header("Accept", "application/json")
            .body(Body::empty())
            .unwrap();
        let resp = client.request(req).await.unwrap();
        assert_eq!(StatusCode::OK, resp.status());
        let body = unwrap_body_as_str(resp).await;
        let json: Value = serde_json::from_str(body.as_str()).unwrap();
        assert_eq!("Luke Skywalker", json.get("name").unwrap().as_str().unwrap());

    }

}