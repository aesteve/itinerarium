use crate::gateway::{start_gateway};
use crate::conf::api::Api;
use hyper::Error;

pub(crate) mod conf;
pub(crate) mod utils;
pub(crate) mod gateway;

#[tokio::main]
async fn main() -> Result<(), Error> {
    start_gateway(
        1234,
        vec![Api::https("swapi.dev", "/swapi".to_string()).unwrap()]
    ).await
}
