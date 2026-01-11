use std::task::{Context, Poll};

use axum::{
    body::Body,
    http::{header, Request, Response, StatusCode},
    response::IntoResponse,
};
use tower::{Layer, Service};

/// Configuration for browsing fee middleware
#[derive(Clone)]
pub struct BrowsingFeeConfig {
    /// Minimum fee in sats
    pub min_fee_sats: u64,
    /// Paths that require browsing fee (prefix matching)
    pub protected_paths: Vec<String>,
    /// Paths that are always free
    pub free_paths: Vec<String>,
}

impl Default for BrowsingFeeConfig {
    fn default() -> Self {
        Self {
            min_fee_sats: 100,
            protected_paths: vec![
                "/listings".to_string(),
            ],
            free_paths: vec![
                "/".to_string(),
                "/login".to_string(),
                "/register".to_string(),
                "/static".to_string(),
                "/health".to_string(),
                "/wallet".to_string(),
                "/cart".to_string(),
                "/orders".to_string(),
            ],
        }
    }
}

/// Browsing fee layer
#[derive(Clone)]
pub struct BrowsingFeeLayer {
    config: BrowsingFeeConfig,
}

impl BrowsingFeeLayer {
    pub fn new(config: BrowsingFeeConfig) -> Self {
        Self { config }
    }
}

impl<S> Layer<S> for BrowsingFeeLayer {
    type Service = BrowsingFeeMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        BrowsingFeeMiddleware {
            inner,
            config: self.config.clone(),
        }
    }
}

/// Browsing fee middleware service
#[derive(Clone)]
pub struct BrowsingFeeMiddleware<S> {
    inner: S,
    config: BrowsingFeeConfig,
}

impl<S> Service<Request<Body>> for BrowsingFeeMiddleware<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let path = req.uri().path().to_string();
        let config = self.config.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // Check if path is free
            for free_path in &config.free_paths {
                if path.starts_with(free_path) {
                    return inner.call(req).await;
                }
            }

            // Check if path requires fee
            let requires_fee = config.protected_paths.iter().any(|p| path.starts_with(p));

            if !requires_fee {
                return inner.call(req).await;
            }

            // Check for X-Cashu header
            let token = req
                .headers()
                .get("X-Cashu")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            // Check for session cookie (logged-in users with balance get free browsing)
            let has_session = req
                .headers()
                .get(header::COOKIE)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.contains("session="))
                .unwrap_or(false);

            // If logged in, allow access (they pay through their account)
            if has_session {
                return inner.call(req).await;
            }

            // If no token and not logged in, return 402 Payment Required
            if token.is_none() {
                let response = PaymentRequiredResponse {
                    min_fee_sats: config.min_fee_sats,
                    message: "Browsing fee required. Send X-Cashu header with valid token.".to_string(),
                };
                return Ok(response.into_response());
            }

            // Token validation would happen here via AppState
            // For now, we just check the token format
            let token = token.unwrap();
            if !token.starts_with("cashuA") {
                let response = PaymentRequiredResponse {
                    min_fee_sats: config.min_fee_sats,
                    message: "Invalid Cashu token format".to_string(),
                };
                return Ok(response.into_response());
            }

            // Token looks valid, proceed
            // In production, we'd validate via CashuService here
            inner.call(req).await
        })
    }
}

/// Response for 402 Payment Required
struct PaymentRequiredResponse {
    min_fee_sats: u64,
    message: String,
}

impl IntoResponse for PaymentRequiredResponse {
    fn into_response(self) -> Response<Body> {
        let body = format!(
            r#"{{"error":"payment_required","min_fee_sats":{},"message":"{}"}}"#,
            self.min_fee_sats, self.message
        );

        Response::builder()
            .status(StatusCode::PAYMENT_REQUIRED)
            .header(header::CONTENT_TYPE, "application/json")
            .header("X-Cashu-Required", self.min_fee_sats.to_string())
            .body(Body::from(body))
            .unwrap()
    }
}

/// Helper to create browsing fee middleware with state access
#[allow(dead_code)]
pub fn browsing_fee_layer(config: BrowsingFeeConfig) -> BrowsingFeeLayer {
    BrowsingFeeLayer::new(config)
}
