use clap::Parser;

/// Simple GitHub webhook program
#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct MainArgs {
    /// The hostname listen on the web server
    #[arg(short = None, long, default_value = "0.0.0.0")]
    pub hostname: String,

    /// The port listen on the web server
    #[arg(short = None, long, default_value_t = 9527)]
    pub port: u16,

    /// Enable TLS on the web server
    #[arg(short = None, long, default_value_t = false)]
    pub tls: bool,

    /// The workers of the web server
    #[arg(short = None, long, default_value_t = 0)]
    pub workers: u8,
    
    /// The maximum payload size (in bytes) on the web server
    #[arg(short = None, long = "maximum-payload", default_value_t = 32768)]
    pub maximum_payload: usize,
}

pub fn parse() -> MainArgs {
    MainArgs::parse()
}
