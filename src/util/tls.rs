pub fn load_tls_config(certificate_dir: &str) -> openssl::ssl::SslAcceptorBuilder {
    let mut builder =
        openssl::ssl::SslAcceptor::mozilla_intermediate(openssl::ssl::SslMethod::tls()).unwrap();
    builder
        .set_private_key_file(
            format!("{}/key.pem", certificate_dir),
            openssl::ssl::SslFiletype::PEM,
        )
        .unwrap();

    // set the certificate chain file location
    builder
        .set_certificate_chain_file(format!("{}/cert.pem", certificate_dir))
        .unwrap();

    builder
}
