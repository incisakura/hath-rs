use std::convert::Infallible;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{ready, Context, Poll};

use axum::body::Body;
use axum::response::Response;
use axum::routing::future::RouteFuture;
use axum::Router;
use axum::extract::Request;
use hyper::body::Incoming;

pub(super) struct ServerService {
    app: Router,
    remote: SocketAddr,
}

impl ServerService {
    pub fn new(app: Router, remote: SocketAddr) -> Self {
        ServerService { app, remote }
    }
}

impl hyper::service::Service<Request<Incoming>> for ServerService {
    type Response = Response<Body>;
    type Error = std::convert::Infallible;
    type Future = ServiceFuture;

    fn call(&self, request: Request<Incoming>) -> Self::Future {
        let app = self.app.clone();
        ServiceFuture::new(self.remote, request, app)
    }
}

pub(super) struct ServiceFuture {
    remote: SocketAddr,
    version: http::Version,
    method: http::Method,
    uri: http::Uri,
    headers: http::HeaderMap,
    inner: RouteFuture<Infallible>,
}

impl ServiceFuture {
    pub fn new(remote: SocketAddr, request: Request<Incoming>, mut service: Router) -> Self {
        use tower::Service;
    
        let version = request.version();
        let method = request.method().clone();
        let uri = request.uri().clone();
        let headers = request.headers().clone();
        let inner = service.call(request);
        ServiceFuture { remote, version, method, uri, headers, inner }
    }
}

impl Future for ServiceFuture {
    type Output = Result<Response<Body>, Infallible>;
    
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use hyper::body::Body;
        use http::header::{REFERER, USER_AGENT};

        let this = self.get_mut();
        let res = ready!(Pin::new(&mut this.inner).poll(cx))?;
        log::info!(
            r#"{} "{} {} {:?}" {} {} "{}" "{}""#,
            this.remote,
            this.method,
            this.uri.path_and_query().map_or("/", |x| x.as_str()),
            this.version,
            res.status().as_str(),
            res.size_hint().exact().unwrap_or(0),
            this.headers.get(REFERER).and_then(|x| x.to_str().ok()).unwrap_or(""),
            this.headers.get(USER_AGENT).and_then(|x| x.to_str().ok()).unwrap_or(""),
        );
        Poll::Ready(Ok(res))
    }
}
