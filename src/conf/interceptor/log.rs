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
    use hyper::{Client, Uri};
    use crate::tests::{test_server, wait_for_gateway};
    use crate::conf::interceptor::log::{LogRequestInterceptor, LogResponseInterceptor};
    use log::{Level, LevelFilter};
    use simple_logger::SimpleLogger;
    use std::str::FromStr;

    #[tokio::test]
    async fn test_log_request() {
        let gw_port = 6000;
        let backend_port = 6001;
        let path = "/logged";
        SimpleLogger::new().with_level(LevelFilter::Info).init().unwrap();
        tokio::spawn(async move {
            test_server("request logged", backend_port).await
        });
        tokio::spawn(async move {
            let mut api = Api::http("127.0.0.1", backend_port, path.to_string()).unwrap();
            api.req_interceptors = vec![Box::new(LogRequestInterceptor { level: Level::Info })];
            api.res_interceptors = vec![Box::new(LogResponseInterceptor { level: Level::Warn })];
            start_gateway(6000, vec![api]).await
        });
        wait_for_gateway(gw_port).await;
        let client = Client::new();
        let url = Uri::from_str(format!("http://127.0.0.1:{}{}", gw_port, path).as_str()).unwrap();
        let resp = client.get(url).await.unwrap();
        assert_eq!(200, resp.status());
    }

}