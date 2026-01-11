use std::sync::Arc;

use askama::Template;
use axum::{
    extract::{Path, State},
    response::{Html, Redirect},
    Form,
};
use axum_extra::extract::CookieJar;

use crate::error::{AppError, AppResult};
use crate::models::{CreateDisputeRequest, Escrow, Order, OrderItem};
use crate::routes::auth::get_current_user;
use crate::services::EscrowService;
use crate::AppState;

#[derive(Template)]
#[template(path = "orders/index.html")]
struct OrdersIndexTemplate {
    title: String,
    orders: Vec<OrderView>,
}

struct OrderView {
    order: Order,
    items: Vec<OrderItem>,
    escrow: Escrow,
}

#[derive(Template)]
#[template(path = "orders/show.html")]
struct OrderShowTemplate {
    title: String,
    order: Order,
    items: Vec<OrderItem>,
    escrow: Escrow,
    can_confirm: bool,
    can_dispute: bool,
}

/// List buyer's orders
pub async fn index(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    let orders: Vec<Order> =
        sqlx::query_as("SELECT * FROM orders WHERE buyer_npub = ? ORDER BY created_at DESC")
            .bind(&user.npub)
            .fetch_all(state.db.pool())
            .await?;

    let mut order_views = Vec::new();
    for order in orders {
        let items: Vec<OrderItem> =
            sqlx::query_as("SELECT * FROM order_items WHERE order_id = ?")
                .bind(&order.id)
                .fetch_all(state.db.pool())
                .await?;

        let escrow: Escrow = sqlx::query_as("SELECT * FROM escrows WHERE id = ?")
            .bind(&order.escrow_id)
            .fetch_one(state.db.pool())
            .await?;

        order_views.push(OrderView {
            order,
            items,
            escrow,
        });
    }

    let template = OrdersIndexTemplate {
        title: "My Orders".to_string(),
        orders: order_views,
    };

    let html = template
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Html(html))
}

/// Show single order
pub async fn show(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> AppResult<Html<String>> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    let order: Order = sqlx::query_as("SELECT * FROM orders WHERE id = ?")
        .bind(&id)
        .fetch_optional(state.db.pool())
        .await?
        .ok_or(AppError::OrderNotFound)?;

    // Verify buyer owns order
    if order.buyer_npub != user.npub {
        return Err(AppError::NotAuthorized);
    }

    let items: Vec<OrderItem> =
        sqlx::query_as("SELECT * FROM order_items WHERE order_id = ?")
            .bind(&order.id)
            .fetch_all(state.db.pool())
            .await?;

    let escrow: Escrow = sqlx::query_as("SELECT * FROM escrows WHERE id = ?")
        .bind(&order.escrow_id)
        .fetch_one(state.db.pool())
        .await?;

    let template = OrderShowTemplate {
        title: format!("Order #{}", &order.id[..8]),
        can_confirm: order.can_confirm(),
        can_dispute: order.can_dispute(),
        order,
        items,
        escrow,
    };

    let html = template
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Html(html))
}

/// Confirm order (release escrow to seller)
pub async fn confirm(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> AppResult<Redirect> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    let order: Order = sqlx::query_as("SELECT * FROM orders WHERE id = ?")
        .bind(&id)
        .fetch_optional(state.db.pool())
        .await?
        .ok_or(AppError::OrderNotFound)?;

    // Verify buyer owns order
    if order.buyer_npub != user.npub {
        return Err(AppError::NotAuthorized);
    }

    if !order.can_confirm() {
        return Err(AppError::OrderAlreadyCompleted);
    }

    // Release escrow
    EscrowService::release_escrow(&state.db, &order.escrow_id).await?;

    // Delete order messages (privacy)
    sqlx::query("DELETE FROM order_messages WHERE order_id = ?")
        .bind(&id)
        .execute(state.db.pool())
        .await?;

    Ok(Redirect::to(&format!("/orders/{}", id)))
}

/// Open dispute
pub async fn dispute(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(id): Path<String>,
    Form(form): Form<CreateDisputeRequest>,
) -> AppResult<Redirect> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    let order: Order = sqlx::query_as("SELECT * FROM orders WHERE id = ?")
        .bind(&id)
        .fetch_optional(state.db.pool())
        .await?
        .ok_or(AppError::OrderNotFound)?;

    // Verify buyer owns order
    if order.buyer_npub != user.npub {
        return Err(AppError::NotAuthorized);
    }

    if !order.can_dispute() {
        return Err(AppError::OrderCannotBeDisputed);
    }

    // Mark escrow as disputed
    EscrowService::mark_disputed(&state.db, &order.escrow_id).await?;

    // Create dispute
    let dispute_id = uuid::Uuid::new_v4().to_string();
    let auto_resolve_at = chrono::Utc::now() + chrono::Duration::days(10);

    sqlx::query(
        r#"
        INSERT INTO disputes (id, order_id, escrow_id, initiated_by, reason, status, auto_resolve_at, created_at)
        VALUES (?, ?, ?, 'buyer', ?, 'open', ?, CURRENT_TIMESTAMP)
        "#,
    )
    .bind(&dispute_id)
    .bind(&id)
    .bind(&order.escrow_id)
    .bind(&form.reason)
    .bind(auto_resolve_at)
    .execute(state.db.pool())
    .await?;

    // Update order status
    sqlx::query("UPDATE orders SET status = 'disputed' WHERE id = ?")
        .bind(&id)
        .execute(state.db.pool())
        .await?;

    Ok(Redirect::to(&format!("/orders/{}", id)))
}
