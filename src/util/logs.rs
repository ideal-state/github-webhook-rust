pub fn init_log(config_path: &str) {
    log4rs::init_file(config_path, Default::default()).unwrap();
    let main_id = std::thread::current().id();
    std::panic::set_hook(Box::new(move |panic_info| {
        log::error!("{}", panic_info.to_string());
        if std::thread::current().id() == main_id {
            log::error!("Press Enter to exit...");
            std::io::stdin().read_line(&mut String::new()).unwrap();
        }
    }));
}
