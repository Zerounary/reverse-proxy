pub mod config;
pub mod log;
pub mod proxy;

use axum_server::tls_rustls::RustlsConfig;
use clap::Parser;
use config::Config;
use std::{error::Error, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::{
    sync::{watch, RwLock},
    task::JoinHandle,
};

use crate::{
    config::{
        read_yaml_file, spawn_hot_reload_task, spawn_tls_watch_task, SharedConfig, TlsReloadSignal,
    },
    log::log_proxy,
    proxy::{build_http_router, build_https_router, create_http_client, create_https_client},
};

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

    let init_config = read_yaml_file(&yaml_path);
    let shared_config: SharedConfig = Arc::new(RwLock::new(init_config.clone()));
    let (tls_reload_tx, tls_reload_rx) = watch::channel(TlsReloadSignal::ConfigChanged);
    spawn_hot_reload_task(
        PathBuf::from(&yaml_path),
        shared_config.clone(),
        tls_reload_tx.clone(),
    );
    spawn_tls_watch_task(shared_config.clone(), tls_reload_tx.clone());

    let httpclient = create_http_client();
    let httpsclient = create_https_client();

    tokio::spawn(https_server_manager(shared_config.clone(), tls_reload_rx));

    let app = build_http_router(httpclient, httpsclient, shared_config.clone());
    let addr = SocketAddr::from(([0, 0, 0, 0], init_config.resolved_http_port()));
    println!("http reverse proxy listening on {}", addr);
    for (domain, host) in &init_config.hosts {
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

type DynError = Box<dyn Error + Send + Sync>;

async fn run_https_server(config: Config, shared_config: SharedConfig) -> Result<(), DynError> {
    let httpclient = create_http_client();
    let httpsclient = create_https_client();
    let app = build_https_router(httpclient, httpsclient, shared_config.clone());
    let addr = SocketAddr::from(([0, 0, 0, 0], config.resolved_ssl_port()));

    let ssl_cfg = RustlsConfig::from_pem_file(
        config.resolved_ssl_cert_path(),
        config.resolved_ssl_key_path(),
    )
    .await
    .map_err(|err| {
        eprintln!(
            "Failed to load TLS files (cert: {:?}, key: {:?}): {}",
            config.resolved_ssl_cert_path(),
            config.resolved_ssl_key_path(),
            err
        );
        Box::new(err) as DynError
    })?;

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
        .map_err(|err| {
            eprintln!("HTTPS server exited with error: {}", err);
            Box::new(err) as DynError
        })
}

async fn spawnable_https(config: Config, shared_config: SharedConfig) {
    if let Err(err) = run_https_server(config, shared_config).await {
        eprintln!("HTTPS server task terminated: {}", err);
    }
}

async fn https_server_manager(
    shared_config: SharedConfig,
    mut reload_rx: watch::Receiver<TlsReloadSignal>,
) {
    let mut https_handle: Option<JoinHandle<()>> = None;
    let mut last_signature: Option<(u16, PathBuf, PathBuf)> = None;
    let mut current_signal = *reload_rx.borrow();

    loop {
        let force_restart = matches!(current_signal, TlsReloadSignal::TlsArtifactChanged);
        let snapshot = shared_config.read().await.clone();
        let ssl_enabled = snapshot.ssl_enabled();
        let signature = (
            snapshot.resolved_ssl_port(),
            snapshot.resolved_ssl_cert_path(),
            snapshot.resolved_ssl_key_path(),
        );

        match (ssl_enabled, https_handle.is_some()) {
            (true, false) => {
                let handle = tokio::spawn(spawnable_https(snapshot.clone(), shared_config.clone()));
                https_handle = Some(handle);
                last_signature = Some(signature);
            }
            (true, true) => {
                if force_restart || last_signature.as_ref() != Some(&signature) {
                    if let Some(handle) = https_handle.take() {
                        handle.abort();
                    }
                    let handle =
                        tokio::spawn(spawnable_https(snapshot.clone(), shared_config.clone()));
                    https_handle = Some(handle);
                    last_signature = Some(signature);
                }
            }
            (false, true) => {
                if let Some(handle) = https_handle.take() {
                    handle.abort();
                }
                last_signature = None;
            }
            (false, false) => {}
        }

        match reload_rx.changed().await {
            Ok(()) => {
                current_signal = *reload_rx.borrow();
            }
            Err(_) => {
                if let Some(handle) = https_handle.take() {
                    handle.abort();
                }
                break;
            }
        };
    }
}
