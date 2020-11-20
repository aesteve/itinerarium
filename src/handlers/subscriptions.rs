#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::{Mutex, Arc};
    use std::sync::mpsc::{Sender, channel};
    use crate::handlers::{GlobalHandler, HandlerResponse};
    use hyper::{Client, Response, Request, Body, StatusCode, Uri};
    use crate::handlers::HandlerResponse::{Continue, Break};
    use crate::handlers::subscriptions::tests::Subscription::{Subscribe, Revoke};
    use crate::tests::{test_server, wait_for_gateway};
    use crate::conf::api::Api;
    use crate::gateway::start_local_gateway;
    use std::str::FromStr;
    use tokio::time::Duration;

    #[derive(Debug, Clone)]
    enum Subscription {
        Subscribe(String),
        Revoke(String)
    }

    #[derive(Debug, Clone)]
    struct SubscriptionHandler {
        header: String,
        validated: Arc<Mutex<HashSet<String>>>,
        publisher: Arc<Mutex<Sender<Subscription>>>,
    }

    impl GlobalHandler for SubscriptionHandler {
        fn handle_req(&self, req: &mut Request<Body>) -> HandlerResponse {
            match req.headers().get(self.header.as_str()) {
                None => Break(Response::builder().status(StatusCode::UNAUTHORIZED).body(Body::empty()).unwrap()),
                Some(api_key) => {
                    match self.validated.lock() {
                        Err(_) => Break(Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::empty()).unwrap()),
                        Ok(keys) => {
                            match api_key.to_str() {
                                Err(_) => Break(Response::builder().status(StatusCode::BAD_REQUEST).body(Body::empty()).unwrap()),
                                Ok(key) => {
                                    if keys.contains(key) {
                                        Continue
                                    } else {
                                        Break(Response::builder().status(StatusCode::FORBIDDEN).body(Body::empty()).unwrap())
                                    }
                                }
                            }
                        }
                    }
                }

            }
        }

        fn handle_res(&self, _res: &mut Response<Body>) -> HandlerResponse {
            Continue
        }
    }

    impl SubscriptionHandler {
        fn create(header: String) -> (Sender<Subscription>, Self) {
            let (sender, receiver) = channel();
            let validated = Arc::new(Mutex::new(HashSet::new()));
            let keys = validated.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::delay_for(Duration::from_millis(100)).await;
                    match receiver.recv() {
                        Ok(Subscribe(key)) => {
                            keys.lock().unwrap().insert(key);
                        },
                        Ok(Revoke(key)) => {
                            keys.lock().unwrap().remove(&key);
                        },
                        Err(_) => {}
                    }
                }
            });
            (sender.clone(), SubscriptionHandler {
                header,
                publisher: Arc::new(Mutex::new(sender)),
                validated,
            })
        }
    }

    fn with_api_key(uri: &Uri, key: &str) -> Request<Body> {
        Request::builder()
            .uri(uri)
            .header("X-Api-Key", key)
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn test_subscriptions() {
        let gw_port = 11000;
        let backend_port = 11001;
        let prefix = "/subscribers_only";
        let header = "X-Api-Key";
        tokio::spawn(async move {
            test_server("Granted!", backend_port).await;
        });
        let (sender, subscriptions) = SubscriptionHandler::create(header.to_string());
        tokio::spawn(async move {
            let mut api = Api::http("127.0.0.1", backend_port, prefix.to_string()).unwrap();
            api.add_global_handler(Box::new(subscriptions));
            start_local_gateway(gw_port, vec![api]).await.unwrap();
        });
        wait_for_gateway(gw_port).await;
        let key = "Something";
        let client = Client::new();
        let uri = Uri::from_str(format!("http://127.0.0.1:{}{}", gw_port, prefix).as_str()).unwrap();

        let resp = client.get(uri.clone()).await.unwrap();
        assert_eq!(StatusCode::UNAUTHORIZED, resp.status()); // No Api-Key => 401
        let resp = client.request(with_api_key(&uri, key)).await.unwrap();
        assert_eq!(StatusCode::FORBIDDEN, resp.status()); // Api-Key is present but not subscribed => 403

        // Validate the Api-Key
        sender.send(Subscribe(key.to_string())).unwrap();
        tokio::time::delay_for(Duration::from_millis(105)).await;

        let resp = client.get(uri.clone()).await.unwrap();
        assert_eq!(StatusCode::UNAUTHORIZED, resp.status()); // No Api-Key => 401
        let resp = client.request(with_api_key(&uri, key)).await.unwrap();
        assert_eq!(StatusCode::OK, resp.status()); // Valid subscription => OK
        let resp = client.request(with_api_key(&uri, "Another key")).await.unwrap();
        assert_eq!(StatusCode::FORBIDDEN, resp.status()); // Not a subscription => 403
        let resp = client.get(uri.clone()).await.unwrap();
        assert_eq!(StatusCode::UNAUTHORIZED, resp.status()); // No Api-Key => Unauthorized

        // Revoke the Api-Key
        sender.send(Revoke(key.to_string())).unwrap();
        tokio::time::delay_for(Duration::from_millis(105)).await;

        let resp = client.get(uri.clone()).await.unwrap();
        assert_eq!(StatusCode::UNAUTHORIZED, resp.status());  // No Api-Key => Unauthorized
        let resp = client.request(with_api_key(&uri, key)).await.unwrap();
        assert_eq!(StatusCode::FORBIDDEN, resp.status());  // Key is no longer valid
        let resp = client.request(with_api_key(&uri, "Something else")).await.unwrap();
        assert_eq!(StatusCode::FORBIDDEN, resp.status());  // Another key is still not valid

        let resp = client.request(with_api_key(&uri, ".भारत")).await.unwrap();
        assert_eq!(StatusCode::BAD_REQUEST, resp.status());  // Non ASCII char in header is considered invalid
    }


}