use std::sync::Arc;

use askama::Template;
use axum::{
    extract::State,
    response::{Html, Redirect},
    Form,
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::models::WalletTransaction;
use crate::routes::auth::get_current_user;
use crate::AppState;

#[derive(Template)]
#[template(path = "wallet/show.html")]
struct WalletTemplate {
    title: String,
    balance: i64,
    transactions: Vec<WalletTransaction>,
}

#[derive(Template)]
#[template(path = "wallet/deposit.html")]
struct DepositTemplate {
    title: String,
    invoice: Option<String>,
    amount: Option<u64>,
}

#[derive(Template)]
#[template(path = "wallet/withdraw.html")]
struct WithdrawTemplate {
    title: String,
    balance: i64,
    error: Option<String>,
}

#[derive(Deserialize)]
pub struct DepositForm {
    amount: Option<u64>,
    cashu_token: Option<String>,
}

#[derive(Deserialize)]
pub struct WithdrawForm {
    amount: u64,
    invoice: String,
}

/// Show wallet balance and transactions
pub async fn show(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    let transactions: Vec<WalletTransaction> = sqlx::query_as(
        "SELECT * FROM wallet_transactions WHERE user_npub = ? ORDER BY created_at DESC LIMIT 50",
    )
    .bind(&user.npub)
    .fetch_all(state.db.pool())
    .await?;

    let template = WalletTemplate {
        title: "Wallet".to_string(),
        balance: user.wallet_balance,
        transactions,
    };

    let html = template
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Html(html))
}

/// Deposit page
pub async fn deposit_page(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    let _user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    let template = DepositTemplate {
        title: "Deposit".to_string(),
        invoice: None,
        amount: None,
    };

    let html = template
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Html(html))
}

/// Handle deposit
pub async fn deposit(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Form(form): Form<DepositForm>,
) -> AppResult<Html<String>> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    // If Cashu token provided, receive it directly
    if let Some(token) = form.cashu_token {
        let amount = state.cashu.receive_tokens(&token).await?;

        // Credit wallet
        let new_balance = user.wallet_balance + amount as i64;
        sqlx::query("UPDATE users SET wallet_balance = ? WHERE npub = ?")
            .bind(new_balance)
            .bind(&user.npub)
            .execute(state.db.pool())
            .await?;

        // Log transaction
        let tx_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO wallet_transactions (id, user_npub, transaction_type, amount, balance_after, description, created_at) VALUES (?, ?, 'deposit', ?, ?, 'Cashu token deposit', CURRENT_TIMESTAMP)",
        )
        .bind(&tx_id)
        .bind(&user.npub)
        .bind(amount as i64)
        .bind(new_balance)
        .execute(state.db.pool())
        .await?;

        return Ok(Html(format!(
            "<p>Deposited {} sats. New balance: {} sats</p><a href=\"/wallet\">Back to Wallet</a>",
            amount, new_balance
        )));
    }

    // Generate Lightning invoice if amount specified
    if let Some(amount) = form.amount {
        let invoice = state.cashu.create_deposit_invoice(amount).await?;

        let template = DepositTemplate {
            title: "Deposit".to_string(),
            invoice: Some(invoice.payment_request),
            amount: Some(amount),
        };

        let html = template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?;

        return Ok(Html(html));
    }

    // Show deposit form
    let template = DepositTemplate {
        title: "Deposit".to_string(),
        invoice: None,
        amount: None,
    };

    let html = template
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Html(html))
}

/// Withdraw page
pub async fn withdraw_page(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    let template = WithdrawTemplate {
        title: "Withdraw".to_string(),
        balance: user.wallet_balance,
        error: None,
    };

    let html = template
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Html(html))
}

/// Handle withdrawal
pub async fn withdraw(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Form(form): Form<WithdrawForm>,
) -> AppResult<Redirect> {
    let user = get_current_user(&state, &jar)
        .await?
        .ok_or(AppError::NotAuthenticated)?;

    if user.wallet_balance < form.amount as i64 {
        return Err(AppError::InsufficientBalanceDetails {
            needed: form.amount,
            available: user.wallet_balance as u64,
        });
    }

    // Pay Lightning invoice
    state.cashu.withdraw(&form.invoice, form.amount).await?;

    // Deduct from wallet
    let new_balance = user.wallet_balance - form.amount as i64;
    sqlx::query("UPDATE users SET wallet_balance = ? WHERE npub = ?")
        .bind(new_balance)
        .bind(&user.npub)
        .execute(state.db.pool())
        .await?;

    // Log transaction
    let tx_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO wallet_transactions (id, user_npub, transaction_type, amount, balance_after, description, created_at) VALUES (?, ?, 'withdraw', ?, ?, 'Lightning withdrawal', CURRENT_TIMESTAMP)",
    )
    .bind(&tx_id)
    .bind(&user.npub)
    .bind(-(form.amount as i64))
    .bind(new_balance)
    .execute(state.db.pool())
    .await?;

    Ok(Redirect::to("/wallet"))
}
