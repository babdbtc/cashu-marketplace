use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use cdk::nuts::{CurrencyUnit, MintQuoteState, Token};
use cdk::wallet::{ReceiveOptions, Wallet, WalletBuilder};
use cdk::Amount;
use cdk_sqlite::WalletSqliteDatabase;

use crate::config::Config;
use crate::error::{AppError, AppResult};

/// Cashu wallet service using an external mint (e.g., Minibits)
///
/// This service manages ecash operations through CDK wallet.
/// It connects to an external Cashu mint for real token operations.
/// Mock mode is available for offline testing.
pub struct CashuService {
    /// CDK wallet instance
    wallet: Option<Arc<Wallet>>,
    /// Mint URL
    mint_url: String,
    /// Pending mint quotes (quote_id -> amount)
    pending_quotes: Arc<RwLock<HashMap<String, u64>>>,
    /// Mock mode for offline testing
    mock_mode: bool,
    /// Mock spent tokens (for mock mode only)
    mock_spent_tokens: Arc<RwLock<HashMap<String, bool>>>,
}

impl CashuService {
    /// Initialize Cashu service with external mint
    pub async fn new(config: &Config) -> anyhow::Result<Self> {
        let mint_url = config.mint.url.clone();
        let mock_mode = mint_url.is_empty() || mint_url == "mock";

        if mock_mode {
            tracing::warn!("Cashu service running in MOCK MODE - no real payments");
            return Ok(Self {
                wallet: None,
                mint_url: "mock".to_string(),
                pending_quotes: Arc::new(RwLock::new(HashMap::new())),
                mock_mode: true,
                mock_spent_tokens: Arc::new(RwLock::new(HashMap::new())),
            });
        }

        // Create wallet data directory
        std::fs::create_dir_all(&config.mint.data_dir)?;

        // Parse unit
        let unit = match config.mint.unit.as_str() {
            "msat" => CurrencyUnit::Msat,
            _ => CurrencyUnit::Sat,
        };

        // Create SQLite database for wallet
        let db_path = format!("{}/wallet.db", config.mint.data_dir);
        let localstore = WalletSqliteDatabase::new(db_path.as_str()).await?;

        // Generate or load seed (64 bytes for CDK)
        let seed = Self::get_or_create_seed(&config.mint.data_dir)?;

        // Create wallet using builder
        let wallet = WalletBuilder::new()
            .mint_url(mint_url.parse()?)
            .unit(unit)
            .localstore(Arc::new(localstore))
            .seed(seed)
            .build()?;

        tracing::info!("Cashu wallet connected to mint: {}", mint_url);

        Ok(Self {
            wallet: Some(Arc::new(wallet)),
            mint_url,
            pending_quotes: Arc::new(RwLock::new(HashMap::new())),
            mock_mode: false,
            mock_spent_tokens: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create a Lightning invoice for deposit (mint quote)
    pub async fn create_deposit_invoice(&self, amount_sats: u64) -> AppResult<DepositInvoice> {
        if self.mock_mode {
            return self.mock_create_deposit_invoice(amount_sats).await;
        }

        let wallet = self.wallet.as_ref().ok_or_else(|| {
            AppError::Internal("Wallet not initialized".to_string())
        })?;

        // Create mint quote (Lightning invoice)
        let quote = wallet
            .mint_quote(Amount::from(amount_sats), None)
            .await
            .map_err(|e| AppError::PaymentFailed(e.to_string()))?;

        let quote_id = quote.id.clone();
        let payment_request = quote.request.clone();
        let expires_at = chrono::Utc::now() + chrono::Duration::hours(1);

        // Store pending quote
        self.pending_quotes
            .write()
            .await
            .insert(quote_id.clone(), amount_sats);

        Ok(DepositInvoice {
            payment_request,
            payment_hash: quote_id,
            amount_sats,
            expires_at,
        })
    }

    /// Check if a mint quote (deposit invoice) has been paid
    pub async fn check_invoice_paid(&self, quote_id: &str) -> AppResult<bool> {
        if self.mock_mode {
            return Ok(true); // Mock mode auto-pays
        }

        let wallet = self.wallet.as_ref().ok_or_else(|| {
            AppError::Internal("Wallet not initialized".to_string())
        })?;

        // Check quote status
        let status = wallet
            .mint_quote_state(quote_id)
            .await
            .map_err(|e| AppError::PaymentFailed(e.to_string()))?;

        Ok(status.state == MintQuoteState::Paid)
    }

    /// Mint tokens after invoice is paid (claim from mint)
    pub async fn mint_tokens(&self, quote_id: &str, _amount_sats: u64) -> AppResult<String> {
        if self.mock_mode {
            return self.mock_mint_tokens(_amount_sats).await;
        }

        let wallet = self.wallet.as_ref().ok_or_else(|| {
            AppError::Internal("Wallet not initialized".to_string())
        })?;

        // Check if paid
        if !self.check_invoice_paid(quote_id).await? {
            return Err(AppError::PaymentFailed("Invoice not paid".to_string()));
        }

        // Mint tokens (proofs are stored in wallet)
        let proofs = wallet
            .mint(quote_id, cdk::amount::SplitTarget::default(), None)
            .await
            .map_err(|e| AppError::PaymentFailed(e.to_string()))?;

        // Create token from proofs
        let mint_url = self.mint_url.parse().map_err(|e| {
            AppError::Internal(format!("Invalid mint URL: {}", e))
        })?;

        let token = Token::new(mint_url, proofs, None, wallet.unit.clone());
        let token_str = token.to_string();

        // Remove from pending
        self.pending_quotes.write().await.remove(quote_id);

        Ok(token_str)
    }

    /// Receive and validate Cashu tokens from external source
    pub async fn receive_tokens(&self, token_str: &str) -> AppResult<u64> {
        if self.mock_mode {
            return self.mock_receive_tokens(token_str).await;
        }

        let wallet = self.wallet.as_ref().ok_or_else(|| {
            AppError::Internal("Wallet not initialized".to_string())
        })?;

        // Receive (swap) tokens through the mint
        let amount = wallet
            .receive(token_str, ReceiveOptions::default())
            .await
            .map_err(|e| {
                tracing::error!("Failed to receive token: {}", e);
                AppError::InvalidCashuToken
            })?;

        Ok(u64::from(amount))
    }

    /// Withdraw to Lightning invoice (melt tokens)
    pub async fn withdraw(&self, invoice: &str, amount_sats: u64) -> AppResult<WithdrawalResult> {
        if self.mock_mode {
            return self.mock_withdraw(amount_sats).await;
        }

        let wallet = self.wallet.as_ref().ok_or_else(|| {
            AppError::Internal("Wallet not initialized".to_string())
        })?;

        // Validate invoice format
        if !invoice.starts_with("lnbc") && !invoice.starts_with("lntb") {
            return Err(AppError::WithdrawalFailed(
                "Invalid Lightning invoice format".to_string(),
            ));
        }

        // Create melt quote
        let quote = wallet
            .melt_quote(invoice.to_string(), None)
            .await
            .map_err(|e| AppError::WithdrawalFailed(e.to_string()))?;

        // Execute melt
        let melt_response = wallet
            .melt(&quote.id)
            .await
            .map_err(|e| AppError::WithdrawalFailed(e.to_string()))?;

        Ok(WithdrawalResult {
            preimage: melt_response
                .preimage
                .unwrap_or_else(|| "unknown".to_string()),
            amount_paid: amount_sats,
            fee_paid: u64::from(quote.fee_reserve),
        })
    }

    /// Get wallet balance
    pub async fn get_balance(&self) -> AppResult<u64> {
        if self.mock_mode {
            return Ok(0);
        }

        let wallet = self.wallet.as_ref().ok_or_else(|| {
            AppError::Internal("Wallet not initialized".to_string())
        })?;

        let balance = wallet.total_balance().await.map_err(|e| {
            AppError::Internal(e.to_string())
        })?;

        Ok(u64::from(balance))
    }

    /// Validate a browsing token (X-Cashu header)
    pub async fn validate_browsing_token(&self, token_str: &str) -> AppResult<BrowsingTokenInfo> {
        // For browsing tokens, we receive them (which validates and claims)
        let amount = self.receive_tokens(token_str).await?;

        if amount < 10 {
            return Err(AppError::InvalidBrowsingToken);
        }

        Ok(BrowsingTokenInfo {
            amount_sats: amount,
            is_valid: true,
        })
    }

    /// Create tokens for a user (from wallet balance)
    pub async fn create_tokens(&self, amount_sats: u64) -> AppResult<String> {
        if self.mock_mode {
            return self.mock_mint_tokens(amount_sats).await;
        }

        let wallet = self.wallet.as_ref().ok_or_else(|| {
            AppError::Internal("Wallet not initialized".to_string())
        })?;

        // Prepare send (create token from wallet balance)
        let prepared = wallet
            .prepare_send(
                Amount::from(amount_sats),
                cdk::wallet::SendOptions::default(),
            )
            .await
            .map_err(|e| AppError::PaymentFailed(e.to_string()))?;

        // Confirm to get the token
        let token = prepared
            .confirm(None)
            .await
            .map_err(|e| AppError::PaymentFailed(e.to_string()))?;

        Ok(token.to_string())
    }

    /// Get mint info
    pub fn mint_info(&self) -> MintInfo {
        MintInfo {
            url: self.mint_url.clone(),
            mock_mode: self.mock_mode,
        }
    }

    /// Check if running in mock mode
    pub fn is_mock_mode(&self) -> bool {
        self.mock_mode
    }

    // --- Mock mode helpers ---

    async fn mock_create_deposit_invoice(&self, amount_sats: u64) -> AppResult<DepositInvoice> {
        let quote_id = Self::generate_hash();
        let payment_request = format!("lnbc{}n1mock{}", amount_sats, &quote_id[..16]);
        let expires_at = chrono::Utc::now() + chrono::Duration::hours(1);

        self.pending_quotes
            .write()
            .await
            .insert(quote_id.clone(), amount_sats);

        Ok(DepositInvoice {
            payment_request,
            payment_hash: quote_id,
            amount_sats,
            expires_at,
        })
    }

    async fn mock_mint_tokens(&self, amount_sats: u64) -> AppResult<String> {
        let random = Self::generate_hash();
        Ok(format!("cashuA{}_{}_mock", amount_sats, &random[..32]))
    }

    async fn mock_receive_tokens(&self, token_str: &str) -> AppResult<u64> {
        // Parse mock token format: cashuA{amount}_{random}_mock
        if !token_str.starts_with("cashuA") {
            return Err(AppError::InvalidCashuToken);
        }

        // Check double spend
        let token_id = Self::token_id(token_str);
        {
            let spent = self.mock_spent_tokens.read().await;
            if spent.contains_key(&token_id) {
                return Err(AppError::InvalidCashuToken);
            }
        }

        // Parse amount
        let parts: Vec<&str> = token_str[6..].split('_').collect();
        let amount: u64 = parts
            .first()
            .and_then(|s| s.parse().ok())
            .ok_or(AppError::InvalidCashuToken)?;

        // Mark as spent
        self.mock_spent_tokens
            .write()
            .await
            .insert(token_id, true);

        Ok(amount)
    }

    async fn mock_withdraw(&self, amount_sats: u64) -> AppResult<WithdrawalResult> {
        Ok(WithdrawalResult {
            preimage: Self::generate_hash(),
            amount_paid: amount_sats,
            fee_paid: 0,
        })
    }

    // --- Internal helpers ---

    fn generate_hash() -> String {
        use rand::Rng;
        let bytes: [u8; 32] = rand::thread_rng().gen();
        hex::encode(bytes)
    }

    fn token_id(token: &str) -> String {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(token.as_bytes());
        hex::encode(hash)
    }

    fn get_or_create_seed(data_dir: &str) -> anyhow::Result<[u8; 64]> {
        let seed_path = format!("{}/seed", data_dir);

        if let Ok(seed_hex) = std::fs::read_to_string(&seed_path) {
            let seed_bytes = hex::decode(seed_hex.trim())?;
            if seed_bytes.len() == 64 {
                let mut seed = [0u8; 64];
                seed.copy_from_slice(&seed_bytes);
                return Ok(seed);
            }
        }

        // Generate new seed (64 bytes for CDK) using getrandom
        let mut seed = [0u8; 64];
        getrandom::getrandom(&mut seed)?;

        // Save seed
        std::fs::write(&seed_path, hex::encode(seed))?;
        tracing::info!("Generated new wallet seed");

        Ok(seed)
    }
}

/// Deposit invoice details
#[derive(Debug, Clone)]
pub struct DepositInvoice {
    pub payment_request: String,
    pub payment_hash: String,
    pub amount_sats: u64,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Withdrawal result
#[derive(Debug, Clone)]
pub struct WithdrawalResult {
    pub preimage: String,
    pub amount_paid: u64,
    pub fee_paid: u64,
}

/// Browsing token validation result
#[derive(Debug, Clone)]
pub struct BrowsingTokenInfo {
    pub amount_sats: u64,
    pub is_valid: bool,
}

/// Mint info for display
#[derive(Debug, Clone)]
pub struct MintInfo {
    pub url: String,
    pub mock_mode: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_service() -> CashuService {
        CashuService {
            wallet: None,
            mint_url: "mock".to_string(),
            pending_quotes: Arc::new(RwLock::new(HashMap::new())),
            mock_mode: true,
            mock_spent_tokens: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    #[tokio::test]
    async fn test_mock_deposit_flow() {
        let service = mock_service();

        // Create invoice
        let invoice = service.create_deposit_invoice(1000).await.unwrap();
        assert!(invoice.payment_request.contains("1000"));

        // Check paid (mock always returns true)
        assert!(service.check_invoice_paid(&invoice.payment_hash).await.unwrap());

        // Mint tokens
        let token = service.mint_tokens(&invoice.payment_hash, 1000).await.unwrap();
        assert!(token.starts_with("cashuA"));
    }

    #[tokio::test]
    async fn test_mock_receive_tokens() {
        let service = mock_service();

        let token = service.mock_mint_tokens(500).await.unwrap();
        let amount = service.receive_tokens(&token).await.unwrap();
        assert_eq!(amount, 500);

        // Double spend should fail
        assert!(service.receive_tokens(&token).await.is_err());
    }

    #[tokio::test]
    async fn test_mock_withdraw() {
        let service = mock_service();

        let result = service.withdraw("lnbc1000n1test", 1000).await.unwrap();
        assert_eq!(result.amount_paid, 1000);
        assert_eq!(result.fee_paid, 0);
    }
}
