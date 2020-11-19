#[cfg(test)]
mod tests {
    use crate::tests::{test_server, wait_for_gateway, unwrap_body_as_str, body_as_str};
    use serde_json::{json, Value};
    use crate::conf::api::Api;
    use crate::gateway::start_local_gateway;
    use hyper::{Client, Uri, StatusCode, Response, Body};
    use std::str::FromStr;
    use crate::handlers::ResponseFinalizer;
    use async_trait::async_trait;

    #[derive(Debug, Clone)]
    struct JsonPointer {
        pointer: String
    }

    #[async_trait]
    impl ResponseFinalizer for JsonPointer {
        async fn transform(&self, res: Response<Body>) -> Response<Body> {
            match body_as_str(res).await {
                Err(err) => {
                    log::error!("Could not extract response body {:?}", err);
                    Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::empty()).unwrap()
                },
                Ok(body) => {
                    match serde_json::from_str::<Value>(body.as_str()) {
                        Err(err) => {
                            log::error!("Could not read body as json {:?}", err);
                            Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::empty()).unwrap()

                        },
                        Ok(json) => {
                            match json.pointer(self.pointer.as_str()) {
                                None => Response::builder().status(StatusCode::NOT_FOUND).body(Body::empty()).unwrap(),
                                Some(value) => {
                                    let payload = value.to_string();
                                    Response::builder()
                                        .status(StatusCode::OK)
                                        .body(Body::from(payload))
                                        .unwrap()
                                }
                            }

                        }
                    }
                }
            }

        }
    }


    #[tokio::test]
    async fn test_pointer() {
        let gw_port = 10_000;
        let backend_port = 10_001;
        let prefix_1 = "/json_string";
        let prefix_2 = "/json_array_snd";
        tokio::spawn(async move {
            let json = json!({"string": "value", "array": ["A", "B", 42]});
            test_server(&*json.to_string(), backend_port).await
        });
        tokio::spawn(async move {
            let mut api_1 = Api::http("127.0.0.1", backend_port, prefix_1.to_string()).unwrap();
            api_1.finalize_with(Box::new(JsonPointer { pointer: "/string".to_string() }));
            let mut api_2 = Api::http("127.0.0.1", backend_port, prefix_2.to_string()).unwrap();
            api_2.finalize_with(Box::new(JsonPointer { pointer: "/array/2".to_string() }));
            start_local_gateway(gw_port, vec![api_1, api_2]).await.unwrap();
        });
        wait_for_gateway(gw_port).await;
        let client = Client::new();
        let url = Uri::from_str(format!("http://127.0.0.1:{}{}", gw_port, prefix_1).as_str()).unwrap();
        let resp = client.get(url.clone()).await.unwrap();
        assert_eq!(StatusCode::OK, resp.status());
        let body = unwrap_body_as_str(resp).await;
        assert_eq!(json!("value").to_string(), body);

        let url = Uri::from_str(format!("http://127.0.0.1:{}{}", gw_port, prefix_2).as_str()).unwrap();
        let resp = client.get(url.clone()).await.unwrap();
        assert_eq!(StatusCode::OK, resp.status());
        let body = unwrap_body_as_str(resp).await;
        assert_eq!(json!(42).to_string(), body);
    }

}