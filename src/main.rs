use crate::{
    db::{admin_repository::AdminRepository, schema::init_db}, // Import AdminRepository
    utils::{ensure_upload_dir, security::hash_password},      // Import hash_password
};
use actix_files as fs;
use actix_session::{storage::CookieSessionStore, SessionMiddleware}; // Import session components
use actix_web::{cookie::Key, web, App, HttpServer}; // Import Key and web
use dotenv::dotenv;
use env_logger::Env;
use log::{error, info, warn};

mod config;
mod db;
mod errors;
mod models;
mod routes;
mod utils;

async fn seed_admin_user() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok(); // Ensure .env is loaded

    let admin_email = std::env::var("ADMIN_EMAIL").expect("ADMIN_EMAIL must be set in .env");
    let admin_password =
        std::env::var("ADMIN_PASSWORD").expect("ADMIN_PASSWORD must be set in .env");

    let conn = init_db()?; // Propagate DB errors
    let admin_repo = AdminRepository::new(conn);

    // Check if admin already exists
    if admin_repo.find_admin_by_email(&admin_email)?.is_none() {
        info!("Admin user not found, creating...");
        // Hash the password in a blocking thread
        let password_clone = admin_password.clone();
        let hashed_password = web::block(move || hash_password(&password_clone)).await??; // Handle blocking and hashing errors

        admin_repo.create_admin(&admin_email, &hashed_password)?;
        info!("Admin user created successfully for email: {}", admin_email);
    } else {
        info!("Admin user already exists for email: {}", admin_email);
    }

    Ok(())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    // --- Seed Admin User ---
    if let Err(e) = seed_admin_user().await {
        error!("Failed to seed admin user: {}", e);
        // std::process::exit(1); // Optionally exit on seeding failure
    }
    // --- End Seed ---

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .expect("PORT must be a valid number");

    if let Err(e) = ensure_upload_dir() {
        warn!("Failed to create uploads directory: {}", e);
    }

    // --- Session Key from Environment ---
    let session_secret =
        std::env::var("SESSION_SECRET_KEY").expect("SESSION_SECRET_KEY must be set in .env");
    // The key needs to be &[u8]. We use the raw bytes of the secret string.
    // Ensure the string in .env is long enough (e.g., 64+ chars recommended).
    let secret_key = Key::from(session_secret.as_bytes());
    // --- End Session Key ---

    info!("Starting server on http://{}:{}...", host, port);

    HttpServer::new(move || {
        // Clone the key for the closure
        let secret_key = secret_key.clone();

        App::new()
            // --- Session Middleware ---
            .wrap(
                SessionMiddleware::builder(CookieSessionStore::default(), secret_key) // Use the cloned key
                    .cookie_secure(false) // Set to true if using HTTPS
                    .cookie_path("/".to_string())
                    .cookie_name("ajet-session".to_string())
                    .cookie_http_only(true)
                    .cookie_same_site(actix_web::cookie::SameSite::Lax)
                    .build(),
            )
            // --- End Session Middleware ---
            // --- Logging Middleware ---
            .wrap(actix_web::middleware::Logger::default())
            // --- End Logging ---
            // Serve static files
            .service(fs::Files::new("/static", "./src/static"))
            .service(fs::Files::new("/download", "./data/uploads"))
            // --- Public Routes ---
            .service(routes::landing::landing_handler)
            .service(routes::journals::journal_detail_handler)
            .service(routes::about::about_handler)
            .service(routes::submissions::submit_paper_handler)
            .service(routes::submissions::process_submission)
            .service(routes::editorial::editorial_board_handler)
            .service(routes::journals::journal_handler)
            .service(routes::journals::journal_initial_data)
            .service(routes::journals::journal_api_handler)
            .service(routes::manuscript::manuscript_guide)
            .service(routes::auth::show_login_form)
            .service(routes::auth::login)
            // --- Admin Routes (Scoped under /admin) ---
            .service(
                web::scope("/admin")
                    .service(routes::auth::logout)
                    .service(routes::admin::admin_dashboard_handler)
                    .service(routes::admin::upload_journal_handler)
                    .service(routes::admin::process_upload)
                    .service(routes::admin::delete_journal_handler)
                    .service(routes::admin::admin_submissions_handler)
                    .service(routes::admin::download_submission_handler)
                    .service(routes::admin::edit_journal_form_handler)
                    .service(routes::admin::update_journal_handler),
            )
    })
    .bind((host.as_str(), port))?
    .run()
    .await
}
