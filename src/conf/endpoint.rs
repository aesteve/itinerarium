use std::string::ParseError;
use hyper::client::HttpConnector;
use hyper::{Client, Request, Body};
use hyper_tls::HttpsConnector;
use hyper::http::uri::PathAndQuery;


#[derive(Debug, Clone)]
pub enum HttpEndpoint {
    Plain(Endpoint<HttpConnector>),
    Ssl(Endpoint<HttpsConnector<HttpConnector>>)
}

#[derive(Debug, Clone)]
pub struct Endpoint<T> {
    pub address: String,
    pub client: Client<T>,
}

impl HttpEndpoint {

    pub fn http(host: &str, port: u16) -> Result<Self, ParseError> {
        Ok(HttpEndpoint::Plain(Endpoint {
            address: format!("{}:{}", host, port),
            client: Client::builder() // TODO: configure client according to endpoint conf (retry / timeout / etc.)
                .build_http(),
        }))
    }

    pub fn https(address: &str) -> Result<Self, ParseError> {
        let connector = HttpsConnector::new();
        // TODO: configure client according to endpoint conf (retry / timeout / etc.)
        let client = Client::builder()
        .build(connector);
        Ok(HttpEndpoint::Ssl(Endpoint {
            address: address.to_string(),
            client,
        }))
    }

    /// Changes the request URI to target this endpoint
    pub fn target_req_uri(&self, prefix: &str, req: &mut Request<Body>) {
        let path = build_path(req.uri().path_and_query(), prefix.len());
        *req.uri_mut() = match self {
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

}

fn build_path(path: Option<&PathAndQuery>, from: usize) -> &str {
    let full_path = path.map(PathAndQuery::as_str).unwrap_or("");
    if from > full_path.len() {
        ""
    } else {
        &full_path[from..]
    }
}