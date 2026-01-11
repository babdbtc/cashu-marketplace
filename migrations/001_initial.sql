-- Initial database schema for marketplace

-- Users table
CREATE TABLE IF NOT EXISTS users (
    npub TEXT PRIMARY KEY,
    encrypted_nsec TEXT,  -- NULL if user brought their own key
    role TEXT NOT NULL DEFAULT 'buyer',  -- 'buyer', 'seller', 'admin'
    wallet_balance INTEGER NOT NULL DEFAULT 0,
    message_price INTEGER,  -- NULL = disabled, otherwise sats per message
    last_active_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Sessions table
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    user_npub TEXT NOT NULL,
    expires_at TIMESTAMP NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_npub) REFERENCES users(npub) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_npub);
CREATE INDEX IF NOT EXISTS idx_sessions_expires ON sessions(expires_at);

-- Seller categories (bonds paid)
CREATE TABLE IF NOT EXISTS seller_categories (
    npub TEXT NOT NULL,
    category TEXT NOT NULL,  -- 'digital', 'physical', 'services'
    bond_paid INTEGER NOT NULL,
    paid_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (npub, category),
    FOREIGN KEY (npub) REFERENCES users(npub) ON DELETE CASCADE
);

-- Listings table
CREATE TABLE IF NOT EXISTS listings (
    id TEXT PRIMARY KEY,
    seller_npub TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    price INTEGER NOT NULL,  -- sats
    category TEXT NOT NULL,  -- 'digital', 'physical', 'services'
    is_active BOOLEAN NOT NULL DEFAULT true,
    stock INTEGER,  -- NULL = unlimited
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMP NOT NULL,
    FOREIGN KEY (seller_npub) REFERENCES users(npub)
);

CREATE INDEX IF NOT EXISTS idx_listings_seller ON listings(seller_npub);
CREATE INDEX IF NOT EXISTS idx_listings_category ON listings(category);
CREATE INDEX IF NOT EXISTS idx_listings_active ON listings(is_active, expires_at);

-- Listing images
CREATE TABLE IF NOT EXISTS listing_images (
    id TEXT PRIMARY KEY,
    listing_id TEXT NOT NULL,
    image_data BLOB NOT NULL,
    mime_type TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (listing_id) REFERENCES listings(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_listing_images_listing ON listing_images(listing_id);

-- Shopping cart
CREATE TABLE IF NOT EXISTS cart_items (
    id TEXT PRIMARY KEY,
    user_npub TEXT NOT NULL,
    listing_id TEXT NOT NULL,
    added_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_npub) REFERENCES users(npub) ON DELETE CASCADE,
    FOREIGN KEY (listing_id) REFERENCES listings(id) ON DELETE CASCADE,
    UNIQUE(user_npub, listing_id)
);

CREATE INDEX IF NOT EXISTS idx_cart_user ON cart_items(user_npub);

-- Checkout sessions (price locking)
CREATE TABLE IF NOT EXISTS checkout_sessions (
    id TEXT PRIMARY KEY,
    user_npub TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',  -- 'pending', 'paid', 'expired'
    total_amount INTEGER NOT NULL,
    fee_amount INTEGER NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMP NOT NULL,
    paid_at TIMESTAMP,
    FOREIGN KEY (user_npub) REFERENCES users(npub)
);

CREATE INDEX IF NOT EXISTS idx_checkout_user ON checkout_sessions(user_npub);
CREATE INDEX IF NOT EXISTS idx_checkout_status ON checkout_sessions(status, expires_at);

-- Checkout items (locked prices)
CREATE TABLE IF NOT EXISTS checkout_items (
    id TEXT PRIMARY KEY,
    checkout_id TEXT NOT NULL,
    listing_id TEXT NOT NULL,
    seller_npub TEXT NOT NULL,
    locked_price INTEGER NOT NULL,
    encrypted_shipping TEXT,  -- NULL for digital items
    FOREIGN KEY (checkout_id) REFERENCES checkout_sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (listing_id) REFERENCES listings(id)
);

CREATE INDEX IF NOT EXISTS idx_checkout_items_checkout ON checkout_items(checkout_id);

-- Escrows table
CREATE TABLE IF NOT EXISTS escrows (
    id TEXT PRIMARY KEY,
    buyer_npub TEXT NOT NULL,
    seller_npub TEXT NOT NULL,
    amount INTEGER NOT NULL,  -- sats held
    status TEXT NOT NULL DEFAULT 'held',  -- 'held', 'released', 'refunded', 'disputed'
    auto_release_at TIMESTAMP NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    resolved_at TIMESTAMP,
    FOREIGN KEY (buyer_npub) REFERENCES users(npub),
    FOREIGN KEY (seller_npub) REFERENCES users(npub)
);

CREATE INDEX IF NOT EXISTS idx_escrows_buyer ON escrows(buyer_npub);
CREATE INDEX IF NOT EXISTS idx_escrows_seller ON escrows(seller_npub);
CREATE INDEX IF NOT EXISTS idx_escrows_status ON escrows(status);
CREATE INDEX IF NOT EXISTS idx_escrows_auto_release ON escrows(auto_release_at) WHERE status = 'held';

-- Orders table
CREATE TABLE IF NOT EXISTS orders (
    id TEXT PRIMARY KEY,
    checkout_id TEXT NOT NULL,
    buyer_npub TEXT NOT NULL,
    seller_npub TEXT NOT NULL,
    escrow_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',  -- 'pending', 'shipped', 'completed', 'disputed', 'refunded'
    tracking_info TEXT,
    shipped_at TIMESTAMP,
    completed_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (checkout_id) REFERENCES checkout_sessions(id),
    FOREIGN KEY (escrow_id) REFERENCES escrows(id),
    FOREIGN KEY (buyer_npub) REFERENCES users(npub),
    FOREIGN KEY (seller_npub) REFERENCES users(npub)
);

CREATE INDEX IF NOT EXISTS idx_orders_buyer ON orders(buyer_npub);
CREATE INDEX IF NOT EXISTS idx_orders_seller ON orders(seller_npub);
CREATE INDEX IF NOT EXISTS idx_orders_status ON orders(status);

-- Order items
CREATE TABLE IF NOT EXISTS order_items (
    id TEXT PRIMARY KEY,
    order_id TEXT NOT NULL,
    listing_id TEXT NOT NULL,
    price INTEGER NOT NULL,
    encrypted_shipping TEXT,
    digital_content TEXT,  -- For digital delivery
    FOREIGN KEY (order_id) REFERENCES orders(id) ON DELETE CASCADE,
    FOREIGN KEY (listing_id) REFERENCES listings(id)
);

CREATE INDEX IF NOT EXISTS idx_order_items_order ON order_items(order_id);

-- Disputes table
CREATE TABLE IF NOT EXISTS disputes (
    id TEXT PRIMARY KEY,
    order_id TEXT NOT NULL UNIQUE,
    escrow_id TEXT NOT NULL,
    initiated_by TEXT NOT NULL,  -- 'buyer' or 'seller'
    reason TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',  -- 'open', 'resolved'
    resolution TEXT,  -- 'buyer_full', 'seller_full', 'split_X_Y', 'burn'
    resolution_notes TEXT,
    resolved_by TEXT,  -- admin npub who resolved
    warning_sent_at TIMESTAMP,
    auto_resolve_at TIMESTAMP NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    resolved_at TIMESTAMP,
    FOREIGN KEY (order_id) REFERENCES orders(id),
    FOREIGN KEY (escrow_id) REFERENCES escrows(id)
);

CREATE INDEX IF NOT EXISTS idx_disputes_status ON disputes(status);
CREATE INDEX IF NOT EXISTS idx_disputes_auto_resolve ON disputes(auto_resolve_at) WHERE status = 'open';

-- Dispute evidence
CREATE TABLE IF NOT EXISTS dispute_evidence (
    id TEXT PRIMARY KEY,
    dispute_id TEXT NOT NULL,
    submitted_by TEXT NOT NULL,  -- npub
    evidence_type TEXT NOT NULL,  -- 'text', 'image'
    content TEXT NOT NULL,  -- text or base64 image
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (dispute_id) REFERENCES disputes(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_dispute_evidence_dispute ON dispute_evidence(dispute_id);

-- Pre-purchase conversations
CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY,
    buyer_npub TEXT NOT NULL,
    seller_npub TEXT NOT NULL,
    seller_price INTEGER NOT NULL,  -- price per message at time of conversation
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (buyer_npub) REFERENCES users(npub),
    FOREIGN KEY (seller_npub) REFERENCES users(npub)
);

CREATE INDEX IF NOT EXISTS idx_conversations_buyer ON conversations(buyer_npub);
CREATE INDEX IF NOT EXISTS idx_conversations_seller ON conversations(seller_npub);

-- Pre-purchase messages
CREATE TABLE IF NOT EXISTS conversation_messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    sender_npub TEXT NOT NULL,
    encrypted_content TEXT NOT NULL,
    payment_amount INTEGER,  -- NULL for seller replies (free)
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_conv_messages_conv ON conversation_messages(conversation_id);

-- Order messages (post-purchase)
CREATE TABLE IF NOT EXISTS order_messages (
    id TEXT PRIMARY KEY,
    order_id TEXT NOT NULL,
    sender_npub TEXT NOT NULL,
    encrypted_content TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (order_id) REFERENCES orders(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_order_messages_order ON order_messages(order_id);

-- Featured slots configuration
CREATE TABLE IF NOT EXISTS featured_slots (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    position TEXT NOT NULL,
    price_per_day INTEGER NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Featured slot rentals
CREATE TABLE IF NOT EXISTS featured_rentals (
    id TEXT PRIMARY KEY,
    slot_id TEXT NOT NULL,
    listing_id TEXT NOT NULL,
    seller_npub TEXT NOT NULL,
    price_paid INTEGER NOT NULL,
    starts_at TIMESTAMP NOT NULL,
    expires_at TIMESTAMP NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (slot_id) REFERENCES featured_slots(id),
    FOREIGN KEY (listing_id) REFERENCES listings(id),
    FOREIGN KEY (seller_npub) REFERENCES users(npub)
);

CREATE INDEX IF NOT EXISTS idx_featured_rentals_active
ON featured_rentals(slot_id, starts_at, expires_at);

-- Browsing tokens (L402/X-Cashu)
CREATE TABLE IF NOT EXISTS browsing_tokens (
    token_hash TEXT PRIMARY KEY,
    user_npub TEXT,  -- NULL for guests
    amount INTEGER NOT NULL,
    expires_at TIMESTAMP NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_browsing_tokens_expires ON browsing_tokens(expires_at);

-- Seller reputation stats (cached/computed)
CREATE TABLE IF NOT EXISTS seller_stats (
    npub TEXT PRIMARY KEY,
    total_sales INTEGER NOT NULL DEFAULT 0,
    total_revenue INTEGER NOT NULL DEFAULT 0,
    completed_orders INTEGER NOT NULL DEFAULT 0,
    disputed_orders INTEGER NOT NULL DEFAULT 0,
    dispute_rate REAL NOT NULL DEFAULT 0.0,
    avg_rating REAL,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (npub) REFERENCES users(npub) ON DELETE CASCADE
);

-- Order ratings
CREATE TABLE IF NOT EXISTS order_ratings (
    order_id TEXT PRIMARY KEY,
    buyer_npub TEXT NOT NULL,
    seller_npub TEXT NOT NULL,
    rating INTEGER NOT NULL CHECK (rating >= 1 AND rating <= 5),
    comment TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (order_id) REFERENCES orders(id),
    FOREIGN KEY (buyer_npub) REFERENCES users(npub),
    FOREIGN KEY (seller_npub) REFERENCES users(npub)
);

CREATE INDEX IF NOT EXISTS idx_order_ratings_seller ON order_ratings(seller_npub);

-- Wallet transactions log
CREATE TABLE IF NOT EXISTS wallet_transactions (
    id TEXT PRIMARY KEY,
    user_npub TEXT NOT NULL,
    transaction_type TEXT NOT NULL,  -- 'deposit', 'withdraw', 'payment', 'receipt', 'fee', 'bond', 'escrow_hold', 'escrow_release', 'escrow_refund'
    amount INTEGER NOT NULL,
    balance_after INTEGER NOT NULL,
    reference_id TEXT,  -- order_id, escrow_id, etc.
    description TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_npub) REFERENCES users(npub)
);

CREATE INDEX IF NOT EXISTS idx_wallet_tx_user ON wallet_transactions(user_npub);
CREATE INDEX IF NOT EXISTS idx_wallet_tx_type ON wallet_transactions(transaction_type);

-- Full-text search for listings
CREATE VIRTUAL TABLE IF NOT EXISTS listings_fts USING fts5(
    title,
    description,
    content='listings',
    content_rowid='rowid'
);

-- Triggers to keep FTS in sync
CREATE TRIGGER IF NOT EXISTS listings_ai AFTER INSERT ON listings BEGIN
    INSERT INTO listings_fts(rowid, title, description)
    VALUES (NEW.rowid, NEW.title, NEW.description);
END;

CREATE TRIGGER IF NOT EXISTS listings_ad AFTER DELETE ON listings BEGIN
    INSERT INTO listings_fts(listings_fts, rowid, title, description)
    VALUES ('delete', OLD.rowid, OLD.title, OLD.description);
END;

CREATE TRIGGER IF NOT EXISTS listings_au AFTER UPDATE ON listings BEGIN
    INSERT INTO listings_fts(listings_fts, rowid, title, description)
    VALUES ('delete', OLD.rowid, OLD.title, OLD.description);
    INSERT INTO listings_fts(rowid, title, description)
    VALUES (NEW.rowid, NEW.title, NEW.description);
END;
