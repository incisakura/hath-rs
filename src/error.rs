use std::convert::Infallible;
use std::fmt;
use std::io;
use std::num::ParseIntError;
use std::str::Utf8Error;

use axum::body::Body;
use axum::response::IntoResponse;
use hyper::StatusCode;

// TODO: shell we sperate client and server error?
#[derive(Debug)]
pub enum Error {
    IO(std::io::Error),
    ParseInt(ParseIntError),

    Hyper(hyper::Error),
    OpenSSL(openssl::error::ErrorStack),
    SSL(openssl::ssl::Error),
    BadResponse,
    BadRequest,
    NotFound,

    UnsupportedProtocol,
    InvalidUri,

    IncompleteCertFile,

    Infallible,
}

impl Error {
    pub fn into_status(&self) -> StatusCode {
        match self {
            Error::BadRequest => StatusCode::BAD_REQUEST,
            Error::NotFound => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response<Body> {
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    }
}

impl std::error::Error for Error {
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;

        match self {
            IO(e) => e.fmt(f),
            ParseInt(e) => e.fmt(f),
            Hyper(e) => e.fmt(f),
            OpenSSL(e) => e.fmt(f),
            SSL(e) => e.fmt(f),
            _ => write!(f, "{:?}", self)
        }
    }
}

impl From<ParseIntError> for Error {
    fn from(value: ParseIntError) -> Self {
        Error::ParseInt(value)
    }
}

impl From<hyper::Error> for Error {
    fn from(value: hyper::Error) -> Self {
        Error::Hyper(value)
    }
}

impl From<Utf8Error> for Error {
    fn from(_: Utf8Error) -> Self {
        Error::BadResponse
    }
}

impl From<Infallible> for Error {
    fn from(_: Infallible) -> Self {
        Error::Infallible
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Error::IO(value)
    }
}

impl From<openssl::error::ErrorStack> for Error {
    fn from(value: openssl::error::ErrorStack) -> Self {
        Error::OpenSSL(value)
    }
}

impl From<openssl::ssl::Error> for Error {
    fn from(value: openssl::ssl::Error) -> Self {
        Error::SSL(value)
    }
}
