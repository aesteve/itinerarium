use hyper::{Body, Response};

pub(crate) async fn unwrap_body_as_str(resp: Response<Body>) -> String {
    String::from_utf8(hyper::body::to_bytes(resp.into_body()).await.unwrap().to_vec()).unwrap()
}