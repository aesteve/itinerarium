use hyper::{Body, Response};
use std::string::FromUtf8Error;

#[derive(Debug)]
pub(crate) enum BodyReadError {
    EncodingError(FromUtf8Error),
    BodyError(hyper::Error)
}

pub(crate) async fn body_as_str(resp: Response<Body>) -> Result<String, BodyReadError> {
    hyper::body::to_bytes(resp.into_body())
        .await
        .map(|b| b.to_vec())
        .map_err(BodyReadError::BodyError)
        .and_then(|bytes| String::from_utf8(bytes).map_err(BodyReadError::EncodingError))
}

pub(crate) async fn unwrap_body_as_str(resp: Response<Body>) -> String {
    String::from_utf8(hyper::body::to_bytes(resp.into_body()).await.unwrap().to_vec()).unwrap()
}