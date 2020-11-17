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

#[async_trait]
pub trait ResponseTransformer: Send + Debug + Sync + DynClone {
    async fn transform(&self, res: Response<Body>) -> Response<Body>;
}

clone_trait_object!(Handler);
clone_trait_object!(ResponseTransformer);

#[cfg(test)]
mod tests {
    use crate::tests::{wait_for_gateway, test_server};
    use crate::handlers::{Handler, HandlerResponse};
    use hyper::{Client, Response, Request, Body, StatusCode, Uri};
    use crate::handlers::HandlerResponse::{Continue, Break};
    use crate::gateway::start_local_gateway;
    use crate::conf::api::Api;
    use std::str::FromStr;
    use hyper::header::{HeaderValue, HeaderName};
    use std::time::{SystemTime};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;


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
        let gw_port = 7102;
        let backend_port = 7103;
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
        let gw_port = 7104;
        let backend_port = 7105;
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

}