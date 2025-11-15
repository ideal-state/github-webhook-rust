use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfiguration {
    pub enabled: bool,
    pub name: String,
    pub secret: String,
    pub mappings: HashMap<String, String>,
    pub properties: HashMap<String, serde_json::Value>,
    pub replacements: HashMap<String, String>,
    pub requests: ChannelRequestsConfiguration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelRequestsConfiguration {
    pub url: String,
    pub header: HashMap<String, serde_json::Value>,
    pub body: HashMap<String, serde_json::Value>,
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
pub struct Channel {
    pub config: ChannelConfiguration,
    pub templates: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ChannelManager {
    pub channels: HashMap<String, Channel>,
}
