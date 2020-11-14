use crate::conf::endpoint::{HttpEndpoint};
use std::string::ParseError;
use hyper::{Request, Body, Response, Error};
use hyper::http::uri::PathAndQuery;


#[derive(Debug, Clone)]
pub(crate) struct Api {
    pub(crate) endpoints: Vec<HttpEndpoint>,
    pub(crate) prefix: String,
}

impl Api  {
    pub(crate) fn http(host: &str, port: u16, prefix: String) -> Result<Self, ParseError> {
        Ok(Api {
            endpoints: vec![HttpEndpoint::http(host, port)?],
            prefix
        })
    }

    pub(crate) fn https(host: &str, prefix: String) -> Result<Self, ParseError> {
        Ok(Api {
            endpoints: vec![HttpEndpoint::https(host)?],
            prefix
        })
    }

    pub(crate) async fn forward(&self, mut req: Request<Body>) -> Result<Response<Body>, Error> {
        match self.endpoint_for(&req) {
            HttpEndpoint::Plain(e) => {
                self.mut_req(&mut req);
                e.client.clone().request(req).await
            },
            HttpEndpoint::Ssl(e) => {
                self.mut_req(&mut req);
                e.client.clone().request(req).await
            }
        }
    }

    pub(crate) fn mut_req(&self, req: &mut Request<Body>) {
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

    pub(crate) fn matches(&self, req: &Request<Body>) -> bool {
        req.uri().path().starts_with(&self.prefix)
    }

    pub(crate) fn endpoint_for(&self, _req: &Request<Body>) -> &HttpEndpoint {
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
