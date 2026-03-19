//! Email service for sending 2FA codes, notifications, and overdue reminders.

use std::sync::Arc;

use lettre::{
    message::{header::ContentType, Mailbox, Message, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    SmtpTransport, Transport,
};
use std::path::Path;
use std::str::FromStr;

use crate::{
    dynamic_config::DynamicConfig,
    error::{AppError, AppResult},
    models::Language,
    services::email_templates,
};

#[derive(Clone)]
pub struct EmailService {
    dynamic_config: Arc<DynamicConfig>,
}

impl EmailService {
    pub fn new(dynamic_config: Arc<DynamicConfig>) -> Self {
        Self { dynamic_config }
    }

    fn templates_dir_str(&self) -> String {
        self.dynamic_config.read_email().templates_dir.clone()
    }

    /// Send a 2FA code via email
    pub async fn send_2fa_code(
        &self,
        to: &str,
        code: &str,
        lang: Option<Language>,
    ) -> AppResult<()> {
        let dir = self.templates_dir_str();
        let template = email_templates::load_template(Path::new(&dir), "2fa_code", lang)?;
        let (subject, body_plain, body_html) =
            email_templates::substitute(&template, &[("code", code)]);
        self.send_email_with_html(to, &subject, &body_plain, &body_html).await
    }

    /// Send a recovery code via email
    pub async fn send_recovery_code(
        &self,
        to: &str,
        code: &str,
        lang: Option<Language>,
    ) -> AppResult<()> {
        let dir = self.templates_dir_str();
        let template = email_templates::load_template(Path::new(&dir), "recovery_code", lang)?;
        let (subject, body_plain, body_html) =
            email_templates::substitute(&template, &[("code", code)]);
        self.send_email_with_html(to, &subject, &body_plain, &body_html).await
    }

    /// Send password reset email
    pub async fn send_password_reset(
        &self,
        to: &str,
        token: &str,
        lang: Option<Language>,
        reset_url: Option<&str>,
    ) -> AppResult<()> {
        let dir = self.templates_dir_str();
        let template = email_templates::load_template(Path::new(&dir), "password_reset", lang)?;
        let vars: Vec<(&str, &str)> = match reset_url {
            Some(url) => vec![("token", token), ("reset_url", url)],
            None => vec![("token", token), ("reset_url", "")],
        };
        let (subject, body_plain, body_html) = email_templates::substitute(&template, &vars);
        self.send_email_with_html(to, &subject, &body_plain, &body_html).await
    }

    /// Send a test email using the current live SMTP configuration
    pub async fn send_test_email(&self, to: &str) -> AppResult<()> {
        let subject = "Elidune - Test email / Email de test";
        let body_plain = "This is a test email from Elidune to verify your SMTP configuration.\n\
                          Ceci est un email de test envoyé par Elidune pour vérifier votre configuration SMTP.";
        let body_html = "<html><body>\
            <h2>Elidune - Test email</h2>\
            <p>This is a test email from Elidune to verify your SMTP configuration.</p>\
            <p>Ceci est un email de test envoyé par Elidune pour vérifier votre configuration SMTP.</p>\
            </body></html>";
        self.send_email_with_html(to, subject, body_plain, body_html).await
    }

    /// Low-level send: builds the SMTP transport from the current live config on each call.
    pub async fn send_email_with_html(
        &self,
        to: &str,
        subject: &str,
        body_plain: &str,
        body_html: &str,
    ) -> AppResult<()> {
        let config = self.dynamic_config.read_email();

        let from_name = config.smtp_from_name.as_deref().unwrap_or("Elidune");
        let from_mailbox =
            Mailbox::from_str(&format!("{} <{}>", from_name, config.smtp_from))
                .map_err(|e| AppError::Internal(format!("Invalid from address: {}", e)))?;

        let to_mailbox = Mailbox::from_str(to)
            .map_err(|e| AppError::Internal(format!("Invalid to address: {}", e)))?;

        let email = Message::builder()
            .from(from_mailbox)
            .to(to_mailbox)
            .subject(subject)
            .multipart(
                MultiPart::alternative()
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_PLAIN)
                            .body(body_plain.to_string()),
                    )
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_HTML)
                            .body(body_html.to_string()),
                    ),
            )
            .map_err(|e| AppError::Internal(format!("Failed to build email: {}", e)))?;

        let mailer_builder = if config.smtp_use_tls {
            SmtpTransport::starttls_relay(&config.smtp_host)
                .map_err(|e| AppError::Internal(format!("Failed to create SMTP transport: {}", e)))?
        } else {
            SmtpTransport::builder_dangerous(&config.smtp_host)
        }
        .port(config.smtp_port);

        let mailer_builder = if let (Some(username), Some(password)) =
            (&config.smtp_username, &config.smtp_password)
        {
            mailer_builder.credentials(Credentials::new(username.clone(), password.clone()))
        } else {
            mailer_builder
        };

        let mailer = mailer_builder.build();

        mailer
            .send(&email)
            .map_err(|e| AppError::Internal(format!("Failed to send email: {}", e)))?;

        Ok(())
    }
}
