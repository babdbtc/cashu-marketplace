// Auth extractors are part of the public API - may not all be used internally yet
#![allow(dead_code)]

use std::sync::Arc;

use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::CookieJar;

use crate::models::User;
use crate::AppState;

const SESSION_COOKIE: &str = "session";

/// Extractor for the current authenticated user (required)
pub struct CurrentUser(pub User);

/// Extractor for optional user (may or may not be logged in)
pub struct OptionalUser(pub Option<User>);

/// Require authentication middleware marker
pub struct RequireAuth;

/// Require seller role middleware marker
pub struct RequireSeller;

/// Require admin role middleware marker
pub struct RequireAdmin;

/// Authentication layer (for reference, actual auth is done via extractors)
pub struct AuthLayer;

#[async_trait]
impl FromRequestParts<Arc<AppState>> for CurrentUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| AuthError::Internal)?;

        let user = get_user_from_session(state, &jar)
            .await
            .map_err(|_| AuthError::Internal)?
            .ok_or(AuthError::NotAuthenticated)?;

        Ok(CurrentUser(user))
    }
}

#[async_trait]
impl FromRequestParts<Arc<AppState>> for OptionalUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| AuthError::Internal)?;

        let user = get_user_from_session(state, &jar)
            .await
            .map_err(|_| AuthError::Internal)?;

        Ok(OptionalUser(user))
    }
}

/// Extractor that requires seller role
pub struct SellerUser(pub User);

#[async_trait]
impl FromRequestParts<Arc<AppState>> for SellerUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let CurrentUser(user) = CurrentUser::from_request_parts(parts, state).await?;

        if !user.is_seller() {
            return Err(AuthError::NotSeller);
        }

        Ok(SellerUser(user))
    }
}

/// Extractor that requires admin role
pub struct AdminUser(pub User);

#[async_trait]
impl FromRequestParts<Arc<AppState>> for AdminUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let CurrentUser(user) = CurrentUser::from_request_parts(parts, state).await?;

        if !user.is_admin() {
            return Err(AuthError::NotAdmin);
        }

        Ok(AdminUser(user))
    }
}

/// Authentication errors
#[derive(Debug)]
pub enum AuthError {
    NotAuthenticated,
    NotSeller,
    NotAdmin,
    Internal,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        match self {
            AuthError::NotAuthenticated => {
                Redirect::to("/login").into_response()
            }
            AuthError::NotSeller => {
                (StatusCode::FORBIDDEN, "Seller access required").into_response()
            }
            AuthError::NotAdmin => {
                (StatusCode::FORBIDDEN, "Admin access required").into_response()
            }
            AuthError::Internal => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal error").into_response()
            }
        }
    }
}

/// Get user from session cookie
async fn get_user_from_session(
    state: &AppState,
    jar: &CookieJar,
) -> Result<Option<User>, sqlx::Error> {
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
    let user: Option<User> = sqlx::query_as("SELECT * FROM users WHERE npub = ?")
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
