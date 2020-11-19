use crate::conf::endpoint::{HttpEndpoint};
use std::string::ParseError;
use hyper::{Request, Body, Response, Error};
use futures::{FutureExt, TryFutureExt};
use crate::handlers::{HandlerResponse, GlobalHandler, ResponseFinalizer, ScopedHandler, ScopedHandlerFactory};
use crate::conf::endpoint::HttpEndpoint::{Ssl, Plain};

#[derive(Debug)]
pub struct Api {
    pub prefix: String,
    pub endpoints: Vec<HttpEndpoint>,
    pub global_handlers: Vec<Box<dyn GlobalHandler>>,
    pub finalizer: Option<Box<dyn ResponseFinalizer>>,
    pub scoped_handlers: Vec<Box<dyn ScopedHandlerFactory>>
}

impl Api  {

    pub fn http(host: &str, port: u16, prefix: String) -> Result<Self, ParseError> {
        Ok(Api {
            prefix,
            endpoints: vec![HttpEndpoint::http(host, port)?],
            global_handlers: vec![],
            finalizer: None,
            scoped_handlers: vec![]
        })
    }

    pub fn https(host: &str, prefix: String) -> Result<Self, ParseError> {
        Ok(Api {
            prefix,
            endpoints: vec![HttpEndpoint::https(host)?],
            global_handlers: vec![],
            finalizer: None,
            scoped_handlers: vec![]
        })
    }

    pub fn add_global_handler(&mut self, handler: Box<dyn GlobalHandler>) {
        self.global_handlers.push(handler);
    }

    pub fn add_scoped_handler(&mut self, handler: Box<dyn ScopedHandlerFactory>) {
        self.scoped_handlers.push(handler);
    }

    pub fn finalize_with(&mut self, finalizer: Box<dyn ResponseFinalizer>) {
        self.finalizer = Some(finalizer);
    }


    /// Proxies a request to the appropriate endpoint
    /// Invoking every handlers on request / response
    pub async fn proxy(&self, mut req: Request<Body>) -> Result<Response<Body>, Error> {
        let endpoint = self.endpoint_for(&req);
        let hooks_for_roundtrip: Vec<Box<dyn ScopedHandler>> = self.scoped_handlers.iter().map(|hf| hf.create()).collect();
        endpoint.target_req_uri(&self.prefix, &mut req);
        for handler in &self.global_handlers {
            if let HandlerResponse::Break(resp) = handler.handle_req(&mut req) {
                return Ok(resp)
            }
        }
        for hook in &hooks_for_roundtrip {
            hook.handle_req(&mut req);
        }
        self.send(endpoint, req, &hooks_for_roundtrip).await
    }

    /// Sends the request to upstream and handles the response
    async fn send(&self, endpoint: &HttpEndpoint, req: Request<Body>, hooks: &[Box<dyn ScopedHandler>]) -> Result<Response<Body>, Error> {
        match endpoint {
            Ssl(e) => e.client.request(req),
            Plain(e) => e.client.request(req),
        }.map(|res| {
            if res.is_err() { return res }
            let mut resp = res.unwrap();
            for handler in &self.global_handlers {
                if let HandlerResponse::Break(overriden) = handler.handle_res(&mut resp) {
                    return Ok(overriden)
                }
            }
            Ok(resp)
        }).and_then(|res| async move {
            let mut resp = if let Some(transformer) = &self.finalizer {
                transformer.transform(res).await
            } else {
                res
            };
            for hook in hooks {
                hook.handle_res(&mut resp);
            }
            Ok(resp)
        }).await
    }

    pub fn matches(&self, req: &Request<Body>) -> bool {
        req.uri().path().starts_with(&self.prefix)
    }

    pub fn endpoint_for(&self, _req: &Request<Body>) -> &HttpEndpoint {
        // TODO: decision tree (based on health checks, response times, etc.)
        // How to deal with multiple ? Should this be moved to another trait?
        self.endpoints.get(0).unwrap()
    }
}

