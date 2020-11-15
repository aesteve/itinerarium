use hyper::{Response, Body, Request};
use dyn_clone::{clone_trait_object, DynClone};
use std::fmt::Debug;

pub(crate) mod interceptor;
pub(crate) mod transformer;

pub(crate) enum HandlerResponse {
    Continue,
    Break(Response<Body>) // <-- breaks and returns the response
}


pub(crate) trait Handler: Send + Debug + Sync + DynClone {
    fn handle_req(&self, req: &mut Request<Body>) -> HandlerResponse;
    fn handle_res(&self, res: &mut Response<Body>) -> HandlerResponse;
}

clone_trait_object!(Handler);