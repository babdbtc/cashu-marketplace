use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::SellerCategory;

/// Listing model
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Listing {
    pub id: String,
    pub seller_npub: String,
    pub title: String,
    pub description: String,
    pub price: i64,
    pub category: String,
    pub is_active: bool,
    pub stock: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl Listing {
    /// Get category as enum
    pub fn category_enum(&self) -> SellerCategory {
        SellerCategory::from(self.category.clone())
    }

    /// Check if listing is available for purchase
    pub fn is_available(&self) -> bool {
        self.is_active
            && self.expires_at > Utc::now()
            && self.stock.map_or(true, |s| s > 0)
    }

    /// Check if listing is expired
    pub fn is_expired(&self) -> bool {
        self.expires_at <= Utc::now()
    }
}

/// Listing image
#[derive(Debug, Clone, FromRow)]
pub struct ListingImage {
    pub id: String,
    pub listing_id: String,
    pub image_data: Vec<u8>,
    pub mime_type: String,
    pub position: i32,
    pub created_at: DateTime<Utc>,
}

/// Cart item
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct CartItem {
    pub id: String,
    pub user_npub: String,
    pub listing_id: String,
    pub added_at: DateTime<Utc>,
}

/// Cart item with listing details (for display)
#[derive(Debug, Clone, Serialize)]
pub struct CartItemWithListing {
    pub cart_item: CartItem,
    pub listing: Listing,
    pub price_changed: bool,
    pub original_price: Option<i64>,
}

/// Checkout session
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct CheckoutSession {
    pub id: String,
    pub user_npub: String,
    pub status: String,
    pub total_amount: i64,
    pub fee_amount: i64,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
}

/// Checkout session status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckoutStatus {
    Pending,
    Paid,
    Expired,
}

impl From<String> for CheckoutStatus {
    fn from(s: String) -> Self {
        match s.as_str() {
            "paid" => CheckoutStatus::Paid,
            "expired" => CheckoutStatus::Expired,
            _ => CheckoutStatus::Pending,
        }
    }
}

impl CheckoutSession {
    /// Check if checkout is expired
    pub fn is_expired(&self) -> bool {
        self.expires_at <= Utc::now() || self.status == "expired"
    }

    /// Get time remaining in seconds
    pub fn time_remaining(&self) -> i64 {
        (self.expires_at - Utc::now()).num_seconds().max(0)
    }
}

/// Checkout item (locked price)
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct CheckoutItem {
    pub id: String,
    pub checkout_id: String,
    pub listing_id: String,
    pub seller_npub: String,
    pub locked_price: i64,
    pub encrypted_shipping: Option<String>,
}

/// Featured slot configuration
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct FeaturedSlot {
    pub id: String,
    pub name: String,
    pub position: String,
    pub price_per_day: i64,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

/// Featured slot rental
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct FeaturedRental {
    pub id: String,
    pub slot_id: String,
    pub listing_id: String,
    pub seller_npub: String,
    pub price_paid: i64,
    pub starts_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl FeaturedRental {
    /// Check if rental is currently active
    pub fn is_active(&self) -> bool {
        let now = Utc::now();
        self.starts_at <= now && self.expires_at > now
    }
}

/// Create listing request
#[derive(Debug, Clone, Deserialize)]
pub struct CreateListingRequest {
    pub title: String,
    pub description: String,
    pub price: i64,
    pub category: String,
    pub stock: Option<i64>,
}

/// Search query for listings
#[derive(Debug, Clone, Deserialize)]
pub struct ListingSearchQuery {
    pub q: Option<String>,
    pub category: Option<String>,
    pub min_price: Option<i64>,
    pub max_price: Option<i64>,
    pub seller: Option<String>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

impl ListingSearchQuery {
    pub fn page(&self) -> u32 {
        self.page.unwrap_or(1).max(1)
    }

    pub fn per_page(&self) -> u32 {
        self.per_page.unwrap_or(20).min(100)
    }

    pub fn offset(&self) -> u32 {
        (self.page() - 1) * self.per_page()
    }
}
