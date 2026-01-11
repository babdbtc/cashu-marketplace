use std::sync::Arc;

use askama::Template;
use axum::{
    extract::State,
    response::{Html, Redirect},
    Form,
};
use axum_extra::extract::CookieJar;
use chrono::{Duration, Utc};
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::services::NostrService;
use crate::AppState;

const SESSION_COOKIE: &str = "session";

#[derive(Template)]
#[template(path = "auth/login.html")]
struct LoginTemplate {
    title: String,
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "auth/register.html")]
struct RegisterTemplate {
    title: String,
    generated_nsec: Option<String>,
    generated_npub: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginForm {
    nsec: String,
}

#[derive(Deserialize)]
pub struct RegisterForm {
    nsec: Option<String>,
    generate_new: Option<String>,
}

/// Login page
pub async fn login_page() -> AppResult<Html<String>> {
    let template = LoginTemplate {
        title: "Login".to_string(),
        error: None,
    };

    let html = template
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Html(html))
}

/// Handle login
pub async fn login(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Form(form): Form<LoginForm>,
) -> AppResult<(CookieJar, Redirect)> {
    // Validate nsec and get npub
    let npub = NostrService::npub_from_nsec(&form.nsec)?;

    // Check if user exists, create if not
    let user_exists: Option<(String,)> =
        sqlx::query_as("SELECT npub FROM users WHERE npub = ?")
            .bind(&npub)
            .fetch_optional(state.db.pool())
            .await?;

    if user_exists.is_none() {
        // Create new user
        sqlx::query(
            "INSERT INTO users (npub, role, wallet_balance, last_active_at, created_at) VALUES (?, 'buyer', 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .bind(&npub)
        .execute(state.db.pool())
        .await?;
    }

    // Update last active
    sqlx::query("UPDATE users SET last_active_at = CURRENT_TIMESTAMP WHERE npub = ?")
        .bind(&npub)
        .execute(state.db.pool())
        .await?;

    // Create session
    let session_id = uuid::Uuid::new_v4().to_string();
    let expires_at = Utc::now() + Duration::hours(state.config.session_hours as i64);

    sqlx::query("INSERT INTO sessions (id, user_npub, expires_at, created_at) VALUES (?, ?, ?, CURRENT_TIMESTAMP)")
        .bind(&session_id)
        .bind(&npub)
        .bind(expires_at)
        .execute(state.db.pool())
        .await?;

    // Set session cookie
    let cookie = axum_extra::extract::cookie::Cookie::build((SESSION_COOKIE, session_id))
        .path("/")
        .http_only(true)
        .secure(true) // Requires HTTPS (Tor hidden service)
        .same_site(axum_extra::extract::cookie::SameSite::Strict)
        .build();

    Ok((jar.add(cookie), Redirect::to("/")))
}

/// Register page
pub async fn register_page() -> AppResult<Html<String>> {
    let template = RegisterTemplate {
        title: "Register".to_string(),
        generated_nsec: None,
        generated_npub: None,
        error: None,
    };

    let html = template
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Html(html))
}

/// Handle registration
pub async fn register(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Form(form): Form<RegisterForm>,
) -> AppResult<(CookieJar, Html<String>)> {
    // Generate new keypair if requested
    if form.generate_new.is_some() {
        let (nsec, npub) = NostrService::generate_keypair()?;

        let template = RegisterTemplate {
            title: "Register".to_string(),
            generated_nsec: Some(nsec),
            generated_npub: Some(npub),
            error: None,
        };

        let html = template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?;

        return Ok((jar, Html(html)));
    }

    // User provided or confirmed their nsec
    let nsec = form.nsec.ok_or(AppError::InvalidNsec)?;
    let npub = NostrService::npub_from_nsec(&nsec)?;

    // Check if user already exists
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT npub FROM users WHERE npub = ?")
            .bind(&npub)
            .fetch_optional(state.db.pool())
            .await?;

    if existing.is_some() {
        return Err(AppError::UserAlreadyExists);
    }

    // Create user
    sqlx::query(
        "INSERT INTO users (npub, role, wallet_balance, last_active_at, created_at) VALUES (?, 'buyer', 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
    )
    .bind(&npub)
    .execute(state.db.pool())
    .await?;

    // Create session
    let session_id = uuid::Uuid::new_v4().to_string();
    let expires_at = Utc::now() + Duration::hours(state.config.session_hours as i64);

    sqlx::query("INSERT INTO sessions (id, user_npub, expires_at, created_at) VALUES (?, ?, ?, CURRENT_TIMESTAMP)")
        .bind(&session_id)
        .bind(&npub)
        .bind(expires_at)
        .execute(state.db.pool())
        .await?;

    // Set session cookie and redirect
    let cookie = axum_extra::extract::cookie::Cookie::build((SESSION_COOKIE, session_id))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(axum_extra::extract::cookie::SameSite::Strict)
        .build();

    // Show success page with reminder to save nsec
    let template = RegisterTemplate {
        title: "Registration Complete".to_string(),
        generated_nsec: Some(nsec),
        generated_npub: Some(npub),
        error: None,
    };

    let html = template
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok((jar.add(cookie), Html(html)))
}

/// Handle logout
pub async fn logout(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> AppResult<(CookieJar, Redirect)> {
    // Get session from cookie
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        let session_id = cookie.value();

        // Delete session from database
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(session_id)
            .execute(state.db.pool())
            .await?;
    }

    // Remove cookie
    let jar = jar.remove(axum_extra::extract::cookie::Cookie::from(SESSION_COOKIE));

    Ok((jar, Redirect::to("/")))
}

/// Extract current user from session cookie (middleware helper)
pub async fn get_current_user(
    state: &AppState,
    jar: &CookieJar,
) -> AppResult<Option<crate::models::User>> {
    let session_id = match jar.get(SESSION_COOKIE) {
        Some(cookie) => cookie.value().to_string(),
        None => return Ok(None),
    };

    // Get session
    let session: Option<crate::models::Session> =
        sqlx::query_as("SELECT * FROM sessions WHERE id = ? AND expires_at > CURRENT_TIMESTAMP")
            .bind(&session_id)
            .fetch_optional(state.db.pool())
            .await?;

    let session = match session {
        Some(s) => s,
        None => return Ok(None),
    };

    // Get user
    let user: Option<crate::models::User> =
        sqlx::query_as("SELECT * FROM users WHERE npub = ?")
            .bind(&session.user_npub)
            .fetch_optional(state.db.pool())
            .await?;

    // Update last active
    if user.is_some() {
        sqlx::query("UPDATE users SET last_active_at = CURRENT_TIMESTAMP WHERE npub = ?")
            .bind(&session.user_npub)
            .execute(state.db.pool())
            .await?;
    }

    Ok(user)
}
