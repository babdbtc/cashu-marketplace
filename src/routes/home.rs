use std::sync::Arc;

use askama::Template;
use axum::extract::State;
use axum::response::Html;

use crate::error::AppResult;
use crate::AppState;

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTemplate {
    title: String,
    // TODO: Add featured listings
    // featured_listings: Vec<FeaturedListingView>,
    // recent_listings: Vec<ListingView>,
}

/// Homepage handler
pub async fn index(State(_state): State<Arc<AppState>>) -> AppResult<Html<String>> {
    // TODO: Fetch featured and recent listings
    let template = HomeTemplate {
        title: "Marketplace".to_string(),
    };

    let html = template
        .render()
        .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

    Ok(Html(html))
}
