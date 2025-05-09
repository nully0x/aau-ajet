use actix_files::NamedFile;
use actix_multipart::Multipart;
use actix_session::Session;
use actix_web::http::header::{ContentDisposition, DispositionParam, DispositionType};
use actix_web::{delete, get, post, web, Error as ActixError, HttpRequest, HttpResponse}; // Keep ActixError
use askama::Template;
use chrono::{DateTime, NaiveDate, Utc};
use futures::StreamExt;
use log::{debug, error, warn};
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf; // Use PathBuf

use crate::{
    db::{
        journal_repository::JournalRepository, schema::init_db,
        submission_repository::SubmissionRepository,
    },
    errors::SubmissionError,
    models::{journals::Journal, response::UploadResponse, submission::Submission},
    utils, // Import the utils module
};

type AuthResult = Result<i32, HttpResponse>;

fn check_authentication(session: &Session) -> AuthResult {
    match session.get::<i32>("admin_id") {
        Ok(Some(admin_id)) => Ok(admin_id),
        _ => {
            warn!("Unauthorized access attempt to admin route.");
            Err(HttpResponse::Found()
                .append_header(("Location", "/admin/login")) // Redirect path
                .finish())
        }
    }
}

// --- Templates ---
#[derive(Template)]
#[template(path = "admin/index.html")]
struct AdminDashboardTemplate {
    current_page: &'static str,
    recent_submissions: Vec<Submission>,
}

#[derive(Template)]
#[template(path = "admin/upload.html")]
struct AdminUploadTemplate {
    current_page: &'static str,
}

#[derive(Template)]
#[template(path = "admin/submitted.html")]
struct AdminSubmissionsTemplate {
    submissions: Vec<Submission>,
    current_page: &'static str,
    title: &'static str,
}

#[derive(Template)]
#[template(path = "admin/login.html")]
struct AdminLoginTemplate {
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "admin/edit_journal.html")]
pub struct EditJournalTemplate {
    pub journal: Journal,
    pub error: Option<String>,
    pub current_page: String,
}

impl EditJournalTemplate {
    pub fn new(journal: Journal) -> Self {
        Self {
            journal,
            error: None,
            current_page: "journals".to_string(),
        }
    }
}

#[derive(Deserialize)]
pub struct EditJournalForm {
    pub title: String,
    pub authors: String,
    pub abstract_text: String,
    pub keywords: String,
    pub volume_number: i32,
    pub issue_number: i32,
    pub pages: String,
    pub publication_date: String,
    pub pdf_url: String,
}

// --- Handlers ---

#[get("/login")]
pub async fn admin_login_form_handler() -> HttpResponse {
    // No auth check needed to view the login form
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(
            AdminLoginTemplate { error: None }
                .render()
                .unwrap_or_else(|e| {
                    error!("Login template render error: {:?}", e); // Changed {} to {:?}
                    "Error rendering login page.".to_string()
                }),
        )
}

#[get("/dashboard")]
pub async fn admin_dashboard_handler(session: Session) -> Result<HttpResponse, ActixError> {
    match check_authentication(&session) {
        Ok(_admin_id) => {
            // Get recent submissions
            let conn = init_db().map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;
            let sub_repo = SubmissionRepository::new(conn);
            let recent_submissions = sub_repo.get_recent_submissions(10)?;

            let template = AdminDashboardTemplate {
                current_page: "dashboard",
                recent_submissions,
            };

            Ok(HttpResponse::Ok()
                .content_type("text/html; charset=utf-8")
                .body(template.render().map_err(|e| {
                    error!("Dashboard template render error: {:?}", e); // Changed {} to {:?}
                    actix_web::error::ErrorInternalServerError("Template error")
                })?))
        }
        Err(redirect) => Ok(redirect),
    }
}

#[get("/upload")]
pub async fn upload_journal_handler(session: Session) -> Result<HttpResponse, ActixError> {
    match check_authentication(&session) {
        Ok(_) => Ok(HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            // Pass the current page identifier
            .body(
                AdminUploadTemplate {
                    current_page: "upload",
                }
                .render()
                .map_err(|e| {
                    error!("Upload template render error: {:?}", e); // Changed {} to {:?}
                    actix_web::error::ErrorInternalServerError("Template error")
                })?,
            )),
        Err(redirect) => Ok(redirect),
    }
}

#[post("/upload")]
pub async fn process_upload(
    session: Session,
    mut payload: Multipart,
) -> Result<HttpResponse, ActixError> {
    match check_authentication(&session) {
        Ok(_) => {
            let result: Result<HttpResponse, SubmissionError> = async move {
                let mut title: Option<String> = None;
                let mut authors: Option<String> = None;
                let mut abstract_text: Option<String> = None;
                let mut keywords: Option<String> = None;
                let mut volume_number: Option<i32> = None;
                let mut issue_number: Option<i32> = None;
                let mut pages: Option<String> = None;
                let mut publication_date: Option<String> = None;
                let mut pdf_filename: Option<String> = None;

                while let Some(field_result) = payload.next().await {
                    let mut field = field_result.map_err(|e| {
                        SubmissionError::FileProcessingError(format!("Multipart error: {:?}", e))
                        // Changed {} to {:?}
                    })?;
                    let content_disposition =
                        field.content_disposition().cloned().ok_or_else(|| {
                            SubmissionError::ValidationError(
                                "Content disposition missing".to_string(),
                            )
                        })?;

                    let name = content_disposition.get_name().ok_or_else(|| {
                        SubmissionError::ValidationError("Field name missing".to_string())
                    })?;

                    match name {
                        "title" => title = Some(utils::read_field(field).await?),
                        "authors" => authors = Some(utils::read_field(field).await?),
                        "abstract_text" => abstract_text = Some(utils::read_field(field).await?),
                        "keywords" => keywords = Some(utils::read_field(field).await?),
                        "volume_number" => {
                            volume_number =
                                Some(utils::read_field(field).await?.parse().map_err(|_| {
                                    SubmissionError::ValidationError(
                                        "Invalid volume number".to_string(),
                                    )
                                })?)
                        }
                        "issue_number" => {
                            issue_number =
                                Some(utils::read_field(field).await?.parse().map_err(|_| {
                                    SubmissionError::ValidationError(
                                        "Invalid issue number".to_string(),
                                    )
                                })?)
                        }
                        "pages" => pages = Some(utils::read_field(field).await?),
                        "publication_date" => {
                            publication_date = Some(utils::read_field(field).await?)
                        }
                        "pdf" => pdf_filename = Some(utils::save_uploaded_file(field).await?),
                        _ => while field.next().await.is_some() {},
                    }
                }

                let title = title.ok_or(SubmissionError::ValidationError(
                    "Title is required".to_string(),
                ))?;
                let authors = authors.ok_or(SubmissionError::ValidationError(
                    "Authors are required".to_string(),
                ))?;
                let abstract_text = abstract_text.ok_or(SubmissionError::ValidationError(
                    "Abstract is required".to_string(),
                ))?;
                let keywords = keywords.ok_or(SubmissionError::ValidationError(
                    "Keywords are required".to_string(),
                ))?;
                let volume_number = volume_number.ok_or(SubmissionError::ValidationError(
                    "Volume number is required".to_string(),
                ))?;
                let issue_number = issue_number.ok_or(SubmissionError::ValidationError(
                    "Issue number is required".to_string(),
                ))?;
                let pages = pages.ok_or(SubmissionError::ValidationError(
                    "Pages are required".to_string(),
                ))?;
                let publication_date_str = publication_date.ok_or(
                    SubmissionError::ValidationError("Publication date is required".to_string()),
                )?;
                let pdf_url = pdf_filename.ok_or(SubmissionError::ValidationError(
                    "PDF file is required".to_string(),
                ))?;

                let naive_date = NaiveDate::parse_from_str(&publication_date_str, "%Y-%m-%d")
                    .map_err(|_| {
                        SubmissionError::ValidationError(
                            "Invalid publication date format".to_string(),
                        )
                    })?;
                let publication_datetime = DateTime::<Utc>::from_naive_utc_and_offset(
                    naive_date.and_hms_opt(0, 0, 0).unwrap(),
                    Utc,
                );

                let journal = Journal::new(
                    title,
                    authors,
                    abstract_text,
                    keywords,
                    volume_number,
                    issue_number,
                    pages,
                    publication_datetime,
                    pdf_url,
                );

                let conn = init_db().map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;
                let repository = JournalRepository::new(conn);
                let journal_id = repository.save_journal(&journal)?;

                Ok(HttpResponse::Ok().json(UploadResponse {
                    success: true,
                    journal_id: journal_id as i32,
                    message: "Journal uploaded successfully".to_string(),
                }))
            }
            .await;

            result.map_err(ActixError::from)
        }
        Err(redirect) => Ok(redirect),
    }
}

#[delete("/journals/{id}")]
pub async fn delete_journal_handler(
    session: Session,
    id: web::Path<i32>,
) -> Result<HttpResponse, ActixError> {
    match check_authentication(&session) {
        Ok(_) => {
            let result: Result<HttpResponse, SubmissionError> = async move {
                let journal_id = id.into_inner();
                debug!("Attempting to delete journal with ID: {}", journal_id);

                let conn = init_db().map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;
                let repository = JournalRepository::new(conn);
                repository.delete_journal_by_id(journal_id)?; // Returns Result<(), SubmissionError>

                Ok(HttpResponse::Ok().json(json!({
                    "success": true,
                    "message": format!("Journal with ID {} deleted successfully", journal_id)
                })))
            }
            .await; // Await the inner async block

            result.map_err(ActixError::from) // Map SubmissionError -> ActixError
        }
        Err(redirect) => Ok(redirect), // Return the redirect HttpResponse
    }
}

#[get("/submissions")]
pub async fn admin_submissions_handler(session: Session) -> Result<HttpResponse, ActixError> {
    match check_authentication(&session) {
        Ok(_) => {
            let result: Result<HttpResponse, SubmissionError> = async move {
                let conn = init_db().map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;
                let sub_repo = SubmissionRepository::new(conn);
                let submissions = sub_repo.get_all_submissions()?;

                // Pass the current page identifier
                let template = AdminSubmissionsTemplate {
                    submissions,
                    current_page: "submissions",
                    title: "Admin Submissions",
                };
                Ok(HttpResponse::Ok()
                    .content_type("text/html; charset=utf-8")
                    .body(template.render().map_err(|e| {
                        error!("Submissions template render error: {:?}", e); // Changed {} to {:?}
                        SubmissionError::FileProcessingError(format!("Template error: {:?}", e))
                        // Changed {} to {:?}
                    })?))
            }
            .await;
            result.map_err(ActixError::from)
        }
        Err(redirect) => Ok(redirect),
    }
}

#[get("/submissions/{id}/download")]
pub async fn download_submission_handler(
    session: Session,
    req: HttpRequest, // Need request to build absolute paths if needed, but NamedFile handles relative
    id: web::Path<i32>,
) -> Result<HttpResponse, ActixError> {
    match check_authentication(&session) {
        Ok(_) => {
            let submission_id = id.into_inner();
            debug!(
                "Attempting to download submission PDF for ID: {}",
                submission_id
            );

            let result: Result<NamedFile, SubmissionError> = async move {
                let conn = init_db().map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;
                let sub_repo = SubmissionRepository::new(conn);
                let submission = sub_repo.get_submission_by_id(submission_id)?;

                // pdf_url in submission should be like "./data/uploads/uuid.pdf"
                let file_path = PathBuf::from(&submission.pdf_url);

                // Extract filename for content disposition
                let filename = file_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("submission.pdf") // Fallback filename
                    .to_string();

                // Attempt to open the file
                let named_file = NamedFile::open_async(&file_path).await.map_err(|io_err| {
                    error!(
                        "Failed to open submission file {:?} for ID {}: {:?}", // Changed {} to {:?}
                        file_path, submission_id, io_err
                    );
                    // Map IO error to NotFound or InternalError appropriately
                    if io_err.kind() == std::io::ErrorKind::NotFound {
                        SubmissionError::NotFound(format!(
                            "Submission file not found for ID {}",
                            submission_id
                        ))
                    } else {
                        SubmissionError::StorageError(format!(
                            "Error opening submission file: {:?}", // Changed {} to {:?}
                            io_err
                        ))
                    }
                })?;

                // Set headers for download
                Ok(named_file.set_content_disposition(ContentDisposition {
                    disposition: DispositionType::Attachment, // Force download
                    parameters: vec![DispositionParam::Filename(filename)],
                }))
            }
            .await; // await the inner block

            // Map SubmissionError to ActixError OR directly return NamedFile response
            match result {
                Ok(named_file) => Ok(named_file.into_response(&req)), // Convert NamedFile to HttpResponse
                Err(e) => Err(ActixError::from(e)), // Convert SubmissionError to ActixError
            }
        }
        Err(redirect) => Ok(redirect),
    }
}

#[post("/{id}/edit")]
pub async fn update_journal_handler(
    session: Session,
    id: web::Path<i32>,
    form: web::Form<EditJournalForm>,
) -> Result<HttpResponse, ActixError> {
    match check_authentication(&session) {
        Ok(_) => {
            let journal_id = id.into_inner();

            // Parse the publication date
            let naive_date = NaiveDate::parse_from_str(&form.publication_date, "%Y-%m-%d")
                .map_err(|_| {
                    SubmissionError::ValidationError("Invalid publication date format".to_string())
                })?;

            let publication_datetime = DateTime::<Utc>::from_naive_utc_and_offset(
                naive_date.and_hms_opt(0, 0, 0).unwrap(),
                Utc,
            );

            // Validate form data
            if form.title.is_empty()
                || form.authors.is_empty()
                || form.abstract_text.is_empty()
                || form.keywords.is_empty()
                || form.pages.is_empty()
                || form.pdf_url.is_empty()
            {
                return Err(SubmissionError::ValidationError(
                    "All fields are required".to_string(),
                )
                .into());
            }

            // Create updated journal
            let updated_journal = Journal {
                id: Some(journal_id),
                title: form.title.clone(),
                authors: form.authors.clone(),
                abstract_text: form.abstract_text.clone(),
                keywords: form.keywords.clone(),
                volume_number: form.volume_number,
                issue_number: form.issue_number,
                pages: form.pages.clone(),
                publication_date: publication_datetime,
                pdf_url: form.pdf_url.clone(),
                created_at: None, // We don't update created_at
            };

            let conn = init_db().map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;
            let repository = JournalRepository::new(conn);

            // Update the journal
            repository.update_journal(&updated_journal)?;

            // Redirect to the journal detail page
            Ok(HttpResponse::Found()
                .append_header(("Location", format!("/journals/{}", journal_id)))
                .finish())
        }
        Err(redirect) => Ok(redirect),
    }
}

#[get("/{id}/edit")]
pub async fn edit_journal_form_handler(
    session: Session,
    id: web::Path<i32>,
) -> Result<HttpResponse, ActixError> {
    match check_authentication(&session) {
        Ok(_) => {
            let journal_id = id.into_inner();

            let conn = init_db().map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;
            let repository = JournalRepository::new(conn);

            match repository.get_journal_by_id(journal_id) {
                Ok(journal) => {
                    let template = EditJournalTemplate::new(journal);

                    Ok(HttpResponse::Ok()
                        .content_type("text/html; charset=utf-8")
                        .body(template.render().map_err(|e| {
                            error!("Edit journal template render error: {:?}", e); // Changed {} to {:?}
                            actix_web::error::ErrorInternalServerError("Template error")
                        })?))
                }
                Err(e) => {
                    error!("Failed to fetch journal: {:?}", e); // Changed {} to {:?}
                    Ok(HttpResponse::Found()
                        .append_header(("Location", "/admin/dashboard"))
                        .finish())
                }
            }
        }
        Err(redirect) => Ok(redirect),
    }
}
