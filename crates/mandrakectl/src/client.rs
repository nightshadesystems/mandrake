//! HTTP client over HTTPS or the root Unix socket.

use std::{fmt::Write as _, path::PathBuf, time::Duration};

use mandrake_core::Problem;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::cli::Cli;

/// Where the daemon's Unix socket lives by default.
#[cfg(unix)]
pub const DEFAULT_SOCKET: &str = "/var/run/mandrake/mandraked.sock";
const API_PREFIX: &str = "/api/v1";

/// Errors from configuring or talking to the daemon.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Bad flags or files.
    #[error("{0}")]
    Config(String),
    /// Could not reach or talk to the daemon.
    #[error("{0}")]
    Transport(String),
    /// The daemon answered with a problem.
    #[error("{0}")]
    Api(Box<Problem>),
    /// The daemon answered with something unexpected.
    #[error("unexpected response ({status}): {body}")]
    Unexpected {
        /// HTTP status.
        status: u16,
        /// Body, truncated.
        body: String,
    },
}

/// A raw response.
#[derive(Debug)]
pub struct Reply {
    /// HTTP status.
    pub status: u16,
    /// Body bytes.
    pub body: Vec<u8>,
}

enum Transport {
    Https {
        http: reqwest::Client,
        base: String,
    },
    #[cfg(unix)]
    Unix(PathBuf),
}

/// A configured client.
pub struct Client {
    transport: Transport,
    token: Option<String>,
}

fn token_from_default_file() -> Option<String> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)?;
    std::fs::read_to_string(home.join(".config/mandrake/token"))
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

#[cfg(unix)]
async fn socket_connectable(path: &std::path::Path) -> bool {
    tokio::net::UnixStream::connect(path).await.is_ok()
}

impl Client {
    /// Pick a transport and credentials from the flags.
    pub async fn connect(cli: &Cli) -> Result<Self, Error> {
        let token = match (&cli.token, &cli.token_file) {
            (Some(t), _) => Some(t.clone()),
            (None, Some(f)) => Some(
                tokio::fs::read_to_string(f)
                    .await
                    .map_err(|e| Error::Config(format!("{}: {e}", f.display())))?
                    .trim()
                    .to_owned(),
            ),
            (None, None) => token_from_default_file(),
        };

        #[cfg(unix)]
        {
            if let Some(path) = &cli.socket {
                return Ok(Self {
                    transport: Transport::Unix(path.clone()),
                    token,
                });
            }
            if cli.server.is_none() && token.is_none() {
                let default = PathBuf::from(DEFAULT_SOCKET);
                if socket_connectable(&default).await {
                    return Ok(Self {
                        transport: Transport::Unix(default),
                        token: None,
                    });
                }
            }
        }
        #[cfg(not(unix))]
        if cli.socket.is_some() {
            return Err(Error::Config(
                "--socket is only available on Unix hosts".to_owned(),
            ));
        }

        let base = cli
            .server
            .clone()
            .unwrap_or_else(|| "https://localhost".to_owned())
            .trim_end_matches('/')
            .to_owned();
        if !base.starts_with("https://") && !base.starts_with("http://") {
            return Err(Error::Config(format!(
                "--server must be a URL, got `{base}`"
            )));
        }
        let http = https_client(cli)?;
        Ok(Self {
            transport: Transport::Https { http, base },
            token,
        })
    }

    /// One API call. `path` is relative to `/api/v1`.
    pub async fn call(
        &self,
        method: &str,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> Result<Reply, Error> {
        match &self.transport {
            Transport::Https { http, base } => {
                let mut url = format!("{base}{API_PREFIX}{path}");
                if !query.is_empty() {
                    url.push('?');
                    url.push_str(&encode_query(query));
                }
                let method = reqwest::Method::from_bytes(method.as_bytes())
                    .map_err(|e| Error::Config(e.to_string()))?;
                let mut req = http.request(method, &url);
                if let Some(t) = &self.token {
                    req = req.bearer_auth(t);
                }
                if let Some(b) = body {
                    req = req.json(b);
                }
                let resp = req
                    .send()
                    .await
                    .map_err(|e| Error::Transport(describe(&e)))?;
                let status = resp.status().as_u16();
                let body = resp
                    .bytes()
                    .await
                    .map_err(|e| Error::Transport(e.to_string()))?
                    .to_vec();
                Ok(Reply { status, body })
            }
            #[cfg(unix)]
            Transport::Unix(socket) => {
                let mut target = format!("{API_PREFIX}{path}");
                if !query.is_empty() {
                    target.push('?');
                    target.push_str(&encode_query(query));
                }
                let bytes = body.map(|b| b.to_string().into_bytes());
                unix::call(socket, method, &target, self.token.as_deref(), bytes).await
            }
        }
    }

    /// Call and decode a JSON body, turning problems into [`Error::Api`].
    pub async fn json<T: DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> Result<T, Error> {
        let reply = self.call(method, path, query, body).await?;
        check(&reply)?;
        serde_json::from_slice(&reply.body).map_err(|e| Error::Unexpected {
            status: reply.status,
            body: format!("{e}: {}", snippet(&reply.body)),
        })
    }

    /// Call expecting no body (204).
    pub async fn empty(&self, method: &str, path: &str, body: Option<&Value>) -> Result<(), Error> {
        let reply = self.call(method, path, &[], body).await?;
        check(&reply)
    }
}

/// Turn a non-2xx reply into an error.
pub fn check(reply: &Reply) -> Result<(), Error> {
    if (200..300).contains(&reply.status) {
        return Ok(());
    }
    if let Ok(problem) = serde_json::from_slice::<Problem>(&reply.body) {
        return Err(Error::Api(Box::new(problem)));
    }
    Err(Error::Unexpected {
        status: reply.status,
        body: snippet(&reply.body),
    })
}

fn snippet(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    let trimmed = text.trim();
    if trimmed.len() > 200 {
        format!("{}...", &trimmed[..200])
    } else {
        trimmed.to_owned()
    }
}

fn describe(e: &reqwest::Error) -> String {
    let mut out = e.to_string();
    let mut source = std::error::Error::source(e);
    while let Some(s) = source {
        out.push_str(": ");
        out.push_str(&s.to_string());
        source = s.source();
    }
    if out.contains("certificate") || out.contains("UnknownIssuer") {
        out.push_str(
            "\nhint: pass --fingerprint <sha256 printed by mandraked>, --ca <pem>, or --insecure",
        );
    }
    out
}

/// Percent-encode a query string.
pub fn encode_query(query: &[(&str, String)]) -> String {
    fn enc(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for b in s.bytes() {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                out.push(char::from(b));
            } else {
                let _ = write!(out, "%{b:02X}");
            }
        }
        out
    }
    query
        .iter()
        .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn https_client(cli: &Cli) -> Result<reqwest::Client, Error> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(cli.timeout))
        .user_agent(concat!("mandrakectl/", env!("CARGO_PKG_VERSION")));
    if let Some(fp) = &cli.fingerprint {
        let expected = parse_fingerprint(fp)?;
        builder = builder.use_preconfigured_tls(pin::config(expected)?);
    } else {
        if cli.insecure {
            builder = builder.danger_accept_invalid_certs(true);
        }
        if let Some(ca) = &cli.ca {
            let pem =
                std::fs::read(ca).map_err(|e| Error::Config(format!("{}: {e}", ca.display())))?;
            let cert = reqwest::Certificate::from_pem(&pem)
                .map_err(|e| Error::Config(format!("{}: {e}", ca.display())))?;
            builder = builder.add_root_certificate(cert);
        }
    }
    builder.build().map_err(|e| Error::Config(e.to_string()))
}

/// Parse `AA:BB:...` or `aabb...` into 32 bytes.
pub fn parse_fingerprint(s: &str) -> Result<Vec<u8>, Error> {
    let hex: String = s
        .chars()
        .filter(|c| *c != ':' && !c.is_whitespace())
        .collect();
    if hex.len() != 64 {
        return Err(Error::Config(
            "--fingerprint must be a SHA-256 (64 hex digits)".to_owned(),
        ));
    }
    (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| Error::Config("--fingerprint is not hex".to_owned()))
}

mod pin {
    //! A rustls verifier that trusts exactly one certificate by fingerprint.

    use std::sync::Arc;

    use rustls::{
        DigitallySignedStruct, SignatureScheme,
        client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
        crypto::CryptoProvider,
        pki_types::{CertificateDer, ServerName, UnixTime},
    };
    use sha2::{Digest, Sha256};

    use super::Error;

    #[derive(Debug)]
    struct Pinned {
        expected: Vec<u8>,
        provider: Arc<CryptoProvider>,
    }

    impl ServerCertVerifier for Pinned {
        fn verify_server_cert(
            &self,
            end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp_response: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, rustls::Error> {
            if Sha256::digest(end_entity.as_ref()).as_slice() == self.expected.as_slice() {
                Ok(ServerCertVerified::assertion())
            } else {
                Err(rustls::Error::General(
                    "server certificate does not match --fingerprint".to_owned(),
                ))
            }
        }

        fn verify_tls12_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            rustls::crypto::verify_tls12_signature(
                message,
                cert,
                dss,
                &self.provider.signature_verification_algorithms,
            )
        }

        fn verify_tls13_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            rustls::crypto::verify_tls13_signature(
                message,
                cert,
                dss,
                &self.provider.signature_verification_algorithms,
            )
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            self.provider
                .signature_verification_algorithms
                .supported_schemes()
        }
    }

    /// A client config trusting only the certificate with `expected`.
    pub fn config(expected: Vec<u8>) -> Result<rustls::ClientConfig, Error> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = Arc::new(Pinned {
            expected,
            provider: Arc::clone(&provider),
        });
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| Error::Config(e.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(verifier)
            .with_no_client_auth();
        Ok(config)
    }
}

#[cfg(unix)]
mod unix {
    //! One HTTP/1.1 request per connection over the Unix socket.

    use std::{
        convert::Infallible,
        path::Path,
        pin::Pin,
        task::{Context, Poll},
    };

    use hyper::body::{Body, Bytes, Frame, Incoming, SizeHint};
    use hyper_util::rt::TokioIo;
    use tokio::net::UnixStream;

    use super::{Error, Reply};

    struct Once(Option<Bytes>);

    impl Body for Once {
        type Data = Bytes;
        type Error = Infallible;

        fn poll_frame(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
            Poll::Ready(self.0.take().map(|b| Ok(Frame::data(b))))
        }

        fn is_end_stream(&self) -> bool {
            self.0.is_none()
        }

        fn size_hint(&self) -> SizeHint {
            SizeHint::with_exact(self.0.as_ref().map_or(0, |b| b.len() as u64))
        }
    }

    async fn collect(mut body: Incoming) -> Result<Vec<u8>, Error> {
        let mut out = Vec::new();
        while let Some(frame) = std::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)).await
        {
            let frame = frame.map_err(|e| Error::Transport(e.to_string()))?;
            if let Ok(data) = frame.into_data() {
                out.extend_from_slice(&data);
            }
        }
        Ok(out)
    }

    pub async fn call(
        socket: &Path,
        method: &str,
        target: &str,
        token: Option<&str>,
        body: Option<Vec<u8>>,
    ) -> Result<Reply, Error> {
        let stream = UnixStream::connect(socket)
            .await
            .map_err(|e| Error::Transport(format!("{}: {e}", socket.display())))?;
        let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .map_err(|e| Error::Transport(e.to_string()))?;
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let mut req = hyper::Request::builder()
            .method(method)
            .uri(target)
            .header("host", "mandraked")
            .header("accept", "application/json");
        if body.is_some() {
            req = req.header("content-type", "application/json");
        }
        if let Some(t) = token {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        let req = req
            .body(Once(body.map(Bytes::from)))
            .map_err(|e| Error::Transport(e.to_string()))?;
        let resp = sender
            .send_request(req)
            .await
            .map_err(|e| Error::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        let body = collect(resp.into_body()).await?;
        Ok(Reply { status, body })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_encoding() {
        let q = encode_query(&[
            ("since", "2026-09-01T00:00:00Z".to_owned()),
            ("action", "user.create".to_owned()),
        ]);
        assert_eq!(q, "since=2026-09-01T00%3A00%3A00Z&action=user.create");
    }

    #[test]
    fn fingerprint_parsing() {
        let fp = "35:47:1B:0B:99:8C:89:89:C7:D2:88:6C:77:E9:4E:B4:DC:38:E8:0A:31:8C:DB:73:12:68:F0:86:93:1C:53:48";
        let bytes = parse_fingerprint(fp).unwrap_or_default();
        assert_eq!(bytes.len(), 32);
        assert_eq!(bytes[0], 0x35);
        assert!(parse_fingerprint("abc").is_err());
    }
}
