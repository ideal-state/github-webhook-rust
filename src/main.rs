mod api;
mod implement;
mod util;

use std::{env, sync::Arc};

use api::configs;
use util::{args, assets, logs, tls};

static mut MAXIMUM_PAYLOAD: usize = 32768;

/// Push GitHub webhook event
#[actix_web::post("/push")]
async fn push(
    channel_manager: actix_web::web::Data<Arc<tokio::sync::Mutex<configs::ChannelManager>>>,
    request: actix_web::HttpRequest,
    payload: actix_web::web::Payload,
) -> actix_web::HttpResponse {
    let channel_manager = if let Ok(channel_manager) = channel_manager.try_lock() {
        Some(channel_manager.clone())
    } else {
        None
    };
    if let Some(channel_manager) = channel_manager {
        let maximum_payload = unsafe {
            MAXIMUM_PAYLOAD
        };
        return channel_manager.push(maximum_payload, request.headers(), payload).await;
    }
    actix_web::HttpResponse::RequestTimeout().finish()
}

async fn default() -> actix_web::HttpResponse {
    actix_web::HttpResponse::Forbidden().finish()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    assets::extract_assets(".")?;
    logs::init_log("./logs/log4rs.yaml");

    let args = args::parse();
    unsafe {
        MAXIMUM_PAYLOAD = args.maximum_payload;
    }

    let addr = format!("{}:{}", args.hostname, args.port);
    let mut server;
    {
        let channel_manager = Arc::new(tokio::sync::Mutex::new(configs::ChannelManager::new(
            "./channels",
        )));
        server = actix_web::HttpServer::new(move || {
            actix_web::App::new()
                .wrap(actix_web::middleware::Logger::default())
                .app_data(actix_web::web::Data::new(channel_manager.clone()))
                .default_service(actix_web::web::to(default))
                .service(push)
        });
    }
    if args.tls {
        server = server.bind_openssl(addr.clone(), tls::load_tls_config("./certificates"))?;
    } else {
        server = server.bind(addr.clone())?;
    }
    if args.workers > 0 {
        server = server.workers(usize::from(args.workers));
    }
    let server = server.run();
    actix_web::rt::System::current()
        .arbiter()
        .spawn(async move {
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
            log::info!(
                "Working directory: '{}'",
                env::current_dir().unwrap().display()
            );
            log::info!("Server started: '{}'", url);
        });

    server.await
}
