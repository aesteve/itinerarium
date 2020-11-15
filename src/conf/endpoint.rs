use std::string::ParseError;
use hyper::client::HttpConnector;
use hyper::{Client};
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

}