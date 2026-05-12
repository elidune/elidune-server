//! Overdue loan reminder service.
//!
//! Groups overdue loans by user, sends a single email per user listing all their overdue items,
//! updates reminder tracking columns, and records audit events.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    dynamic_config::DynamicConfig,
    error::AppResult,
    models::Language,
    repository::LoansRepository,
    services::{
        audit::{self, AuditService},
        email::EmailService,
        email_templates,
    },
};

/// Summary returned by a reminder run
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReminderReport {
    /// Whether this was a dry run (no emails actually sent)
    pub dry_run: bool,
    /// Number of emails successfully sent (or that would be sent in dry-run)
    pub emails_sent: u32,
    /// Total number of overdue loans covered
    pub loans_reminded: u32,
    /// Per-user details
    pub details: Vec<ReminderDetail>,
    /// Errors encountered (email not sent for these users)
    pub errors: Vec<ReminderError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReminderDetail {
    pub user_id: i64,
    pub email: String,
    pub firstname: Option<String>,
    pub lastname: Option<String>,
    pub loan_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReminderError {
    pub user_id: i64,
    pub email: String,
    pub error_message: String,
}

/// Overdue loan item for the admin dashboard
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OverdueLoanInfo {
    pub loan_id: i64,
    pub user_id: i64,
    pub firstname: Option<String>,
    pub lastname: Option<String>,
    pub user_email: Option<String>,
    pub biblio_id: i64,
    pub title: Option<String>,
    pub authors: Option<String>,
    pub item_barcode: Option<String>,
    pub loan_date: DateTime<Utc>,
    pub expiry_at: Option<DateTime<Utc>>,
    pub last_reminder_sent_at: Option<DateTime<Utc>>,
    pub reminder_count: i32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OverdueLoansPage {
    pub loans: Vec<OverdueLoanInfo>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

#[derive(Clone)]
pub struct RemindersService {
    repository: Arc<dyn LoansRepository>,
    email: EmailService,
    audit: AuditService,
    dynamic_config: Arc<DynamicConfig>,
}

impl RemindersService {
    pub fn new(
        repository: Arc<dyn LoansRepository>,
        email: EmailService,
        audit: AuditService,
        dynamic_config: Arc<DynamicConfig>,
    ) -> Self {
        Self { repository, email, audit, dynamic_config }
    }

    /// Get paginated overdue loans for the admin dashboard.
    #[tracing::instrument(skip(self), err)]
    pub async fn get_overdue_loans(&self, page: i64, per_page: i64) -> AppResult<OverdueLoansPage> {
        let page = page.max(1);
        let per_page = per_page.clamp(1, 200);
        let (rows, total) = self.repository.loans_get_overdue(page, per_page).await?;

        let loans = rows
            .into_iter()
            .map(|r| OverdueLoanInfo {
                loan_id: r.loan_id,
                user_id: r.user_id,
                firstname: r.firstname,
                lastname: r.lastname,
                user_email: r.user_email,
                biblio_id: r.biblio_id,
                title: r.title,
                authors: r.authors,
                item_barcode: r.item_barcode,
                loan_date: r.loan_date,
                expiry_at: r.expiry_at,
                last_reminder_sent_at: r.last_reminder_sent_at,
                reminder_count: r.reminder_count,
            })
            .collect();

        Ok(OverdueLoansPage { loans, total, page, per_page })
    }

    /// Send overdue reminder emails.
    /// If `dry_run` is true, builds the report but does NOT send emails or update the DB.
    #[tracing::instrument(skip(self), err)]
    pub async fn send_overdue_reminders(
        &self,
        dry_run: bool,
        triggered_by: Option<i64>,
        client_ip: Option<String>,
    ) -> AppResult<ReminderReport> {
        let reminders_cfg = self.dynamic_config.read_reminders();

        let overdue_rows = self
            .repository
            .loans_get_overdue_for_reminders(reminders_cfg.frequency_days)
            .await?;

        if overdue_rows.is_empty() {
            return Ok(ReminderReport {
                dry_run,
                emails_sent: 0,
                loans_reminded: 0,
                details: vec![],
                errors: vec![],
            });
        }

        // Group by user_id
        let mut by_user: HashMap<i64, Vec<_>> = HashMap::new();
        for row in &overdue_rows {
            by_user.entry(row.user_id).or_default().push(row);
        }

        let mut details = Vec::new();
        let mut errors = Vec::new();
        let mut all_reminded_ids: Vec<i64> = Vec::new();

        for (user_id, loans) in &by_user {
            let first = &loans[0];
            let email_addr = match &first.user_email {
                Some(e) if !e.is_empty() => e.clone(),
                _ => continue,
            };

            let firstname = first.firstname.as_deref().unwrap_or("");
            let lastname = first.lastname.as_deref().unwrap_or("");

            // Determine language
            let lang = first
                .user_language
                .as_deref()
                .map(Language::from);

            // Build loan list (plain text)
            let loans_list = loans
                .iter()
                .map(|l| {
                    let title = l.title.as_deref().unwrap_or("(unknown title)");
                    let authors = l.authors.as_deref().unwrap_or("");
                    let loan_date = l.loan_date.format("%d/%m/%Y").to_string();
                    let due_date = l
                        .expiry_at
                        .map(|d| d.format("%d/%m/%Y").to_string())
                        .unwrap_or_else(|| "N/A".to_string());
                    format!(
                        "- {} ({}) — borrowed: {}, due: {}",
                        title, authors, loan_date, due_date
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");

            // Build HTML table
            let table_rows = loans
                .iter()
                .map(|l| {
                    let title = l.title.as_deref().unwrap_or("(unknown title)");
                    let authors = l.authors.as_deref().unwrap_or("");
                    let loan_date = l.loan_date.format("%d/%m/%Y").to_string();
                    let due_date = l
                        .expiry_at
                        .map(|d| d.format("%d/%m/%Y").to_string())
                        .unwrap_or_else(|| "N/A".to_string());
                    format!(
                        "<tr><td style=\"padding:4px 8px;border:1px solid #ccc\">{}</td>\
                         <td style=\"padding:4px 8px;border:1px solid #ccc\">{}</td>\
                         <td style=\"padding:4px 8px;border:1px solid #ccc\">{}</td>\
                         <td style=\"padding:4px 8px;border:1px solid #ccc;color:#c00\"><strong>{}</strong></td></tr>",
                        title, authors, loan_date, due_date
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");

            let loans_table_html = format!(
                "<table style=\"border-collapse:collapse;width:100%\">\
                 <thead><tr>\
                 <th style=\"padding:4px 8px;border:1px solid #ccc;background:#f5f5f5\">Title</th>\
                 <th style=\"padding:4px 8px;border:1px solid #ccc;background:#f5f5f5\">Author(s)</th>\
                 <th style=\"padding:4px 8px;border:1px solid #ccc;background:#f5f5f5\">Borrowed</th>\
                 <th style=\"padding:4px 8px;border:1px solid #ccc;background:#f5f5f5\">Due date</th>\
                 </tr></thead><tbody>{}</tbody></table>",
                table_rows
            );

            if !dry_run {
                let template_result = self
                    .email
                    .load_template("overdue_reminder", lang)
                    .await;

                match template_result {
                    Err(e) => {
                        errors.push(ReminderError {
                            user_id: *user_id,
                            email: email_addr.clone(),
                            error_message: format!("Template load error: {}", e),
                        });
                        continue;
                    }
                    Ok(template) => {
                        let vars: Vec<(&str, &str)> = vec![
                            ("firstname", firstname),
                            ("lastname", lastname),
                            ("loans_list", &loans_list),
                            ("loans_table_html", &loans_table_html),
                        ];
                        let (subject, body_plain, body_html) =
                            email_templates::substitute(&template, &vars);

                        match self
                            .email
                            .send_email_with_html(&email_addr, &subject, &body_plain, &body_html)
                            .await
                        {
                            Ok(()) => {
                                let loan_ids: Vec<i64> =
                                    loans.iter().map(|l| l.loan_id).collect();
                                all_reminded_ids.extend(&loan_ids);

                                self.audit.log(
                                    audit::event::EMAIL_OVERDUE_REMINDER_SENT,
                                    triggered_by,
                                    Some("user"),
                                    Some(*user_id),
                                    client_ip.clone(),
                                    Some(serde_json::json!({
                                        "email": email_addr,
                                        "loan_ids": loan_ids,
                                        "loan_count": loans.len(),
                                    })),
                                    audit::AuditLogMeta::success(),
                                );

                                details.push(ReminderDetail {
                                    user_id: *user_id,
                                    email: email_addr.clone(),
                                    firstname: first.firstname.clone(),
                                    lastname: first.lastname.clone(),
                                    loan_count: loans.len(),
                                });
                            }
                            Err(e) => {
                                self.audit.log(
                                    audit::event::EMAIL_OVERDUE_REMINDER_SENT,
                                    triggered_by,
                                    Some("user"),
                                    Some(*user_id),
                                    client_ip.clone(),
                                    Some(serde_json::json!({
                                        "email": email_addr,
                                        "loan_count": loans.len(),
                                    })),
                                    audit::AuditLogMeta::from_app_error(&e),
                                );
                                errors.push(ReminderError {
                                    user_id: *user_id,
                                    email: email_addr.clone(),
                                    error_message: e.to_string(),
                                });
                            }
                        }

                        // SMTP throttle
                        if reminders_cfg.smtp_throttle_ms > 0 {
                            tokio::time::sleep(std::time::Duration::from_millis(
                                reminders_cfg.smtp_throttle_ms,
                            ))
                            .await;
                        }
                    }
                }
            } else {
                // Dry run: just collect details
                details.push(ReminderDetail {
                    user_id: *user_id,
                    email: email_addr.clone(),
                    firstname: first.firstname.clone(),
                    lastname: first.lastname.clone(),
                    loan_count: loans.len(),
                });
            }
        }

        // Update reminder tracking in DB (not in dry-run mode)
        if !dry_run && !all_reminded_ids.is_empty() {
            self.repository
                .loans_update_reminder_sent(&all_reminded_ids)
                .await?;
        }

        let emails_sent = details.len() as u32;
        let loans_reminded = all_reminded_ids.len() as u32;

        Ok(ReminderReport {
            dry_run,
            emails_sent,
            loans_reminded,
            details,
            errors,
        })
    }
}
