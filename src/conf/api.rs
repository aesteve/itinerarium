use crate::conf::endpoint::Endpoint;
use std::string::ParseError;
use hyper::{Request, Body};
use hyper::http::uri::PathAndQuery;

#[derive(Debug, Clone)]
pub(crate) struct Api {
    pub(crate) endpoints: Vec<Endpoint>,
    pub(crate) prefix: String,
}

impl Api {
    pub(crate) fn new(host: &str, port: u16, prefix: String) -> Result<Self, ParseError> {
        Ok(Api {
            endpoints: vec![Endpoint::new(host, port)?],
            prefix
        })
    }

    pub(crate) fn forward_mut(&self, req: &mut Request<Body>) {
        // TODO: complete request mapping (applying filters/map/policies/...)
        // TODO: gateway headers (X-Forwarded-For, etc.)
        let endpoint = self.best_endpoint(req);
        let uri_string = format!(
            "http://{}{}",
            endpoint.address.clone(),
            build_path(req.uri().path_and_query(), self.prefix.len())
        );
        let uri = uri_string.parse().unwrap();
        *req.uri_mut() = uri;
    }

    pub(crate) fn matches(&self, req: &Request<Body>) -> bool {
        req.uri().path().starts_with(&self.prefix)
    }

    pub(crate) fn best_endpoint(&self, _req: &Request<Body>) -> &Endpoint {
        self.endpoints.iter().next().unwrap() // TODO: decision tree (health checks, etc.)
    }
}

fn build_path(path: Option<&PathAndQuery>, from: usize) -> String {
    let full_path = path.map(|x| x.as_str()).unwrap_or("");
    full_path[from..].to_string()
}
