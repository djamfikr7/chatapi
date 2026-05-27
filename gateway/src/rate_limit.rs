use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

/// Per-client rate limit state: (request count, window start)
#[derive(Clone)]
pub struct RateLimiter {
    max_requests: u32,
    window_secs: u64,
    state: Arc<Mutex<HashMap<String, (u32, Instant)>>>,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window_secs: u64) -> Self {
        Self {
            max_requests,
            window_secs,
            state: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn check(&self, key: &str) -> Result<u32, u64> {
        let now = Instant::now();
        let mut state = self.state.lock().await;

        // Evict entries expired by 2x window (prevents unbounded growth)
        let stale = self.window_secs * 2;
        state.retain(|_, (_, started)| now.duration_since(*started).as_secs() < stale);

        let entry = state.entry(key.to_string()).or_insert((0, now));

        if now.duration_since(entry.1).as_secs() >= self.window_secs {
            *entry = (0, now);
        }

        if entry.0 >= self.max_requests {
            let elapsed = now.duration_since(entry.1).as_secs();
            let retry_after = self.window_secs.saturating_sub(elapsed).max(1);
            return Err(retry_after);
        }

        entry.0 += 1;
        Ok(self.max_requests - entry.0)
    }
}

/// Two-tier rate limiter: stricter for chat, looser for general API.
#[derive(Clone)]
pub struct TwoTierRateLimiter {
    chat: RateLimiter,
    api: RateLimiter,
}

impl TwoTierRateLimiter {
    pub fn new(chat_per_min: u32, api_per_min: u32) -> Self {
        Self {
            chat: RateLimiter::new(chat_per_min, 60),
            api: RateLimiter::new(api_per_min, 60),
        }
    }
}

fn client_ip(req: &Request) -> String {
    req.headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
        .or_else(|| {
            req.headers()
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "global".to_string())
}

/// Axum middleware that enforces per-IP rate limiting with two tiers.
/// Chat completions (/v1/chat/completions) get the stricter limit; everything else gets the general limit.
pub async fn rate_limit_middleware(
    req: Request,
    next: Next,
) -> Response {
    let limiter = match req.extensions().get::<TwoTierRateLimiter>().cloned() {
        Some(l) => l,
        None => return next.run(req).await,
    };

    let key = client_ip(&req);
    let is_chat = req.uri().path() == "/v1/chat/completions";

    let check = if is_chat {
        limiter.chat.check(&key).await
    } else {
        limiter.api.check(&key).await
    };

    match check {
        Ok(_remaining) => next.run(req).await,
        Err(retry_after) => (
            StatusCode::TOO_MANY_REQUESTS,
            [
                ("Retry-After", retry_after.to_string()),
                ("Content-Type", "application/json".to_string()),
            ],
            format!(
                r#"{{"error":"rate_limit_exceeded","retry_after":{}}}"#,
                retry_after
            ),
        )
            .into_response(),
    }
}
