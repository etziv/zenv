//! Local HTTPS: a certificate authority owned by this Zenv installation and
//! the per-site certificates it issues.
//!
//! - The CA is created only on an explicit request, and trusted only on a
//!   second explicit request (Windows then shows its own confirmation).
//! - It is name-constrained to local suffixes (`.test`, `.localhost`, …), so
//!   even a leaked key cannot sign certificates for real domains.
//! - Its folder is restricted to the current Windows user.
//! - Site certificates are reissued automatically when they are missing,
//!   close to expiry, cover different names or come from another CA.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rcgen::{
    BasicConstraints, CertificateParams, CidrSubnet, DistinguishedName, DnType, ExtendedKeyUsagePurpose, GeneralSubtree,
    IsCa, Issuer, KeyPair, KeyUsagePurpose, NameConstraints,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime};

use crate::error::{AppError, AppResult, IoContext};
use crate::paths::AppPaths;
use crate::platform;

/// Suffixes the CA may issue for. HTTPS sites must use one of them. Only
/// names no real network uses: a stolen CA key must not be able to vouch for
/// a router (.lan) or a company's intranet (.internal).
pub const TLS_SUFFIXES: &[&str] = &["test", "localhost", "example"];

const CA_YEARS: i64 = 10;
/// Under Apple's 398-day limit for TLS server certificates.
const LEAF_DAYS: i64 = 397;
/// Reissue when fewer days than this remain.
const RENEW_DAYS: i64 = 30;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaStatus {
    pub exists: bool,
    pub cert_path: PathBuf,
    pub subject: Option<String>,
    pub sha256: Option<String>,
    pub not_after: Option<String>,
    /// In the current user's Trusted Root store (what Zenv manages).
    pub trusted_user: bool,
    /// In the machine-wide Trusted Root store (added by someone else).
    pub trusted_machine: bool,
    pub suffixes: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertFiles {
    pub cert: PathBuf,
    pub key: PathBuf,
}

pub struct Tls {
    dir: PathBuf,
    /// Serializes issuing (several web servers may apply sites at once).
    lock: Mutex<()>,
}

/// Can the CA issue for this domain?
pub fn covers(domain: &str) -> bool {
    TLS_SUFFIXES.iter().any(|s| domain == *s || domain.ends_with(&format!(".{s}")))
}

fn pem_der(pem_text: &str) -> AppResult<Vec<u8>> {
    let (_, pem) = x509_parser::pem::parse_x509_pem(pem_text.as_bytes())
        .map_err(|e| AppError::Validation(format!("unreadable certificate: {e}")))?;
    Ok(pem.contents)
}

fn rcgen_err(e: rcgen::Error) -> AppError {
    AppError::Platform(format!("certificate generation failed: {e}"))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// File stem for a site certificate: the domain, which is already validated
/// to letters, digits, dots and hyphens.
fn stem(domain: &str) -> AppResult<&str> {
    if domain.is_empty() || !domain.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
        return Err(AppError::Validation(format!("'{domain}' cannot be used as a certificate name")));
    }
    Ok(domain)
}

impl Tls {
    pub fn new(paths: &AppPaths) -> Tls {
        Tls { dir: paths.generated_dir.join("tls"), lock: Mutex::new(()) }
    }

    pub fn ca_cert_path(&self) -> PathBuf {
        self.dir.join("zenv-ca.pem")
    }

    fn ca_key_path(&self) -> PathBuf {
        self.dir.join("zenv-ca-key.pem")
    }

    fn sites_dir(&self) -> PathBuf {
        self.dir.join("sites")
    }

    pub fn has_ca(&self) -> bool {
        self.ca_cert_path().is_file() && self.ca_key_path().is_file()
    }

    fn ca_der(&self) -> AppResult<Vec<u8>> {
        let text = std::fs::read_to_string(self.ca_cert_path()).ctx(|| "reading the Zenv CA certificate".into())?;
        pem_der(&text)
    }

    pub fn status(&self) -> AppResult<CaStatus> {
        let mut status = CaStatus {
            exists: self.has_ca(),
            cert_path: self.ca_cert_path(),
            subject: None,
            sha256: None,
            not_after: None,
            trusted_user: false,
            trusted_machine: false,
            suffixes: TLS_SUFFIXES.to_vec(),
        };
        if !status.exists {
            return Ok(status);
        }
        let der = self.ca_der()?;
        let (_, cert) = x509_parser::parse_x509_certificate(&der)
            .map_err(|e| AppError::Validation(format!("unreadable Zenv CA certificate: {e}")))?;
        status.subject = Some(cert.subject().to_string());
        status.not_after = cert.validity().not_after.to_datetime().format(&time::format_description::well_known::Rfc3339).ok();
        status.sha256 = Some(hex(&Sha256::digest(&der)));
        let trust = platform::root_trust(&der)?;
        status.trusted_user = trust.current_user;
        status.trusted_machine = trust.local_machine;
        Ok(status)
    }

    /// Creates the CA. Never replaces an existing one.
    pub fn create_ca(&self) -> AppResult<CaStatus> {
        let _guard = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        if self.ca_cert_path().exists() || self.ca_key_path().exists() {
            return Err(AppError::Conflict("a Zenv certificate authority already exists".into()));
        }
        std::fs::create_dir_all(&self.dir).ctx(|| format!("creating {}", self.dir.display()))?;
        platform::restrict_to_current_user(&self.dir)?;

        let key = KeyPair::generate().map_err(rcgen_err)?;
        let params = ca_params()?;
        let cert = params.self_signed(&key).map_err(rcgen_err)?;
        let key_path = self.ca_key_path();
        std::fs::write(&key_path, key.serialize_pem()).ctx(|| format!("writing {}", key_path.display()))?;
        std::fs::write(self.ca_cert_path(), cert.pem()).ctx(|| "writing the Zenv CA certificate".into())?;
        tracing::info!(path = %self.ca_cert_path().display(), "created the Zenv certificate authority");
        drop(_guard);
        self.status()
    }

    /// Adds the CA to the current user's trusted roots (Windows asks first).
    pub fn trust(&self) -> AppResult<CaStatus> {
        if !self.has_ca() {
            return Err(AppError::Conflict("create the Zenv certificate authority first".into()));
        }
        platform::trust_root(&self.ca_der()?)?;
        tracing::info!("the Zenv certificate authority was added to the current user's trusted roots");
        self.status()
    }

    /// Removes the CA from the current user's trusted roots (Windows asks first).
    pub fn untrust(&self) -> AppResult<CaStatus> {
        if self.has_ca() && platform::untrust_root(&self.ca_der()?)? {
            tracing::info!("the Zenv certificate authority was removed from the current user's trusted roots");
        }
        self.status()
    }

    /// Certificate for `names` (first = primary), issued or reissued as needed.
    pub fn ensure_cert(&self, names: &[String]) -> AppResult<CertFiles> {
        let primary = names.first().ok_or_else(|| AppError::Validation("a certificate needs at least one name".into()))?;
        let dir = self.sites_dir();
        let files = CertFiles { cert: dir.join(format!("{}.pem", stem(primary)?)), key: dir.join(format!("{}-key.pem", stem(primary)?)) };
        let _guard = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        if !self.has_ca() {
            return Err(AppError::Conflict(
                "HTTPS needs Zenv's local certificate authority; create it in Settings → HTTPS".into(),
            ));
        }
        for name in names.iter().filter(|n| n.parse::<std::net::IpAddr>().is_err()) {
            if !covers(name) {
                return Err(AppError::Validation(format!(
                    "HTTPS is available for domains ending in .{}; '{name}' is not one of them",
                    TLS_SUFFIXES.join(", .")
                )));
            }
        }
        let ca_der = self.ca_der()?;
        if files.key.is_file() && self.is_current(&files.cert, names, &ca_der) {
            return Ok(files);
        }

        let ca_key_pem = std::fs::read_to_string(self.ca_key_path()).ctx(|| "reading the Zenv CA key".into())?;
        let ca_key = KeyPair::from_pem(&ca_key_pem).map_err(rcgen_err)?;
        let ca_pem = std::fs::read_to_string(self.ca_cert_path()).ctx(|| "reading the Zenv CA certificate".into())?;
        let issuer = Issuer::from_ca_cert_pem(&ca_pem, &ca_key).map_err(rcgen_err)?;

        let mut params = CertificateParams::new(names.to_vec()).map_err(rcgen_err)?;
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, primary.as_str());
        dn.push(DnType::OrganizationName, "Zenv local development");
        params.distinguished_name = dn;
        let now = OffsetDateTime::now_utc();
        params.not_before = now - Duration::days(1);
        params.not_after = now + Duration::days(LEAF_DAYS);
        params.is_ca = IsCa::ExplicitNoCa;
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        params.use_authority_key_identifier_extension = true;
        let key = KeyPair::generate().map_err(rcgen_err)?;
        let cert = params.signed_by(&key, &issuer).map_err(rcgen_err)?;

        std::fs::create_dir_all(&dir).ctx(|| format!("creating {}", dir.display()))?;
        std::fs::write(&files.key, key.serialize_pem()).ctx(|| format!("writing {}", files.key.display()))?;
        std::fs::write(&files.cert, cert.pem()).ctx(|| format!("writing {}", files.cert.display()))?;
        tracing::info!(names = ?names, "issued a site certificate");
        Ok(files)
    }

    /// Valid for a while yet, for exactly these names, signed by this CA.
    fn is_current(&self, cert_path: &Path, names: &[String], ca_der: &[u8]) -> bool {
        let Ok(text) = std::fs::read_to_string(cert_path) else { return false };
        let Ok(der) = pem_der(&text) else { return false };
        let Ok((_, cert)) = x509_parser::parse_x509_certificate(&der) else { return false };
        let Ok((_, ca)) = x509_parser::parse_x509_certificate(ca_der) else { return false };
        let remaining = cert.validity().not_after.timestamp() - OffsetDateTime::now_utc().unix_timestamp();
        if remaining < RENEW_DAYS * 86_400 || cert.verify_signature(Some(ca.public_key())).is_err() {
            return false;
        }
        let mut have: Vec<String> = cert
            .subject_alternative_name()
            .ok()
            .flatten()
            .map(|san| {
                san.value
                    .general_names
                    .iter()
                    .filter_map(|n| match n {
                        x509_parser::extensions::GeneralName::DNSName(d) => Some(d.to_string()),
                        x509_parser::extensions::GeneralName::IPAddress(b) => ip_from_bytes(b),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut want: Vec<String> = names.iter().map(|n| n.parse::<std::net::IpAddr>().map(|ip| ip.to_string()).unwrap_or_else(|_| n.clone())).collect();
        have.sort();
        want.sort();
        have == want
    }
}

fn ip_from_bytes(b: &[u8]) -> Option<String> {
    match b.len() {
        4 => Some(std::net::Ipv4Addr::from(<[u8; 4]>::try_from(b).ok()?).to_string()),
        16 => Some(std::net::Ipv6Addr::from(<[u8; 16]>::try_from(b).ok()?).to_string()),
        _ => None,
    }
}

fn ca_params() -> AppResult<CertificateParams> {
    let mut params = CertificateParams::new(Vec::<String>::new()).map_err(rcgen_err)?;
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "user".into());
    let host = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "computer".into());
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, format!("Zenv Local CA ({user}@{host})"));
    dn.push(DnType::OrganizationName, "Zenv local development");
    params.distinguished_name = dn;
    let now = OffsetDateTime::now_utc();
    params.not_before = now - Duration::days(1);
    params.not_after = now + Duration::days(365 * CA_YEARS);
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign, KeyUsagePurpose::DigitalSignature];
    // Windows and browsers apply a CA's EKU to everything it issues: website
    // certificates only, never code signing or e-mail.
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let mut permitted: Vec<GeneralSubtree> = TLS_SUFFIXES.iter().map(|s| GeneralSubtree::DnsName((*s).to_string())).collect();
    permitted.push(GeneralSubtree::IpAddress(CidrSubnet::from_v4_prefix([127, 0, 0, 0], 8)));
    let mut v6 = [0u8; 16];
    v6[15] = 1;
    permitted.push(GeneralSubtree::IpAddress(CidrSubnet::from_v6_prefix(v6, 128)));
    params.name_constraints = Some(NameConstraints { permitted_subtrees: permitted, excluded_subtrees: vec![] });
    Ok(params)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tls() -> (tempfile::TempDir, Tls) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        (dir, Tls::new(&paths))
    }

    #[test]
    fn issues_name_constrained_certificates_and_reuses_them() {
        let (_d, tls) = tls();
        assert!(matches!(tls.ensure_cert(&["blog.test".into()]), Err(AppError::Conflict(_))), "no CA yet");
        let status = tls.create_ca().unwrap();
        assert!(status.exists && status.sha256.as_deref().is_some_and(|s| s.len() == 64));
        assert!(matches!(tls.create_ca(), Err(AppError::Conflict(_))), "never replaces the CA");

        let names = vec!["blog.test".to_string()];
        let files = tls.ensure_cert(&names).unwrap();
        let first = std::fs::read_to_string(&files.cert).unwrap();
        assert_eq!(tls.ensure_cert(&names).unwrap(), files);
        assert_eq!(std::fs::read_to_string(&files.cert).unwrap(), first, "a current certificate is reused");

        // Different names → reissued.
        let both = vec!["blog.test".to_string(), "www.blog.test".to_string()];
        tls.ensure_cert(&both).unwrap();
        assert_ne!(std::fs::read_to_string(&files.cert).unwrap(), first);

        let der = pem_der(&std::fs::read_to_string(&files.cert).unwrap()).unwrap();
        let (_, leaf) = x509_parser::parse_x509_certificate(&der).unwrap();
        let ca_der = tls.ca_der().unwrap();
        let (_, ca) = x509_parser::parse_x509_certificate(&ca_der).unwrap();
        assert!(leaf.verify_signature(Some(ca.public_key())).is_ok());
        assert!(ca.name_constraints().unwrap().is_some(), "the CA is name-constrained");
        let eku = ca.extended_key_usage().unwrap().expect("the CA limits what it vouches for").value;
        assert!(eku.server_auth && !eku.code_signing && !eku.email_protection && !eku.any);
        assert!(!leaf.is_ca());
        let days = (leaf.validity().not_after.timestamp() - OffsetDateTime::now_utc().unix_timestamp()) / 86_400;
        assert!((390..=LEAF_DAYS).contains(&days), "{days}");
    }

    #[test]
    fn refuses_names_outside_local_suffixes() {
        let (_d, tls) = tls();
        tls.create_ca().unwrap();
        assert!(covers("shop.test") && covers("api.localhost") && !covers("example.com") && !covers("testing"));
        assert!(!covers("router.lan") && !covers("wiki.corp.internal"), "real network names");
        assert!(matches!(tls.ensure_cert(&["example.com".into()]), Err(AppError::Validation(_))));
        assert!(tls.ensure_cert(&["localhost".into(), "127.0.0.1".into(), "::1".into()]).is_ok());
        assert!(matches!(tls.ensure_cert(&["../evil".into()]), Err(AppError::Validation(_))));
    }
}
