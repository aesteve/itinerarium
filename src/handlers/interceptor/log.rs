use hyper::{Request, Body, Response};
use log::{Level};
use crate::handlers::{HandlerResponse, GlobalHandler};
use crate::handlers::HandlerResponse::Continue;

#[derive(Debug, Clone)]
pub struct LogRequestInterceptor {
    pub level: Level,
}

impl GlobalHandler for LogRequestInterceptor {
    fn handle_req(&self, req: &mut Request<Body>) -> HandlerResponse {
        log::log!(self.level, "{:?}", req);
        Continue
    }

    fn handle_res(&self, _res: &mut Response<Body>) -> HandlerResponse {
        Continue
    }
}

#[derive(Debug, Clone)]
pub struct LogResponseInterceptor {
    pub level: Level,
}

impl GlobalHandler for LogResponseInterceptor {
    fn handle_req(&self, _req: &mut Request<Body>) -> HandlerResponse {
        Continue
    }

    fn handle_res(&self, req: &mut Response<Body>) -> HandlerResponse {
        log::log!(self.level, "{:?}", req);
        Continue
    }
}

#[cfg(test)]
mod tests {
    use crate::gateway::start_local_gateway;
    use hyper::{Client, Uri};
    use ::log::{Level, LevelFilter};
    use simple_logger::SimpleLogger;
    use std::str::FromStr;
    use crate::tests::{test_server, wait_for_gateway};
    use crate::conf::api::Api;
    use crate::handlers::interceptor::log::{LogRequestInterceptor, LogResponseInterceptor};

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
            api.add_global_handler(Box::new(LogRequestInterceptor { level: Level::Info }));
            api.add_global_handler(Box::new(LogResponseInterceptor { level: Level::Warn }));
            start_local_gateway(6000, vec![api]).await
        });
        wait_for_gateway(gw_port).await;
        let client = Client::new();
        let url = Uri::from_str(format!("http://127.0.0.1:{}{}", gw_port, path).as_str()).unwrap();
        let resp = client.get(url).await.unwrap();
        assert_eq!(200, resp.status());
    }

}