use serde::Deserialize;

/// Application configuration
#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    /// Server host address
    #[serde(default = "default_host")]
    pub host: String,

    /// Server port
    #[serde(default = "default_port")]
    pub port: u16,

    /// Database URL (SQLite path)
    #[serde(default = "default_database_url")]
    pub database_url: String,

    /// Database encryption key (for SQLCipher)
    pub database_key: Option<String>,

    /// Session secret for cookie signing
    pub session_secret: String,

    /// Session duration in hours
    #[serde(default = "default_session_hours")]
    pub session_hours: u64,

    /// Admin npub (marketplace owner)
    pub admin_npub: String,

    /// Cashu mint configuration
    #[serde(default)]
    pub mint: MintConfig,

    /// Lightning backend configuration
    #[serde(default)]
    pub lightning: LightningConfig,

    /// Marketplace fee percentage (e.g., 1 for 1%)
    #[serde(default = "default_fee_percent")]
    pub fee_percent: u8,

    /// Escrow auto-release days
    #[serde(default = "default_escrow_days")]
    pub escrow_days: u32,

    /// Browsing fee in sats
    #[serde(default = "default_browsing_fee")]
    pub browsing_fee_sats: u64,

    /// Seller bond amounts per category
    #[serde(default)]
    pub seller_bonds: SellerBondConfig,

    /// Price lock duration in checkout (hours)
    #[serde(default = "default_price_lock_hours")]
    pub price_lock_hours: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MintConfig {
    /// External mint URL (e.g., Minibits)
    #[serde(default = "default_mint_url")]
    pub url: String,

    /// Wallet data directory for CDK
    #[serde(default = "default_mint_data_dir")]
    pub data_dir: String,

    /// Unit (sat or msat)
    #[serde(default = "default_mint_unit")]
    pub unit: String,
}

impl Default for MintConfig {
    fn default() -> Self {
        Self {
            url: default_mint_url(),
            data_dir: default_mint_data_dir(),
            unit: default_mint_unit(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct LightningConfig {
    /// Lightning backend type: "lnd", "cln", "lnbits"
    #[serde(default = "default_lightning_backend")]
    pub backend: String,

    /// Backend API URL
    pub url: Option<String>,

    /// API key or macaroon path
    pub api_key: Option<String>,
}

impl Default for LightningConfig {
    fn default() -> Self {
        Self {
            backend: default_lightning_backend(),
            url: None,
            api_key: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct SellerBondConfig {
    /// Bond for digital category (sats)
    #[serde(default = "default_digital_bond")]
    pub digital: u64,

    /// Bond for physical category (sats)
    #[serde(default = "default_physical_bond")]
    pub physical: u64,

    /// Bond for services category (sats)
    #[serde(default = "default_services_bond")]
    pub services: u64,

    /// Bond for all categories (sats)
    #[serde(default = "default_all_bond")]
    pub all: u64,
}

impl Default for SellerBondConfig {
    fn default() -> Self {
        Self {
            digital: default_digital_bond(),
            physical: default_physical_bond(),
            services: default_services_bond(),
            all: default_all_bond(),
        }
    }
}

// Default value functions
fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    3000
}

fn default_database_url() -> String {
    "sqlite:data/marketplace.db".to_string()
}

fn default_session_hours() -> u64 {
    24 * 7 // 1 week
}

fn default_fee_percent() -> u8 {
    1
}

fn default_escrow_days() -> u32 {
    10
}

fn default_browsing_fee() -> u64 {
    100 // 100 sats
}

fn default_price_lock_hours() -> u32 {
    3
}

fn default_mint_url() -> String {
    "https://mint.minibits.cash/Bitcoin".to_string()
}

fn default_mint_data_dir() -> String {
    "data/wallet".to_string()
}

fn default_mint_unit() -> String {
    "sat".to_string()
}

fn default_lightning_backend() -> String {
    "lnbits".to_string()
}

fn default_digital_bond() -> u64 {
    250_000
}

fn default_physical_bond() -> u64 {
    250_000
}

fn default_services_bond() -> u64 {
    250_000
}

fn default_all_bond() -> u64 {
    600_000
}

impl Config {
    /// Load configuration from environment and config file
    pub fn load() -> anyhow::Result<Self> {
        // Load .env file if present
        dotenvy::dotenv().ok();

        let config = config::Config::builder()
            // Start with defaults
            .set_default("host", default_host())?
            .set_default("port", default_port())?
            .set_default("database_url", default_database_url())?
            .set_default("session_hours", default_session_hours())?
            .set_default("fee_percent", default_fee_percent())?
            .set_default("escrow_days", default_escrow_days())?
            .set_default("browsing_fee_sats", default_browsing_fee())?
            .set_default("price_lock_hours", default_price_lock_hours())?
            // Load from config file if exists
            .add_source(config::File::with_name("config").required(false))
            // Override with environment variables (MARKETPLACE_ prefix)
            .add_source(
                config::Environment::with_prefix("MARKETPLACE")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?;

        let config: Config = config.try_deserialize()?;

        // Validate required fields
        if config.session_secret.is_empty() {
            anyhow::bail!("session_secret is required");
        }
        if config.admin_npub.is_empty() {
            anyhow::bail!("admin_npub is required");
        }

        Ok(config)
    }
}
