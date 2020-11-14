use crate::conf::interceptor::{RequestInterceptor, ResponseInterceptor};
use hyper::{Request, Body, Response};
use log::{Level};

#[derive(Debug, Clone)]
pub(crate) struct LogRequestInterceptor {
    pub(crate) level: Level,
}

impl RequestInterceptor for LogRequestInterceptor {
    fn intercept(&self, req: &Request<Body>) {
        log::log!(self.level, "{:?}", req)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LogResponseInterceptor {
    pub(crate) level: Level,
}

impl ResponseInterceptor for LogResponseInterceptor {
    fn intercept(&self, req: &Response<Body>) {
        log::log!(self.level, "{:?}", req)
    }
}


#[cfg(test)]
mod tests {
    use crate::gateway::start_gateway;
    use crate::conf::api::Api;
    use hyper::{Client, Request, Body};
    use crate::tests::{wait_for, test_server};
    use crate::conf::interceptor::log::{LogRequestInterceptor, LogResponseInterceptor};
    use log::{Level, LevelFilter};
    use simple_logger::SimpleLogger;

    fn get_logged() -> Request<Body> {
        Request::builder().method("GET").uri("http://127.0.0.1:6000/logged").body(Body::empty()).unwrap()
    }

    #[tokio::test]
    async fn test_log_request() {
        SimpleLogger::new().with_level(LevelFilter::Info).init().unwrap();
        tokio::spawn(async move {
            test_server("request logged", 6001).await
        });
        tokio::spawn(async move {
            let mut api = Api::http("127.0.0.1", 6001, "/logged".to_string()).unwrap();
            api.req_interceptors = vec![Box::new(LogRequestInterceptor { level: Level::Info })];
            api.res_interceptors = vec![Box::new(LogResponseInterceptor { level: Level::Warn })];
            start_gateway(6000, vec![api]).await
        });
        let client = Client::new();
        wait_for(&client, get_logged).await;
        let resp = client.request(get_logged()).await.unwrap();
        assert_eq!(200, resp.status());
    }

}