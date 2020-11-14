use std::net::SocketAddr;
use std::string::ParseError;

#[derive(Debug, Clone)]
pub(crate) struct Endpoint {
    pub(crate) address: SocketAddr,
    pub(crate) prefix: String,
    pub(crate) ssl: bool
}

impl Endpoint {

    pub(crate) fn new(host: &str, port: u16, prefix: String) -> Result<Self, ParseError> {
        let address: SocketAddr = format!("{}:{}", host, port).parse().unwrap();
        Ok(Endpoint {
            address,
            prefix,
            ssl: false
        })
    }

}