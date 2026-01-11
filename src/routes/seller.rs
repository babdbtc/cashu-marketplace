use std::sync::Arc;

use askama::Template;
use axum::{
    extract::{Path, State},
    response::{Html, Redirect},
    Form,
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::models::{Listing, MarkShippedRequest, Order, SellerStats};
use crate::routes::auth::get_current_user;
use crate::AppState;

#[derive(Template)]
#[template(path = "seller/dashboard.html")]
struct DashboardTemplate {
    title: String,
    stats: Option<SellerStats>,
    listings: Vec<Listing>,
    pending_orders: i64,
    wallet_balance: i64,
}

#[derive(Template)]
#[template(path = "seller/orders.html")]
struct SellerOrdersTemplate {
    title: String,
    orders: Vec<SellerOrderView>,
}

struct SellerOrderView {
    order: Order,
    buyer_npub_short: String,
    item_count: i64,
    total_amount: i64,
}

/// Seller dashboard
pub async fn dashboard(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    if !user.is_seller() {
        return Err(AppError::NotASeller);
    }

    // Get seller stats
    let stats: Option<SellerStats> =
        sqlx::query_as("SELECT * FROM seller_stats WHERE npub = ?")
            .bind(&user.npub)
            .fetch_optional(state.db.pool())
            .await?;

    // Get active listings
    let listings: Vec<Listing> = sqlx::query_as(
        "SELECT * FROM listings WHERE seller_npub = ? AND is_active = true ORDER BY created_at DESC LIMIT 10",
    )
    .bind(&user.npub)
    .fetch_all(state.db.pool())
    .await?;

    // Get pending order count
    let (pending_orders,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM orders WHERE seller_npub = ? AND status IN ('pending', 'shipped')",
    )
    .bind(&user.npub)
    .fetch_one(state.db.pool())
    .await?;

    let template = DashboardTemplate {
        title: "Seller Dashboard".to_string(),
        stats,
        listings,
        pending_orders,
        wallet_balance: user.wallet_balance,
    };

    let html = template
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Html(html))
}

/// Seller orders list
pub async fn orders(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    if !user.is_seller() {
        return Err(AppError::NotASeller);
    }

    let orders: Vec<Order> =
        sqlx::query_as("SELECT * FROM orders WHERE seller_npub = ? ORDER BY created_at DESC")
            .bind(&user.npub)
            .fetch_all(state.db.pool())
            .await?;

    let mut order_views = Vec::new();
    for order in orders {
        let (item_count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM order_items WHERE order_id = ?")
                .bind(&order.id)
                .fetch_one(state.db.pool())
                .await?;

        let (total_amount,): (i64,) =
            sqlx::query_as("SELECT COALESCE(SUM(price), 0) FROM order_items WHERE order_id = ?")
                .bind(&order.id)
                .fetch_one(state.db.pool())
                .await?;

        order_views.push(SellerOrderView {
            buyer_npub_short: format!("{}...", &order.buyer_npub[..12]),
            order,
            item_count,
            total_amount,
        });
    }

    let template = SellerOrdersTemplate {
        title: "Seller Orders".to_string(),
        orders: order_views,
    };

    let html = template
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Html(html))
}

/// Mark order as shipped
pub async fn mark_shipped(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(id): Path<String>,
    Form(form): Form<MarkShippedRequest>,
) -> AppResult<Redirect> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    if !user.is_seller() {
        return Err(AppError::NotASeller);
    }

    let order: Order = sqlx::query_as("SELECT * FROM orders WHERE id = ?")
        .bind(&id)
        .fetch_optional(state.db.pool())
        .await?
        .ok_or(AppError::OrderNotFound)?;

    // Verify seller owns order
    if order.seller_npub != user.npub {
        return Err(AppError::NotAuthorized);
    }

    if !order.can_ship() {
        return Err(AppError::OrderAlreadyCompleted);
    }

    // Update order
    sqlx::query(
        "UPDATE orders SET status = 'shipped', tracking_info = ?, shipped_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(&form.tracking_info)
    .bind(&id)
    .execute(state.db.pool())
    .await?;

    Ok(Redirect::to("/seller/orders"))
}

#[derive(Template)]
#[template(path = "seller/become_seller.html")]
struct BecomeSellerTemplate {
    title: String,
    category_bond: u64,
    all_categories_bond: u64,
    wallet_balance: i64,
}

/// Become a seller page
pub async fn become_seller_page(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    // If already a seller, redirect to dashboard
    if user.is_seller() {
        return Err(AppError::Redirect("/seller/dashboard".to_string()));
    }

    let template = BecomeSellerTemplate {
        title: "Become a Seller".to_string(),
        category_bond: state.config.seller_bonds.digital,
        all_categories_bond: state.config.seller_bonds.all,
        wallet_balance: user.wallet_balance,
    };

    let html = template
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Html(html))
}

#[derive(Deserialize)]
pub struct BecomeSellerForm {
    pub category: String,
}

/// Process becoming a seller
pub async fn become_seller(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Form(form): Form<BecomeSellerForm>,
) -> AppResult<Redirect> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    if user.is_seller() {
        return Ok(Redirect::to("/seller/dashboard"));
    }

    // Get the bond amount for the selected category
    let bond_amount = match form.category.as_str() {
        "digital" => state.config.seller_bonds.digital,
        "physical" => state.config.seller_bonds.physical,
        "services" => state.config.seller_bonds.services,
        "all" => state.config.seller_bonds.all,
        _ => return Err(AppError::InvalidInput("Invalid category".to_string())),
    };

    // Check if user has enough balance
    if user.wallet_balance < bond_amount as i64 {
        return Err(AppError::InsufficientBalance);
    }

    // Begin transaction
    let mut tx = state.db.pool().begin().await?;

    // Deduct bond from wallet
    let new_balance = user.wallet_balance - bond_amount as i64;
    sqlx::query("UPDATE users SET wallet_balance = ?, role = 'seller' WHERE npub = ?")
        .bind(new_balance)
        .bind(&user.npub)
        .execute(&mut *tx)
        .await?;

    // Record the bond payment
    let tx_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO wallet_transactions (id, user_npub, transaction_type, amount, balance_after, description)
         VALUES (?, ?, 'bond', ?, ?, ?)",
    )
    .bind(&tx_id)
    .bind(&user.npub)
    .bind(-(bond_amount as i64))
    .bind(new_balance)
    .bind(format!("Seller bond for {} category", form.category))
    .execute(&mut *tx)
    .await?;

    // Add category access
    if form.category == "all" {
        // Grant access to all categories
        for cat in &["digital", "physical", "services"] {
            sqlx::query(
                "INSERT OR REPLACE INTO seller_categories (npub, category, bond_paid, paid_at)
                 VALUES (?, ?, ?, CURRENT_TIMESTAMP)",
            )
            .bind(&user.npub)
            .bind(cat)
            .bind(bond_amount as i64 / 3) // Split bond across categories
            .execute(&mut *tx)
            .await?;
        }
    } else {
        sqlx::query(
            "INSERT INTO seller_categories (npub, category, bond_paid, paid_at)
             VALUES (?, ?, ?, CURRENT_TIMESTAMP)",
        )
        .bind(&user.npub)
        .bind(&form.category)
        .bind(bond_amount as i64)
        .execute(&mut *tx)
        .await?;
    }

    // Initialize seller stats
    sqlx::query(
        "INSERT INTO seller_stats (npub, total_sales, total_revenue, completed_orders, disputed_orders, dispute_rate)
         VALUES (?, 0, 0, 0, 0, 0.0)",
    )
    .bind(&user.npub)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Redirect::to("/seller/dashboard"))
}

#[derive(Template)]
#[template(path = "seller/categories.html")]
struct CategoriesTemplate {
    title: String,
    owned_categories: Vec<String>,
    available_categories: Vec<CategoryInfo>,
    wallet_balance: i64,
}

struct CategoryInfo {
    id: String,
    name: String,
    bond_amount: u64,
}

/// Categories management page
pub async fn categories_page(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    if !user.is_seller() {
        return Err(AppError::NotASeller);
    }

    // Get owned categories
    let owned: Vec<(String,)> =
        sqlx::query_as("SELECT category FROM seller_categories WHERE npub = ?")
            .bind(&user.npub)
            .fetch_all(state.db.pool())
            .await?;

    let owned_categories: Vec<String> = owned.into_iter().map(|(c,)| c).collect();

    // Build available categories list
    let all_categories = vec![
        ("digital", "Digital Goods", state.config.seller_bonds.digital),
        ("physical", "Physical Goods", state.config.seller_bonds.physical),
        ("services", "Services", state.config.seller_bonds.services),
    ];

    let available_categories: Vec<CategoryInfo> = all_categories
        .into_iter()
        .filter(|(id, _, _)| !owned_categories.contains(&id.to_string()))
        .map(|(id, name, bond)| CategoryInfo {
            id: id.to_string(),
            name: name.to_string(),
            bond_amount: bond,
        })
        .collect();

    let template = CategoriesTemplate {
        title: "Manage Categories".to_string(),
        owned_categories,
        available_categories,
        wallet_balance: user.wallet_balance,
    };

    let html = template
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Html(html))
}

#[derive(Deserialize)]
pub struct BuyCategoryForm {
    pub category: String,
}

/// Buy access to a new category
pub async fn buy_category(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Form(form): Form<BuyCategoryForm>,
) -> AppResult<Redirect> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    if !user.is_seller() {
        return Err(AppError::NotASeller);
    }

    // Check if already has this category
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT category FROM seller_categories WHERE npub = ? AND category = ?")
            .bind(&user.npub)
            .bind(&form.category)
            .fetch_optional(state.db.pool())
            .await?;

    if existing.is_some() {
        return Err(AppError::InvalidInput("Already own this category".to_string()));
    }

    // Get bond amount
    let bond_amount = match form.category.as_str() {
        "digital" => state.config.seller_bonds.digital,
        "physical" => state.config.seller_bonds.physical,
        "services" => state.config.seller_bonds.services,
        _ => return Err(AppError::InvalidInput("Invalid category".to_string())),
    };

    // Check balance
    if user.wallet_balance < bond_amount as i64 {
        return Err(AppError::InsufficientBalance);
    }

    // Begin transaction
    let mut tx = state.db.pool().begin().await?;

    // Deduct from wallet
    let new_balance = user.wallet_balance - bond_amount as i64;
    sqlx::query("UPDATE users SET wallet_balance = ? WHERE npub = ?")
        .bind(new_balance)
        .bind(&user.npub)
        .execute(&mut *tx)
        .await?;

    // Record transaction
    let tx_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO wallet_transactions (id, user_npub, transaction_type, amount, balance_after, description)
         VALUES (?, ?, 'bond', ?, ?, ?)",
    )
    .bind(&tx_id)
    .bind(&user.npub)
    .bind(-(bond_amount as i64))
    .bind(new_balance)
    .bind(format!("Category bond for {}", form.category))
    .execute(&mut *tx)
    .await?;

    // Add category access
    sqlx::query(
        "INSERT INTO seller_categories (npub, category, bond_paid, paid_at)
         VALUES (?, ?, ?, CURRENT_TIMESTAMP)",
    )
    .bind(&user.npub)
    .bind(&form.category)
    .bind(bond_amount as i64)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Redirect::to("/seller/categories"))
}
