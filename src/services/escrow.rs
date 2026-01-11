use chrono::{Duration, Utc};

use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::models::{DisputeResolution, Escrow, EscrowStatus, TransactionType};

/// Escrow management service
pub struct EscrowService;

impl EscrowService {
    /// Create a new escrow for an order
    pub async fn create_escrow(
        db: &Database,
        buyer_npub: &str,
        seller_npub: &str,
        amount: i64,
        escrow_days: u32,
    ) -> AppResult<Escrow> {
        let id = uuid::Uuid::new_v4().to_string();
        let auto_release_at = Utc::now() + Duration::days(escrow_days as i64);

        sqlx::query(
            r#"
            INSERT INTO escrows (id, buyer_npub, seller_npub, amount, status, auto_release_at, created_at)
            VALUES (?, ?, ?, ?, 'held', ?, CURRENT_TIMESTAMP)
            "#,
        )
        .bind(&id)
        .bind(buyer_npub)
        .bind(seller_npub)
        .bind(amount)
        .bind(auto_release_at)
        .execute(db.pool())
        .await?;

        // Deduct from buyer's wallet and log transaction
        Self::deduct_wallet(db, buyer_npub, amount, TransactionType::EscrowHold, Some(&id)).await?;

        let escrow = sqlx::query_as::<_, Escrow>("SELECT * FROM escrows WHERE id = ?")
            .bind(&id)
            .fetch_one(db.pool())
            .await?;

        Ok(escrow)
    }

    /// Release escrow funds to seller (buyer confirms or auto-release)
    pub async fn release_escrow(db: &Database, escrow_id: &str) -> AppResult<()> {
        let escrow = Self::get_escrow(db, escrow_id).await?;

        if escrow.status_enum() != EscrowStatus::Held {
            return Err(AppError::EscrowAlreadyReleased);
        }

        // Update escrow status
        sqlx::query("UPDATE escrows SET status = 'released', resolved_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(escrow_id)
            .execute(db.pool())
            .await?;

        // Credit seller's wallet
        Self::credit_wallet(
            db,
            &escrow.seller_npub,
            escrow.amount,
            TransactionType::EscrowRelease,
            Some(escrow_id),
        )
        .await?;

        // Update related order status
        sqlx::query("UPDATE orders SET status = 'completed', completed_at = CURRENT_TIMESTAMP WHERE escrow_id = ?")
            .bind(escrow_id)
            .execute(db.pool())
            .await?;

        Ok(())
    }

    /// Refund escrow funds to buyer
    #[allow(dead_code)]
    pub async fn refund_escrow(db: &Database, escrow_id: &str) -> AppResult<()> {
        let escrow = Self::get_escrow(db, escrow_id).await?;

        if escrow.status_enum() != EscrowStatus::Held
            && escrow.status_enum() != EscrowStatus::Disputed
        {
            return Err(AppError::EscrowAlreadyRefunded);
        }

        // Update escrow status
        sqlx::query("UPDATE escrows SET status = 'refunded', resolved_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(escrow_id)
            .execute(db.pool())
            .await?;

        // Credit buyer's wallet
        Self::credit_wallet(
            db,
            &escrow.buyer_npub,
            escrow.amount,
            TransactionType::EscrowRefund,
            Some(escrow_id),
        )
        .await?;

        // Update related order status
        sqlx::query("UPDATE orders SET status = 'refunded' WHERE escrow_id = ?")
            .bind(escrow_id)
            .execute(db.pool())
            .await?;

        Ok(())
    }

    /// Resolve dispute with specified resolution
    pub async fn resolve_dispute(
        db: &Database,
        escrow_id: &str,
        resolution: DisputeResolution,
    ) -> AppResult<()> {
        let escrow = Self::get_escrow(db, escrow_id).await?;

        if escrow.status_enum() != EscrowStatus::Disputed {
            return Err(AppError::EscrowNotFound);
        }

        let (buyer_amount, seller_amount) = resolution.calculate_amounts(escrow.amount);

        // Update escrow status based on resolution
        let new_status = match resolution {
            DisputeResolution::BuyerFull => "refunded",
            DisputeResolution::SellerFull => "released",
            DisputeResolution::Split { .. } => "released", // partial release
            DisputeResolution::Burn => "released",         // funds burned
        };

        sqlx::query("UPDATE escrows SET status = ?, resolved_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(new_status)
            .bind(escrow_id)
            .execute(db.pool())
            .await?;

        // Distribute funds
        if buyer_amount > 0 {
            Self::credit_wallet(
                db,
                &escrow.buyer_npub,
                buyer_amount,
                TransactionType::EscrowRefund,
                Some(escrow_id),
            )
            .await?;
        }

        if seller_amount > 0 {
            Self::credit_wallet(
                db,
                &escrow.seller_npub,
                seller_amount,
                TransactionType::EscrowRelease,
                Some(escrow_id),
            )
            .await?;
        }

        // Update order status
        let order_status = match resolution {
            DisputeResolution::BuyerFull => "refunded",
            _ => "completed",
        };
        sqlx::query("UPDATE orders SET status = ?, completed_at = CURRENT_TIMESTAMP WHERE escrow_id = ?")
            .bind(order_status)
            .bind(escrow_id)
            .execute(db.pool())
            .await?;

        Ok(())
    }

    /// Mark escrow as disputed
    pub async fn mark_disputed(db: &Database, escrow_id: &str) -> AppResult<()> {
        sqlx::query("UPDATE escrows SET status = 'disputed' WHERE id = ? AND status = 'held'")
            .bind(escrow_id)
            .execute(db.pool())
            .await?;
        Ok(())
    }

    /// Get escrow by ID
    pub async fn get_escrow(db: &Database, escrow_id: &str) -> AppResult<Escrow> {
        sqlx::query_as::<_, Escrow>("SELECT * FROM escrows WHERE id = ?")
            .bind(escrow_id)
            .fetch_optional(db.pool())
            .await?
            .ok_or(AppError::EscrowNotFound)
    }

    /// Get escrows ready for auto-release
    pub async fn get_pending_auto_releases(db: &Database) -> AppResult<Vec<Escrow>> {
        let escrows = sqlx::query_as::<_, Escrow>(
            "SELECT * FROM escrows WHERE status = 'held' AND auto_release_at <= CURRENT_TIMESTAMP",
        )
        .fetch_all(db.pool())
        .await?;
        Ok(escrows)
    }

    /// Process auto-releases (call periodically)
    pub async fn process_auto_releases(db: &Database) -> AppResult<u32> {
        let pending = Self::get_pending_auto_releases(db).await?;
        let mut released = 0;

        for escrow in pending {
            // Only auto-release if not disputed
            if escrow.status_enum() == EscrowStatus::Held {
                if let Ok(()) = Self::release_escrow(db, &escrow.id).await {
                    released += 1;
                    tracing::info!("Auto-released escrow {}", escrow.id);
                }
            }
        }

        Ok(released)
    }

    /// Deduct from user wallet with transaction logging
    async fn deduct_wallet(
        db: &Database,
        user_npub: &str,
        amount: i64,
        tx_type: TransactionType,
        reference_id: Option<&str>,
    ) -> AppResult<i64> {
        // Get current balance
        let row: (i64,) =
            sqlx::query_as("SELECT wallet_balance FROM users WHERE npub = ?")
                .bind(user_npub)
                .fetch_one(db.pool())
                .await?;

        let current_balance = row.0;
        if current_balance < amount {
            return Err(AppError::InsufficientBalanceDetails {
                needed: amount as u64,
                available: current_balance as u64,
            });
        }

        let new_balance = current_balance - amount;

        // Update balance
        sqlx::query("UPDATE users SET wallet_balance = ? WHERE npub = ?")
            .bind(new_balance)
            .bind(user_npub)
            .execute(db.pool())
            .await?;

        // Log transaction
        let tx_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO wallet_transactions (id, user_npub, transaction_type, amount, balance_after, reference_id, created_at)
            VALUES (?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
            "#,
        )
        .bind(&tx_id)
        .bind(user_npub)
        .bind(String::from(tx_type))
        .bind(-amount) // negative for deduction
        .bind(new_balance)
        .bind(reference_id)
        .execute(db.pool())
        .await?;

        Ok(new_balance)
    }

    /// Credit user wallet with transaction logging
    async fn credit_wallet(
        db: &Database,
        user_npub: &str,
        amount: i64,
        tx_type: TransactionType,
        reference_id: Option<&str>,
    ) -> AppResult<i64> {
        // Get current balance
        let row: (i64,) =
            sqlx::query_as("SELECT wallet_balance FROM users WHERE npub = ?")
                .bind(user_npub)
                .fetch_one(db.pool())
                .await?;

        let current_balance = row.0;
        let new_balance = current_balance + amount;

        // Update balance
        sqlx::query("UPDATE users SET wallet_balance = ? WHERE npub = ?")
            .bind(new_balance)
            .bind(user_npub)
            .execute(db.pool())
            .await?;

        // Log transaction
        let tx_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO wallet_transactions (id, user_npub, transaction_type, amount, balance_after, reference_id, created_at)
            VALUES (?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
            "#,
        )
        .bind(&tx_id)
        .bind(user_npub)
        .bind(String::from(tx_type))
        .bind(amount)
        .bind(new_balance)
        .bind(reference_id)
        .execute(db.pool())
        .await?;

        Ok(new_balance)
    }
}
