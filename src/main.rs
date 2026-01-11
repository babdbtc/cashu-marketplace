mod config;
mod db;
mod error;
mod middleware;
mod models;
mod routes;
mod services;

use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::Config;
use crate::db::Database;
use crate::middleware::{BrowsingFeeConfig, BrowsingFeeLayer};
use crate::services::{CashuService, EscrowService, NostrService};

/// Application state shared across all handlers
pub struct AppState {
    pub db: Database,
    pub cashu: CashuService,
    pub nostr: NostrService,
    pub config: Config,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "marketplace=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let config = Config::load()?;
    tracing::info!("Configuration loaded");

    // Initialize database
    let db = Database::connect(&config.database_url).await?;
    db.run_migrations().await?;
    tracing::info!("Database initialized");

    // Initialize Cashu wallet service
    let cashu = CashuService::new(&config).await?;
    let mint_info = cashu.mint_info();
    if cashu.is_mock_mode() {
        tracing::warn!("Running in MOCK payment mode - set mint.url in config for real payments");
    } else {
        tracing::info!("Cashu wallet connected to mint: {}", mint_info.url);
    }

    // Initialize Nostr service
    let nostr = NostrService::new(&config)?;
    tracing::info!("Nostr service initialized");

    // Create shared application state
    let state = Arc::new(AppState {
        db,
        cashu,
        nostr,
        config: config.clone(),
    });

    // Spawn background task for escrow auto-release
    let bg_state = state.clone();
    tokio::spawn(async move {
        escrow_auto_release_task(bg_state).await;
    });

    // Configure browsing fee middleware
    let browsing_fee_config = BrowsingFeeConfig {
        min_fee_sats: config.browsing_fee_sats,
        ..Default::default()
    };

    // Build router
    let app = Router::new()
        // Public routes
        .route("/", get(routes::home::index))
        .route("/health", get(routes::health))
        // Auth routes
        .route("/login", get(routes::auth::login_page))
        .route("/login", post(routes::auth::login))
        .route("/register", get(routes::auth::register_page))
        .route("/register", post(routes::auth::register))
        .route("/logout", post(routes::auth::logout))
        // Listing routes
        .route("/listings", get(routes::listings::index))
        .route("/listings/:id", get(routes::listings::show))
        .route("/listings/new", get(routes::listings::new_page))
        .route("/listings/new", post(routes::listings::create))
        // Cart routes
        .route("/cart", get(routes::cart::show))
        .route("/cart/add/:listing_id", post(routes::cart::add))
        .route("/cart/remove/:item_id", post(routes::cart::remove))
        .route("/checkout", get(routes::cart::checkout_page))
        .route("/checkout", post(routes::cart::checkout))
        // Wallet routes
        .route("/wallet", get(routes::wallet::show))
        .route("/wallet/deposit", get(routes::wallet::deposit_page))
        .route("/wallet/deposit", post(routes::wallet::deposit))
        .route("/wallet/withdraw", get(routes::wallet::withdraw_page))
        .route("/wallet/withdraw", post(routes::wallet::withdraw))
        // Order routes
        .route("/orders", get(routes::orders::index))
        .route("/orders/:id", get(routes::orders::show))
        .route("/orders/:id/confirm", post(routes::orders::confirm))
        .route("/orders/:id/dispute", post(routes::orders::dispute))
        // Seller routes
        .route("/seller/dashboard", get(routes::seller::dashboard))
        .route("/seller/orders", get(routes::seller::orders))
        .route("/seller/orders/:id/ship", post(routes::seller::mark_shipped))
        .route("/seller/become", get(routes::seller::become_seller_page))
        .route("/seller/become", post(routes::seller::become_seller))
        .route("/seller/categories", get(routes::seller::categories_page))
        .route("/seller/categories", post(routes::seller::buy_category))
        // Admin routes
        .route("/admin", get(routes::admin::dashboard))
        .route("/admin/disputes", get(routes::admin::disputes))
        .route("/admin/disputes/:id", get(routes::admin::dispute_detail))
        .route(
            "/admin/disputes/:id/resolve",
            post(routes::admin::resolve_dispute),
        )
        // Static files
        .nest_service("/static", tower_http::services::ServeDir::new("static"))
        // Middleware
        .layer(BrowsingFeeLayer::new(browsing_fee_config))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Start server
    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Server listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

/// Background task to process escrow auto-releases
async fn escrow_auto_release_task(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));

    loop {
        interval.tick().await;

        match EscrowService::process_auto_releases(&state.db).await {
            Ok(count) => {
                if count > 0 {
                    tracing::info!("Auto-released {} escrows", count);
                }
            }
            Err(e) => {
                tracing::error!("Error processing auto-releases: {}", e);
            }
        }
    }
}
