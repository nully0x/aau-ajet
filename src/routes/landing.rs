use actix_web::{get, HttpResponse};
use askama::Template;

use crate::db::journal_repository::JournalRepository;
use crate::db::schema::init_db;
use crate::errors::SubmissionError;
use crate::models::journals::Journal;

#[derive(Template)]
#[template(path = "landing.html")]
struct LandingTemplate {
    journals: Vec<Journal>,
}

#[get("/")]
pub async fn landing_handler() -> Result<HttpResponse, SubmissionError> {
    let conn = init_db().map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;
    let repository = JournalRepository::new(conn);
    let journals = repository.get_latest_journals(3)?; // Get latest 3 journals

    Ok(HttpResponse::Ok().body(
        LandingTemplate { journals }
            .render()
            .map_err(|e| SubmissionError::InternalError(format!("Template error: {}", e)))?,
    ))
}
