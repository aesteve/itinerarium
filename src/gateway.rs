use hyper::service::{Service};
use hyper::{Error, Response, StatusCode, Body, Request};
use futures::task::{Context, Poll};
use std::pin::Pin;
use std::future::Future;
use crate::conf::api::Api;
use hyper::http::uri::PathAndQuery;

pub(crate) struct Gateway {
    apis: Vec<Api>
}

fn build_path(path: Option<&PathAndQuery>, from: usize) -> String {
    let full_path = path.map(|x| x.as_str()).unwrap_or("");
    full_path[from..].to_string()
}

impl Service<Request<Body>> for Gateway {
    type Response = Response<Body>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut req: Request<Body>) -> Self::Future {
        // Box::pin(async { Ok(Response::builder().body(Body::from("ok")).unwrap()) })
        let apis = self.apis.clone();
        Box::pin(
            async move {
                let incoming_uri = req.uri();
                let path = incoming_uri.path();
                match apis.iter().find_map(|api| {
                    if path.starts_with(&api.prefix) {
                        Some((format!(
                            "http://{}{}",
                            api.endpoint.address.clone(),
                            build_path(req.uri().path_and_query(), api.prefix.len())
                        ), api.endpoint.client()))
                    } else {
                        None
                    }
                }) {
                    Some((uri_string, client)) => {
                        let uri = uri_string.parse().unwrap();
                        *req.uri_mut() = uri;
                        match client.request(req).await {
                            Err(_) => Ok(Response::builder()
                                .status(StatusCode::BAD_GATEWAY)
                                .body(Body::empty()).unwrap()),
                            res => res,
                        }
                    },
                    None => {
                        // return ok(Response::builder().status(404).body(json!({"message": "endpoint not found"})).unwrap())
                        Ok(Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(Body::empty()).unwrap())
                    }
                }
            }

        )
    }
}

pub(crate) struct MkGateway {
    apis: Vec<Api>
}
impl <T> Service<T> for MkGateway {
    type Response = Gateway;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: T) -> Self::Future {
        let apis = self.apis.clone();
        let fut = async move { Ok(Gateway { apis }) };
        Box::pin(fut)
    }
}

#[cfg(test)]
mod tests {
    use hyper::{Response, Server, Body, Client, Uri, Request};
    use std::convert::Infallible;
    use hyper::service::{make_service_fn, service_fn};
    use std::net::SocketAddr;
    use crate::gateway::MkGateway;
    use crate::utils::unwrap_body_as_str;
    use crate::conf::api::Api;

    async fn echo_path(port: u16) {
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
        println!("Mock server listening on http://{}", addr);
        if let Err(e) = server.await {
            eprintln!("server error: {}", e);
        }

    }

    async fn echo_body(port: u16) {
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
        println!("Mock server listening on http://{}", addr);
        if let Err(e) = server.await {
            eprintln!("server error: {}", e);
        }

    }


    async fn test_server(payload: &'static str, port: u16) {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let payload = payload.to_string();
        let make_svc = make_service_fn(|_conn| {
            let payload = payload.clone();
            async move {
                let payload = payload.clone();
                Ok::<_, Infallible>(
                    service_fn(move |_req| {
                        let payload = payload.clone();
                        async move {
                            Ok::<_, Infallible>(Response::<Body>::new(payload.into()))
                        }
                    }))
            }
        });

        let server = Server::bind(&addr).serve(make_svc);
        println!("Mock server listening on http://{}", addr);
        if let Err(e) = server.await {
            eprintln!("server error: {}", e);
        }
    }

    #[tokio::test]
    async fn test() {
        tokio::spawn(async move {
            test_server("server1", 3001).await
        });
        tokio::spawn(async move {
            test_server("server2", 3002).await
        });
        tokio::spawn(async move {
            let endpoints = vec![
                Api::new("127.0.0.1", 3001, "/first".to_string()).unwrap(),
                Api::new("127.0.0.1", 3002, "/second".to_string()).unwrap(),
                Api::new("127.0.0.1", 3003, "/third".to_string()).unwrap(), // <-- does not exist
            ];
            let gateway = MkGateway { apis: endpoints };
            let in_addr = ([127, 0, 0, 1], 3000).into();
            let server = Server::bind(&in_addr).serve(gateway);
            println!("Listening on http://{}", in_addr);
            server.await
        });
        let client = Client::new();
        let mut attempts = 0;
        while attempts < 10 && client.get(Uri::from_static("http://127.0.0.1:3000/first")).await.is_err() {
            attempts += 1;
        }
        let resp = client.get(Uri::from_static("http://127.0.0.1:3000/first")).await.unwrap();
        assert_eq!(200, resp.status());
        assert_eq!("server1", unwrap_body_as_str(resp).await);
        let resp = client.get(Uri::from_static("http://127.0.0.1:3000/second")).await.unwrap();
        assert_eq!(200, resp.status());
        assert_eq!("server2", unwrap_body_as_str(resp).await);
        let resp = client.get(Uri::from_static("http://127.0.0.1:3000/third")).await.unwrap();
        assert_eq!(502, resp.status());
        let resp = client.get(Uri::from_static("http://127.0.0.1:3000/fourth")).await.unwrap();
        assert_eq!(404, resp.status());
    }

    #[tokio::test]
    async fn check_path() {
        tokio::spawn(async move {
            echo_path(4001).await
        });
        tokio::spawn(async move {
            let endpoints = vec![
                Api::new("127.0.0.1", 4001, "/echo".to_string()).unwrap(),
            ];
            let gateway = MkGateway { apis: endpoints };
            let in_addr = ([127, 0, 0, 1], 4000).into();
            let server = Server::bind(&in_addr).serve(gateway);
            println!("Listening on http://{}", in_addr);
            server.await
        });
        let client = Client::new();
        let mut attempts = 0;
        while attempts < 10 && client.get(Uri::from_static("http://127.0.0.1:4000/echo")).await.is_err() {
            attempts += 1;
        }
        let resp = client.get(Uri::from_static("http://127.0.0.1:4000/echo/some/path?and_query=value")).await.unwrap();
        assert_eq!(200, resp.status());
        let body_str = unwrap_body_as_str(resp).await;
        assert_eq!("/some/path?and_query=value", body_str);
    }

    #[tokio::test]
    async fn check_forwarded_body() {
        tokio::spawn(async move {
            echo_body(5001).await
        });
        tokio::spawn(async move {
            let endpoints = vec![
                Api::new("127.0.0.1", 5001, "/echo".to_string()).unwrap(),
            ];
            let gateway = MkGateway { apis: endpoints };
            let in_addr = ([127, 0, 0, 1], 5000).into();
            let server = Server::bind(&in_addr).serve(gateway);
            println!("Listening on http://{}", in_addr);
            server.await
        });
        let client = Client::new();
        let mut attempts = 0;
        while attempts < 10 &&
            client.request(Request::builder().method("POST").uri("http://127.0.0.1:5000/echo").body("the_body".into()).unwrap())
                .await
                .is_err() {
            attempts += 1;
        }
        let resp = client.request(Request::builder().method("POST").uri("http://127.0.0.1:5000/echo").body("the_body".into()).unwrap()).await.unwrap();
        assert_eq!(200, resp.status());
        let body_str = unwrap_body_as_str(resp).await;
        assert_eq!("the_body", body_str);
    }

}