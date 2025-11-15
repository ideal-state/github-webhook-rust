use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "resources"]
struct Asset;

pub fn extract_assets(target_dir: &str) -> std::io::Result<()> {
    let target_dir = std::path::Path::new(target_dir);
    for file in Asset::iter() {
        if let Some(content) = Asset::get(&file) {
            let path = target_dir.join(file.as_ref());
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            if std::fs::exists(&path)? {
                continue;
            }
            std::fs::write(path, content.data)?;
        }
    }
    Ok(())
}
