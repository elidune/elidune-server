//! Email notifications when inventory consolidation closes active loans (`force: true`).

use std::collections::HashMap;

use crate::{
    email::EmailService,
    email_templates,
    models::Language,
    repository::inventory::InventoryLoanClosureRow,
};

/// Send one email per reader listing all loans closed during consolidation.
pub async fn send_loan_closure_notifications(
    email_svc: &EmailService,
    session_name: &str,
    rows: &[InventoryLoanClosureRow],
) -> (u64, Vec<(i64, String, String)>) {
    let mut by_user: HashMap<i64, Vec<&InventoryLoanClosureRow>> = HashMap::new();
    for row in rows {
        by_user.entry(row.user_id).or_default().push(row);
    }

    let mut sent = 0u64;
    let mut errors = Vec::new();

    for (user_id, user_rows) in by_user {
        let first = user_rows[0];
        let to = match first.user_email.as_deref().map(str::trim) {
            Some(e) if !e.is_empty() => e,
            _ => {
                tracing::debug!(user_id, "No email — skipping inventory loan closure notification");
                continue;
            }
        };

        let firstname = first.user_firstname.clone().unwrap_or_default();
        let lastname = first.user_lastname.clone().unwrap_or_default();
        let lang = first.user_language.as_deref().map(Language::from);

        let items_list = user_rows
            .iter()
            .map(|r| format_item_line(r))
            .collect::<Vec<_>>()
            .join("\n");

        let items_list_html = format!(
            "<ul>{}</ul>",
            user_rows
                .iter()
                .map(|r| format!("<li>{}</li>", html_escape(&format_item_line(r))))
                .collect::<String>()
        );

        match email_svc
            .load_template("inventory_loan_closed", lang)
            .await
        {
            Ok(template) => {
                let vars: Vec<(&str, &str)> = vec![
                    ("firstname", firstname.as_str()),
                    ("lastname", lastname.as_str()),
                    ("session_name", session_name),
                    ("items_list", items_list.as_str()),
                    ("items_list_html", items_list_html.as_str()),
                ];
                let (subject, body_plain, body_html) = email_templates::substitute(&template, &vars);
                match email_svc
                    .send_email_with_html(to, &subject, &body_plain, &body_html)
                    .await
                {
                    Ok(()) => sent += 1,
                    Err(e) => errors.push((user_id, to.to_string(), e.to_string())),
                }
            }
            Err(e) => errors.push((user_id, to.to_string(), e.to_string())),
        }
    }

    (sent, errors)
}

fn format_item_line(row: &InventoryLoanClosureRow) -> String {
    let title = row.biblio_title.as_deref().unwrap_or("(unknown title)");
    match row.barcode.as_deref() {
        Some(b) if !b.is_empty() => format!("- {title} (barcode: {b})"),
        _ => format!("- {title}"),
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
