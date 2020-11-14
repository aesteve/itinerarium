use hyper::{Body, Request, Response};
use std::fmt::Debug;
use dyn_clone::{clone_trait_object, DynClone};

pub(crate) mod log;

pub(crate) trait RequestInterceptor: Send + Debug + Sync + DynClone {
    fn intercept(&self, req: &Request<Body>);
}

clone_trait_object!(RequestInterceptor);

pub(crate) trait ResponseInterceptor: Send + Debug + Sync + DynClone {
    fn intercept(&self, req: &Response<Body>);
}

clone_trait_object!(ResponseInterceptor);
