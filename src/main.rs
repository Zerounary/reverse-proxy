pub mod config;
pub mod log;

use axum::{
    http::{uri::Uri, Request, },
    Router, middleware::{self, Next}, response::IntoResponse,
};
use axum_server::tls_rustls::RustlsConfig;
use config::Config;
use hyper::{client::HttpConnector, Body, StatusCode, header::HOST, Version};
use hyper::Client;
use hyper_tls::HttpsConnector;
use std::{net::SocketAddr, path::PathBuf};
use clap::{Parser};

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
    let app = Router::new()
        .layer(middleware::from_fn(move |req, next| {
            proxy_http_reqs(req, next, httpclient.clone(), httpsclient.clone(), fn_config.clone())
        }));
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port.unwrap_or(80)));
    println!("http reverse proxy listening on {}", addr);
    for (domain, host) in &config.hosts {
        log_proxy(&format!("http://{}", &domain), &host.protocol, &host.ip, &host.port.to_string());
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
    let app = Router::new()
        .layer(middleware::from_fn(move |req, next| {
            proxy_https_reqs(req, next, httpclient.clone(), httpsclient.clone(), fn_config.clone())
        }));
    let addr = SocketAddr::from(([0, 0, 0, 0], config.ssl_port.unwrap_or(443)));
    
    let ssl_cfg = RustlsConfig::from_pem_file(
        PathBuf::from(config.ssl_cert_file.unwrap_or("./ssl/certificate.crt".to_string())), 
        PathBuf::from(config.ssl_key_file.unwrap_or("./ssl/private.pem".to_string())), 
    )
    .await
    .unwrap();

    println!("https reverse proxy listening on {}", addr);
    for (domain, host) in &config.hosts {
        log_proxy(&format!("https://{}", &domain), &host.protocol, &host.ip, &host.port.to_string());
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
    config: Config
) -> Result<impl IntoResponse, (StatusCode, String)> {
        let path = req.uri().path();
        let path_query = req
            .uri()
            .path_and_query()
            .map(|v| v.as_str())
            .unwrap_or(path);
        
        let host = if let Some(header_host) = req.headers().get(HOST){
            Some(header_host.to_str().unwrap())
        }else {
            req.uri().host()
        };

        if let Some(host) = host {
            let host = host.to_string();
            let host_config = config.hosts.get(&host);
            match host_config {
                Some(cfg) => {
                    let uri = format!("{}://{}:{}{}", cfg.protocol, cfg.ip, cfg.port, path_query);
                    *req.uri_mut() = Uri::try_from(uri).unwrap();
                    *req.version_mut() = Version::HTTP_11;
                    let res = match cfg.protocol.as_str() {
                        "https" => httpsclient.request(req).await.unwrap(),
                        _ =>  httpclient.request(req).await.unwrap(),
                    };
                    Ok(res)
                },
                None => {
                    Err((StatusCode::FAILED_DEPENDENCY, "Unkown `Host` in the headers".to_string()))
                },
            }
        }else {
            Err((StatusCode::FAILED_DEPENDENCY, "The `Host` does not exist in the headers".to_string()))
        }

}

async fn proxy_http_reqs(
    mut req: Request<Body>,
    _next: Next<Body>,
    httpclient: HttpClient,
    httpsclient: HttpsClient,
    config: Config
) -> Result<impl IntoResponse, (StatusCode, String)> {
        let path = req.uri().path();
        let path_query = req
            .uri()
            .path_and_query()
            .map(|v| v.as_str())
            .unwrap_or(path);
        
        let host = if let Some(header_host) = req.headers().get(HOST){
            Some(header_host.to_str().unwrap())
        }else {
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
                        _ =>  httpclient.request(req).await.unwrap(),
                    };
                    Ok(res)
                },
                None => {
                    Err((StatusCode::FAILED_DEPENDENCY, "Unkown `Host` in the headers".to_string()))
                },
            }
        }else {
            Err((StatusCode::FAILED_DEPENDENCY, "The `Host` does not exist in the headers".to_string()))
        }

}