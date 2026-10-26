//! TLS for `wss:` and `https:`: one rustls client config, with the `ring` provider set on it (never
//! as the process default, so a native crate's own provider is left alone) and ALPN `http/1.1`.
//! The engine's config verifies through the platform (`rustls-platform-verifier`); a test hands
//! its own authority with [`NetTls::with_roots`]. Nothing turns verification off.
//!
//! The platform's config is built at the first `wss:` or `https:` connection, not when the engine
//! starts: on Linux it reads every system certificate file, and fails on a machine with none. A
//! failure fails that connection and the ones after it, with the reason in the log; a game that
//! makes none never builds it.

use async_io::Async;
use bevy::tasks::futures_lite::{AsyncRead, AsyncWrite};
use futures_rustls::TlsConnector;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{CertificateError, ClientConfig, Error, RootCertStore};
use std::io;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll};

/// How a [`NetTls`] makes its config, the first time a connection needs it.
pub type BuildConfig = fn() -> Result<ClientConfig, String>;

/// The TLS config every `wss:` connection and `https:` fetch uses, built once, when the first
/// one needs it. Clones share it.
#[derive(Clone)]
pub struct NetTls {
    config: Arc<OnceLock<Result<Arc<ClientConfig>, String>>>,
    build: BuildConfig,
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn finish(mut config: ClientConfig) -> Arc<ClientConfig> {
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Arc::new(config)
}

/// The platform's verifier: the system trust store (and its revocation checks on macOS and
/// Windows), the system's certificate files on Linux.
#[cfg(not(target_os = "android"))]
fn platform() -> Result<ClientConfig, String> {
    let verifier = rustls_platform_verifier::Verifier::new(provider()).map_err(|e| format!("the system's certificates could not be loaded: {e}"))?;
    let config = ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("no TLS version to offer: {e}"))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    Ok(config)
}

/// On Android, Mozilla's roots (webpki-roots): the platform verifier needs the app's `Context`,
/// which an embedded engine is not given.
#[cfg(target_os = "android")]
fn platform() -> Result<ClientConfig, String> {
    let roots = RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
    let config = ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("no TLS version to offer: {e}"))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(config)
}

impl Default for NetTls {
    /// The platform's verifier, built at the first connection that needs it.
    fn default() -> Self {
        NetTls::built_by(platform)
    }
}

impl NetTls {
    /// A config `build` makes at the first connection that needs it; its error fails that
    /// connection and every later one. For tests of what a failure does.
    pub fn built_by(build: BuildConfig) -> Self {
        NetTls { config: Arc::new(OnceLock::new()), build }
    }

    /// Trusts only the authorities in `pem`, for tests: the platform's store is not read.
    pub fn with_roots(pem: &[u8]) -> Result<Self, String> {
        let mut roots = RootCertStore::empty();
        for cert in CertificateDer::pem_slice_iter(pem) {
            let cert = cert.map_err(|e| format!("a certificate that does not parse: {e}"))?;
            roots.add(cert).map_err(|e| format!("a root rustls refuses: {e}"))?;
        }
        if roots.is_empty() {
            return Err("no certificate in the PEM".into());
        }
        let config = ClientConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .map_err(|e| format!("no TLS version to offer: {e}"))?
            .with_root_certificates(roots)
            .with_no_client_auth();
        let tls = NetTls::built_by(platform);
        let _ = tls.config.set(Ok(finish(config)));
        Ok(tls)
    }

    /// The connector, building the config the first time; the build's error otherwise.
    pub fn connector(&self) -> Result<TlsConnector, String> {
        let config = self.config.get_or_init(|| (self.build)().map(finish));
        config.clone().map(TlsConnector::from)
    }
}

/// The URL's host as the name the certificate must carry: an IP address as an IP name.
pub fn server_name(host: &str) -> Result<ServerName<'static>, String> {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    ServerName::try_from(host.to_string()).map_err(|_| format!("{host} is not a valid server name"))
}

/// Why a TLS handshake failed, in plain words for the log.
pub fn reason(error: &std::io::Error, host: &str) -> String {
    let rustls = error.get_ref().and_then(|e| e.downcast_ref::<Error>());
    match rustls {
        Some(Error::InvalidCertificate(c)) => certificate_reason(c, host),
        Some(e) => format!("the TLS handshake failed: {e}"),
        None => format!("the TLS handshake failed: {error}"),
    }
}

fn certificate_reason(c: &CertificateError, host: &str) -> String {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    match c {
        CertificateError::UnknownIssuer => "the certificate was not trusted (its authority is not in the trust store)".into(),
        CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. } => format!("the certificate does not match {host}"),
        CertificateError::Expired | CertificateError::ExpiredContext { .. } => "the certificate has expired".into(),
        CertificateError::NotValidYet | CertificateError::NotValidYetContext { .. } => "the certificate is not valid yet".into(),
        CertificateError::Revoked => "the certificate was revoked".into(),
        other => format!("the certificate was not trusted: {other:?}"),
    }
}

/// A connection's byte stream: TCP, or TLS over it.
pub enum Stream {
    Plain(Async<TcpStream>),
    Tls(Box<futures_rustls::client::TlsStream<Async<TcpStream>>>),
}

/// Resolves `host` (off the main thread), connects to the first address that answers and, when
/// `tls` is given, handshakes with the host as server name. The error is the log's reason.
pub async fn connect(host: &str, port: u16, tls: Option<&NetTls>) -> Result<Stream, String> {
    // The config first: when it cannot be built, no socket opens.
    let connector = tls.map(NetTls::connector).transpose()?;
    let bare = host.trim_start_matches('[').trim_end_matches(']').to_string();
    let lookup = bare.clone();
    let addrs: Vec<SocketAddr> = blocking::unblock(move || (lookup.as_str(), port).to_socket_addrs().map(Iterator::collect))
        .await
        .map_err(|e| format!("the name did not resolve: {e}"))?;
    let mut last = None;
    let mut stream = None;
    for addr in addrs {
        match Async::<TcpStream>::connect(addr).await {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(e) => last = Some(e),
        }
    }
    let stream = stream.ok_or_else(|| match last {
        Some(e) => format!("the connection failed: {e}"),
        None => "the name resolved to no address".into(),
    })?;
    let Some(connector) = connector else { return Ok(Stream::Plain(stream)) };
    let name = server_name(&bare)?;
    let tls_stream = connector.connect(name, stream).await.map_err(|e| reason(&e, &bare))?;
    Ok(Stream::Tls(Box::new(tls_stream)))
}

impl AsyncRead for Stream {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_read(cx, buf),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_write(cx, buf),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_flush(cx),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_close(cx),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_close(cx),
        }
    }
}
