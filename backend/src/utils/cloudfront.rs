// CloudFront signed URLs (custom policy, wildcard resource) for image loads.
use base64::{engine::general_purpose::STANDARD, Engine};
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::rand_core::OsRng;
use rsa::signature::{RandomizedSigner, SignatureEncoding};
use rsa::RsaPrivateKey;
use sha1::Sha1;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct CloudFrontSigner {
    domain: String,
    key_pair_id: String,
    key: SigningKey<Sha1>,
    // (expires, query) for the current hour bucket
    cache: Mutex<Option<(u64, String)>>,
}

impl CloudFrontSigner {
    /// None (with a warning) when the env vars are unset, so local dev without CloudFront works.
    /// Panics on a malformed key so a bad deploy fails at boot.
    pub fn from_env() -> Option<Self> {
        // docker compose passes unset vars as "", treat that as unset too
        let var = |name| dotenv::var(name).ok().filter(|v| !v.trim().is_empty());
        let (Some(domain), Some(key_pair_id), Some(pem)) = (
            var("CLOUDFRONT_DOMAIN"),
            var("CLOUDFRONT_KEY_PAIR_ID"),
            var("CLOUDFRONT_PRIVATE_KEY"),
        ) else {
            eprintln!("warning: CLOUDFRONT_DOMAIN/KEY_PAIR_ID/PRIVATE_KEY not set, image URLs will be unsigned");
            return None;
        };
        Some(Self::new(domain, key_pair_id, &pem))
    }

    fn new(domain: String, key_pair_id: String, pem: &str) -> Self {
        Self {
            domain,
            key_pair_id,
            key: SigningKey::new(parse_key(pem)),
            cache: Mutex::new(None),
        }
    }

    /// Returns (query, expires_at). Same string for a whole hour so browsers can cache images.
    pub fn signed_query(&self) -> (String, u64) {
        let expires = bucket_expiry();
        let mut cache = self.cache.lock().unwrap();
        if let Some((cached_expires, query)) = cache.as_ref() {
            if *cached_expires == expires {
                return (query.clone(), expires);
            }
        }
        let query = self.sign(expires);
        *cache = Some((expires, query.clone()));
        (query, expires)
    }

    fn sign(&self, expires: u64) -> String {
        let policy = policy_json(&self.domain, expires);
        // randomized = RSA blinding, mitigates timing side channels
        let signature = self.key.sign_with_rng(&mut OsRng, policy.as_bytes());
        format!(
            "Policy={}&Signature={}&Key-Pair-Id={}",
            cloudfront_base64(policy.as_bytes()),
            cloudfront_base64(&signature.to_bytes()),
            self.key_pair_id
        )
    }
}

/// Expiry on the hour, 1-2h from now.
pub fn bucket_expiry() -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    (now / 3600 + 2) * 3600
}

// CloudFront requires the policy with no whitespace.
fn policy_json(domain: &str, expires: u64) -> String {
    format!(
        r#"{{"Statement":[{{"Resource":"https://{domain}/*","Condition":{{"DateLessThan":{{"AWS:EpochTime":{expires}}}}}}}]}}"#
    )
}

fn cloudfront_base64(bytes: &[u8]) -> String {
    STANDARD
        .encode(bytes)
        .replace('+', "-")
        .replace('=', "_")
        .replace('/', "~")
}

// Accepts PKCS#8 or PKCS#1 PEM, with real newlines or literal "\n".
// backend/.env must hold it on one line with "\n" escapes: dotenv 0.15 parses line by line
// and main.rs ignores its errors, so a multi-line value silently drops the rest of the file.
// Real newlines only work via the docker-compose root .env or the process environment.
fn parse_key(pem: &str) -> RsaPrivateKey {
    let pem = pem.trim().replace("\\n", "\n");
    RsaPrivateKey::from_pkcs8_pem(&pem)
        .or_else(|_| RsaPrivateKey::from_pkcs1_pem(&pem))
        .expect("CLOUDFRONT_PRIVATE_KEY is not a PKCS#1 or PKCS#8 RSA PEM")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::pkcs1::{EncodeRsaPrivateKey, LineEnding};
    use rsa::pkcs1v15::{Signature, VerifyingKey};
    use rsa::signature::Verifier;

    fn param<'a>(query: &'a str, name: &str) -> &'a str {
        query
            .split('&')
            .find_map(|kv| kv.strip_prefix(name)?.strip_prefix('='))
            .unwrap()
    }

    fn decode(s: &str) -> Vec<u8> {
        let s = s.replace('-', "+").replace('_', "=").replace('~', "/");
        STANDARD.decode(s).unwrap()
    }

    #[test]
    fn signs_policy_verifiable_with_public_key() {
        let key = RsaPrivateKey::new(&mut OsRng, 1024).unwrap();
        // PKCS#1 with escaped newlines, like a single-line env var
        let pem = key
            .to_pkcs1_pem(LineEnding::LF)
            .unwrap()
            .replace('\n', "\\n");
        let signer = CloudFrontSigner::new("cdn.example.com".into(), "KTESTID".into(), &pem);

        let (query, expires) = signer.signed_query();
        assert_eq!(expires % 3600, 0);
        assert_eq!(param(&query, "Key-Pair-Id"), "KTESTID");
        for name in ["Policy", "Signature"] {
            assert!(!param(&query, name).contains(&['+', '=', '/'][..]));
        }

        let policy = decode(param(&query, "Policy"));
        let policy_str = String::from_utf8(policy.clone()).unwrap();
        assert!(!policy_str.chars().any(char::is_whitespace));
        assert_eq!(
            policy_str,
            format!(
                r#"{{"Statement":[{{"Resource":"https://cdn.example.com/*","Condition":{{"DateLessThan":{{"AWS:EpochTime":{expires}}}}}}}]}}"#
            )
        );
        let parsed: serde_json::Value = serde_json::from_slice(&policy).unwrap();
        assert_eq!(
            parsed["Statement"][0]["Condition"]["DateLessThan"]["AWS:EpochTime"],
            expires
        );

        let signature = Signature::try_from(decode(param(&query, "Signature")).as_slice()).unwrap();
        VerifyingKey::<Sha1>::new(key.to_public_key())
            .verify(&policy, &signature)
            .unwrap();

        assert_eq!(signer.signed_query().0, query);
    }

    #[test]
    fn parses_pkcs8() {
        use rsa::pkcs8::EncodePrivateKey;
        let key = RsaPrivateKey::new(&mut OsRng, 1024).unwrap();
        let pem = key.to_pkcs8_pem(LineEnding::LF).unwrap();
        assert_eq!(parse_key(&pem), key);
    }
}
