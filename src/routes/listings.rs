use std::sync::Arc;

use askama::Template;
use axum::{
    extract::{Path, Query, State},
    response::{Html, Redirect},
    Form,
};
use axum_extra::extract::CookieJar;

use crate::error::{AppError, AppResult};
use crate::models::{CreateListingRequest, Listing, ListingSearchQuery};
use crate::routes::auth::get_current_user;
use crate::AppState;

#[derive(Template)]
#[template(path = "listings/index.html")]
struct ListingsIndexTemplate {
    title: String,
    listings: Vec<Listing>,
    query: ListingSearchQuery,
    total_pages: u32,
    current_page: u32,
}

#[derive(Template)]
#[template(path = "listings/show.html")]
struct ListingShowTemplate {
    title: String,
    listing: Listing,
    seller_rating: Option<f64>,
    seller_sales: i64,
    is_owner: bool,
    in_cart: bool,
}

#[derive(Template)]
#[template(path = "listings/new.html")]
struct NewListingTemplate {
    title: String,
    categories: Vec<String>,
    error: Option<String>,
}

/// List all listings with search/filter
pub async fn index(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListingSearchQuery>,
) -> AppResult<Html<String>> {
    let offset = query.offset();
    let limit = query.per_page();

    // Build query based on filters
    let mut sql = String::from(
        "SELECT * FROM listings WHERE is_active = true AND expires_at > CURRENT_TIMESTAMP",
    );
    let mut count_sql =
        String::from("SELECT COUNT(*) FROM listings WHERE is_active = true AND expires_at > CURRENT_TIMESTAMP");

    if let Some(ref cat) = query.category {
        sql.push_str(&format!(" AND category = '{}'", cat));
        count_sql.push_str(&format!(" AND category = '{}'", cat));
    }

    if let Some(min) = query.min_price {
        sql.push_str(&format!(" AND price >= {}", min));
        count_sql.push_str(&format!(" AND price >= {}", min));
    }

    if let Some(max) = query.max_price {
        sql.push_str(&format!(" AND price <= {}", max));
        count_sql.push_str(&format!(" AND price <= {}", max));
    }

    if let Some(ref seller) = query.seller {
        sql.push_str(&format!(" AND seller_npub = '{}'", seller));
        count_sql.push_str(&format!(" AND seller_npub = '{}'", seller));
    }

    // TODO: Full-text search with FTS5
    if let Some(ref _q) = query.q {
        // sql.push_str(&format!(" AND id IN (SELECT rowid FROM listings_fts WHERE listings_fts MATCH '{}')", q));
    }

    sql.push_str(" ORDER BY created_at DESC");
    sql.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));

    let listings: Vec<Listing> = sqlx::query_as(&sql).fetch_all(state.db.pool()).await?;

    // Get total count for pagination
    let (total,): (i64,) = sqlx::query_as(&count_sql)
        .fetch_one(state.db.pool())
        .await?;

    let total_pages = ((total as u32) + limit - 1) / limit;
    let current_page = query.page();

    let template = ListingsIndexTemplate {
        title: "Listings".to_string(),
        listings,
        query,
        total_pages,
        current_page,
    };

    let html = template
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Html(html))
}

/// Show single listing
pub async fn show(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> AppResult<Html<String>> {
    let listing: Listing = sqlx::query_as("SELECT * FROM listings WHERE id = ?")
        .bind(&id)
        .fetch_optional(state.db.pool())
        .await?
        .ok_or(AppError::ListingNotFound)?;

    // Get seller stats
    let stats: Option<(Option<f64>, i64)> = sqlx::query_as(
        "SELECT avg_rating, completed_orders FROM seller_stats WHERE npub = ?",
    )
    .bind(&listing.seller_npub)
    .fetch_optional(state.db.pool())
    .await?;

    let (seller_rating, seller_sales) = stats.unwrap_or((None, 0));

    // Check if current user owns the listing
    let current_user = get_current_user(&state, &jar).await?;
    let is_owner = current_user
        .as_ref()
        .map(|u| u.npub == listing.seller_npub)
        .unwrap_or(false);

    // Check if listing is in user's cart
    let in_cart = if let Some(ref user) = current_user {
        let cart_item: Option<(String,)> =
            sqlx::query_as("SELECT id FROM cart_items WHERE user_npub = ? AND listing_id = ?")
                .bind(&user.npub)
                .bind(&id)
                .fetch_optional(state.db.pool())
                .await?;
        cart_item.is_some()
    } else {
        false
    };

    let template = ListingShowTemplate {
        title: listing.title.clone(),
        listing,
        seller_rating,
        seller_sales,
        is_owner,
        in_cart,
    };

    let html = template
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Html(html))
}

/// New listing form
pub async fn new_page(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    if !user.is_seller() {
        return Err(AppError::NotASeller);
    }

    // Get seller's authorized categories
    let categories: Vec<(String,)> =
        sqlx::query_as("SELECT category FROM seller_categories WHERE npub = ?")
            .bind(&user.npub)
            .fetch_all(state.db.pool())
            .await?;

    let categories: Vec<String> = categories.into_iter().map(|(c,)| c).collect();

    let template = NewListingTemplate {
        title: "New Listing".to_string(),
        categories,
        error: None,
    };

    let html = template
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Html(html))
}

/// Create new listing
pub async fn create(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Form(form): Form<CreateListingRequest>,
) -> AppResult<Redirect> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    if !user.is_seller() {
        return Err(AppError::NotASeller);
    }

    // Check if seller has access to the category
    let has_category: Option<(String,)> = sqlx::query_as(
        "SELECT category FROM seller_categories WHERE npub = ? AND category = ?",
    )
    .bind(&user.npub)
    .bind(&form.category)
    .fetch_optional(state.db.pool())
    .await?;

    if has_category.is_none() {
        return Err(AppError::CategoryNotAuthorized);
    }

    // Create listing
    let id = uuid::Uuid::new_v4().to_string();
    let expires_at = chrono::Utc::now() + chrono::Duration::days(30);

    sqlx::query(
        r#"
        INSERT INTO listings (id, seller_npub, title, description, price, category, is_active, stock, created_at, updated_at, expires_at)
        VALUES (?, ?, ?, ?, ?, ?, true, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, ?)
        "#,
    )
    .bind(&id)
    .bind(&user.npub)
    .bind(&form.title)
    .bind(&form.description)
    .bind(form.price)
    .bind(&form.category)
    .bind(form.stock)
    .bind(expires_at)
    .execute(state.db.pool())
    .await?;

    Ok(Redirect::to(&format!("/listings/{}", id)))
}
