pub mod config;
pub mod log;

use axum::{
    extract::{FromRequest, RequestParts, WebSocketUpgrade},
    http::{uri::Uri, Request, Response},
    middleware::{self, Next},
    response::IntoResponse,
    Router,
};
use axum_server::tls_rustls::RustlsConfig;
use clap::Parser;
use config::Config;
use futures_util::{future, pin_mut, SinkExt, StreamExt};
use hyper::Client;
use hyper::{client::HttpConnector, header, Body, StatusCode, Version};
use hyper_tls::HttpsConnector;
use std::{net::SocketAddr, path::PathBuf};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;

use base64::encode;
use sha1::{Digest, Sha1};

use crate::{config::read_yaml_file, log::log_proxy};

type HttpClient = hyper::client::Client<HttpConnector, Body>;
type HttpsClient = Client<HttpsConnector<HttpConnector>>;
extern crate pest;
#[macro_use]
extern crate pest_derive;

#[derive(clap::Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, value_parser, value_name = "YAML")]
    config: Option<String>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let yaml_path = args.config.unwrap_or("./config.yml".to_string());

    let config = read_yaml_file(&yaml_path);

    let httpclient = Client::new();
    let httpsclient = Client::builder().build::<_, hyper::Body>(HttpsConnector::new());

    if let Some(enable_ssl) = config.ssl {
        if enable_ssl {
            tokio::spawn(https_server(config.clone()));
        }
    }

    let fn_config = config.clone();
    let app = Router::new().layer(middleware::from_fn(move |req, next| {
        proxy_http_reqs(
            req,
            next,
            httpclient.clone(),
            httpsclient.clone(),
            fn_config.clone(),
        )
    }));
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port.unwrap_or(80)));
    println!("http reverse proxy listening on {}", addr);
    for (domain, host) in &config.hosts {
        log_proxy(
            &format!("http://{}", &domain),
            &host.protocol,
            &host.ip,
            &host.port.to_string(),
        );
    }
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn https_server(config: Config) {
    let httpclient = Client::new();
    let httpsclient = Client::builder().build::<_, hyper::Body>(HttpsConnector::new());

    let fn_config = config.clone();
    let app = Router::new().layer(middleware::from_fn(move |req, next| {
        proxy_https_reqs(
            req,
            next,
            httpclient.clone(),
            httpsclient.clone(),
            fn_config.clone(),
        )
    }));
    let addr = SocketAddr::from(([0, 0, 0, 0], config.ssl_port.unwrap_or(443)));

    let ssl_cfg = RustlsConfig::from_pem_file(
        PathBuf::from(
            config
                .ssl_cert_file
                .unwrap_or("./ssl/certificate.crt".to_string()),
        ),
        PathBuf::from(
            config
                .ssl_key_file
                .unwrap_or("./ssl/private.pem".to_string()),
        ),
    )
    .await
    .unwrap();

    println!("https reverse proxy listening on {}", addr);
    for (domain, host) in &config.hosts {
        log_proxy(
            &format!("https://{}", &domain),
            &host.protocol,
            &host.ip,
            &host.port.to_string(),
        );
    }
    axum_server::bind_rustls(addr, ssl_cfg)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn proxy_https_reqs(
    mut req: Request<Body>,
    _next: Next<Body>,
    httpclient: HttpClient,
    httpsclient: HttpsClient,
    config: Config,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let path = req.uri().path();
    let path_query = req
        .uri()
        .path_and_query()
        .map(|v| v.as_str())
        .unwrap_or(path);

    let host = if let Some(header_host) = req.headers().get(header::HOST) {
        Some(header_host.to_str().unwrap())
    } else {
        req.uri().host()
    };

    if let Some(host) = host {
        let host = host.to_string();
        let host_config = config.hosts.get(&host);
        match host_config {
            Some(cfg) => {
                let uri = format!("{}://{}:{}{}", cfg.protocol, cfg.ip, cfg.port, path_query);
                *req.uri_mut() = Uri::try_from(uri.clone()).unwrap();
                *req.version_mut() = Version::HTTP_11;
                let res = match cfg.protocol.as_str() {
                    "https" => httpsclient.request(req).await.unwrap(),
                    "http" => {
                        if (req.headers().get(header::UPGRADE) != None) {
                            websocket_proxy(uri, req).await
                        } else {
                            httpclient.request(req).await.unwrap()
                        }
                    }
                    _ => httpclient.request(req).await.unwrap(),
                };
                Ok(res)
            }
            None => Err((
                StatusCode::FAILED_DEPENDENCY,
                "Unkown `Host` in the headers".to_string(),
            )),
        }
    } else {
        Err((
            StatusCode::FAILED_DEPENDENCY,
            "The `Host` does not exist in the headers".to_string(),
        ))
    }
}

async fn websocket_proxy(uri: String, req: Request<Body>) -> Response<Body> {
    let uri = format!("ws{}", uri.clone().trim_start_matches("http"));
    // 升级连接到WebSocket
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
    // 将WebSocket消息转换为HTTP响应体

    // 代理转发
    ws.on_upgrade(|client| {
        handle_socket(client)
    });

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

async fn handle_socket(client: axum::extract::ws::WebSocket) {
    // 连接到目标 WebSocket 服务器
    let (mut server_socket, _) = connect_async("ws://127.0.0.1:90/ws/connect")
        .await
        .expect("Failed to connect to server");

    // 使用 tokio::select! 宏来同时处理来自客户端和服务器的消息
    let (mut client_sender, mut client_receiver) = client.split();
    let (mut server_sender, mut server_receiver) = server_socket.split();

    tokio::select! {
        biased;

        _ = async {
            while let Some(msg) = client_receiver.next().await {
                let msg = msg.expect("Failed to receive message from client");
                server_sender.send(Message::Text(msg.to_text().unwrap().to_string())).await.expect("Failed to send message to server");
            }
        } => {}
        _ = async {
            while let Some(msg) = server_receiver.next().await {
                let msg = msg.expect("Failed to receive message from server");
                client_sender.send(axum::extract::ws::Message::Text(msg.to_string())).await.expect("Failed to send message to client");
            }
        } => {}
    }
}

async fn proxy_http_reqs(
    mut req: Request<Body>,
    _next: Next<Body>,
    httpclient: HttpClient,
    httpsclient: HttpsClient,
    config: Config,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let path = req.uri().path();
    let path_query = req
        .uri()
        .path_and_query()
        .map(|v| v.as_str())
        .unwrap_or(path);

    let host = if let Some(header_host) = req.headers().get(header::HOST) {
        Some(header_host.to_str().unwrap())
    } else {
        req.uri().host()
    };

    if let Some(host) = host {
        let host = host.to_string();
        let host_config = config.hosts.get(&host);
        match host_config {
            Some(cfg) => {
                let uri = format!("{}://{}:{}{}", cfg.protocol, cfg.ip, cfg.port, path_query);
                *req.uri_mut() = Uri::try_from(uri).unwrap();
                let res = match cfg.protocol.as_str() {
                    "https" => httpsclient.request(req).await.unwrap(),
                    _ => httpclient.request(req).await.unwrap(),
                };
                Ok(res)
            }
            None => Err((
                StatusCode::FAILED_DEPENDENCY,
                "Unkown `Host` in the headers".to_string(),
            )),
        }
    } else {
        Err((
            StatusCode::FAILED_DEPENDENCY,
            "The `Host` does not exist in the headers".to_string(),
        ))
    }
}

fn generate_sec_websocket_accept(key: &str) -> String {
    let mut sha1 = Sha1::new();
    let combined = format!("{}{}", key, "258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    sha1.update(combined.as_bytes());
    let digest = sha1.finalize();
    encode(digest)
}
