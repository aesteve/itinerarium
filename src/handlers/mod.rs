use hyper::{Response, Body, Request};
use dyn_clone::{clone_trait_object, DynClone};
use std::fmt::Debug;
use async_trait::async_trait;
pub mod interceptor;
pub mod transformer;

pub enum HandlerResponse {
    Continue,               // move on to next handler
    Break(Response<Body>),  // breaks and returns the response
}


pub trait Handler: Send + Debug + Sync + DynClone {
    fn handle_req(&self, req: &mut Request<Body>) -> HandlerResponse;
    fn handle_res(&self, res: &mut Response<Body>) -> HandlerResponse;
}

pub trait Hook: Send + Debug + Sync + DynClone {
    fn on_request(&self, req: &Request<Body>);
    fn on_response(&self, req: &Response<Body>);
}

pub trait HookFactory: Send + Debug + Sync + DynClone {
    fn create(&self) -> Box<dyn Hook>;
}

#[async_trait]
pub trait ResponseTransformer: Send + Debug + Sync + DynClone {
    async fn transform(&self, res: Response<Body>) -> Response<Body>;
}

clone_trait_object!(Handler);
clone_trait_object!(Hook);
clone_trait_object!(HookFactory);
clone_trait_object!(ResponseTransformer);

#[cfg(test)]
mod tests {
    use crate::tests::{wait_for_gateway, test_server};
    use crate::handlers::{Handler, HandlerResponse, Hook, HookFactory};
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
        impl Handler for BreakingHandler {
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
            api.add_handler(Box::new(BreakingHandler {}));
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
        impl Handler for TimeHandler {
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
            api.add_handler(Box::new(TimeHandler { name: "time-1".to_string(), origin }));
            api.add_handler(Box::new(TimeHandler { name: "time-2".to_string(), origin })); // <- should always be invoked after
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
        impl Handler for CountHandler {
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
        let backend_port = 7120;
        let path = "/counter";
        tokio::spawn(async move {
            test_server("check X-Count header", backend_port).await
        });
        tokio::spawn(async move {
            let mut api = Api::http("127.0.0.1", backend_port, path.to_string()).unwrap();
            api.add_handler(Box::new(CountHandler { counter: Arc::new(AtomicU32::new(0)) }));
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
    async fn test_mut_hook() {
        #[derive(Debug, Clone)] struct TestHook { id: Arc<Mutex<Option<usize>>> }
        impl Hook for TestHook {
            fn on_request(&self, req: &Request<Body>) {
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
            }
            fn on_response(&self, res: &Response<Body>) {
                // The response X-Id (since echoed back) should match the local id, no matter the order in responses
                let id: usize = res.headers().get("X-Id").unwrap().to_str().unwrap().parse().unwrap();
                let this_id = self.id.lock().unwrap().unwrap();
                if this_id != id {
                    panic!("Received X-Id {:?}. Self.id is {}", id, this_id);
                } else {
                    info!("Correct id")
                }
            }
        }
        #[derive(Debug, Clone)]
        struct TestHookFactory {};
        impl HookFactory for TestHookFactory {
            fn create(&self) -> Box<dyn Hook> {
                Box::new(TestHook { id: Arc::new(Mutex::new(Some(0)))})
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
            api.add_hook(Box::new(TestHookFactory {}));
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
    async fn measure_response_time() {
        let durations: Arc<Mutex<Vec<Duration>>> = Arc::new(Mutex::new(Vec::new()));
        #[derive(Debug, Clone)] struct ResponseDurationHook { start: Arc<Mutex<Option<Instant>>>, durations: Arc<Mutex<Vec<Duration>>>}
        impl Hook for ResponseDurationHook {
            fn on_request(&self, _req: &Request<Body>) {
                if let Some(id) = self.start.lock().unwrap().as_mut() {
                    *id = Instant::now()
                }
            }
            fn on_response(&self, _res: &Response<Body>) {
                let start: Instant = self.start.lock().unwrap().unwrap();
                self.durations.lock().unwrap().push(start.elapsed());
            }
        }
        #[derive(Debug, Clone)]
        struct ResponseDurationHookFactory { durations: Arc<Mutex<Vec<Duration>>> };
        impl HookFactory for ResponseDurationHookFactory {
            fn create(&self) -> Box<dyn Hook> {
                Box::new(ResponseDurationHook { start: Arc::new(Mutex::new(Some(Instant::now()))), durations: self.durations.clone() })
            }
        }
        let gw_port = 7140;
        let backend_port = 7141;
        let path = "/response_duration";
        tokio::spawn(async move {
            let addr = SocketAddr::from(([127, 0, 0, 1], backend_port));
            let make_svc = make_service_fn(|_conn| {
                async move {
                    Ok::<_, Infallible>(
                        service_fn(move |_req| {
                            async move {
                                delay_for(Duration::from_millis(rand::rngs::OsRng.gen_range(400, 500))).await;
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
        let durations_cloned = durations.clone();
        tokio::spawn(async move {
            let mut api = Api::http("127.0.0.1", backend_port, path.to_string()).unwrap();
            api.add_hook(Box::new(ResponseDurationHookFactory { durations: durations_cloned }));
            start_local_gateway(gw_port, vec![api]).await
        });
        wait_for_gateway(gw_port).await;
        let client = Client::new();
        let reqs: Vec<ResponseFuture> = (1..100_usize).into_iter().map(|_| {
            let url = Uri::from_str(format!("http://127.0.0.1:{}{}", gw_port, path).as_str()).unwrap();
            let req = Request::builder()
                .uri(url)
                .body(Body::empty())
                .unwrap();
            client.request(req)
        }).collect();
        futures::future::join_all(reqs).await;
        let mut dur = durations.lock().unwrap();
        dur.sort();
        for duration in dur.iter() {
            println!("duration is: {:?}", duration);
        }
    }

}