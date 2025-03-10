use hyper::body::Bytes;

use crate::Error;

pub type BoxBody<D = Bytes, E = Error> = http_body_util::combinators::BoxBody<D, E>;
