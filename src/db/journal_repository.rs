use crate::errors::SubmissionError;
use crate::models::journals::Journal;
use chrono::{DateTime, NaiveDateTime, Utc};
use log::{error, info};
use rusqlite::{params, Connection, OptionalExtension, Result as RusqliteResult};
use std::fs; // Import fs for file deletion
use std::path::Path; // Import Path

pub struct JournalRepository {
    conn: Connection,
}

impl JournalRepository {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    fn map_row_to_journal(row: &rusqlite::Row) -> RusqliteResult<Journal> {
        let publication_date_timestamp: i64 = row.get(8)?; // publication_date is at index 8
        let created_at_str: Option<String> = row.get(10)?; // created_at is at index 10
        let pdf_filename: String = row.get(9)?; // pdf_url is at index 9

        let naive_dt = NaiveDateTime::from_timestamp_opt(publication_date_timestamp, 0).unwrap();
        let publication_date = DateTime::<Utc>::from_utc(naive_dt, Utc);

        let created_at = match created_at_str {
            Some(s) => NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|dt| DateTime::<Utc>::from_utc(dt, Utc)),
            None => None,
        };

        Ok(Journal {
            id: Some(row.get(0)?),
            title: row.get(1)?,
            authors: row.get(2)?,
            abstract_text: row.get(3)?,
            keywords: row.get(4)?,
            volume_number: row.get(5)?,
            issue_number: row.get(6)?,
            pages: row.get(7)?, // pages is at index 7 and should be read as TEXT
            publication_date,
            pdf_url: pdf_filename,
            created_at,
        })
    }

    // Base SELECT statement for consistency
    const SELECT_FIELDS: &'static str =
           "id, title, authors, abstract_text, keywords, volume_number, issue_number, pages, publication_date, pdf_url, created_at";

    // Updated INSERT statement
    pub fn save_journal(&self, journal: &Journal) -> Result<i64, SubmissionError> {
        let result = self.conn.execute(
                      "INSERT INTO journals (title, authors, abstract_text, keywords, volume_number, issue_number, pages, publication_date, pdf_url)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                      params![
                          journal.title,
                          journal.authors,
                          journal.abstract_text,
                          journal.keywords,
                          journal.volume_number, // Use new field
                          journal.issue_number,  // Use new field
                          journal.pages,
                          journal.publication_date.timestamp(),
                          journal.pdf_url,
                      ],
                  );

        match result {
            Ok(_) => Ok(self.conn.last_insert_rowid()),
            Err(e) => {
                // Check for UNIQUE constraint violation on pdf_url
                if e.to_string()
                    .contains("UNIQUE constraint failed: journals.pdf_url")
                {
                    Err(SubmissionError::Conflict(
                        "A journal with the same PDF filename already exists.".to_string(),
                    ))
                } else {
                    Err(SubmissionError::DatabaseError(e.to_string()))
                }
            }
        }
    }

    // Updated UPDATE statement
    pub fn update_journal(&self, journal: &Journal) -> Result<(), SubmissionError> {
        let journal_id = journal.id.ok_or_else(|| {
            SubmissionError::ValidationError("Cannot update journal without ID".to_string())
        })?;

        let result = self.conn.execute(
            "UPDATE journals SET
                      title = ?1, authors = ?2, abstract_text = ?3, keywords = ?4,
                      volume_number = ?5, issue_number = ?6, pages = ?7,
                      publication_date = ?8, pdf_url = ?9
                  WHERE id = ?10",
            params![
                journal.title,
                journal.authors,
                journal.abstract_text,
                journal.keywords,
                journal.volume_number,
                journal.issue_number,
                journal.pages,
                journal.publication_date.timestamp(),
                journal.pdf_url,
                journal_id, // Use the extracted ID here
            ],
        );

        match result {
            Ok(0) => Err(SubmissionError::NotFound(format!(
                "Journal with ID {} not found for update",
                journal_id
            ))),
            Ok(_) => Ok(()),
            Err(e) => {
                // Check for UNIQUE constraint violation on pdf_url during update
                if e.to_string()
                    .contains("UNIQUE constraint failed: journals.pdf_url")
                {
                    Err(SubmissionError::Conflict(
                        "A journal with the same PDF filename already exists.".to_string(),
                    ))
                } else {
                    Err(SubmissionError::DatabaseError(e.to_string()))
                }
            }
        }
    }

    // Updated SELECT queries
    pub fn get_journal_by_id(&self, id: i32) -> Result<Journal, SubmissionError> {
        let query = format!("SELECT {} FROM journals WHERE id = ?1", Self::SELECT_FIELDS);
        self.conn
            .query_row(&query, params![id], Self::map_row_to_journal)
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    SubmissionError::NotFound(format!("Journal with ID {} not found", id))
                }
                _ => SubmissionError::DatabaseError(e.to_string()),
            })
    }

    // Updated ordering for pagination (volume/issue then date)
    pub fn get_all_journals(
        &self,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<Journal>, SubmissionError> {
        let query = format!(
               "SELECT {} FROM journals ORDER BY volume_number DESC, issue_number DESC, publication_date DESC LIMIT ?1 OFFSET ?2",
               Self::SELECT_FIELDS
           );
        let mut stmt = self
            .conn
            .prepare(&query)
            .map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;

        let journal_iter = stmt
            .query_map(params![limit, offset], Self::map_row_to_journal)
            .map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;

        journal_iter
            .collect::<Result<Vec<Journal>, _>>()
            .map_err(|e| SubmissionError::DatabaseError(e.to_string()))
    }

    // No longer needs pagination, gets all for grouping
    pub fn get_all_journals_for_archive(&self) -> Result<Vec<Journal>, SubmissionError> {
        let query = format!(
             "SELECT {} FROM journals ORDER BY volume_number DESC, issue_number DESC, publication_date DESC",
             Self::SELECT_FIELDS
         );
        let mut stmt = self
            .conn
            .prepare(&query)
            .map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;

        let journal_iter = stmt
            .query_map([], Self::map_row_to_journal)
            .map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;

        journal_iter
            .collect::<Result<Vec<Journal>, _>>()
            .map_err(|e| SubmissionError::DatabaseError(e.to_string()))
    }

    // Gets N most recent publications regardless of volume/issue
    pub fn get_latest_journals(&self, limit: i32) -> Result<Vec<Journal>, SubmissionError> {
        let query = format!(
            "SELECT {} FROM journals ORDER BY publication_date DESC LIMIT ?1",
            Self::SELECT_FIELDS
        );
        let mut stmt = self
            .conn
            .prepare(&query)
            .map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;

        let journal_iter = stmt
            .query_map(params![limit], Self::map_row_to_journal)
            .map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;

        journal_iter
            .collect::<Result<Vec<Journal>, _>>()
            .map_err(|e| SubmissionError::DatabaseError(e.to_string()))
    }

    // Helper to find the latest volume and issue number
    fn get_latest_volume_issue(&self) -> RusqliteResult<Option<(i32, i32)>> {
        self.conn
            .query_row(
                "SELECT volume_number, issue_number FROM journals
                    ORDER BY volume_number DESC, issue_number DESC, publication_date DESC
                    LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional() // Return Option<(i32, i32)>
    }

    // Gets journals matching the latest volume/issue
    pub fn get_current_edition(&self, limit: i32) -> Result<Vec<Journal>, SubmissionError> {
        let latest_vi = self.get_latest_volume_issue().map_err(|e| {
            SubmissionError::DatabaseError(format!("Failed to get latest volume/issue: {}", e))
        })?;

        match latest_vi {
            Some((latest_vol, latest_iss)) => {
                let query = format!(
                    "SELECT {} FROM journals
                            WHERE volume_number = ?1 AND issue_number = ?2
                            ORDER BY publication_date DESC LIMIT ?3",
                    Self::SELECT_FIELDS
                );
                let mut stmt = self
                    .conn
                    .prepare(&query)
                    .map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;

                let journal_iter = stmt
                    .query_map(
                        params![latest_vol, latest_iss, limit],
                        Self::map_row_to_journal,
                    )
                    .map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;

                journal_iter
                    .collect::<Result<Vec<Journal>, _>>()
                    .map_err(|e| SubmissionError::DatabaseError(e.to_string()))
            }
            None => Ok(Vec::new()), // No journals exist, so no current edition
        }
    }

    pub fn get_past_issues(
        &self,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<Journal>, SubmissionError> {
        let latest_vi = self.get_latest_volume_issue().map_err(|e| {
            SubmissionError::DatabaseError(format!("Failed to get latest volume/issue: {}", e))
        })?;

        match latest_vi {
            Some((latest_vol, latest_iss)) => {
                let query = format!(
                    "SELECT {} FROM journals
                     WHERE NOT (volume_number = ?1 AND issue_number = ?2)
                     ORDER BY volume_number DESC, issue_number DESC, publication_date DESC
                     LIMIT ?3 OFFSET ?4",
                    Self::SELECT_FIELDS
                );
                let mut stmt = self
                    .conn
                    .prepare(&query)
                    .map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;

                let journal_iter = stmt
                    .query_map(
                        params![latest_vol, latest_iss, limit, offset],
                        Self::map_row_to_journal,
                    )
                    .map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;

                journal_iter
                    .collect::<Result<Vec<Journal>, _>>()
                    .map_err(|e| SubmissionError::DatabaseError(e.to_string()))
            }
            None => {
                // No journals exist, so no past issues either
                Ok(Vec::new())
            }
        }
    }

    pub fn delete_journal_by_id(&self, id: i32) -> Result<(), SubmissionError> {
        let journal = self.get_journal_by_id(id)?; // Fetch details first (incl. filename)

        let rows_affected = self
            .conn
            .execute("DELETE FROM journals WHERE id = ?1", params![id])
            .map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;

        if rows_affected == 0 {
            return Err(SubmissionError::NotFound(format!(
                "Journal with ID {} could not be deleted (already removed?)",
                id
            )));
        }
        info!("Successfully deleted journal record with ID: {}", id);

        // Construct the full path relative to where the application runs
        let pdf_path = Path::new("./data/uploads").join(&journal.pdf_url);

        match fs::remove_file(&pdf_path) {
            Ok(_) => {
                info!("Successfully deleted PDF file: {:?}", pdf_path);
                Ok(())
            }
            Err(e) => {
                error!(
                    "Failed to delete PDF file {:?}: {}. DB record was deleted.",
                    pdf_path, e
                );
                // Consider this an error, as the file system is inconsistent
                Err(SubmissionError::StorageError(format!(
                    "Journal record deleted, but failed to remove file {:?}: {}",
                    pdf_path, e
                )))
            }
        }
    }

    pub fn get_journals_by_volume_issue(
        &self,
        volume: i32,
        issue: Option<i32>,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<Journal>, SubmissionError> {
        let (query, param_values) = if let Some(issue_num) = issue {
            (
                format!(
                    "SELECT {} FROM journals
                    WHERE volume_number = ?1 AND issue_number = ?2
                    ORDER BY publication_date DESC
                    LIMIT ?3 OFFSET ?4",
                    Self::SELECT_FIELDS
                ),
                vec![volume, issue_num, limit, offset],
            )
        } else {
            (
                format!(
                    "SELECT {} FROM journals
                    WHERE volume_number = ?1
                    ORDER BY issue_number DESC, publication_date DESC
                    LIMIT ?2 OFFSET ?3",
                    Self::SELECT_FIELDS
                ),
                vec![volume, limit, offset],
            )
        };

        let mut stmt = self
            .conn
            .prepare(&query)
            .map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;

        let params = rusqlite::params_from_iter(param_values);

        let journal_iter = stmt
            .query_map(params, Self::map_row_to_journal)
            .map_err(|e| SubmissionError::DatabaseError(e.to_string()))?;

        journal_iter
            .collect::<Result<Vec<Journal>, _>>()
            .map_err(|e| SubmissionError::DatabaseError(e.to_string()))
    }
}
