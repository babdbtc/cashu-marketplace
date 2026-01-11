use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Escrow model
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Escrow {
    pub id: String,
    pub buyer_npub: String,
    pub seller_npub: String,
    pub amount: i64,
    pub status: String,
    pub auto_release_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

/// Escrow status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EscrowStatus {
    Held,
    Released,
    Refunded,
    Disputed,
}

impl From<String> for EscrowStatus {
    fn from(s: String) -> Self {
        match s.as_str() {
            "released" => EscrowStatus::Released,
            "refunded" => EscrowStatus::Refunded,
            "disputed" => EscrowStatus::Disputed,
            _ => EscrowStatus::Held,
        }
    }
}

impl From<EscrowStatus> for String {
    fn from(s: EscrowStatus) -> Self {
        match s {
            EscrowStatus::Held => "held",
            EscrowStatus::Released => "released",
            EscrowStatus::Refunded => "refunded",
            EscrowStatus::Disputed => "disputed",
        }
        .to_string()
    }
}

impl Escrow {
    /// Get status as enum
    pub fn status_enum(&self) -> EscrowStatus {
        EscrowStatus::from(self.status.clone())
    }

    /// Check if escrow can be released
    pub fn can_release(&self) -> bool {
        self.status_enum() == EscrowStatus::Held
    }

    /// Check if escrow can be refunded
    pub fn can_refund(&self) -> bool {
        self.status_enum() == EscrowStatus::Held
    }

    /// Check if escrow should auto-release
    pub fn should_auto_release(&self) -> bool {
        self.status_enum() == EscrowStatus::Held && self.auto_release_at <= Utc::now()
    }

    /// Get time until auto-release in seconds
    pub fn time_until_release(&self) -> i64 {
        (self.auto_release_at - Utc::now()).num_seconds().max(0)
    }
}

/// Dispute model
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Dispute {
    pub id: String,
    pub order_id: String,
    pub escrow_id: String,
    pub initiated_by: String,
    pub reason: String,
    pub status: String,
    pub resolution: Option<String>,
    pub resolution_notes: Option<String>,
    pub resolved_by: Option<String>,
    pub warning_sent_at: Option<DateTime<Utc>>,
    pub auto_resolve_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

/// Dispute status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisputeStatus {
    Open,
    Resolved,
}

impl From<String> for DisputeStatus {
    fn from(s: String) -> Self {
        match s.as_str() {
            "resolved" => DisputeStatus::Resolved,
            _ => DisputeStatus::Open,
        }
    }
}

impl Dispute {
    /// Get status as enum
    pub fn status_enum(&self) -> DisputeStatus {
        DisputeStatus::from(self.status.clone())
    }

    /// Check if dispute is open
    pub fn is_open(&self) -> bool {
        self.status_enum() == DisputeStatus::Open
    }

    /// Check if dispute should auto-resolve
    pub fn should_auto_resolve(&self) -> bool {
        self.is_open() && self.auto_resolve_at <= Utc::now()
    }

    /// Check if warning should be sent (7 days before auto-resolve)
    pub fn should_send_warning(&self) -> bool {
        self.is_open()
            && self.warning_sent_at.is_none()
            && (self.auto_resolve_at - Utc::now()).num_days() <= 7
    }
}

/// Dispute resolution types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisputeResolution {
    /// Full refund to buyer
    BuyerFull,
    /// Full release to seller
    SellerFull,
    /// Split: X% to buyer, Y% to seller
    Split { buyer_percent: u8, seller_percent: u8 },
    /// Burn funds (extreme cases)
    Burn,
}

impl DisputeResolution {
    /// Parse from string like "buyer_full", "seller_full", "split_50_50", "burn"
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "buyer_full" => Some(Self::BuyerFull),
            "seller_full" => Some(Self::SellerFull),
            "burn" => Some(Self::Burn),
            s if s.starts_with("split_") => {
                let parts: Vec<&str> = s[6..].split('_').collect();
                if parts.len() == 2 {
                    let buyer: u8 = parts[0].parse().ok()?;
                    let seller: u8 = parts[1].parse().ok()?;
                    if buyer + seller == 100 {
                        return Some(Self::Split {
                            buyer_percent: buyer,
                            seller_percent: seller,
                        });
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Convert to string
    pub fn to_str(&self) -> String {
        match self {
            Self::BuyerFull => "buyer_full".to_string(),
            Self::SellerFull => "seller_full".to_string(),
            Self::Burn => "burn".to_string(),
            Self::Split {
                buyer_percent,
                seller_percent,
            } => format!("split_{}_{}", buyer_percent, seller_percent),
        }
    }

    /// Calculate amounts for buyer and seller
    pub fn calculate_amounts(&self, total: i64) -> (i64, i64) {
        match self {
            Self::BuyerFull => (total, 0),
            Self::SellerFull => (0, total),
            Self::Burn => (0, 0),
            Self::Split {
                buyer_percent,
                seller_percent,
            } => {
                let buyer_amount = (total * *buyer_percent as i64) / 100;
                let seller_amount = (total * *seller_percent as i64) / 100;
                (buyer_amount, seller_amount)
            }
        }
    }
}

/// Dispute evidence
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct DisputeEvidence {
    pub id: String,
    pub dispute_id: String,
    pub submitted_by: String,
    pub evidence_type: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

/// Evidence type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceType {
    Text,
    Image,
}

impl From<String> for EvidenceType {
    fn from(s: String) -> Self {
        match s.as_str() {
            "image" => EvidenceType::Image,
            _ => EvidenceType::Text,
        }
    }
}

/// Create dispute request
#[derive(Debug, Clone, Deserialize)]
pub struct CreateDisputeRequest {
    pub reason: String,
}

/// Resolve dispute request
#[derive(Debug, Clone, Deserialize)]
pub struct ResolveDisputeRequest {
    pub resolution: String,
    pub notes: Option<String>,
}

/// Submit evidence request
#[derive(Debug, Clone, Deserialize)]
pub struct SubmitEvidenceRequest {
    pub evidence_type: String,
    pub content: String,
}
