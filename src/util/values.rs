use std::collections::HashMap;

pub fn as_str(value: &serde_json::Value) -> String {
    if value.is_null() {
        return "null".to_string();
    }
    if value.is_string() {
        return value.as_str().unwrap().to_string();
    }
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

pub fn flatten_value(value: &serde_json::Value) -> serde_json::Value {
    flatten_json_object::Flattener::new()
        .set_key_separator(".")
        .set_array_formatting(flatten_json_object::ArrayFormatting::Surrounded {
            start: "[".to_string(),
            end: "]".to_string(),
        })
        .set_preserve_empty_arrays(false)
        .set_preserve_empty_objects(false)
        .flatten(value)
        .unwrap()
}

pub fn resolve_template(
    template: String,
    properties: &serde_json::Map<String, serde_json::Value>,
    replacements: &HashMap<String, String>,
) -> String {
    resolve_string(template, None, properties, Some(replacements))
}

pub fn resolve_string(
    mut string: String,
    template: Option<&String>,
    properties: &serde_json::Map<String, serde_json::Value>,
    replacements: Option<&HashMap<String, String>>,
) -> String {
    if let Some(template) = template {
        if string.contains("${__message__}") {
            string = string.replace("${__message__}", template);
        }
    }
    for (key, value) in properties {
        let k = format!("${{{}}}", key);
        if !string.contains(k.as_str()) {
            continue;
        }
        let mut v = as_str(value);
        if let Some(replacements) = replacements {
            for (key1, value1) in replacements {
                v = v.replace(key1, value1);
            }
        }
        string = string.replace(&k, &v);
    }
    string
}
