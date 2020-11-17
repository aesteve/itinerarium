use crate::conf::endpoint::{HttpEndpoint};
use std::string::ParseError;
use hyper::{Request, Body, Response, Error};
use futures::{FutureExt, TryFutureExt};
use crate::handlers::{HandlerResponse, Handler, ResponseTransformer, Hook, HookFactory};
use crate::conf::endpoint::HttpEndpoint::{Ssl, Plain};

#[derive(Debug, Clone)]
pub struct Api {
    pub prefix: String,
    pub endpoints: Vec<HttpEndpoint>,
    pub handlers: Vec<Box<dyn Handler>>,
    pub transformer: Option<Box<dyn ResponseTransformer>>,
    pub hooks: Vec<Box<dyn HookFactory>>
}

impl Api  {
    pub fn http(host: &str, port: u16, prefix: String) -> Result<Self, ParseError> {
        Ok(Api {
            prefix,
            endpoints: vec![HttpEndpoint::http(host, port)?],
            handlers: vec![],
            transformer: None,
            hooks: vec![]
        })
    }

    pub fn https(host: &str, prefix: String) -> Result<Self, ParseError> {
        Ok(Api {
            prefix,
            endpoints: vec![HttpEndpoint::https(host)?],
            handlers: vec![],
            transformer: None,
            hooks: vec![]
        })
    }

    pub fn add_handler(&mut self, handler: Box<dyn Handler>) {
        self.handlers.push(handler);
    }

    pub fn set_transformer(&mut self, transformer: Box<dyn ResponseTransformer>) {
        self.transformer = Some(transformer);
    }

    pub fn add_hook(&mut self, hook: Box<dyn HookFactory>) {
        self.hooks.push(hook);
    }

    /// Proxies a request to the appropriate endpoint
    /// Invoking every handlers on request / response
    pub async fn proxy(&self, mut req: Request<Body>) -> Result<Response<Body>, Error> {
        let endpoint = self.endpoint_for(&req);
        let hooks_for_roundtrip: Vec<Box<dyn Hook>> = self.hooks.iter().map(|hf| hf.create()).collect();
        endpoint.target_req_uri(&self.prefix, &mut req);
        for handler in &self.handlers {
            if let HandlerResponse::Break(resp) = handler.handle_req(&mut req) {
                return Ok(resp)
            }
        }
        for hook in &hooks_for_roundtrip {
            hook.on_request(&req);
        }
        self.send(endpoint, req, &hooks_for_roundtrip).await
    }

    /// Sends the request to upstream and handles the response
    async fn send(&self, endpoint: &HttpEndpoint, req: Request<Body>, hooks: &[Box<dyn Hook>]) -> Result<Response<Body>, Error> {
        let before = Insta
        match endpoint {
            Ssl(e) => e.client.request(req),
            Plain(e) => e.client.request(req),
        }.map(|res| {
            if res.is_err() { return res }
            let mut resp = res.unwrap();
            for handler in &self.handlers {
                if let HandlerResponse::Break(overriden) = handler.handle_res(&mut resp) {
                    return Ok(overriden)
                }
            }
            Ok(resp)
        }).and_then(|res| async move {
            let resp = if let Some(transformer) = &self.transformer {
                transformer.clone().transform(res).await
            } else {
                res
            };
            for hook in hooks {
                hook.on_response(&resp);
            }
            Ok(resp)
        }).await
    }

    pub fn matches(&self, req: &Request<Body>) -> bool {
        req.uri().path().starts_with(&self.prefix)
    }

    pub fn endpoint_for(&self, _req: &Request<Body>) -> &HttpEndpoint {
        // TODO: decision tree (based on health checks, response times, etc.)
        self.endpoints.get(0).unwrap()
    }
}

