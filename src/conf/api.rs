use crate::conf::endpoint::Endpoint;
use std::string::ParseError;

#[derive(Debug, Clone)]
pub(crate) struct Api {
    pub(crate) endpoint: Endpoint, // TODO: Vec<Endpoint>
    pub(crate) prefix: String,
}

impl Api {
    pub(crate) fn new(host: &str, port: u16, prefix: String) -> Result<Self, ParseError> {
        Ok(Api {
            endpoint: Endpoint::new(host, port)?,
            prefix
        })
    }
}