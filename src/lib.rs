pub mod conf;
pub mod gateway;
pub(crate) mod utils;
pub mod handlers;

#[cfg(test)]
mod tests {
    use hyper::{Body, Client, Response, Server, Uri};
    use hyper::service::{make_service_fn, service_fn};
    use std::convert::Infallible;
    use std::net::SocketAddr;
    use std::str::FromStr;
    use log::*;
    use crate::utils::body_as_str;

    pub(crate) async fn unwrap_body_as_str(resp: Response<Body>) -> String {
        body_as_str(resp)
            .await
            .unwrap()
    }

    pub async fn wait_for_gateway(port: u16) {
        let mut attempts = 0;
        let client = Client::new();
        let health_uri =  format!("http://127.0.0.1:{}/health", port);
        while attempts < 10 && client.get(Uri::from_str(health_uri.as_str()).unwrap()).await.is_err() {
            attempts += 1;
        }
    }

    pub async fn test_server(payload: &str, port: u16) {
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
        info!("Mock server listening on http://{}", addr);
        if let Err(e) = server.await {
            error!("server error: {}", e);
        }
    }

}
