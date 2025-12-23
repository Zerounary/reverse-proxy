use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufReader},
    path::Path,
    sync::Arc,
};

use axum_server::tls_rustls::RustlsConfig;
use rustls::{
    server::{ClientHello, ResolvesServerCert},
    sign::{any_supported_type, CertifiedKey},
    Certificate, PrivateKey, ServerConfig,
};
use rustls_pemfile::{certs, read_one, Item};

use crate::config::Config;

type DynError = Box<dyn std::error::Error + Send + Sync>;

pub fn build_rustls_config(config: &Config) -> Result<RustlsConfig, DynError> {
    let mut host_map: HashMap<String, Arc<CertifiedKey>> = HashMap::new();
    for (host, cert_path, key_path) in config.host_tls_entries() {
        match load_certified_key(cert_path.as_path(), key_path.as_path()) {
            Ok(cert) => {
                host_map.insert(host, Arc::new(cert));
            }
            Err(err) => {
                eprintln!(
                    "Failed to load TLS files for host ({}): {}",
                    cert_path.display(),
                    err
                );
            }
        }
    }

    let default_cert_path = config.resolved_ssl_cert_path();
    let default_key_path = config.resolved_ssl_key_path();
    let default_cert = match load_certified_key(default_cert_path.as_path(), default_key_path.as_path())
    {
        Ok(cert) => Arc::new(cert),
        Err(err) => {
            eprintln!(
                "Failed to load global TLS files (cert: {:?}, key: {:?}): {}",
                default_cert_path, default_key_path, err
            );
            if let Some(any_cert) = host_map.values().next() {
                eprintln!("Falling back to host-specific TLS certificate as default.");
                any_cert.clone()
            } else {
                return Err(err);
            }
        }
    };

    let resolver = Arc::new(HostCertResolver::new(default_cert, host_map));
    let server_config = ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_cert_resolver(resolver);

    Ok(RustlsConfig::from_config(Arc::new(server_config)))
}

struct HostCertResolver {
    default_cert: Arc<CertifiedKey>,
    host_map: HashMap<String, Arc<CertifiedKey>>,
}

impl HostCertResolver {
    fn new(default_cert: Arc<CertifiedKey>, host_map: HashMap<String, Arc<CertifiedKey>>) -> Self {
        Self {
            default_cert,
            host_map,
        }
    }
}

impl ResolvesServerCert for HostCertResolver {
    fn resolve(&self, client_hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        let name = client_hello.server_name().map(|s| s.to_ascii_lowercase());
        if let Some(name) = name {
            if let Some(cert) = self.host_map.get(&name) {
                return Some(cert.clone());
            }
        }
        Some(self.default_cert.clone())
    }
}

fn load_certified_key(cert_path: &Path, key_path: &Path) -> Result<CertifiedKey, DynError> {
    let cert_chain = load_cert_chain(cert_path)?;
    let private_key = load_private_key(key_path)?;
    let signing_key = any_supported_type(&private_key)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Unsupported private key type"))?;
    Ok(CertifiedKey::new(cert_chain, signing_key))
}

fn load_cert_chain(path: &Path) -> Result<Vec<Certificate>, DynError> {
    let mut reader = BufReader::new(File::open(path)?);
    let certs = certs(&mut reader)?
        .into_iter()
        .map(Certificate)
        .collect::<Vec<_>>();
    if certs.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Empty certificate chain").into());
    }
    Ok(certs)
}

fn load_private_key(path: &Path) -> Result<PrivateKey, DynError> {
    let mut reader = BufReader::new(File::open(path)?);
    while let Some(item) = read_one(&mut reader)? {
        match item {
            Item::PKCS8Key(key) | Item::RSAKey(key) | Item::ECKey(key) => {
                return Ok(PrivateKey(key))
            }
            _ => continue,
        }
    }
    Err(io::Error::new(io::ErrorKind::InvalidData, "No private key found").into())
}
