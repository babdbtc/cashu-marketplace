use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Order model
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Order {
    pub id: String,
    pub checkout_id: String,
    pub buyer_npub: String,
    pub seller_npub: String,
    pub escrow_id: String,
    pub status: String,
    pub tracking_info: Option<String>,
    pub shipped_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Order status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderStatus {
    Pending,
    Shipped,
    Completed,
    Disputed,
    Refunded,
}

impl From<String> for OrderStatus {
    fn from(s: String) -> Self {
        match s.as_str() {
            "shipped" => OrderStatus::Shipped,
            "completed" => OrderStatus::Completed,
            "disputed" => OrderStatus::Disputed,
            "refunded" => OrderStatus::Refunded,
            _ => OrderStatus::Pending,
        }
    }
}

impl From<OrderStatus> for String {
    fn from(s: OrderStatus) -> Self {
        match s {
            OrderStatus::Pending => "pending",
            OrderStatus::Shipped => "shipped",
            OrderStatus::Completed => "completed",
            OrderStatus::Disputed => "disputed",
            OrderStatus::Refunded => "refunded",
        }
        .to_string()
    }
}

impl Order {
    /// Get status as enum
    pub fn status_enum(&self) -> OrderStatus {
        OrderStatus::from(self.status.clone())
    }

    /// Check if order can be confirmed by buyer
    pub fn can_confirm(&self) -> bool {
        matches!(
            self.status_enum(),
            OrderStatus::Pending | OrderStatus::Shipped
        )
    }

    /// Check if order can be disputed
    pub fn can_dispute(&self) -> bool {
        matches!(
            self.status_enum(),
            OrderStatus::Pending | OrderStatus::Shipped
        )
    }

    /// Check if order can be shipped
    pub fn can_ship(&self) -> bool {
        self.status_enum() == OrderStatus::Pending
    }
}

/// Order item
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct OrderItem {
    pub id: String,
    pub order_id: String,
    pub listing_id: String,
    pub price: i64,
    pub encrypted_shipping: Option<String>,
    pub digital_content: Option<String>,
}

/// Order rating
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct OrderRating {
    pub order_id: String,
    pub buyer_npub: String,
    pub seller_npub: String,
    pub rating: i32,
    pub comment: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Order message (post-purchase)
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct OrderMessage {
    pub id: String,
    pub order_id: String,
    pub sender_npub: String,
    pub encrypted_content: String,
    pub created_at: DateTime<Utc>,
}

/// Pre-purchase conversation
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Conversation {
    pub id: String,
    pub buyer_npub: String,
    pub seller_npub: String,
    pub seller_price: i64,
    pub created_at: DateTime<Utc>,
}

/// Pre-purchase message
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ConversationMessage {
    pub id: String,
    pub conversation_id: String,
    pub sender_npub: String,
    pub encrypted_content: String,
    pub payment_amount: Option<i64>,
    pub created_at: DateTime<Utc>,
}

/// Create rating request
#[derive(Debug, Clone, Deserialize)]
pub struct CreateRatingRequest {
    pub rating: i32,
    pub comment: Option<String>,
}

/// Send message request
#[derive(Debug, Clone, Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
}

/// Mark shipped request
#[derive(Debug, Clone, Deserialize)]
pub struct MarkShippedRequest {
    pub tracking_info: Option<String>,
}
