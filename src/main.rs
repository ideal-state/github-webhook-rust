use actix_web::error;
use actix_web::{middleware, post, rt::System, web, App, HttpRequest, HttpResponse, HttpServer};
use clap::Parser;
use flatten_json_object::{ArrayFormatting, Flattener};
use futures::StreamExt;
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::sign::Signer;
use openssl::ssl::SslAcceptorBuilder;
use reqwest::Client;
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, env};
use std::io::BufReader;
use std::sync::Arc;
use std::{
    fs,
    io::{self},
    panic::set_hook,
    path::Path,
    thread,
};
use tokio::sync::Mutex;
use openssl::{
    ssl::{SslAcceptor, SslMethod},
};

const PAYLOAD_MAX_SIZE: usize = 16384;

#[derive(RustEmbed)]
#[folder = "resources"]
struct Asset;

fn extract_assets(target_dir: &str) -> io::Result<()> {
    let target_dir = Path::new(target_dir);
    for file in Asset::iter() {
        if let Some(content) = Asset::get(&file) {
            let path = target_dir.join(file.as_ref());
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            if fs::exists(&path)? { 
                continue;
            }
            fs::write(path, content.data)?;
        }
    }
    Ok(())
}

/// Simple GitHub webhook program
#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
struct MainArgs {

    /// The hostname listen on the web server
    #[arg(short = None, long, default_value = "localhost")]
    hostname: String,

    /// The port listen on the web server
    #[arg(short = None, long, default_value_t = 9527)]
    port: u16,

    /// Enable TLS on the web server
    #[arg(short = None, long, default_value_t = false)]
    tls: bool,

    /// The workers of the web server
    #[arg(short = None, long, default_value_t = 0)]
    workers: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChannelConfiguration {
    name: String,
    properties: HashMap<String, Value>,
    url: String,
    mode: String,
    replacements: HashMap<String, String>,
    request: ChannelRequestConfiguration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChannelRequestConfiguration {
    header: HashMap<String, Value>,
    body: HashMap<String, Value>,
}

#[derive(Debug, Clone)]
struct Channel {
    config: ChannelConfiguration,
    templates: HashMap<String, String>,
}

macro_rules! content_types {
    ($($name:ident => $str:expr),+) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum ChannelRequestContentType {
            $($name),+
        }

        impl ChannelRequestContentType {
            pub fn to_header(&self) -> &'static str {
                match self {
                    $(Self::$name => $str),+
                }
            }

            pub fn from_header(header: &str) -> Self {
                return match header {
                    $($str => Self::$name),+,
                    _ => panic!("Unknown content type: '{}'", header),
                };
            }
        }
    };
}

content_types! {
    Json => "application/json",
    Form => "application/x-www-form-urlencoded"
}

#[derive(Debug, Clone)]
struct ChannelManager {
    channels: HashMap<String, Channel>,
}

fn as_str(value: &Value) -> String {
    if value.is_string() {
        return value.as_str().unwrap().to_string();
    }
    value.to_string()
}

impl ChannelManager {
    pub fn new(base_dir: &str) -> Self {
        let mut channels = HashMap::new();
        let base_dir = Path::new(base_dir);
        if base_dir.exists() {
            if base_dir.is_dir() {
                for entry in fs::read_dir(base_dir).unwrap() {
                    let path = entry.unwrap().path();
                    if !path.is_dir() {
                        continue;
                    }
                    let config_file = path.join("config.json");
                    if !config_file.exists() || !config_file.is_file() {
                        continue;
                    }
                    let mut config: ChannelConfiguration = serde_json::from_reader(BufReader::new(
                        fs::File::open(config_file).unwrap(),
                    ))
                    .unwrap();
                    if channels.contains_key(&config.name) {
                        panic!("Channel '{}' already exists.", config.name);
                    }
                    for (key, value) in &config.properties {
                        config.url = config.url.replace(&format!("${{{}}}", key), &as_str(value))
                    }
                    log::info!("Load channel '{}'.", config.name);
                    let mut templates = HashMap::new();
                    let templates_dir = path.join("templates");
                    if templates_dir.exists() {
                        if templates_dir.is_dir() {
                            for template in fs::read_dir(templates_dir).unwrap() {
                                let template_path = template.unwrap().path();
                                if template_path.is_dir() {
                                    continue;
                                }
                                let mut event = template_path
                                    .file_name()
                                    .unwrap()
                                    .to_str()
                                    .unwrap()
                                    .to_string();
                                if let Some(found) = event.find('.') {
                                    event = event[0..found].to_string();
                                }
                                log::info!("Load template '{}'.", event);
                                templates.insert(event, fs::read_to_string(template_path).unwrap());
                            }
                        }
                    }
                    log::info!(
                        "Loaded channel '{}' with {} template(s).",
                        config.name,
                        templates.len()
                    );
                    channels.insert(config.name.clone(), Channel { config, templates });
                }
            }
        } else {
            fs::create_dir_all(base_dir).unwrap();
        }
        ChannelManager { channels }
    }

    pub async fn push(
        &self,
        webhook: &Webhook,
        event: &str,
        signature: &str,
        mut payload: web::Payload,
    ) -> Result<HttpResponse, actix_web::Error> {
        if event.is_empty() {
            return Err(error::ErrorBadRequest("Event is empty."));
        }
        if self.channels.is_empty() {
            return Err(error::ErrorNotFound("No channel found."));
        }
        let mut body = web::BytesMut::new();
        while let Some(chunk) = payload.next().await {
            let chunk = chunk.unwrap();
            let size = body.len() + chunk.len();
            if (size) > PAYLOAD_MAX_SIZE {
                return Err(error::ErrorPayloadTooLarge(format!(
                    "Payload size is ({} > {})",
                    size, PAYLOAD_MAX_SIZE
                )));
            }
            body.extend_from_slice(&chunk);
        }
        if !verify_signature(&webhook.secret, signature, &body) {
            return Err(error::ErrorForbidden("Invalid signature."));
        }
        let result = serde_json::from_slice::<Value>(&body);
        if result.is_err() {
            log::error!("Failed to parse JSON: {}", result.unwrap_err());
            return Err(error::ErrorBadRequest("Invalid payload."));
        }
        let data = Flattener::new()
                .set_key_separator(".")
                .set_array_formatting(ArrayFormatting::Surrounded {
                    start: "[".to_string(),
                    end: "]".to_string(),
                })
                .set_preserve_empty_arrays(false)
                .set_preserve_empty_objects(false)
                .flatten(&result.unwrap())
                .unwrap();
        let channels = self.channels.clone();
        let mut requests = Vec::with_capacity(channels.len());
        for (_, channel) in channels {
            if let Some(template) = channel.templates.get(event) {
                let template = template.clone();
                let data_clone = data.as_object().unwrap().clone();
                requests.push(tokio::task::spawn(async move {
                    let configuration = &channel.config;
                    let request = &configuration.request;
                    let header = &request.header;
                    let content_type = if let Some(value) = header.get("Content-Type") {
                        ChannelRequestContentType::from_header(&as_str(value))
                    } else {
                        ChannelRequestContentType::Json
                    };
                    let properties = &configuration.properties;
                    let mut template = template.clone();
                    for (key, value) in &data_clone {
                        template = template.replace(&format!("${{{}}}", key), &as_str(value));
                    }
                    for (key, value) in properties {
                        template = template.replace(&format!("${{{}}}", key), &as_str(value));
                    }
                    for (key, value) in &configuration.replacements {
                        template = template.replace(key, value);
                    }
                    // match name.as_str() {
                    //     "telegram" => {
                    //         let renderer = teloxide::utils::render::Renderer::new(&template, &[]);
                    //         match configuration.mode.as_str() {
                    //             "markdown" => {
                    //                 template = renderer.as_markdown();
                    //             }
                    //             "html" => {
                    //                 template = renderer.as_html();
                    //             }
                    //             _ => {}
                    //         }
                    //     }
                    //     _ => {}
                    // }
                    let client = Client::new();
                    let mut builder = client.post(&configuration.url);
                    log::info!("Request URL: '{}'", &configuration.url);
                    for (header_key, header_value) in header {
                        let mut header_value =
                            as_str(header_value).replace("${__message__}", &template);
                        for (key, value) in &data_clone {
                            header_value =
                                header_value.replace(&format!("${{{}}}", key), &as_str(value));
                        }
                        for (key, value) in properties {
                            header_value =
                                header_value.replace(&format!("${{{}}}", key), &as_str(value));
                        }
                        log::info!("Request Header: '{}' to '{}'", header_key, header_value);
                        builder = builder.header(header_key, header_value);
                    }
                    let body = &request.body;
                    let mut payload = HashMap::<String, String>::with_capacity(body.len());
                    for (body_key, body_value) in body {
                        let mut body_value =
                            as_str(body_value).replace("${__message__}", &template);
                        for (key, value) in &data_clone {
                            body_value =
                                body_value.replace(&format!("${{{}}}", key), &as_str(value));
                        }
                        for (key, value) in properties {
                            body_value =
                                body_value.replace(&format!("${{{}}}", key), &as_str(value));
                        }
                        log::info!("Request Body: '{}' to '{}'", body_key, body_value);
                        payload.insert(body_key.clone(), body_value);
                    }
                    match content_type {
                        ChannelRequestContentType::Json => {
                            builder = builder.json(&payload);
                        }
                        ChannelRequestContentType::Form => {
                            builder = builder.form(&payload);
                        }
                    }
                    builder.send()
                }));
            }
        }
        let mut count = 0;
        for request in requests {
            let response = request.await.unwrap().await;
            if response.is_ok() {
                let response = response.unwrap();
                if response.status().is_success() {
                    log::info!("Pushed event request '{}'.", event);
                    count += 1;
                } else {
                    log::error!(
                        "Failed to push event request '{}' HTTP '{}': '{}'",
                        event,
                        response.status(),
                        response.text().await.unwrap()
                    );
                }
            } else {
                log::error!(
                    "Failed to push event request '{}': {}",
                    event,
                    response.unwrap_err()
                );
            }
        }
        log::info!("Pushed {} event request(s).", count);
        Ok(HttpResponse::Ok().finish())
    }
}

/// Push GitHub webhook event
#[post("/push")]
async fn push(
    webhook: web::Data<Arc<Mutex<Webhook>>>,
    request: HttpRequest,
    payload: web::Payload,
) -> HttpResponse {
    log::debug!("{request:?}");
    let headers = request.headers();
    let signature = headers
        .get("X-Hub-Signature-256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let event = headers
        .get("X-GitHub-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if let Ok(webhook) = webhook.try_lock() {
        return match webhook.channel_manager.push(&webhook, event, signature, payload).await {
            Ok(response) => {
                response
            }
            Err(error) => {
                error.error_response()
            }
        };
    }
    HttpResponse::RequestTimeout().finish()
}

async fn default() -> HttpResponse {
    HttpResponse::Forbidden().finish()
}

#[derive(Debug, Clone)]
struct Webhook {
    secret: String,
    channel_manager: ChannelManager,
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    extract_assets(".")?;
    init_log("./logs/log4rs.yaml");

    let args = MainArgs::parse();
    let secret = load_secret("./secret");

    let addr = format!("{}:{}", args.hostname, args.port);
    let mut server;
    {
        let webhook = Arc::new(Mutex::new(Webhook {
            secret: secret.clone(),
            channel_manager: ChannelManager::new("./channels"),
        }));
        server = HttpServer::new(move || {
            App::new()
                .wrap(middleware::Logger::default())
                .app_data(web::Data::new(webhook.clone()))
                .default_service(web::to(default))
                .service(push)
        });
    }
    if args.tls {
        server = server.bind_openssl(addr.clone(), load_tls_config("./certificates"))?;
    } else {
        server = server.bind(addr.clone())?;
    }
    if args.workers > 0 {
        server = server.workers(usize::from(args.workers));
    }
    let server = server.run();
    System::current().arbiter().spawn(async move {
        let url: String;
        if args.tls {
            if args.port == 443 {
                url = format!("https://{}", args.hostname);
            } else {
                url = format!("https://{}", addr);
            }
        } else if args.port == 80 {
            url = format!("http://{}", args.hostname);
        } else {
            url = format!("http://{}", addr);
        }
        log::info!("Working directory: '{}'", env::current_dir().unwrap().display());
        log::info!("Secret: '{}'", secret);
        log::info!("Server started: '{}'", url);
    });

    server.await
}

fn load_secret(secret_path: &str) -> String {
    if !fs::exists(secret_path).unwrap() {
        return String::new();
    }
    fs::read_to_string(secret_path).unwrap().trim_end_matches(|c| c == '\n' || c == '\r').to_string()
}

fn hmac_sha256(secret: &[u8], payload: &[u8]) -> Result<String, openssl::error::ErrorStack> {
    let key = PKey::hmac(secret)?;
    let mut signer = Signer::new(MessageDigest::sha256(), &key)?;
    signer.update(payload)?;
    let hmac = signer.sign_to_vec()?;
    
    Ok(hmac.iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>())
}

// fn main() {
//     let secret = "It's a Secret to Everybody";
//     let payload = "Hello, World!";
//     let signature = "sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17";
//     println!("{}", verify_signature(secret, signature, payload.as_bytes()))
// }

fn verify_signature(secret: &str, signature: &str, payload: &[u8]) -> bool {
    const SIGNATURE_PREFIX: &str = "sha256=";
    let computed_hmac = hmac_sha256(secret.as_bytes(), payload).unwrap();
    log::debug!("Actual: {}{}, Expected: {}", SIGNATURE_PREFIX, computed_hmac, signature);
    signature.strip_prefix(SIGNATURE_PREFIX)
        .map(|sig| constant_time_eq::constant_time_eq(
            sig.as_bytes(),
            computed_hmac.as_bytes()
        ))
        .unwrap_or(false)
}

// fn load_tls_config(certificate_dir: &str) -> ServerConfig {
//     rustls::crypto::aws_lc_rs::default_provider()
//         .install_default()
//         .unwrap();

//     let cert_chain = CertificateDer::pem_file_iter(format!("{}/cert.pem", certificate_dir))
//         .unwrap()
//         .flatten()
//         .collect();

//     let key_der =
//         PrivateKeyDer::from_pem_file(format!("{}/key.pem", certificate_dir)).expect("Could not locate PKCS 8 private keys.");

//     ServerConfig::builder()
//         .with_no_client_auth()
//         .with_single_cert(cert_chain, key_der)
//         .unwrap()
// }

fn load_tls_config(certificate_dir: &str) -> SslAcceptorBuilder {
    let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
    builder
        .set_private_key_file(format!("{}/key.pem", certificate_dir), openssl::ssl::SslFiletype::PEM)
        .unwrap();

    // set the certificate chain file location
    builder.set_certificate_chain_file(format!("{}/cert.pem", certificate_dir)).unwrap();

    builder
}

fn init_log(config_path: &str) {
    log4rs::init_file(config_path, Default::default()).unwrap();
    let main_id = thread::current().id();
    set_hook(Box::new(move |panic_info| {
        log::error!("{}", panic_info.to_string());
        if thread::current().id() == main_id {
            log::error!("Press Enter to exit...");
            io::stdin().read_line(&mut String::new()).unwrap();
        }
    }));
}
