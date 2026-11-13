//! One rustls configuration (ring; webpki roots plus the OS trust store) for IMAP, SMTP and HTTPS.

use std::sync::{Arc, OnceLock};

use rustls::ClientConfig;
use rustls::pki_types::CertificateDer;

/// The roots the OS trusts besides the bundled ones, such as a self-hosted server's private CA:
/// the system store on Windows and macOS; `SSL_CERT_FILE`/`SSL_CERT_DIR` or the distribution's
/// bundle on Linux; on Android the CAs the user installed, which the app exports to a file it
/// names in `SSL_CERT_FILE`. Read once per process; certificates rustls cannot use are skipped.
pub(crate) fn os_roots() -> &'static [CertificateDer<'static>] {
    static ROOTS: OnceLock<Vec<CertificateDer<'static>>> = OnceLock::new();
    ROOTS.get_or_init(|| {
        let found = rustls_native_certs::load_native_certs();
        for e in &found.errors {
            tracing::warn!("system certificates: {e}");
        }
        let mut check = rustls::RootCertStore::empty();
        found
            .certs
            .into_iter()
            .filter(|c| check.add(c.clone()).is_ok())
            .collect()
    })
}

pub(crate) fn client_config() -> Arc<ClientConfig> {
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let mut roots = rustls::RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            roots.add_parsable_certificates(os_roots().iter().cloned());
            let builder = ClientConfig::builder_with_provider(provider())
                .with_safe_default_protocol_versions()
                .expect("ring supports the default TLS versions");
            #[cfg(feature = "insecure-test-tls")]
            if insecure() {
                return Arc::new(
                    builder
                        .dangerous()
                        .with_custom_certificate_verifier(Arc::new(test::AcceptAny))
                        .with_no_client_auth(),
                );
            }
            Arc::new(builder.with_root_certificates(roots).with_no_client_auth())
        })
        .clone()
}

pub(crate) fn http_config() -> ClientConfig {
    let mut config = (*client_config()).clone();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    config
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

#[cfg(feature = "insecure-test-tls")]
static INSECURE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Accept any server certificate for the rest of the process. Must be called before the first
/// connection. Only exists with the test-only feature `insecure-test-tls`.
#[cfg(feature = "insecure-test-tls")]
pub fn accept_invalid_certs_for_tests() {
    INSECURE.store(true, std::sync::atomic::Ordering::SeqCst);
}

pub(crate) fn insecure() -> bool {
    #[cfg(feature = "insecure-test-tls")]
    return INSECURE.load(std::sync::atomic::Ordering::SeqCst);
    #[cfg(not(feature = "insecure-test-tls"))]
    false
}

#[cfg(feature = "insecure-test-tls")]
mod test {
    use rustls::DigitallySignedStruct;
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};

    #[derive(Debug)]
    pub struct AcceptAny;

    impl ServerCertVerifier for AcceptAny {
        fn verify_server_cert(
            &self,
            _: &CertificateDer<'_>,
            _: &[CertificateDer<'_>],
            _: &ServerName<'_>,
            _: &[u8],
            _: UnixTime,
        ) -> Result<ServerCertVerified, rustls::Error> {
            Ok(ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _: &[u8],
            _: &CertificateDer<'_>,
            _: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _: &[u8],
            _: &CertificateDer<'_>,
            _: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            rustls::crypto::ring::default_provider()
                .signature_verification_algorithms
                .supported_schemes()
        }
    }
}

pub(crate) fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .tls_backend_preconfigured(http_config())
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(concat!("nsh-mail/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("HTTP client with a preconfigured rustls config")
}
