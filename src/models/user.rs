use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// User roles
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    Buyer,
    Seller,
    Admin,
}

impl From<String> for UserRole {
    fn from(s: String) -> Self {
        match s.as_str() {
            "seller" => UserRole::Seller,
            "admin" => UserRole::Admin,
            _ => UserRole::Buyer,
        }
    }
}

impl From<UserRole> for String {
    fn from(role: UserRole) -> Self {
        match role {
            UserRole::Buyer => "buyer".to_string(),
            UserRole::Seller => "seller".to_string(),
            UserRole::Admin => "admin".to_string(),
        }
    }
}

/// User model
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct User {
    pub npub: String,
    pub encrypted_nsec: Option<String>,
    #[sqlx(try_from = "String")]
    pub role: UserRole,
    pub wallet_balance: i64,
    pub message_price: Option<i64>,
    pub last_active_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl User {
    /// Check if user is a seller
    pub fn is_seller(&self) -> bool {
        matches!(self.role, UserRole::Seller | UserRole::Admin)
    }

    /// Check if user is admin
    pub fn is_admin(&self) -> bool {
        matches!(self.role, UserRole::Admin)
    }

    /// Check if seller is active (within 14 days)
    pub fn is_active(&self) -> bool {
        let inactive_threshold = Utc::now() - chrono::Duration::days(14);
        self.last_active_at > inactive_threshold
    }
}

/// Seller category access
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SellerCategory {
    Digital,
    Physical,
    Services,
}

impl From<String> for SellerCategory {
    fn from(s: String) -> Self {
        match s.as_str() {
            "physical" => SellerCategory::Physical,
            "services" => SellerCategory::Services,
            _ => SellerCategory::Digital,
        }
    }
}

impl From<SellerCategory> for String {
    fn from(cat: SellerCategory) -> Self {
        match cat {
            SellerCategory::Digital => "digital".to_string(),
            SellerCategory::Physical => "physical".to_string(),
            SellerCategory::Services => "services".to_string(),
        }
    }
}

/// Seller category bond record
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct SellerCategoryAccess {
    pub npub: String,
    pub category: String,
    pub bond_paid: i64,
    pub paid_at: DateTime<Utc>,
}

/// User session
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Session {
    pub id: String,
    pub user_npub: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl Session {
    /// Check if session is expired
    pub fn is_expired(&self) -> bool {
        self.expires_at < Utc::now()
    }
}

/// Seller statistics
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct SellerStats {
    pub npub: String,
    pub total_sales: i64,
    pub total_revenue: i64,
    pub completed_orders: i64,
    pub disputed_orders: i64,
    pub dispute_rate: f64,
    pub avg_rating: Option<f64>,
    pub updated_at: DateTime<Utc>,
}

/// Wallet transaction record
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct WalletTransaction {
    pub id: String,
    pub user_npub: String,
    pub transaction_type: String,
    pub amount: i64,
    pub balance_after: i64,
    pub reference_id: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Transaction types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionType {
    Deposit,
    Withdraw,
    Payment,
    Receipt,
    Fee,
    Bond,
    EscrowHold,
    EscrowRelease,
    EscrowRefund,
}

impl From<TransactionType> for String {
    fn from(t: TransactionType) -> Self {
        match t {
            TransactionType::Deposit => "deposit",
            TransactionType::Withdraw => "withdraw",
            TransactionType::Payment => "payment",
            TransactionType::Receipt => "receipt",
            TransactionType::Fee => "fee",
            TransactionType::Bond => "bond",
            TransactionType::EscrowHold => "escrow_hold",
            TransactionType::EscrowRelease => "escrow_release",
            TransactionType::EscrowRefund => "escrow_refund",
        }
        .to_string()
    }
}
