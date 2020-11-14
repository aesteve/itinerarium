use crate::gateway::{start_gateway};
use crate::conf::api::Api;
use hyper::Error;

pub(crate) mod conf;
pub(crate) mod utils;
pub(crate) mod gateway;

#[tokio::main]
async fn main() -> Result<(), Error> {
    start_gateway(
        1234,
        vec![Api::https("swapi.dev", "/swapi".to_string()).unwrap()]
    ).await
}

#[cfg(test)]
mod tests {
    use hyper::{Request, Body, Client, Response, Server};
    use hyper::client::HttpConnector;
    use hyper::service::{make_service_fn, service_fn};
    use std::convert::Infallible;
    use std::net::SocketAddr;

    pub(crate) type RequestBuilder = fn() -> Request<Body>;

    pub(crate) async fn wait_for(client: &Client<HttpConnector>, req_builder: RequestBuilder) {
        let mut attempts = 0;
        while attempts < 10 && client.request(req_builder()).await.is_err() {
            attempts += 1;
        }
    }

    pub(crate) async fn test_server(payload: &'static str, port: u16) {
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




}