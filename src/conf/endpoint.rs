use std::string::ParseError;
use hyper::client::HttpConnector;
use hyper::{Client};
use hyper_tls::HttpsConnector;


#[derive(Debug, Clone)]
pub(crate) enum Endpoints {
    Plain(Endpoint<HttpConnector>),
    Ssl(Endpoint<HttpsConnector<HttpConnector>>)
}

#[derive(Debug, Clone)]
pub(crate) struct Endpoint<T> {
    pub(crate) address: String,
    pub(crate) ssl: bool,
    pub(crate) client: Client<T>,
}

impl Endpoints {

    pub(crate) fn http(host: &str, port: u16) -> Result<Self, ParseError> {
        Ok(Endpoints::Plain(Endpoint {
            address: format!("{}:{}", host, port),
            ssl: false,
            client: Client::builder()
                .build_http(), // TODO: configure client according to endpoint conf (retry / timeout / etc.)
        }))
    }

    pub(crate) fn https(address: &str) -> Result<Self, ParseError> {
        let connector = HttpsConnector::new();
        let client = Client::builder()
        .build(connector);
        Ok(Endpoints::Ssl(Endpoint {
            address: address.to_string(),
            ssl: true,
            client, // TODO: configure client according to endpoint conf (retry / timeout / etc.)

        }))
    }

}