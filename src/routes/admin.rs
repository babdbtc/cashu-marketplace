use std::sync::Arc;

use askama::Template;
use axum::{
    extract::{Path, State},
    response::{Html, Redirect},
    Form,
};
use axum_extra::extract::CookieJar;

use crate::error::{AppError, AppResult};
use crate::models::{Dispute, DisputeResolution, Escrow, Order, ResolveDisputeRequest};
use crate::routes::auth::get_current_user;
use crate::services::EscrowService;
use crate::AppState;

#[derive(Template)]
#[template(path = "admin/dashboard.html")]
struct AdminDashboardTemplate {
    title: String,
    open_disputes: i64,
    total_users: i64,
    total_listings: i64,
    total_orders: i64,
    total_escrow_held: i64,
}

#[derive(Template)]
#[template(path = "admin/disputes.html")]
struct DisputesListTemplate {
    title: String,
    disputes: Vec<DisputeView>,
}

struct DisputeView {
    dispute: Dispute,
    order: Order,
    escrow: Escrow,
    time_remaining: i64,
}

#[derive(Template)]
#[template(path = "admin/dispute_detail.html")]
struct DisputeDetailTemplate {
    title: String,
    dispute: Dispute,
    order: Order,
    escrow: Escrow,
    evidence: Vec<crate::models::DisputeEvidence>,
}

/// Admin dashboard
pub async fn dashboard(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    if !user.is_admin() {
        return Err(AppError::NotAuthorized);
    }

    // Get stats
    let (open_disputes,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM disputes WHERE status = 'open'")
            .fetch_one(state.db.pool())
            .await?;

    let (total_users,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(state.db.pool())
        .await?;

    let (total_listings,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM listings WHERE is_active = true")
            .fetch_one(state.db.pool())
            .await?;

    let (total_orders,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM orders")
        .fetch_one(state.db.pool())
        .await?;

    let (total_escrow_held,): (i64,) =
        sqlx::query_as("SELECT COALESCE(SUM(amount), 0) FROM escrows WHERE status = 'held'")
            .fetch_one(state.db.pool())
            .await?;

    let template = AdminDashboardTemplate {
        title: "Admin Dashboard".to_string(),
        open_disputes,
        total_users,
        total_listings,
        total_orders,
        total_escrow_held,
    };

    let html = template
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Html(html))
}

/// List open disputes
pub async fn disputes(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    if !user.is_admin() {
        return Err(AppError::NotAuthorized);
    }

    let disputes: Vec<Dispute> =
        sqlx::query_as("SELECT * FROM disputes WHERE status = 'open' ORDER BY auto_resolve_at ASC")
            .fetch_all(state.db.pool())
            .await?;

    let mut dispute_views = Vec::new();
    for dispute in disputes {
        let order: Order = sqlx::query_as("SELECT * FROM orders WHERE id = ?")
            .bind(&dispute.order_id)
            .fetch_one(state.db.pool())
            .await?;

        let escrow: Escrow = sqlx::query_as("SELECT * FROM escrows WHERE id = ?")
            .bind(&dispute.escrow_id)
            .fetch_one(state.db.pool())
            .await?;

        let time_remaining =
            (dispute.auto_resolve_at - chrono::Utc::now()).num_seconds().max(0);

        dispute_views.push(DisputeView {
            dispute,
            order,
            escrow,
            time_remaining,
        });
    }

    let template = DisputesListTemplate {
        title: "Open Disputes".to_string(),
        disputes: dispute_views,
    };

    let html = template
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Html(html))
}

/// Show dispute detail
pub async fn dispute_detail(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> AppResult<Html<String>> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    if !user.is_admin() {
        return Err(AppError::NotAuthorized);
    }

    let dispute: Dispute = sqlx::query_as("SELECT * FROM disputes WHERE id = ?")
        .bind(&id)
        .fetch_optional(state.db.pool())
        .await?
        .ok_or(AppError::DisputeNotFound)?;

    let order: Order = sqlx::query_as("SELECT * FROM orders WHERE id = ?")
        .bind(&dispute.order_id)
        .fetch_one(state.db.pool())
        .await?;

    let escrow: Escrow = sqlx::query_as("SELECT * FROM escrows WHERE id = ?")
        .bind(&dispute.escrow_id)
        .fetch_one(state.db.pool())
        .await?;

    let evidence: Vec<crate::models::DisputeEvidence> =
        sqlx::query_as("SELECT * FROM dispute_evidence WHERE dispute_id = ? ORDER BY created_at ASC")
            .bind(&id)
            .fetch_all(state.db.pool())
            .await?;

    let template = DisputeDetailTemplate {
        title: format!("Dispute #{}", &dispute.id[..8]),
        dispute,
        order,
        escrow,
        evidence,
    };

    let html = template
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Html(html))
}

/// Resolve dispute
pub async fn resolve_dispute(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(id): Path<String>,
    Form(form): Form<ResolveDisputeRequest>,
) -> AppResult<Redirect> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    if !user.is_admin() {
        return Err(AppError::NotAuthorized);
    }

    let dispute: Dispute = sqlx::query_as("SELECT * FROM disputes WHERE id = ?")
        .bind(&id)
        .fetch_optional(state.db.pool())
        .await?
        .ok_or(AppError::DisputeNotFound)?;

    if !dispute.is_open() {
        return Err(AppError::DisputeAlreadyResolved);
    }

    // Parse resolution
    let resolution =
        DisputeResolution::from_str(&form.resolution).ok_or(AppError::InvalidResolution)?;

    // Resolve escrow
    EscrowService::resolve_dispute(&state.db, &dispute.escrow_id, resolution).await?;

    // Update dispute record
    sqlx::query(
        "UPDATE disputes SET status = 'resolved', resolution = ?, resolution_notes = ?, resolved_by = ?, resolved_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(&form.resolution)
    .bind(&form.notes)
    .bind(&user.npub)
    .bind(&id)
    .execute(state.db.pool())
    .await?;

    Ok(Redirect::to("/admin/disputes"))
}
