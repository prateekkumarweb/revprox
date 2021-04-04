use config::{Config, File};
use serde::Deserialize;
use std::{collections::HashMap, path::PathBuf};

#[derive(Debug, Deserialize)]
pub struct Settings {
    servers: Vec<ServerSetting>,
}

#[derive(Debug, Deserialize)]
struct ServerSetting {
    host: String,
    proxy_pass: String,
}

impl Settings {
    pub fn from_config_file(config_file: PathBuf) -> Settings {
        let mut settings = Config::default();
        settings
            .merge(File::from(config_file))
            .expect("Could not read config file");
        settings.try_into().expect("Could not parse settings")
    }

    pub fn servers(self) -> HashMap<String, String> {
        self.servers
            .into_iter()
            .map(|setting| (setting.host, setting.proxy_pass))
            .collect()
    }
}
