use std::string::ParseError;
use hyper::client::HttpConnector;
use hyper::{Client, Request, Body};
use hyper::http::uri::PathAndQuery;
use hyper_tls::HttpsConnector;


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
            // TODO: configure client according to endpoint conf (retry / timeout / protocol (HTTP2/HTTPS) etc.)
            client: Client::builder().build_http(),
        }))
    }

    pub fn https(address: &str) -> Result<Self, ParseError> {
        Ok(HttpEndpoint::Ssl(Endpoint {
            address: address.to_string(),
            // TODO: configure client according to endpoint conf (retry / timeout / protocol (HTTP2/HTTPS) etc.)
            client: Client::builder().build(HttpsConnector::new()),
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