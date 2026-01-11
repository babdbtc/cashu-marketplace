use std::sync::Arc;

use askama::Template;
use axum::{
    extract::{Path, State},
    response::{Html, Redirect},
    Form,
};
use axum_extra::extract::CookieJar;
use chrono::{Duration, Utc};
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::models::{CartItem, CheckoutItem, CheckoutSession, Listing};
use crate::routes::auth::get_current_user;
use crate::services::EscrowService;
use crate::AppState;

#[derive(Template)]
#[template(path = "cart/show.html")]
struct CartTemplate {
    title: String,
    items: Vec<CartItemView>,
    subtotal: i64,
    fee: i64,
    total: i64,
}

struct CartItemView {
    cart_item: CartItem,
    listing: Listing,
}

#[derive(Template)]
#[template(path = "cart/checkout.html")]
struct CheckoutTemplate {
    title: String,
    checkout: CheckoutSession,
    items: Vec<CheckoutItemView>,
    time_remaining: i64,
    wallet_balance: i64,
}

struct CheckoutItemView {
    item: CheckoutItem,
    listing: Listing,
}

#[derive(Deserialize)]
pub struct CheckoutForm {
    payment_method: String, // "wallet" or "external"
    cashu_token: Option<String>,
}

/// Show cart
pub async fn show(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    // Get cart items with listings
    let cart_items: Vec<CartItem> =
        sqlx::query_as("SELECT * FROM cart_items WHERE user_npub = ? ORDER BY added_at DESC")
            .bind(&user.npub)
            .fetch_all(state.db.pool())
            .await?;

    let mut items = Vec::new();
    let mut subtotal: i64 = 0;

    for cart_item in cart_items {
        let listing: Listing = sqlx::query_as("SELECT * FROM listings WHERE id = ?")
            .bind(&cart_item.listing_id)
            .fetch_one(state.db.pool())
            .await?;

        subtotal += listing.price;
        items.push(CartItemView { cart_item, listing });
    }

    let fee = (subtotal * state.config.fee_percent as i64) / 100;
    let total = subtotal + fee;

    let template = CartTemplate {
        title: "Shopping Cart".to_string(),
        items,
        subtotal,
        fee,
        total,
    };

    let html = template
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Html(html))
}

/// Add item to cart
pub async fn add(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(listing_id): Path<String>,
) -> AppResult<Redirect> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    // Check listing exists and is available
    let listing: Listing = sqlx::query_as("SELECT * FROM listings WHERE id = ?")
        .bind(&listing_id)
        .fetch_optional(state.db.pool())
        .await?
        .ok_or(AppError::ListingNotFound)?;

    if !listing.is_available() {
        return Err(AppError::ListingNotAvailable);
    }

    // Check not already in cart
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM cart_items WHERE user_npub = ? AND listing_id = ?")
            .bind(&user.npub)
            .bind(&listing_id)
            .fetch_optional(state.db.pool())
            .await?;

    if existing.is_some() {
        return Err(AppError::ItemAlreadyInCart);
    }

    // Add to cart
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO cart_items (id, user_npub, listing_id, added_at) VALUES (?, ?, ?, CURRENT_TIMESTAMP)")
        .bind(&id)
        .bind(&user.npub)
        .bind(&listing_id)
        .execute(state.db.pool())
        .await?;

    Ok(Redirect::to("/cart"))
}

/// Remove item from cart
pub async fn remove(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(item_id): Path<String>,
) -> AppResult<Redirect> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    sqlx::query("DELETE FROM cart_items WHERE id = ? AND user_npub = ?")
        .bind(&item_id)
        .bind(&user.npub)
        .execute(state.db.pool())
        .await?;

    Ok(Redirect::to("/cart"))
}

/// Checkout page (create checkout session with price lock)
pub async fn checkout_page(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    // Check for existing pending checkout
    let existing: Option<CheckoutSession> = sqlx::query_as(
        "SELECT * FROM checkout_sessions WHERE user_npub = ? AND status = 'pending' AND expires_at > CURRENT_TIMESTAMP",
    )
    .bind(&user.npub)
    .fetch_optional(state.db.pool())
    .await?;

    let checkout = if let Some(session) = existing {
        session
    } else {
        // Create new checkout session
        let cart_items: Vec<CartItem> =
            sqlx::query_as("SELECT * FROM cart_items WHERE user_npub = ?")
                .bind(&user.npub)
                .fetch_all(state.db.pool())
                .await?;

        if cart_items.is_empty() {
            return Err(AppError::CartEmpty);
        }

        let checkout_id = uuid::Uuid::new_v4().to_string();
        let mut total_amount: i64 = 0;

        // Lock prices by creating checkout items
        for cart_item in &cart_items {
            let listing: Listing = sqlx::query_as("SELECT * FROM listings WHERE id = ?")
                .bind(&cart_item.listing_id)
                .fetch_one(state.db.pool())
                .await?;

            if !listing.is_available() {
                continue; // Skip unavailable items
            }

            total_amount += listing.price;

            let item_id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO checkout_items (id, checkout_id, listing_id, seller_npub, locked_price) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&item_id)
            .bind(&checkout_id)
            .bind(&listing.id)
            .bind(&listing.seller_npub)
            .bind(listing.price)
            .execute(state.db.pool())
            .await?;
        }

        let fee_amount = (total_amount * state.config.fee_percent as i64) / 100;
        let expires_at = Utc::now() + Duration::hours(state.config.price_lock_hours as i64);

        sqlx::query(
            "INSERT INTO checkout_sessions (id, user_npub, status, total_amount, fee_amount, created_at, expires_at) VALUES (?, ?, 'pending', ?, ?, CURRENT_TIMESTAMP, ?)",
        )
        .bind(&checkout_id)
        .bind(&user.npub)
        .bind(total_amount)
        .bind(fee_amount)
        .bind(expires_at)
        .execute(state.db.pool())
        .await?;

        sqlx::query_as("SELECT * FROM checkout_sessions WHERE id = ?")
            .bind(&checkout_id)
            .fetch_one(state.db.pool())
            .await?
    };

    // Get checkout items
    let items: Vec<CheckoutItem> =
        sqlx::query_as("SELECT * FROM checkout_items WHERE checkout_id = ?")
            .bind(&checkout.id)
            .fetch_all(state.db.pool())
            .await?;

    let mut item_views = Vec::new();
    for item in items {
        let listing: Listing = sqlx::query_as("SELECT * FROM listings WHERE id = ?")
            .bind(&item.listing_id)
            .fetch_one(state.db.pool())
            .await?;
        item_views.push(CheckoutItemView { item, listing });
    }

    let template = CheckoutTemplate {
        title: "Checkout".to_string(),
        time_remaining: checkout.time_remaining(),
        wallet_balance: user.wallet_balance,
        checkout,
        items: item_views,
    };

    let html = template
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Html(html))
}

/// Process checkout payment
pub async fn checkout(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Form(form): Form<CheckoutForm>,
) -> AppResult<Redirect> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    // Get pending checkout
    let checkout: CheckoutSession = sqlx::query_as(
        "SELECT * FROM checkout_sessions WHERE user_npub = ? AND status = 'pending' AND expires_at > CURRENT_TIMESTAMP",
    )
    .bind(&user.npub)
    .fetch_optional(state.db.pool())
    .await?
    .ok_or(AppError::PriceLockExpired)?;

    let total = checkout.total_amount + checkout.fee_amount;

    // Process payment
    match form.payment_method.as_str() {
        "wallet" => {
            if user.wallet_balance < total {
                return Err(AppError::InsufficientBalanceDetails {
                    needed: total as u64,
                    available: user.wallet_balance as u64,
                });
            }

            // Deduct from wallet
            sqlx::query("UPDATE users SET wallet_balance = wallet_balance - ? WHERE npub = ?")
                .bind(total)
                .bind(&user.npub)
                .execute(state.db.pool())
                .await?;
        }
        "external" => {
            // Receive Cashu token
            let token = form.cashu_token.ok_or(AppError::InvalidCashuToken)?;
            let amount = state.cashu.receive_tokens(&token).await?;

            if (amount as i64) < total {
                return Err(AppError::InsufficientBalanceDetails {
                    needed: total as u64,
                    available: amount,
                });
            }

            // If overpaid, credit difference to wallet
            if (amount as i64) > total {
                let overpayment = amount as i64 - total;
                sqlx::query("UPDATE users SET wallet_balance = wallet_balance + ? WHERE npub = ?")
                    .bind(overpayment)
                    .bind(&user.npub)
                    .execute(state.db.pool())
                    .await?;
            }
        }
        _ => return Err(AppError::PaymentFailed("Invalid payment method".to_string())),
    }

    // Mark checkout as paid
    sqlx::query("UPDATE checkout_sessions SET status = 'paid', paid_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(&checkout.id)
        .execute(state.db.pool())
        .await?;

    // Create escrows and orders grouped by seller
    let items: Vec<CheckoutItem> =
        sqlx::query_as("SELECT * FROM checkout_items WHERE checkout_id = ?")
            .bind(&checkout.id)
            .fetch_all(state.db.pool())
            .await?;

    // Group items by seller
    let mut seller_items: std::collections::HashMap<String, Vec<CheckoutItem>> =
        std::collections::HashMap::new();
    for item in items {
        seller_items
            .entry(item.seller_npub.clone())
            .or_default()
            .push(item);
    }

    // Create one escrow and order per seller
    for (seller_npub, items) in seller_items {
        let seller_total: i64 = items.iter().map(|i| i.locked_price).sum();

        // Create escrow
        let escrow = EscrowService::create_escrow(
            &state.db,
            &user.npub,
            &seller_npub,
            seller_total,
            state.config.escrow_days,
        )
        .await?;

        // Create order
        let order_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO orders (id, checkout_id, buyer_npub, seller_npub, escrow_id, status, created_at) VALUES (?, ?, ?, ?, ?, 'pending', CURRENT_TIMESTAMP)",
        )
        .bind(&order_id)
        .bind(&checkout.id)
        .bind(&user.npub)
        .bind(&seller_npub)
        .bind(&escrow.id)
        .execute(state.db.pool())
        .await?;

        // Create order items
        for item in items {
            let item_id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO order_items (id, order_id, listing_id, price, encrypted_shipping) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&item_id)
            .bind(&order_id)
            .bind(&item.listing_id)
            .bind(item.locked_price)
            .bind(&item.encrypted_shipping)
            .execute(state.db.pool())
            .await?;
        }
    }

    // Clear cart
    sqlx::query("DELETE FROM cart_items WHERE user_npub = ?")
        .bind(&user.npub)
        .execute(state.db.pool())
        .await?;

    Ok(Redirect::to("/orders"))
}
