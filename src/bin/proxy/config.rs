use std::net::SocketAddr;

use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    pub proxy: ProxyConfig,
    #[serde(default)]
    pub api: ApiConfig,
    #[serde(default)]
    pub local_database: LocalDatabaseConfig,
    pub rosslare_database: RosslareDatabaseConfig,
    pub posthog: Option<PosthogConfig>,
}

#[derive(Deserialize)]
pub struct ProxyConfig {
    #[serde(default = "default_proxy_listen")]
    pub listen: SocketAddr,
    /// Network name in tblNetworks.tDescNetwork — used to resolve the panel address.
    pub target: String,
}

#[derive(Deserialize)]
pub struct ApiConfig {
    #[serde(default = "default_api_listen")]
    pub listen: SocketAddr,
}

#[derive(Deserialize)]
pub struct LocalDatabaseConfig {
    #[serde(default = "default_local_database_path")]
    pub path: String,
}

#[derive(Deserialize)]
pub struct RosslareDatabaseConfig {
    /// Named pipe path, e.g. `\\.\pipe\MSSQL$Veritrax2019\sql\query`
    pub pipe: String,
    pub username: String,
    pub password: String,
    #[serde(default = "default_rosslare_database")]
    pub database: String,
}

#[derive(Deserialize)]
pub struct PosthogConfig {
    pub api_key: String,
}

fn default_proxy_listen() -> SocketAddr {
    "0.0.0.0:1918".parse().unwrap()
}

fn default_api_listen() -> SocketAddr {
    "0.0.0.0:8181".parse().unwrap()
}

fn default_local_database_path() -> String {
    "proxy.db".to_string()
}

fn default_rosslare_database() -> String {
    "AxTrax1".to_string()
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            listen: default_api_listen(),
        }
    }
}

impl Default for LocalDatabaseConfig {
    fn default() -> Self {
        Self {
            path: default_local_database_path(),
        }
    }
}

impl Config {
    pub fn load(path: &str) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => toml::from_str(&contents).expect("failed to parse config file"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                panic!("Config file not found: {path}");
            }
            Err(e) => panic!("failed to read config file: {e}"),
        }
    }
}
