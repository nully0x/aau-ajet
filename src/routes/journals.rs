use actix_session::Session;
use actix_web::{get, web, HttpResponse};
use askama::Template;
use chrono::Datelike;
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;

use crate::db::journal_repository::JournalRepository;
use crate::db::schema::init_db;
use crate::errors::SubmissionError;
use crate::models::journals::Journal;

#[derive(Template)]
#[template(path = "journals/details.html")]
struct JournalDetailTemplate {
    journal: Journal,
    id_string: String,
    is_admin: bool,
}

#[derive(Template, Debug)]
#[template(path = "journals/journal.html")]
struct JournalTemplate {
    journals: Vec<Journal>,
    archives: BTreeMap<i32, BTreeMap<i32, Vec<Journal>>>,
}

impl JournalTemplate {
    fn get_journal_count(&self, journals: &[Journal]) -> usize {
        journals.len()
    }
}

#[derive(Deserialize)]
pub struct JournalQueryParams {
    pub page: Option<i32>,
    pub limit: Option<i32>,
    pub category: Option<String>,
    pub volume: Option<i32>,
    pub issue: Option<i32>,
}

// New API endpoint for initial data
#[get("/api/journals/initial-data")]
pub async fn journal_initial_data() -> Result<HttpResponse, SubmissionError> {
    let conn = init_db().map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;
    let repository = JournalRepository::new(conn);
    let all_journals = repository.get_all_journals_for_archive()?;

    Ok(HttpResponse::Ok().json(all_journals))
}

#[get("/journals/{id}")]
pub async fn journal_detail_handler(
    id: web::Path<i32>,
    session: Session,
) -> Result<HttpResponse, SubmissionError> {
    let journal_id = id.into_inner();

    let conn = init_db().map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;
    let repository = JournalRepository::new(conn);
    let journal = repository.get_journal_by_id(journal_id)?;

    let is_admin = session
        .get::<i32>("admin_id")
        .map_err(|e| SubmissionError::DatabaseError(e.to_string()))?
        .is_some();

    Ok(HttpResponse::Ok().body(
        JournalDetailTemplate {
            journal,
            id_string: journal_id.to_string(),
            is_admin,
        }
        .render()
        .map_err(|e| SubmissionError::InternalError(format!("Template error: {}", e)))?,
    ))
}

#[get("/journal")]
pub async fn journal_handler() -> Result<HttpResponse, SubmissionError> {
    let conn = init_db().map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;
    let repository = JournalRepository::new(conn);

    let all_journals = repository.get_all_journals_for_archive()?;

    let mut archives: BTreeMap<i32, BTreeMap<i32, Vec<Journal>>> = BTreeMap::new();
    for journal in all_journals.iter() {
        archives
            .entry(journal.volume_number)
            .or_insert_with(BTreeMap::new)
            .entry(journal.issue_number)
            .or_insert_with(Vec::new)
            .push(journal.clone());
    }

    let initial_journals: Vec<Journal> = all_journals.iter().take(12).map(|j| j.clone()).collect();

    let template = JournalTemplate {
        journals: initial_journals,
        archives,
    };

    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(
            template
                .render()
                .map_err(|e| SubmissionError::InternalError(format!("Template error: {}", e)))?,
        ))
}

#[get("/api/journals")]
pub async fn journal_api_handler(
    query: web::Query<JournalQueryParams>,
) -> Result<HttpResponse, SubmissionError> {
    let page = query.page.unwrap_or(1);
    let limit = query.limit.unwrap_or(12);
    let offset = (page - 1) * limit;
    let category = query.category.as_deref().unwrap_or("all");

    let conn = init_db().map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;
    let repository = JournalRepository::new(conn);

    let mut journals = match category {
        "latest" => repository.get_latest_journals(limit)?,
        "current" => repository.get_current_edition(limit)?,
        "past" => repository.get_past_issues(limit, offset)?,
        _ => repository.get_all_journals(limit, offset)?,
    };

    if let Some(volume) = query.volume {
        journals.retain(|j| j.volume_number == volume);

        if let Some(issue) = query.issue {
            journals.retain(|j| j.issue_number == issue);
        }
    }

    Ok(HttpResponse::Ok().json(json!({
        "journals": journals,
        "hasMore": journals.len() == limit as usize
    })))
}
