pub mod config;
pub mod log;

use axum::{
    http::{uri::Uri, Request, },
    Router, middleware::{self, Next}, response::IntoResponse,
};
use config::Config;
use hyper::{client::HttpConnector, Body, StatusCode, header::HOST};
use std::net::SocketAddr;
use clap::{Parser};

use crate::{config::read_yaml_file, log::log_proxy};

type Client = hyper::client::Client<HttpConnector, Body>;

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

    let client = Client::new();

    let fn_config = config.clone();
    let app = Router::new()
        .layer(middleware::from_fn(move |req, next| {
            proxy_reqs(req, next, client.clone(), fn_config.clone())
        }));
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    println!("reverse proxy listening on {}", addr);
    for (domain, host) in &config.hosts {
        log_proxy(&domain, &host.protocol, &host.ip, &host.port.to_string());
    }
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn proxy_reqs(
    mut req: Request<Body>,
    _next: Next<Body>,
    client: Client,
    config: Config
) -> Result<impl IntoResponse, (StatusCode, String)> {
        let path = req.uri().path();
        let path_query = req
            .uri()
            .path_and_query()
            .map(|v| v.as_str())
            .unwrap_or(path);
        
        let host = req.headers().get(HOST);

        if let Some(host) = host {
            let host = host.to_str().unwrap().to_string();
            let host_config = config.hosts.get(&host);
            match host_config {
                Some(cfg) => {
                    let uri = format!("{}://{}:{}{}", cfg.protocol, cfg.ip, cfg.port, path_query);
                    *req.uri_mut() = Uri::try_from(uri).unwrap();
                    let res = client.request(req).await.unwrap();
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