use hyper::{Response, Body, Request};
use dyn_clone::{clone_trait_object, DynClone};
use std::fmt::Debug;

pub(crate) mod interceptor;
pub(crate) mod transformer;

pub(crate) enum HandlerResponse {
    Continue,
    Break(Response<Body>) // <-- breaks and returns the response
}


pub(crate) trait Handler: Send + Debug + Sync + DynClone {
    fn handle_req(&self, req: &mut Request<Body>) -> HandlerResponse;
    fn handle_res(&self, res: &mut Response<Body>) -> HandlerResponse;
}

clone_trait_object!(Handler);


#[cfg(test)]
mod tests {
    use crate::tests::{wait_for_gateway};
    use crate::conf::handlers::{Handler, HandlerResponse};
    use hyper::{Client, Response, Request, Body, StatusCode, Uri};
    use crate::conf::handlers::HandlerResponse::{Continue, Break};
    use crate::gateway::start_gateway;
    use crate::conf::api::Api;
    use std::str::FromStr;


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
            api.register_handler(Box::new(BreakingHandler {}));
            start_gateway(gw_port, vec![api]).await
        });
        wait_for_gateway(gw_port).await;
        let client = Client::new();
        let url = Uri::from_str(format!("http://127.0.0.1:{}{}", gw_port, path).as_str()).unwrap();
        let resp = client.get(url).await.unwrap();
        assert_eq!(StatusCode::GONE, resp.status());
    }
}