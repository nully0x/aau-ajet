use actix_web::{
    body::{EitherBody, MessageBody},
    dev::{ServiceRequest, ServiceResponse},
    middleware::Next,
    HttpResponse,
};
use actix_web_ratelimit::config::RateLimitConfig;
use log::warn;

/// Bots that provide no value to an academic journal site and are commonly
/// used for AI training corpora or SEO competitive scraping. Blocked with 403
/// before any handler runs, so they never touch the DB, templates, or uploads.
///
/// Legitimate indexers (Googlebot, Bingbot, facebookexternalhit) are intentionally
/// NOT listed so the journal stays discoverable. Add/remove entries as needed.
const BLOCKED_USER_AGENTS: &[&str] = &[
    "semrushbot",   // SEO competitive analysis
    "amazonbot",    // Amazon general crawl / AI training
    "bytespider",   // ByteDance (TikTok) AI training
    "360spider",    // 360 search, aggressive
    "chatgpt-user", // OpenAI user-initiated fetch
    "gptbot",       // OpenAI training
    "claudebot",    // Anthropic training
    "anthropic-ai", // Anthropic training
    "ahrefsbot",    // SEO
    "dotbot",       // SEO
    "mj12bot",      // Majestic, very aggressive
    "petalbot",     // Huawei
    "blexbot",      // SEO
    "sogou",        // Sogou search
    "yandexbot",    // Yandex, low value for this audience
    "applebot",     // Apple AI/Spotlight crawl
    "cohere-ai",    // Cohere training
    "diffbot",      // Diffbot extraction
    "imagesiftbot", // image scraping
    "perplexitybot", // Perplexity AI
];

/// Middleware: 403 any request whose User-Agent matches a known scraper.
/// Comparison is case-insensitive on a substring of the UA. Requests with no
/// User-Agent pass through (legitimate clients sometimes omit it).
pub async fn block_scrapers<B>(
    req: ServiceRequest,
    next: Next<B>,
) -> Result<ServiceResponse<EitherBody<B>>, actix_web::Error>
where
    B: MessageBody,
{
    let blocked = req
        .headers()
        .get("user-agent")
        .and_then(|h| h.to_str().ok())
        .map(|ua| {
            let ua = ua.to_ascii_lowercase();
            BLOCKED_USER_AGENTS.iter().any(|bot| ua.contains(bot))
        })
        .unwrap_or(false);

    if blocked {
        warn!("Blocked scraper User-Agent on {}", req.path());
        return Ok(req
            .into_response(HttpResponse::Forbidden().finish())
            .map_into_right_body());
    }

    let res = next.call(req).await?;
    Ok(res.map_into_left_body())
}

pub fn rate_limit_config(max_requests: usize, window_secs: u64) -> RateLimitConfig {
    RateLimitConfig::default()
        .max_requests(max_requests)
        .window_secs(window_secs)
        .exceeded(on_rate_limit_exceeded)
}

fn on_rate_limit_exceeded(
    id: &String,
    config: &RateLimitConfig,
    _req: &ServiceRequest,
) -> HttpResponse {
    warn!(
        "Rate limit exceeded for client {} (limit: {} req per {}s)",
        id,
        config.max_requests,
        config.window_secs.as_secs()
    );
    HttpResponse::TooManyRequests()
        .append_header(("Retry-After", config.window_secs.as_secs().to_string()))
        .body("Too many requests. Please try again later.")
}
