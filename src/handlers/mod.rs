use hyper::{Response, Body, Request};
use std::fmt::Debug;
use async_trait::async_trait;
pub mod interceptor;
pub mod transformer;

/// Controls the Gateway flow
/// After an Handler has been invoked, should it move on and invoke the next Handler in the chain
/// Or circuit-break and return the Response immediately
pub enum HandlerResponse {
    Continue,               // move on to next handler
    Break(Response<Body>),  // breaks and returns the response immediately
}

/// Global for the whole gateway lifetime
/// i.e. a single handler for all the requests/responses
pub trait GlobalHandler: Send + Debug + Sync {
    fn handle_req(&self, req: &mut Request<Body>) -> HandlerResponse;
    fn handle_res(&self, res: &mut Response<Body>) -> HandlerResponse;
}

/// Scoped to a single request/response flow
/// i.e. a new Handler is created when the request is hitting the gateway, and dropped when the response is sent back
pub trait ScopedHandler: Send + Debug + Sync {
    fn handle_req(&self, req: &mut Request<Body>) -> HandlerResponse;
    fn handle_res(&self, req: &mut Response<Body>) -> HandlerResponse;
}

/// Creates a ScopedHandler for every incoming request
pub trait ScopedHandlerFactory: Send + Debug + Sync {
    fn create(&self) -> Box<dyn ScopedHandler>;
}

/// Takes ownership of the upstream response, maps it, and return a new response
/// Async cause reading the response body may be async
#[async_trait]
pub trait ResponseFinalizer: Send + Debug + Sync {
    async fn transform(&self, res: Response<Body>) -> Response<Body>;
}

#[cfg(test)]
mod tests {
    use crate::tests::{wait_for_gateway, test_server};
    use crate::handlers::{GlobalHandler, HandlerResponse, ScopedHandler, ScopedHandlerFactory};
    use hyper::{Client, Server, Response, Request, Body, StatusCode, Uri};
    use crate::handlers::HandlerResponse::{Continue, Break};
    use crate::gateway::start_local_gateway;
    use crate::conf::api::Api;
    use std::str::FromStr;
    use hyper::header::*;
    use std::time::{SystemTime, Instant};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::time::{Duration, delay_for};
    use hyper::service::{make_service_fn, service_fn};
    use std::convert::Infallible;
    use log::*;
    use std::net::SocketAddr;
    use rand::{Rng};
    use hyper::client::ResponseFuture;

    #[tokio::test]
    async fn test_break() {
        #[derive(Debug, Clone)]
        struct BreakingHandler {}
        impl GlobalHandler for BreakingHandler {
            fn handle_req(&self, _req: &mut Request<Body>) -> HandlerResponse {
                Break(Response::builder().status(StatusCode::GONE).body(Body::empty()).unwrap())
            }

            fn handle_res(&self, _res: &mut Response<Body>) -> HandlerResponse {
                Continue
            }
        }
        let gw_port = 7100;
        let backend_port = 7101;
        let path = "/shortcut";
        tokio::spawn(async move {
            let mut api = Api::http("127.0.0.1", backend_port, path.to_string()).unwrap();
            api.add_global_handler(Box::new(BreakingHandler {}));
            start_local_gateway(gw_port, vec![api]).await
        });
        wait_for_gateway(gw_port).await;
        let client = Client::new();
        let url = Uri::from_str(format!("http://127.0.0.1:{}{}", gw_port, path).as_str()).unwrap();
        let resp = client.get(url).await.unwrap();
        assert_eq!(StatusCode::GONE, resp.status());
    }

    #[tokio::test]
    async fn test_chain_response_handlers() {
        #[derive(Debug, Clone)] struct TimeHandler { name: String, origin: SystemTime }
        impl GlobalHandler for TimeHandler {
            fn handle_req(&self, _req: &mut Request<Body>) -> HandlerResponse {
                Continue
            }
            fn handle_res(&self, res: &mut Response<Body>) -> HandlerResponse {
                let now  = SystemTime::now().duration_since(self.origin).unwrap().as_nanos() as u64;
                let header = HeaderName::from_str(self.name.as_str()).unwrap();
                res.headers_mut().insert(header, HeaderValue::from(now));
                Continue
            }
        }
        let gw_port = 7110;
        let backend_port = 7111;
        let path = "/shortcut";
        tokio::spawn(async move {
            test_server("something", backend_port).await
        });
        tokio::spawn(async move {
            let mut api = Api::http("127.0.0.1", backend_port, path.to_string()).unwrap();
            let origin = SystemTime::now();
            api.add_global_handler(Box::new(TimeHandler { name: "time-1".to_string(), origin }));
            api.add_global_handler(Box::new(TimeHandler { name: "time-2".to_string(), origin })); // <- should always be invoked after
            start_local_gateway(gw_port, vec![api]).await
        });
        wait_for_gateway(gw_port).await;
        let client = Client::new();
        let url = Uri::from_str(format!("http://127.0.0.1:{}{}", gw_port, path).as_str()).unwrap();
        let resp = client.get(url).await.unwrap();
        assert_eq!(StatusCode::OK, resp.status());
        let headers = resp.headers();
        let fst: u64 = headers.get("time-1").unwrap().to_str().unwrap().parse().unwrap();
        let snd: u64 = headers.get("time-2").unwrap().to_str().unwrap().parse().unwrap();
        assert!(fst < snd);
    }

    #[tokio::test]
    async fn test_mut_handler() {
        #[derive(Debug, Clone)] struct CountHandler { counter: Arc<AtomicU32> }
        impl GlobalHandler for CountHandler {
            fn handle_req(&self, _req: &mut Request<Body>) -> HandlerResponse {
                Continue
            }
            fn handle_res(&self, res: &mut Response<Body>) -> HandlerResponse {
                let old = self.counter.fetch_add(1, Ordering::SeqCst);
                res.headers_mut().insert("X-Count", HeaderValue::from(old + 1));
                Continue
            }
        }
        let gw_port = 7120;
        let backend_port = 7121;
        let path = "/counter";
        tokio::spawn(async move {
            test_server("check X-Count header", backend_port).await
        });
        tokio::spawn(async move {
            let mut api = Api::http("127.0.0.1", backend_port, path.to_string()).unwrap();
            api.add_global_handler(Box::new(CountHandler { counter: Arc::new(AtomicU32::new(0)) }));
            start_local_gateway(gw_port, vec![api]).await
        });
        wait_for_gateway(gw_port).await;
        let client = Client::new();
        let url = Uri::from_str(format!("http://127.0.0.1:{}{}", gw_port, path).as_str()).unwrap();
        client.get(url.clone()).await.unwrap();
        let client = Client::new(); // create a new Client => new connection?
        client.get(url.clone()).await.unwrap();
        let client = Client::new(); // create a new Client => new connection?
        client.get(url.clone()).await.unwrap();
        let resp = client.get(url.clone()).await.unwrap();
        assert_eq!(StatusCode::OK, resp.status());
        let headers = resp.headers();
        let count: u32 = headers.get("X-Count").unwrap().to_str().unwrap().parse().unwrap();
        assert_eq!(count, 4);
        drop(client);

        // try to drop the client to create a new connection
        let client = Client::new();
        let resp = client.get(url).await.unwrap();
        assert_eq!(StatusCode::OK, resp.status());
        let headers = resp.headers();
        let count: u32 = headers.get("X-Count").unwrap().to_str().unwrap().parse().unwrap();
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn test_scoped_handler() {
        #[derive(Debug, Clone)] struct TestScopedHandler { id: Arc<Mutex<Option<usize>>> }
        impl ScopedHandler for TestScopedHandler {
            fn handle_req(&self, req: &mut Request<Body>) -> HandlerResponse {
                // Store the received X-Id locally
                if let Some(id) = self.id.lock().unwrap().as_mut() {
                    *id = req.headers()
                        .get("X-Id")
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .parse()
                        .unwrap()
                }
                Continue
            }
            fn handle_res(&self, res: &mut Response<Body>) -> HandlerResponse {
                // The response X-Id (since echoed back) should match the local id, no matter the order in responses
                let id: usize = res.headers().get("X-Id").unwrap().to_str().unwrap().parse().unwrap();
                let this_id = self.id.lock().unwrap().unwrap();
                if this_id != id {
                    return Break(Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::empty()).unwrap())
                }
                Continue
            }
        }
        #[derive(Debug, Clone)]
        struct TestScopedFactory {};
        impl ScopedHandlerFactory for TestScopedFactory {
            fn create(&self) -> Box<dyn ScopedHandler> {
                Box::new(TestScopedHandler { id: Arc::new(Mutex::new(Some(0)))})
            }
        }
        let gw_port = 7130;
        let backend_port = 7131;
        let path = "/test_hook";
        tokio::spawn(async move {
            let addr = SocketAddr::from(([127, 0, 0, 1], backend_port));
            let make_svc = make_service_fn(|_conn| {
                async move {
                    Ok::<_, Infallible>(
                        service_fn(move |req| {
                            async move {
                                delay_for(Duration::from_millis(rand::rngs::OsRng.gen_range(100, 1_100))).await;
                                let id: usize = (req.headers().get("X-Id").unwrap().to_str().unwrap()).parse().unwrap();
                                let resp = Response::builder()
                                    .status(StatusCode::OK)
                                    .header("X-Id", id) // echo the X-Id back
                                    .body(Body::empty())
                                    .unwrap();
                                Ok::<_, Infallible>(resp)
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
            let mut api = Api::http("127.0.0.1", backend_port, path.to_string()).unwrap();
            api.add_scoped_handler(Box::new(TestScopedFactory {}));
            start_local_gateway(gw_port, vec![api]).await
        });
        wait_for_gateway(gw_port).await;
        let client = Client::new();
        let reqs: Vec<ResponseFuture> = (1..10_usize).into_iter().map(|i| {
            let url = Uri::from_str(format!("http://127.0.0.1:{}{}", gw_port, path).as_str()).unwrap();
            let req = Request::builder()
                .uri(url)
                .header("X-Id", i)
                .body(Body::empty())
                .unwrap();
            client.request(req)
        }).collect();
        futures::future::join_all(reqs).await;
    }

    #[tokio::test]
    async fn measure_response_time_by_using_scoped_handler() {
        #[derive(Debug, Clone)] struct ResponseDurationScoped { start: Arc<Mutex<Option<Instant>>>}
        impl ScopedHandler for ResponseDurationScoped {
            fn handle_req(&self, _req: &mut Request<Body>) -> HandlerResponse {
                if let Some(id) = self.start.lock().unwrap().as_mut() {
                    *id = Instant::now()
                }
                Continue
            }
            fn handle_res(&self, res: &mut Response<Body>) -> HandlerResponse {
                let start: Instant = self.start.lock().unwrap().unwrap();
                res.headers_mut()
                    .insert(
                        "X-Response-Time",
                        HeaderValue::from(start.elapsed().as_millis() as u32)
                    );
                Continue
            }
        }
        #[derive(Debug, Clone)]
        struct ResponseDurationScopedFactory {};
        impl ScopedHandlerFactory for ResponseDurationScopedFactory {
            fn create(&self) -> Box<dyn ScopedHandler> {
                Box::new(ResponseDurationScoped { start: Arc::new(Mutex::new(Some(Instant::now()))) })
            }
        }
        let gw_port = 7140;
        let backend_port = 7141;
        let path = "/response_duration";
        let sleep = 200;
        tokio::spawn(async move {
            let addr = SocketAddr::from(([127, 0, 0, 1], backend_port));
            let make_svc = make_service_fn(|_conn| {
                async move {
                    Ok::<_, Infallible>(
                        service_fn(move |_req| {
                            async move {
                                delay_for(Duration::from_millis(sleep)).await;
                                let resp = Response::builder()
                                    .status(StatusCode::OK)
                                    .body(Body::empty())
                                    .unwrap();
                                Ok::<_, Infallible>(resp)
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
            let mut api = Api::http("127.0.0.1", backend_port, path.to_string()).unwrap();
            api.add_scoped_handler(Box::new(ResponseDurationScopedFactory {}));
            start_local_gateway(gw_port, vec![api]).await
        });
        wait_for_gateway(gw_port).await;
        let nb_req = 10;
        let reqs: Vec<ResponseFuture> = (0..nb_req).into_iter()
            .map(|_| {
            let client = Client::new();
            let url = Uri::from_str(format!("http://127.0.0.1:{}{}", gw_port, path).as_str()).unwrap();
            let req = Request::builder()
                .uri(url)
                .body(Body::empty())
                .unwrap();
            client.request(req)
        }).collect();
        let durs: Vec<Duration> = futures::future::join_all(reqs)
            .await
            .iter()
            .filter_map(|res| {
                match res {
                    Ok(resp) => {
                        let millis = u64::from_str(resp.headers().get("X-Response-Time").unwrap().to_str().unwrap()).unwrap();
                        Some(Duration::from_millis(millis))
                    },
                    _ => None
                }
            })
            .collect();
        assert_eq!(nb_req, durs.len());
        let gw_max_overhead = Duration::from_millis(20);
        for duration in durs.iter() {
            info!("duration is: {:?}", duration);
            assert!(*duration <= Duration::from_millis(sleep) + gw_max_overhead);
            assert!(*duration >= Duration::from_millis(sleep));
        }
    }

}