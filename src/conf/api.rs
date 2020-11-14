use crate::conf::endpoint::{Endpoints};
use std::string::ParseError;
use hyper::{Request, Body, Response, Error};
use hyper::http::uri::PathAndQuery;


#[derive(Debug, Clone)]
pub(crate) struct Api {
    pub(crate) endpoints: Vec<Endpoints>,
    pub(crate) prefix: String,
}

impl Api  {
    pub(crate) fn http(host: &str, port: u16, prefix: String) -> Result<Self, ParseError> {
        Ok(Api {
            endpoints: vec![Endpoints::http(host, port)?],
            prefix
        })
    }

    pub(crate) fn https(host: &str, prefix: String) -> Result<Self, ParseError> {
        Ok(Api {
            endpoints: vec![Endpoints::https(host)?],
            prefix
        })
    }

    pub(crate) async fn forward(&self, mut req: Request<Body>) -> Result<Response<Body>, Error> {
        match self.endpoint_for(&req) {
            Endpoints::Plain(e) => {
                self.mut_req(&mut req);
                e.client.clone().request(req).await
            },
            Endpoints::Ssl(e) => {
                self.mut_req(&mut req);
                e.client.clone().request(req).await
            }
        }
    }

    pub(crate) fn mut_req(&self, req: &mut Request<Body>) {
        // TODO: complete request mapping (applying filters/map/policies/...)
        // TODO: gateway headers (X-Forwarded-For, etc.)
        let uri_string = match self.endpoint_for(req) {
            Endpoints::Plain(e) => format!(
                "http://{}{}",
                e.address.clone(),
                build_path(req.uri().path_and_query(), self.prefix.len())
            ),
            Endpoints::Ssl(e) => format!(
                "https://{}{}",
                e.address.clone(),
                build_path(req.uri().path_and_query(), self.prefix.len())
            ),
        };
        let uri = uri_string.parse().unwrap();
        println!("Forwarding to {:?}", uri);
        *req.uri_mut() = uri;
    }

    pub(crate) fn matches(&self, req: &Request<Body>) -> bool {
        req.uri().path().starts_with(&self.prefix)
    }

    pub(crate) fn endpoint_for(&self, _req: &Request<Body>) -> &Endpoints {
        self.endpoints.get(0).unwrap() // TODO: decision tree (health checks, etc.)
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
