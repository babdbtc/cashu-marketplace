use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use thiserror::Error;

/// Application error types
#[derive(Error, Debug)]
pub enum AppError {
    // Authentication errors
    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Invalid nsec format")]
    InvalidNsec,

    #[error("Session expired")]
    SessionExpired,

    #[error("Not authenticated")]
    NotAuthenticated,

    #[error("Not authorized")]
    NotAuthorized,

    // User errors
    #[error("User not found")]
    UserNotFound,

    #[error("User already exists")]
    UserAlreadyExists,

    // Listing errors
    #[error("Listing not found")]
    ListingNotFound,

    #[error("Listing not available")]
    ListingNotAvailable,

    #[error("Invalid listing category")]
    InvalidCategory,

    #[error("Seller not authorized for category")]
    CategoryNotAuthorized,

    // Order errors
    #[error("Order not found")]
    OrderNotFound,

    #[error("Order already completed")]
    OrderAlreadyCompleted,

    #[error("Order cannot be disputed")]
    OrderCannotBeDisputed,

    // Cart errors
    #[error("Cart is empty")]
    CartEmpty,

    #[error("Price lock expired")]
    PriceLockExpired,

    #[error("Item already in cart")]
    ItemAlreadyInCart,

    // Payment errors
    #[error("Insufficient balance: need {needed} sats, have {available} sats")]
    InsufficientBalanceDetails { needed: u64, available: u64 },

    #[error("Insufficient balance")]
    InsufficientBalance,

    #[error("Invalid Cashu token")]
    InvalidCashuToken,

    #[error("Payment failed: {0}")]
    PaymentFailed(String),

    #[error("Withdrawal failed: {0}")]
    WithdrawalFailed(String),

    // Escrow errors
    #[error("Escrow not found")]
    EscrowNotFound,

    #[error("Escrow already released")]
    EscrowAlreadyReleased,

    #[error("Escrow already refunded")]
    EscrowAlreadyRefunded,

    // Dispute errors
    #[error("Dispute not found")]
    DisputeNotFound,

    #[error("Dispute already resolved")]
    DisputeAlreadyResolved,

    #[error("Invalid resolution")]
    InvalidResolution,

    // Seller errors
    #[error("Not a seller")]
    NotASeller,

    #[error("Seller inactive")]
    SellerInactive,

    #[error("Bond already paid for category")]
    BondAlreadyPaid,

    // Messaging errors
    #[error("Messaging disabled by seller")]
    MessagingDisabled,

    #[error("Message too long")]
    MessageTooLong,

    // Featured listing errors
    #[error("Slot not found")]
    SlotNotFound,

    #[error("Slot not available")]
    SlotNotAvailable,

    #[error("Slot already occupied")]
    SlotOccupied,

    #[error("Invalid duration: must be 1-14 days")]
    InvalidDuration,

    // Rate limiting
    #[error("Rate limited, try again later")]
    RateLimited,

    // Browsing fee
    #[error("Browsing fee required")]
    BrowsingFeeRequired,

    #[error("Invalid browsing token")]
    InvalidBrowsingToken,

    // Database errors
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    // Internal errors
    #[error("Internal error: {0}")]
    Internal(String),

    // Redirect (for flow control)
    #[error("Redirect to {0}")]
    Redirect(String),

    // Invalid input
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // Handle redirect specially
        if let AppError::Redirect(url) = &self {
            return axum::response::Redirect::to(url).into_response();
        }

        let (status, message) = match &self {
            // 400 Bad Request
            AppError::InvalidNsec
            | AppError::InvalidCategory
            | AppError::InvalidCashuToken
            | AppError::InvalidResolution
            | AppError::InvalidDuration
            | AppError::InvalidBrowsingToken
            | AppError::MessageTooLong
            | AppError::ItemAlreadyInCart
            | AppError::InvalidInput(_)
            | AppError::BondAlreadyPaid => (StatusCode::BAD_REQUEST, self.to_string()),

            // 401 Unauthorized
            AppError::InvalidCredentials
            | AppError::SessionExpired
            | AppError::NotAuthenticated => (StatusCode::UNAUTHORIZED, self.to_string()),

            // 402 Payment Required
            AppError::InsufficientBalanceDetails { .. }
            | AppError::InsufficientBalance
            | AppError::BrowsingFeeRequired
            | AppError::PaymentFailed(_) => (StatusCode::PAYMENT_REQUIRED, self.to_string()),

            // 403 Forbidden
            AppError::NotAuthorized
            | AppError::CategoryNotAuthorized
            | AppError::NotASeller
            | AppError::MessagingDisabled => (StatusCode::FORBIDDEN, self.to_string()),

            // 404 Not Found
            AppError::UserNotFound
            | AppError::ListingNotFound
            | AppError::OrderNotFound
            | AppError::EscrowNotFound
            | AppError::DisputeNotFound
            | AppError::SlotNotFound => (StatusCode::NOT_FOUND, self.to_string()),

            // 409 Conflict
            AppError::UserAlreadyExists
            | AppError::ListingNotAvailable
            | AppError::OrderAlreadyCompleted
            | AppError::OrderCannotBeDisputed
            | AppError::EscrowAlreadyReleased
            | AppError::EscrowAlreadyRefunded
            | AppError::DisputeAlreadyResolved
            | AppError::SlotNotAvailable
            | AppError::SlotOccupied
            | AppError::PriceLockExpired
            | AppError::CartEmpty
            | AppError::SellerInactive => (StatusCode::CONFLICT, self.to_string()),

            // 429 Too Many Requests
            AppError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, self.to_string()),

            // 500 Internal Server Error
            AppError::Database(_)
            | AppError::Internal(_)
            | AppError::WithdrawalFailed(_) => {
                tracing::error!("Internal error: {}", self);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }

            // Redirect is handled above, this is unreachable
            AppError::Redirect(_) => unreachable!(),
        };

        (status, message).into_response()
    }
}

/// Result type alias for convenience
pub type AppResult<T> = Result<T, AppError>;
