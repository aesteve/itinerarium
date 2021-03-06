#[cfg(test)]
mod tests {
    use crate::tests::{test_server, wait_for_gateway};
    use crate::conf::api::Api;
    use crate::gateway::start_local_gateway;
    use tokio::time::{Duration, sleep};
    use std::str::FromStr;
    use crate::handlers::{GlobalHandler, HandlerResponse};
    use hyper::{Client, Uri, Response, Request, Body, StatusCode};
    use std::time::Instant;
    use std::collections::VecDeque;
    use std::sync::{Mutex, Arc};
    use log::error;


    /// Global rate limiter (not a quota per user, a global threshold)
    /// Protecting the backend against too many requests
    #[derive(Debug, Clone)]
    pub struct RateLimiter {
        conf: RateLimiting,
        accesses: Arc<Mutex<VecDeque<Instant>>>
    }

    /// Conf. for a rate limiter
    #[derive(Debug, Clone)]
    struct RateLimiting {
        pub nb: usize,
        pub span: Duration
    }

    impl RateLimiter {
        pub fn new(nb: usize, span: Duration) -> Self {
            RateLimiter {
                conf: RateLimiting { nb, span },
                accesses: Arc::new(Mutex::new(VecDeque::with_capacity(nb)))
            }
        }
    }

    impl GlobalHandler for RateLimiter {
        fn handle_req(&self, _req: &mut Request<Body>) -> HandlerResponse {
            let now = Instant::now();
            let threshold = now - self.conf.span;
            match self.accesses.lock() {
                Err(e) => {
                    error!("Rate limiter couldn't count accesses {:?}", e);
                    HandlerResponse::Break(Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::empty()).unwrap())
                }
                Ok(mut q) => {
                    while let Some(access) = q.front() {
                        if access > &threshold {
                            break;
                        }
                        q.pop_front();
                    }
                    q.push_back(now);
                    if q.len() > self.conf.nb {
                        HandlerResponse::Break(Response::builder().status(StatusCode::TOO_MANY_REQUESTS).body(Body::empty()).unwrap())
                    } else {
                        HandlerResponse::Continue
                    }
                }
            }
        }

        fn handle_res(&self, _res: &mut Response<Body>) -> HandlerResponse {
            HandlerResponse::Continue
        }
    }

    #[tokio::test]
    async fn test_rate_limiter() {
        let gw_port = 8000;
        let backend_port = 8001;
        let prefix = "/limited";
        let span = Duration::from_secs(1);
        tokio::spawn(async move {
            test_server("Ok!!", backend_port).await
        });
        tokio::spawn(async move {
            let mut api = Api::http("127.0.0.1", backend_port, prefix.to_string()).unwrap();
            let limiter = RateLimiter::new(2, span);
            api.add_global_handler(Box::new(limiter));
            start_local_gateway(gw_port, vec![api]).await.unwrap();
        });
        wait_for_gateway(gw_port).await;
        let client = Client::new();
        let url = Uri::from_str(format!("http://127.0.0.1:{}{}", gw_port, prefix).as_str()).unwrap();
        assert_eq!(StatusCode::OK, client.get(url.clone()).await.unwrap().status());
        assert_eq!(StatusCode::OK, client.get(url.clone()).await.unwrap().status());
        assert_eq!(StatusCode::TOO_MANY_REQUESTS, client.get(url.clone()).await.unwrap().status());
        assert_eq!(StatusCode::TOO_MANY_REQUESTS, client.get(url.clone()).await.unwrap().status());
        assert_eq!(StatusCode::TOO_MANY_REQUESTS, client.get(url.clone()).await.unwrap().status());
        assert_eq!(StatusCode::TOO_MANY_REQUESTS, client.get(url.clone()).await.unwrap().status());
        assert_eq!(StatusCode::TOO_MANY_REQUESTS, client.get(url.clone()).await.unwrap().status());
        assert_eq!(StatusCode::TOO_MANY_REQUESTS, client.get(url.clone()).await.unwrap().status());
        sleep(span).await;
        assert_eq!(StatusCode::OK, client.get(url.clone()).await.unwrap().status());
        assert_eq!(StatusCode::OK, client.get(url.clone()).await.unwrap().status());
        assert_eq!(StatusCode::TOO_MANY_REQUESTS, client.get(url).await.unwrap().status());
    }


}
