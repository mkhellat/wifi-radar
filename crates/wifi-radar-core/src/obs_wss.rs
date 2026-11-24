// SPDX-FileCopyrightText: 2026 wifi-radar contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Optional TLS around the plaintext WebSocket observation pipe.
//!
//! [`crate::obs_ws`] stays the lab default (`ws://`, no TLS). This module
//! speaks the same NDJSON text frames on `wss://` using rustls. A
//! self-signed certificate is a valid lab trust anchor when the client
//! is given that certificate as its CA. There is no mutual-TLS prompt
//! and no ACME. Binary frames are still rejected by the plaintext reader.

use std::fs::File;
use std::io::BufReader;
use std::net::TcpStream;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use rustls::client::ClientConnection;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::server::ServerConnection;
use rustls::{ClientConfig, RootCertStore, ServerConfig, StreamOwned};

use crate::node_obs::NodeObservation;
use crate::obs_ws::{
    connect_client_tcp, read_observations_from_ws_server, require_host_port,
    write_observations_on_ws_client,
};

/// rustls server identity loaded from PEM files.
#[derive(Clone)]
pub struct WssServerConfig {
    inner: Arc<ServerConfig>,
}

/// rustls client trust anchors loaded from a PEM file.
///
/// For a lab self-signed server, pass the server certificate itself.
/// It is its own trust anchor.
#[derive(Clone)]
pub struct WssClientConfig {
    inner: Arc<ClientConfig>,
}

impl WssServerConfig {
    /// Load a certificate chain and a private key.
    ///
    /// `cert_pem` is one or more PEM certificates (leaf first). `key_pem`
    /// is a PKCS#8, PKCS#1, or SEC1 private key. The files are read once.
    pub fn from_pem_files(cert_pem: &Path, key_pem: &Path) -> Result<Self, String> {
        let certs = load_certs(cert_pem)?;
        let key = load_private_key(key_pem)?;
        let config = ServerConfig::builder_with_provider(ring_provider())
            .with_safe_default_protocol_versions()
            .map_err(|e| format!("tls versions: {e}"))?
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| format!("tls server identity: {e}"))?;
        Ok(Self {
            inner: Arc::new(config),
        })
    }
}

impl WssClientConfig {
    /// Trust every certificate in `ca_pem`.
    ///
    /// A self-signed server certificate PEM is enough for a lab peer.
    /// The name checked during the handshake is the `server_name`
    /// passed to [`write_observations_wss_client`], which must appear
    /// on that certificate (DNS name or IP address).
    pub fn from_ca_file(ca_pem: &Path) -> Result<Self, String> {
        let mut roots = RootCertStore::empty();
        for cert in load_certs(ca_pem)? {
            roots
                .add(cert)
                .map_err(|e| format!("tls trust anchor: {e}"))?;
        }
        let config = ClientConfig::builder_with_provider(ring_provider())
            .with_safe_default_protocol_versions()
            .map_err(|e| format!("tls versions: {e}"))?
            .with_root_certificates(roots)
            .with_no_client_auth();
        Ok(Self {
            inner: Arc::new(config),
        })
    }
}

/// Accept `stream` as TLS, then read NDJSON text frames until close.
///
/// The WebSocket handshake and the binary-frame rejection are the
/// plaintext reader. A binary frame fails the read.
pub fn read_observations_from_wss_server(
    stream: TcpStream,
    config: &WssServerConfig,
) -> Result<Vec<NodeObservation>, String> {
    let _ = stream.set_nodelay(true);
    let conn = ServerConnection::new(Arc::clone(&config.inner))
        .map_err(|e| format!("tls server connection: {e}"))?;
    let mut tls = StreamOwned::new(conn, stream);
    let batches = read_observations_from_ws_server(&mut tls)?;
    // The WebSocket close does not emit a TLS close_notify. Without one,
    // the peer's rustls reader treats TCP EOF as an error.
    shutdown_server(&mut tls);
    Ok(batches)
}

/// Connect to `addr`, verify TLS as `server_name`, send one text frame, and close.
///
/// `addr` is `HOST:PORT`, the same form as plaintext `--publish-ws`.
/// `server_name` is the DNS name or IP address on the certificate. It
/// is often the host part of `addr`. The `wss://` scheme is added here.
pub fn write_observation_wss_client(
    addr: &str,
    server_name: &str,
    config: &WssClientConfig,
    obs: &NodeObservation,
) -> Result<(), String> {
    write_observations_wss_client(addr, server_name, config, std::slice::from_ref(obs))
}

/// Connect to `addr`, verify TLS as `server_name`, and send one text frame per observation.
///
/// Frames go out on a single connection, in order. The socket is closed
/// after the last frame so the peer can finish the batch the way a TCP
/// reader finishes on EOF.
pub fn write_observations_wss_client(
    addr: &str,
    server_name: &str,
    config: &WssClientConfig,
    batches: &[NodeObservation],
) -> Result<(), String> {
    if batches.is_empty() {
        return Err("no observations to send".into());
    }
    require_host_port(addr)?;
    let name = server_name_from(server_name)?;
    let stream = connect_client_tcp(addr)?;
    let conn = ClientConnection::new(Arc::clone(&config.inner), name)
        .map_err(|e| format!("tls client connection: {e}"))?;
    let mut tls = StreamOwned::new(conn, stream);
    let url = format!("wss://{addr}");
    let result = write_observations_on_ws_client(&mut tls, &url, batches);
    shutdown_client(&mut tls);
    result
}

fn shutdown_server(tls: &mut StreamOwned<ServerConnection, TcpStream>) {
    tls.conn.send_close_notify();
    for _ in 0..8 {
        if !tls.conn.wants_write() {
            break;
        }
        if tls.conn.write_tls(&mut tls.sock).is_err() {
            break;
        }
    }
}

fn shutdown_client(tls: &mut StreamOwned<ClientConnection, TcpStream>) {
    tls.conn.send_close_notify();
    for _ in 0..8 {
        if !tls.conn.wants_write() {
            break;
        }
        if tls.conn.write_tls(&mut tls.sock).is_err() {
            break;
        }
    }
}

fn ring_provider() -> Arc<rustls::crypto::CryptoProvider> {
    static PROVIDER: OnceLock<Arc<rustls::crypto::CryptoProvider>> = OnceLock::new();
    PROVIDER
        .get_or_init(|| Arc::new(rustls::crypto::ring::default_provider()))
        .clone()
}

fn server_name_from(host: &str) -> Result<ServerName<'static>, String> {
    if host.is_empty() || host.contains("://") || host.contains('/') || host.contains(':') {
        return Err(format!(
            "TLS server name must be a DNS name or IP address (got {host})"
        ));
    }
    ServerName::try_from(host)
        .map(|name| name.to_owned())
        .map_err(|_| format!("TLS server name must be a DNS name or IP address (got {host})"))
}

fn load_certs(path: &Path) -> Result<Vec<CertificateDer<'static>>, String> {
    let file = File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut reader = BufReader::new(file);
    let certs = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("read certificates {}: {e}", path.display()))?;
    if certs.is_empty() {
        return Err(format!("no certificates in {}", path.display()));
    }
    Ok(certs)
}

fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>, String> {
    let file = File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut reader = BufReader::new(file);
    rustls_pemfile::private_key(&mut reader)
        .map_err(|e| format!("read private key {}: {e}", path.display()))?
        .ok_or_else(|| format!("no private key in {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::WifiDevice;
    use crate::node_obs::{DeviceObservation, NodePose};
    use std::net::TcpListener;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;
    use std::time::Duration;

    fn sample_obs(node_id: &str) -> NodeObservation {
        let mut device = WifiDevice::hotspot("aa:bb:cc:00:00:01", -50.0);
        device.ssid = "X".into();
        NodeObservation::new(
            node_id,
            1.0,
            2.0,
            NodePose::new(0.0, 0.0, 0.0, 0.0),
            vec![DeviceObservation::from_wifi_device(&device)],
        )
    }

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let id = N.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("wifi-radar-wss-{}-{id}", std::process::id()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write_cert(dir: &Path, name: &str) -> (PathBuf, PathBuf) {
        use std::net::{IpAddr, Ipv4Addr};

        use rcgen::{CertificateParams, KeyPair, SanType};

        let key = KeyPair::generate().unwrap();
        let mut params = CertificateParams::new(vec!["localhost".into()]).unwrap();
        params
            .subject_alt_names
            .push(SanType::IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        let cert = params.self_signed(&key).unwrap();
        let cert_path = dir.join(format!("{name}.pem"));
        let key_path = dir.join(format!("{name}.key.pem"));
        std::fs::write(&cert_path, cert.pem()).unwrap();
        std::fs::write(&key_path, key.serialize_pem()).unwrap();
        (cert_path, key_path)
    }

    fn serve(
        cert: PathBuf,
        key: PathBuf,
    ) -> (
        std::net::SocketAddr,
        thread::JoinHandle<Result<Vec<NodeObservation>, String>>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let config = WssServerConfig::from_pem_files(&cert, &key).unwrap();
            let (stream, _) = listener.accept().unwrap();
            read_observations_from_wss_server(stream, &config)
        });
        thread::sleep(Duration::from_millis(20));
        (addr, server)
    }

    #[test]
    fn wss_text_frame_roundtrip() {
        let dir = TempDir::new();
        let (cert, key) = write_cert(&dir.0, "server");
        let (addr, server) = serve(cert.clone(), key);
        let client = WssClientConfig::from_ca_file(&cert).unwrap();
        write_observation_wss_client(
            &addr.to_string(),
            "127.0.0.1",
            &client,
            &sample_obs("node-wss"),
        )
        .unwrap();
        let got = server.join().unwrap().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].node_id, "node-wss");
        assert_eq!(got[0].device_observations[0].mac, "aa:bb:cc:00:00:01");
    }

    #[test]
    fn wss_binary_frame_is_rejected() {
        let dir = TempDir::new();
        let (cert, key) = write_cert(&dir.0, "server");
        let (addr, server) = serve(cert.clone(), key);
        let client_cfg = WssClientConfig::from_ca_file(&cert).unwrap();
        let stream = connect_client_tcp(&addr.to_string()).unwrap();
        let name = server_name_from("127.0.0.1").unwrap();
        let conn = ClientConnection::new(Arc::clone(&client_cfg.inner), name).unwrap();
        let tls = StreamOwned::new(conn, stream);
        let url = format!("wss://{addr}");
        let (mut client, _) = tungstenite::client(url, tls).unwrap();
        client
            .send(tungstenite::Message::binary(vec![0u8, 1, 2]))
            .unwrap();
        let err = server.join().unwrap().unwrap_err();
        assert!(
            err.contains("binary"),
            "expected binary rejection, got {err}"
        );
        drop(client);
    }

    #[test]
    fn wss_rejects_a_different_trust_anchor() {
        let dir = TempDir::new();
        let (cert, key) = write_cert(&dir.0, "server");
        let (other, _) = write_cert(&dir.0, "other");
        let (addr, server) = serve(cert, key);
        let client = WssClientConfig::from_ca_file(&other).unwrap();
        let err = write_observation_wss_client(
            &addr.to_string(),
            "127.0.0.1",
            &client,
            &sample_obs("node-wss"),
        )
        .unwrap_err();
        let folded = err.to_ascii_lowercase();
        assert!(
            folded.contains("certificate")
                || folded.contains("handshake")
                || folded.contains("tls"),
            "expected a tls failure, got {err}"
        );
        let server_err = server.join().unwrap().unwrap_err();
        assert!(!server_err.is_empty());
    }

    #[test]
    fn host_port_and_server_name_required() {
        let dir = TempDir::new();
        let (cert, _) = write_cert(&dir.0, "server");
        let client = WssClientConfig::from_ca_file(&cert).unwrap();
        let obs = sample_obs("node-wss");
        assert!(
            write_observation_wss_client("wss://127.0.0.1:9", "127.0.0.1", &client, &obs).is_err()
        );
        assert!(write_observation_wss_client("127.0.0.1:9", "", &client, &obs).is_err());
        assert!(write_observation_wss_client("127.0.0.1:9", "bad name", &client, &obs).is_err());
    }
}
