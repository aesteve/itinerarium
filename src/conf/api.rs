use crate::conf::endpoint::{HttpEndpoint};
use std::string::ParseError;
use hyper::{Request, Body, Response, Error};
use hyper::http::uri::PathAndQuery;
use futures::FutureExt;
use crate::conf::handlers::{HandlerResponse, Handler};

#[derive(Debug, Clone)]
pub struct Api {
    pub prefix: String,
    pub endpoints: Vec<HttpEndpoint>,
    pub handlers: Vec<Box<dyn Handler>>,
}

impl Api  {
    pub fn http(host: &str, port: u16, prefix: String) -> Result<Self, ParseError> {
        Ok(Api {
            prefix,
            endpoints: vec![HttpEndpoint::http(host, port)?],
            handlers: vec![],
        })
    }

    pub fn https(host: &str, prefix: String) -> Result<Self, ParseError> {
        Ok(Api {
            prefix,
            endpoints: vec![HttpEndpoint::https(host)?],
            handlers: vec![],
        })
    }

    pub fn register_handler(&mut self, handler: Box<dyn Handler>) {
        self.handlers.push(handler);
    }

    pub async fn forward(&self, mut req: Request<Body>) -> Result<Response<Body>, Error> {
        match self.endpoint_for(&req) {
            HttpEndpoint::Plain(e) => {
                self.mut_req(&mut req);
                for handler in &self.handlers {
                    if let HandlerResponse::Break(resp) = handler.handle_req(&mut req) {
                        return Ok(resp)
                    }
                }
                e.client
                    .request(req)
                    .map(|res| {
                        match res {
                            Ok(mut resp) => {
                                for handler in &self.handlers {
                                    if let HandlerResponse::Break(overriden) = handler.handle_res(&mut resp) {
                                        return Ok(overriden)
                                    }
                                }
                                Ok(resp)
                            },
                            error => error
                        }
                    }).await
            },
            HttpEndpoint::Ssl(e) => {
                self.mut_req(&mut req);
                for handler in &self.handlers {
                    if let HandlerResponse::Break(resp) = handler.handle_req(&mut req) {
                        return Ok(resp)
                    }
                }
                e.client
                    .request(req)
                    .map(|res| {
                        match res {
                            Ok(mut resp) => {
                                for handler in &self.handlers {
                                    if let HandlerResponse::Break(overriden) = handler.handle_res(&mut resp) {
                                        return Ok(overriden)
                                    }
                                }
                                Ok(resp)
                            },
                            error => error
                        }
                    }).await
            }
        }
    }

    pub fn mut_req(&self, req: &mut Request<Body>) {
        // TODO: complete request mapping (applying filters/map/policies/...)
        // TODO: gateway headers (X-Forwarded-For, etc.)
        let path = build_path(req.uri().path_and_query(), self.prefix.len());
        *req.uri_mut() = match self.endpoint_for(req) {
            HttpEndpoint::Plain(e) => format!(
                "http://{}{}",
                e.address.clone(),
                path
            ),
            HttpEndpoint::Ssl(e) => format!(
                "https://{}{}",
                e.address.clone(),
                path
            ),
        }.parse().unwrap();
    }

    pub fn matches(&self, req: &Request<Body>) -> bool {
        req.uri().path().starts_with(&self.prefix)
    }

    pub fn endpoint_for(&self, _req: &Request<Body>) -> &HttpEndpoint {
        // TODO: decision tree (based on health checks, response times, etc.)
        self.endpoints.get(0).unwrap()
    }
}

fn build_path(path: Option<&PathAndQuery>, from: usize) -> String {
    let full_path = path.map(|x| x.as_str()).unwrap_or("");
    if from > full_path.len() {
        "".to_string()
    } else {
        full_path[from..].to_string()
    }
}
