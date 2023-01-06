use std::env;
use std::io::Read;
fn env_or(k: &str, default: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| default.to_string())
}

pub struct Config {
    pub version: String,

    // host to listen on, defaults to localhost
    pub host: String,
    pub port: u16,

    // used for building redirects, https://api.getpinion.com
    // and auth cookie
    pub real_host: Option<String>,
    pub real_domain: Option<String>,
    pub auth_header_name: String,
    pub auth_cookie_name: String,
    pub cookie_challenge_phone_name: String,
    pub secure_cookie: bool, // only set to false for local dev

    pub log_level: String,
    pub log_json: bool,

    pub twilio_account: String,
    pub twilio_messaging_service_sid: String,
    pub twilio_sid: String,
    pub twilio_secret: String,
    pub default_phone_number: String,
    pub allowed_phone_numbers: Option<Vec<String>>,

    // db config
    pub database_url: String,
    pub db_max_connections: u32,

    // key used for encrypting things
    pub encryption_key: String,

    // key used for signing/hashing things
    pub signing_key: String,

    // auth cookie expiration
    pub auth_expiration_seconds: u32,
    // phone challenge expiration, applies to phone challenge cookie
    // and verification token lifetime
    pub challenge_phone_expiration_seconds: u32,
}
impl Config {
    pub fn load() -> Self {
        let version = std::fs::File::open("commit_hash.txt")
            .map(|mut f| {
                let mut s = String::new();
                f.read_to_string(&mut s).expect("Error reading commit_hash");
                s.trim().to_string()
            })
            .unwrap_or_else(|_| "unknown".to_string());

        let allowed_phone_numbers = env::var("ALLOWED_PHONE_NUMBERS").ok().map(|s| {
            s.trim()
                .split(',')
                .map(|p| p.trim().to_string())
                .collect::<Vec<_>>()
        });
        Self {
            version,
            host: env_or("HOST", "localhost"),
            port: env_or("PORT", "3003").parse().expect("invalid port"),
            real_host: std::env::var("REAL_HOSTNAME").ok(),
            real_domain: std::env::var("REAL_DOMAIN").ok(),
            auth_header_name: "x-pinion-auth".to_string(),
            auth_cookie_name: "pinion_auth".to_string(),
            cookie_challenge_phone_name: "pinion_challenge_phone".to_string(),
            secure_cookie: env_or("SECURE_COOKIE", "true") != "false",
            log_level: env_or("LOG_LEVEL", "info"),
            log_json: env_or("LOG_JSON", "false") == "true",
            twilio_account: env_or("TWILIO_ACCOUNT", "X"),
            twilio_messaging_service_sid: env_or("TWILIO_MESSAGING_SERVICE_SID", "X"),
            twilio_sid: env_or("TWILIO_SID", "X"),
            twilio_secret: env_or("TWILIO_SECRET", "X"),
            default_phone_number: env_or("DEFAULT_PHONE_NUMBER", "0"),
            allowed_phone_numbers,
            database_url: env_or("DATABASE_URL", "error"),
            db_max_connections: env_or("DATABASE_MAX_CONNECTIONS", "5")
                .parse()
                .expect("invalid DATABASE_MAX_CONNECTIONS"),
            // 60 * 60 * 24 * 30
            auth_expiration_seconds: env_or("AUTH_EXPIRATION_SECONDS", "2592000")
                .parse()
                .expect("invalid auth_expiration_seconds"),
            // 60 * 2
            challenge_phone_expiration_seconds: env_or("CHALLENGE_PHONE_EXPIRATION_SECONDS", "120")
                .parse()
                .expect("invalid challenge_phone_expiration_seconds"),
            encryption_key: env_or("ENCRYPTION_KEY", "01234567890123456789012345678901"),
            signing_key: env_or("SIGNING_KEY", "01234567890123456789012345678901"),
        }
    }
    pub fn initialize(&self) {
        use crate::CONFIG;
        tracing::info!(
            version = %CONFIG.version,
            host = %CONFIG.host,
            port = %CONFIG.port,
            real_host = ?CONFIG.real_host,
            db_max_connections = %CONFIG.db_max_connections,
            log_level = %CONFIG.log_level,
            auth_expiration_seconds = %CONFIG.auth_expiration_seconds,
            "initialized config",
        );
    }
    pub fn get_host_port(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
    pub fn get_real_host(&self) -> String {
        self.real_host
            .clone()
            .unwrap_or_else(|| format!("http://{}:{}", self.host, self.port))
    }
    pub fn get_real_domain(&self) -> String {
        self.real_domain
            .clone()
            .unwrap_or_else(|| "localhost".to_string())
    }
    pub fn get_login_url(&self) -> String {
        format!("{}/login", self.get_real_host())
    }
}
