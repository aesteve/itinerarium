use crate::conf::Endpoint;
use std::net::SocketAddr;
use hyper::service::{Service};
use hyper::{Client, Error, Response, StatusCode, Body, Request};
use futures::task::{Context, Poll};
use std::pin::Pin;
use std::future::Future;

pub(crate) mod conf;
pub(crate) mod utils;

#[tokio::main]
async fn main() {}


struct Gateway {
    endpoints: Vec<Endpoint>
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
        let e = self.endpoints.iter().map(|e| (e.address, e.prefix.clone())).collect::<Vec<(SocketAddr, String)>>();
        let client = Client::new();
        Box::pin(
            async move {
                let incoming_uri = req.uri();
                let path = incoming_uri.path();
                match e.iter().find_map(|(address, prefix)| {
                    if path.starts_with(prefix) {
                        Some(format!(
                            "http://{}{}",
                            address.clone(),
                            req.uri().path_and_query().map(|x| x.as_str()).unwrap_or("")
                        ))
                    } else {
                        None
                    }
                }) {
                    Some(uri_string) => {
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

struct MkGateway {
    endpoints: Vec<Endpoint>
}
impl <T> Service<T> for MkGateway {
    type Response = Gateway;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: T) -> Self::Future {
        let endpoints = self.endpoints.clone();
        let fut = async move { Ok(Gateway { endpoints }) };
        Box::pin(fut)
    }
}

#[cfg(test)]
mod tests {
    use hyper::{Response, Server, Body, Client, Uri};
    use std::convert::Infallible;
    use hyper::service::{make_service_fn, service_fn};
    use std::net::SocketAddr;
    use crate::{MkGateway};
    use crate::conf::Endpoint;
    use crate::utils::unwrap_body_as_str;

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
        let gw_port = 3000;
        tokio::spawn(async move {
            test_server("server1", 3001).await
        });
        tokio::spawn(async move {
            test_server("server2", 3002).await
        });
        tokio::spawn(async move {
            let endpoints = vec![
                Endpoint::new("127.0.0.1", 3001, "/first".to_string()).unwrap(),
                Endpoint::new("127.0.0.1", 3002, "/second".to_string()).unwrap(),
                Endpoint::new("127.0.0.1", 3003, "/third".to_string()).unwrap(), // <-- does not exist
            ];
            let gateway = MkGateway { endpoints };
            let in_addr = ([127, 0, 0, 1], gw_port).into();
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

}