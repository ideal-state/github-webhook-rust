pub fn hmac_sha256(secret: &[u8], payload: &[u8]) -> Result<String, openssl::error::ErrorStack> {
    let key = openssl::pkey::PKey::hmac(secret)?;
    let mut signer = openssl::sign::Signer::new(openssl::hash::MessageDigest::sha256(), &key)?;
    signer.update(payload)?;
    let hmac = signer.sign_to_vec()?;

    Ok(hmac
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>())
}

pub fn verify_signature(secret: &str, signature: &str, payload: &[u8]) -> bool {
    if signature.is_empty() {
        return secret.is_empty();
    }
    if secret.is_empty() {
        return false;
    }
    const SIGNATURE_PREFIX: &str = "sha256=";
    let computed_hmac = hmac_sha256(secret.as_bytes(), payload).unwrap();
    log::debug!(
        "Actual: '{}{}', Expected: '{}'",
        SIGNATURE_PREFIX,
        computed_hmac,
        signature
    );
    signature
        .strip_prefix(SIGNATURE_PREFIX)
        .map(|sig| constant_time_eq::constant_time_eq(sig.as_bytes(), computed_hmac.as_bytes()))
        .unwrap_or(false)
}
