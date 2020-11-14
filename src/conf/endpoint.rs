use std::net::SocketAddr;
use std::string::ParseError;
use hyper::client::HttpConnector;
use hyper::Client;


#[derive(Debug, Clone)]
pub(crate) struct Endpoint {
    pub(crate) address: SocketAddr,
    pub(crate) ssl: bool,
    client: Client<HttpConnector>
}

impl Endpoint {

    pub(crate) fn new(host: &str, port: u16) -> Result<Self, ParseError> {
        let address: SocketAddr = format!("{}:{}", host, port).parse().unwrap();
        Ok(Endpoint {
            address,
            ssl: false,
            client: Client::builder()
                // TODO: configure client according to endpoint conf (retry / timeout / etc.)
                .build_http()
        })
    }

    pub(crate) fn client(&self) -> Client<HttpConnector> {
        self.client.clone()
    }

}