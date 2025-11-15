use std::{collections::HashMap, fs, io::BufReader, path::Path};

use futures::StreamExt;

use crate::api::configs;
use crate::util::args;
use crate::util::secerts;
use crate::util::values;

fn resolve_template_ex(
    template: &str,
    properties: &serde_json::Map<String, serde_json::Value>,
    event_body: &serde_json::Map<String, serde_json::Value>,
    replacements: &HashMap<String, String>,
) -> String {
    values::resolve_template(
        values::resolve_template(template.to_string(), properties, replacements),
        event_body,
        replacements,
    )
}

fn resolve_string_ex(
    string: String,
    template: Option<&String>,
    properties: &serde_json::Map<String, serde_json::Value>,
    event_body: &serde_json::Map<String, serde_json::Value>,
    replacements: Option<&HashMap<String, String>>,
) -> String {
    values::resolve_string(
        values::resolve_string(string, template, properties, replacements),
        template,
        event_body,
        replacements,
    )
}

fn resolve_value(
    value: &serde_json::Value,
    template: &Option<String>,
    properties: &serde_json::Map<String, serde_json::Value>,
    event_body: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    if value.is_null() {
        return Some(serde_json::Value::Null);
    }
    if template.is_none() && properties.is_empty() && event_body.is_empty() {
        return None;
    }
    match value {
        serde_json::Value::String(str) => {
            if str.find("${").and_then(|i| str[i..].find("}")).is_none() {
                return None;
            }
            Some(serde_json::Value::String(resolve_string_ex(
                str.to_string(),
                template.as_ref(),
                properties,
                event_body,
                None,
            )))
        }
        serde_json::Value::Object(obj) => {
            let mut object = serde_json::Map::<String, serde_json::Value>::with_capacity(obj.len());
            for (key, v) in obj {
                object.insert(
                    key.clone(),
                    resolve_value(v, template, properties, event_body).unwrap_or_else(|| v.clone()),
                );
            }
            Some(serde_json::Value::Object(object))
        }
        serde_json::Value::Array(arr) => {
            let mut array = Vec::<serde_json::Value>::with_capacity(arr.len());
            for v in arr {
                array.push(
                    resolve_value(v, template, properties, event_body).unwrap_or_else(|| v.clone()),
                );
            }
            Some(serde_json::Value::Array(array))
        }
        _ => None,
    }
}

impl configs::ChannelManager {
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
                    let mut config: configs::ChannelConfiguration = serde_json::from_reader(
                        BufReader::new(fs::File::open(config_file).unwrap()),
                    )
                    .unwrap();
                    if !config.enabled {
                        continue;
                    }
                    if channels.contains_key(&config.name) {
                        panic!("Channel '{}' already exists.", config.name);
                    }
                    for (key, value) in &config.properties {
                        config.requests.url = config
                            .requests
                            .url
                            .replace(&format!("${{{}}}", key), &values::as_str(value))
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
                    channels.insert(config.name.clone(), configs::Channel { config, templates });
                }
            }
        } else {
            fs::create_dir_all(base_dir).unwrap();
        }
        configs::ChannelManager { channels }
    }

    pub async fn push(
        &self,
        maximum_payload: usize,
        headers: &actix_web::http::header::HeaderMap,
        mut payload: actix_web::web::Payload,
    ) -> actix_web::HttpResponse {
        if self.channels.is_empty() {
            return actix_web::HttpResponse::Ok().body("No channel found.");
        }
        let mut body = actix_web::web::BytesMut::new();
        while let Some(chunk) = payload.next().await {
            let chunk = chunk.unwrap();
            let size = body.len() + chunk.len();
            if (size) > maximum_payload {
                return actix_web::HttpResponse::PayloadTooLarge()
                    .body(format!("Payload size is ({} > {})", size, maximum_payload));
            }
            body.extend_from_slice(&chunk);
        }
        let result = serde_json::from_slice::<serde_json::Value>(&body);
        if result.is_err() {
            log::error!("Failed to parse Json: {}", result.unwrap_err());
            return actix_web::HttpResponse::BadRequest().body("Invalid payload.");
        }
        let mut event_header = HashMap::<String, String>::with_capacity(headers.len());
        for (key, value) in headers {
            event_header.insert(key.to_string(), value.to_str().unwrap().to_string());
        }
        log::info!(
            "Event Header: \n{}",
            serde_json::to_string_pretty(&event_header).expect("error")
        );
        let event_body = values::flatten_value(&result.unwrap());
        log::info!(
            "Event Body: \n{}",
            serde_json::to_string_pretty(&event_body).expect("error")
        );
        let mut requests = Vec::with_capacity(self.channels.len());
        for (channel_name, channel) in &self.channels {
            let config = &channel.config;
            let mappings = &config.mappings;
            let event = headers
                .get(
                    mappings
                        .get("X-GitHub-Event")
                        .and_then(|s| Some(s.as_str()))
                        .unwrap_or("X-GitHub-Event"),
                )
                .map_or("", |v| v.to_str().unwrap());
            if event.is_empty() {
                log::warn!("{channel_name} - Invalid event ''.");
                continue;
            } else {
                log::info!("{channel_name} - Event: '{}'.", event);
            }
            let signature = headers
                .get(
                    mappings
                        .get("X-Hub-Signature-256")
                        .and_then(|s| Some(s.as_str()))
                        .unwrap_or("X-Hub-Signature-256"),
                )
                .map_or("", |v| v.to_str().unwrap());
            if !secerts::verify_signature(&config.secret, signature, &body) {
                log::warn!("{channel_name} - Invalid signature '{}'.", signature);
                continue;
            } else {
                log::info!("{channel_name} - Signature: '{}'.", signature);
            }
            if let Some(template) = channel.templates.get(event) {
                let properties = values::flatten_value(
                    &serde_json::from_str(&serde_json::to_string(&config.properties).unwrap())
                        .unwrap(),
                );
                requests.push(
                    self.push_request(
                        channel_name,
                        &template,
                        &properties.as_object().unwrap(),
                        &event_body.as_object().unwrap(),
                        &config.replacements,
                        &config.requests,
                    )
                    .await,
                );
            }
        }
        let mut successes = 0u32;
        for success in &requests {
            if *success {
                successes += 1;
            }
        }
        let message = format!(
            "Pushed event to {}/{} channel(s).",
            successes,
            requests.len()
        );
        log::info!("{}", message);
        actix_web::HttpResponse::Ok().body(message)
    }

    async fn push_request(
        &self,
        channel_name: &str,
        template: &str,
        properties: &serde_json::Map<String, serde_json::Value>,
        event_body: &serde_json::Map<String, serde_json::Value>,
        replacements: &HashMap<String, String>,
        requests: &configs::ChannelRequestsConfiguration,
    ) -> bool {
        let requests_url = &requests.url;
        let header = &requests.header;
        let content_type = if let Some(value) = header.get("Content-Type") {
            configs::ChannelRequestContentType::from_header(&values::as_str(value))
        } else {
            configs::ChannelRequestContentType::Json
        };
        let template = Some(resolve_template_ex(
            template,
            properties,
            event_body,
            replacements,
        ));
        let client = reqwest::Client::new();
        let mut builder: reqwest::RequestBuilder = client.post(requests_url);
        log::info!("{channel_name} - Requests URL: '{}'", requests_url);
        let mut requests_header = HashMap::<String, serde_json::Value>::with_capacity(header.len());
        for (header_key, header_value) in header {
            requests_header.insert(
                header_key.clone(),
                resolve_value(header_value, &template, properties, event_body)
                    .unwrap_or_else(|| header_value.clone()),
            );
        }
        log::info!(
            "{channel_name} - Requests Header: \n{}",
            serde_json::to_string_pretty(&requests_header).expect("error")
        );
        for (key, value) in requests_header {
            builder = builder.header(key, values::as_str(&value));
        }
        let body = &requests.body;
        let mut requests_body = HashMap::<String, serde_json::Value>::with_capacity(body.len());
        for (body_key, body_value) in body {
            requests_body.insert(
                body_key.clone(),
                resolve_value(body_value, &template, properties, event_body)
                    .unwrap_or_else(|| body_value.clone()),
            );
        }
        log::info!(
            "{channel_name} - Requests Body: \n{}",
            serde_json::to_string_pretty(&requests_body).expect("error")
        );
        match content_type {
            configs::ChannelRequestContentType::Json => {
                builder = builder.json(&requests_body);
            }
            configs::ChannelRequestContentType::Form => {
                builder = builder.form(&requests_body);
            }
        }

        match builder.send().await {
            Ok(response) => {
                let status = response.status();
                if !status.is_success() {
                    match &response.text().await {
                        Ok(text) => {
                            log::error!(
                                "{channel_name} - Pushing event to channel, but response is invalid, with 'HTTP {}': '{}'.",
                                status,
                                text
                            );
                        }
                        Err(err) => {
                            log::error!(
                                "{channel_name} - Pushing event to channel, but response is invalid, with 'HTTP {}' and caught error '{}' when getting message.",
                                status,
                                err.to_string()
                            );
                        }
                    }
                    return false;
                }
                log::info!(
                    "{channel_name} - Pushed event to channel, with 'HTTP {}'.",
                    status
                );
                true
            }
            Err(err) => {
                log::error!(
                    "{channel_name} - Pushing event to channel, but request is invalid, with 'HTTP {:?}'.",
                    err.status()
                );
                false
            }
        }
    }
}
