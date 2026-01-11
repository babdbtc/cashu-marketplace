# Marketplace

Privacy-first Tor marketplace with Cashu payments and Nostr identity.

## Tech Stack

- **Backend**: Rust with Axum web framework
- **Database**: SQLite
- **Payments**: Cashu (ecash)
- **Identity**: Nostr
- **Templating**: Askama

## Setup

1. Copy the example environment file:
   ```bash
   cp .env.example .env
   ```

2. Configure your `.env` file with the required settings

3. Run the application:
   ```bash
   cargo run
   ```

## License

MIT
