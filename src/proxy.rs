use axum::{
    extract::{ws::WebSocket, FromRequest, RequestParts, WebSocketUpgrade},
    http::{header, uri::Uri, Request, Response, StatusCode, Version},
    middleware::{self, Next},
    response::IntoResponse,
    Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use futures_util::{SinkExt, StreamExt};
use hyper::{
    client::{Client as HyperClient, HttpConnector},
    Body,
};
use hyper_tls::HttpsConnector;
use sha1::{Digest, Sha1};
use tokio_tungstenite::{
    connect_async,
    tungstenite::protocol::{frame::coding::CloseCode, CloseFrame, Message},
};

use crate::config::SharedConfig;

pub type HttpClient = HyperClient<HttpConnector, Body>;
pub type HttpsClient = HyperClient<HttpsConnector<HttpConnector>, Body>;

pub fn create_http_client() -> HttpClient {
    HyperClient::new()
}

pub fn create_https_client() -> HttpsClient {
    HyperClient::builder().build::<_, Body>(HttpsConnector::new())
}

pub fn build_http_router(
    httpclient: HttpClient,
    httpsclient: HttpsClient,
    shared_config: SharedConfig,
) -> Router {
    Router::new().layer(middleware::from_fn(move |req, next| {
        proxy_http_reqs(
            req,
            next,
            httpclient.clone(),
            httpsclient.clone(),
            shared_config.clone(),
        )
    }))
}

pub fn build_https_router(
    httpclient: HttpClient,
    httpsclient: HttpsClient,
    shared_config: SharedConfig,
) -> Router {
    Router::new().layer(middleware::from_fn(move |req, next| {
        proxy_https_reqs(
            req,
            next,
            httpclient.clone(),
            httpsclient.clone(),
            shared_config.clone(),
        )
    }))
}

async fn proxy_http_reqs(
    req: Request<Body>,
    _next: Next<Body>,
    httpclient: HttpClient,
    httpsclient: HttpsClient,
    shared_config: SharedConfig,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    proxy_request(req, httpclient, httpsclient, shared_config, false).await
}

async fn proxy_https_reqs(
    req: Request<Body>,
    _next: Next<Body>,
    httpclient: HttpClient,
    httpsclient: HttpsClient,
    shared_config: SharedConfig,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    proxy_request(req, httpclient, httpsclient, shared_config, true).await
}

async fn proxy_request(
    mut req: Request<Body>,
    httpclient: HttpClient,
    httpsclient: HttpsClient,
    shared_config: SharedConfig,
    force_http11: bool,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let path_query = req
        .uri()
        .path_and_query()
        .map(|v| v.as_str())
        .unwrap_or(req.uri().path());

    let host = extract_host(&req).ok_or((
        StatusCode::FAILED_DEPENDENCY,
        "The `Host` does not exist in the headers".to_string(),
    ))?;

    let host_config = {
        let config = shared_config.read().await;
        config.hosts.get(&host).cloned()
    }
    .ok_or((
        StatusCode::FAILED_DEPENDENCY,
        "Unknown `Host` in the headers".to_string(),
    ))?;

    let upstream_uri = format!(
        "{}://{}:{}{}",
        host_config.protocol, host_config.ip, host_config.port, path_query
    );
    *req.uri_mut() = Uri::try_from(upstream_uri.clone()).unwrap();

    if force_http11 {
        *req.version_mut() = Version::HTTP_11;
    }

    let response = match host_config.protocol.as_str() {
        "https" => httpsclient.request(req).await.unwrap(),
        "http" => {
            if has_upgrade_header(&req) {
                websocket_proxy(upstream_uri, req).await
            } else {
                httpclient.request(req).await.unwrap()
            }
        }
        _ => httpclient.request(req).await.unwrap(),
    };

    Ok(response)
}

async fn websocket_proxy(uri: String, req: Request<Body>) -> Response<Body> {
    let uri = format!("ws{}", uri.clone().trim_start_matches("http"));
    let mut req_parts = RequestParts::new(req);
    let key = req_parts
        .headers()
        .get(header::SEC_WEBSOCKET_KEY)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let ws = WebSocketUpgrade::from_request(&mut req_parts)
        .await
        .unwrap();

    ws.on_upgrade(|client| handle_socket(client, uri));

    Response::builder()
        .status(101)
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .header(
            header::SEC_WEBSOCKET_ACCEPT,
            &generate_sec_websocket_accept(&key),
        )
        .body(Body::empty())
        .unwrap()
}

async fn handle_socket(client: WebSocket, uri: String) {
    let (server_socket, _) = connect_async(uri)
        .await
        .expect("Failed to connect to server");

    let (mut client_sender, mut client_receiver) = client.split();
    let (mut server_sender, mut server_receiver) = server_socket.split();

    tokio::select! {
        biased;

        _ = async {
            while let Some(msg) = client_receiver.next().await {
                let msg = msg.expect("Failed to receive message from client");
                match msg {
                    axum::extract::ws::Message::Text(txt) => {
                        server_sender.send(Message::Text(txt)).await.expect("Failed to send message to server");
                    },
                    axum::extract::ws::Message::Binary(vec) => {
                        server_sender.send(Message::Binary(vec)).await.expect("Failed to send message to server");
                    },
                    axum::extract::ws::Message::Ping(vec) => {
                        server_sender.send(Message::Ping(vec)).await.expect("Failed to send message to server");
                    },
                    axum::extract::ws::Message::Pong(vec) => {
                        server_sender.send(Message::Pong(vec)).await.expect("Failed to send message to server");
                    },
                    axum::extract::ws::Message::Close(close_frame) => {
                        let cf = close_frame.map(|c| {
                            CloseFrame {
                                code: CloseCode::from(c.code),
                                reason: c.reason,
                            }
                        });
                        server_sender.send(Message::Close(cf)).await.expect("Failed to send message to server");
                    },
                }
            }
        } => {}
        _ = async {
            while let Some(msg) = server_receiver.next().await {
                let msg = msg.expect("Failed to receive message from server");
                use axum::extract::ws::Message::*;
                match msg {
                    Message::Text(txt) => {
                        client_sender.send(Text(txt)).await.expect("Failed to send message to client");
                    },
                    Message::Binary(vec) => {
                        client_sender.send(Binary(vec)).await.expect("Failed to send message to client");
                    },
                    Message::Ping(vec) => {
                        client_sender.send(Ping(vec)).await.expect("Failed to send message to client");
                    },
                    Message::Pong(vec) => {
                        client_sender.send(Pong(vec)).await.expect("Failed to send message to client");
                    },
                    Message::Close(close_frame) => {
                        let cf = close_frame.map(|c| {
                            axum::extract::ws::CloseFrame {
                                code: c.code.into(),
                                reason: c.reason
                            }
                        });
                        client_sender.send(Close(cf)).await.expect("Failed to send message to client");
                    },
                }
            }
        } => {}
    }
}

fn extract_host(req: &Request<Body>) -> Option<String> {
    req.headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| req.uri().host().map(|s| s.to_string()))
}

fn has_upgrade_header(req: &Request<Body>) -> bool {
    req.headers().get(header::UPGRADE).is_some()
}

fn generate_sec_websocket_accept(key: &str) -> String {
    let mut sha1 = Sha1::new();
    let combined = format!("{}{}", key, "258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    sha1.update(combined.as_bytes());
    let digest = sha1.finalize();
    BASE64_STANDARD.encode(digest)
}
