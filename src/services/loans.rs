//! Loan management service

use chrono::{DateTime, Utc};

use crate::{
    error::AppResult,
    models::loan::{CreateLoan, LoanDetails},
    repository::{loans, users, Repository},
};

#[derive(Clone)]
pub struct LoansService {
    repository: Repository,
}

impl LoansService {
    pub fn new(repository: Repository) -> Self {
        Self { repository }
    }

    /// Get loans for a user
    pub async fn get_user_loans(&self, user_id: i64) -> AppResult<Vec<LoanDetails>> {
        // Verify user exists
        self.repository.users_get_by_id(user_id).await?;
        self.repository.loans_get_for_user(user_id).await
    }

    /// Create a new loan (borrow an item)
    pub async fn create_loan(&self, loan: CreateLoan) -> AppResult<(i64, DateTime<Utc>)> {
        // Verify user exists
        self.repository.users_get_by_id(loan.user_id).await?;
        self.repository.loans_create(&loan).await
    }

    /// Return a borrowed item
    pub async fn return_loan(&self, loan_id: i64) -> AppResult<LoanDetails> {
        self.repository.loans_return(loan_id).await
    }

    /// Return a borrowed item by specimen ID
    pub async fn return_loan_by_specimen(&self, specimen_id: &str) -> AppResult<LoanDetails> {
        let loan = self.repository.loans_get_by_specimen_identification(specimen_id).await?;
        self.repository.loans_return(loan.id).await
    }

    /// Renew a loan
    pub async fn renew_loan(&self, loan_id: i64) -> AppResult<(DateTime<Utc>, i16)> {
        self.repository.loans_renew(loan_id).await
    }

    /// Renew a loan by specimen ID
    pub async fn renew_loan_by_specimen(&self, specimen_id: &str) -> AppResult<(i64, DateTime<Utc>, i16)> {
        let loan = self.repository.loans_get_by_specimen_identification(specimen_id).await?;
        let loan_id = loan.id;
        let (new_issue_date, renew_count) = self.repository.loans_renew(loan_id).await?;
        Ok((loan_id, new_issue_date, renew_count))
    }

    /// Count active loans
    pub async fn count_active(&self) -> AppResult<i64> {
        self.repository.loans_count_active().await
    }

    /// Count overdue loans
    pub async fn count_overdue(&self) -> AppResult<i64> {
        self.repository.loans_count_overdue().await
    }
}


