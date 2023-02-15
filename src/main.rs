use axum::{
    http::{uri::Uri, Request, },
    Router, middleware::{self, Next}, response::IntoResponse,
};
use hyper::{client::HttpConnector, Body, StatusCode};
use std::net::SocketAddr;

type Client = hyper::client::Client<HttpConnector, Body>;

#[tokio::main]
async fn main() {
    let client = Client::new();

    let app = Router::new()
        .layer(middleware::from_fn(move |req, next| {
            proxy_reqs(req, next, client.clone())
        }));

    let addr = SocketAddr::from(([127, 0, 0, 1], 4000));
    println!("reverse proxy listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn proxy_reqs(
    mut req: Request<Body>,
    _next: Next<Body>,
    client: Client
) -> Result<impl IntoResponse, (StatusCode, String)> {
        let path = req.uri().path();
        let path_query = req
            .uri()
            .path_and_query()
            .map(|v| v.as_str())
            .unwrap_or(path);

        let uri = format!("http://127.0.0.1:80{}", path_query);
        *req.uri_mut() = Uri::try_from(uri).unwrap();
        let res = client.request(req).await.unwrap();
        Ok(res)
}