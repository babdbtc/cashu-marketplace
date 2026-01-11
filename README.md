# Marketplace

A privacy-first Tor marketplace with Cashu ecash payments and Nostr identity.

## Overview

A fully private marketplace where:
- **Users** authenticate with Nostr keypairs or anonymous account numbers
- **Payments** use Cashu ecash for unlinkable transactions
- **Hosting** is Tor-only for operator and user privacy
- **No JavaScript required** - works in Tor Browser "Safest" mode

## Key Features

- **Nostr Authentication** - Login with your nsec or get an anonymous account number
- **Cashu Payments** - Blind signatures provide payment unlinkability
- **Token-Gated Access** - Small browsing fees prevent DDoS attacks
- **Blind Escrow** - 10-day protection with dispute resolution
- **Tiered Seller Access** - Digital, Physical, and Services categories with bonds
- **Multi-Item Cart** - Buy from multiple sellers in one checkout
- **Encrypted Messaging** - NIP-04/NIP-44 encrypted buyer-seller communication

## Why Cashu?

[Cashu](https://cashu.space/) is an ecash protocol that provides significant privacy advantages for a marketplace:

- **Blind Signatures** - The mint signs tokens without seeing their content, so it cannot link who received tokens to who spent them
- **Unlinkable Transactions** - Payments cannot be traced between sender and receiver, even by the mint operator
- **No Account Required** - Users can pay with tokens from any compatible wallet without registration
- **Instant Settlement** - No on-chain confirmation delays; tokens transfer immediately
- **Lightning Integration** - Deposit via Lightning invoice, withdraw to any Lightning address
- **Offline Verification** - P2PK-locked tokens can be verified locally without contacting the mint

This makes Cashu ideal for privacy-focused commerce where neither the platform nor outside observers should be able to track user purchasing behavior.

## DDoS Protection via Token-Gated Access

Traditional DDoS protection relies on CAPTCHAs or IP-based rate limiting, both of which compromise privacy. This marketplace uses **token-gated access** based on the L402 protocol - every request costs a small amount (1 sat), making attacks economically unfeasible.

### How It Works

1. **Unauthenticated Request** - User visits the site without a valid session
2. **402 Payment Required** - Server responds with payment details:
   ```
   HTTP/1.1 402 Payment Required
   X-Cashu-Price: 1
   X-Cashu-Mint: http://marketplace.onion/mint
   X-Cashu-Pubkey: <marketplace-pubkey>
   ```
3. **User Pays** - Submits a Cashu token via form or `X-Cashu` header
4. **Session Created** - Server validates the token and creates a session with browsing balance
5. **Browsing** - Each page view deducts 1 sat; when depleted, user tops up again

### P2PK Token Locking

Tokens are locked to the marketplace's public key using Cashu's P2PK (Pay-to-Public-Key) feature. This enables:

- **Local Verification** - Tokens can be validated without contacting the mint for every request
- **Faster Processing** - No network round-trip needed for token verification
- **Replay Prevention** - Spent token hashes are stored to prevent double-spending

### Economics

| Metric | Value |
|--------|-------|
| Cost per page | 1 sat |
| Typical session | 10-50 pages = 10-50 sats (~$0.01-0.05) |
| DDoS cost for 1M requests | 1M sats (~$300) |

This creates a negligible cost for legitimate users while making large-scale attacks economically impractical.

## User Types

| Type | Identity | Wallet | Disputes |
|------|----------|--------|----------|
| Guest Buyer | Anonymous | External Cashu wallet | No |
| Registered Buyer | Account # or npub | Integrated wallet | Yes |
| Seller | Account # or npub | Integrated wallet | N/A |

## Tech Stack

| Component | Technology |
|-----------|------------|
| **Language** | Rust |
| **Framework** | Axum |
| **Templating** | Askama (compile-time checked) |
| **Database** | SQLite |
| **Payments** | CDK (Cashu Dev Kit) |
| **Identity** | nostr-sdk |
| **Hosting** | Tor Hidden Service |

### Cashu Mint

For production, it is recommended to run your own self-hosted Cashu mint for full control and privacy. The codebase supports any Cashu-compliant mint.

Currently, the [Minibits](https://www.minibits.cash/) public mint is used for testing and development, which can be swapped out by changing a single config value.

## Fee Structure

| Action | Fee |
|--------|-----|
| Browsing | ~1 sat per page |
| Purchase | 1% of sale price |
| Withdrawal | Lightning network fees |

## Escrow System

All purchases go through escrow:

1. Buyer pays - funds locked in escrow
2. Seller ships - provides tracking/proof
3. Buyer confirms OR 10 days pass - funds released (minus 1% fee)
4. Disputes resolved by admin within 10-day window

## Setup

1. Clone the repository

2. Copy the example environment file:
   ```bash
   cp .env.example .env
   ```

3. Configure your `.env` with:
   - Session secret
   - Admin npub
   - Lightning backend (LND, CLN, or LNbits)
   - Mint configuration

4. Run database migrations:
   ```bash
   cargo run -- migrate
   ```

5. Start the server:
   ```bash
   cargo run
   ```

## Configuration

Key environment variables:

```bash
# Server
MARKETPLACE__HOST=127.0.0.1
MARKETPLACE__PORT=3000

# Database
MARKETPLACE__DATABASE_URL=sqlite:data/marketplace.db

# Admin
MARKETPLACE__ADMIN_NPUB=npub1...

# Lightning backend
MARKETPLACE__LIGHTNING__BACKEND=lnbits
MARKETPLACE__LIGHTNING__URL=https://your-instance.com
MARKETPLACE__LIGHTNING__API_KEY=your-key

# Marketplace settings
MARKETPLACE__FEE_PERCENT=1
MARKETPLACE__ESCROW_DAYS=10
```

## Privacy Design

- **No IP logging** - Tor handles anonymity
- **Minimal data retention** - Orders deleted after completion
- **No analytics or tracking**
- **Encrypted database** - SQLCipher support
- **Guest sessions** - Auto-expire after 24h inactivity

## Security

- Nostr challenge-response authentication
- Cashu double-spend prevention
- CSRF protection on all forms
- Rate limiting per session
- Input sanitization (XSS/injection prevention)

## License

MIT
