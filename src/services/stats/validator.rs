//! Size and shape validation before building SQL.

use crate::error::AppError;
use crate::models::stats_builder::StatsBuilderBody;

const MAX_JOIN_PATHS: usize = 32;
const MAX_FILTERS: usize = 64;
const MAX_FILTER_GROUPS: usize = 16;
const MAX_FILTERS_PER_OR_GROUP: usize = 32;
const MAX_TOTAL_FILTER_CLAUSES: usize = 128;
const MAX_SELECT: usize = 64;
const MAX_AGGREGATIONS: usize = 32;
const MAX_GROUP_BY: usize = 32;
const MAX_ORDER_BY: usize = 16;

pub fn validate(query: &StatsBuilderBody) -> Result<(), AppError> {
    if query.entity.is_empty() {
        return Err(AppError::Validation("entity is required".into()));
    }
    if query.joins.len() > MAX_JOIN_PATHS {
        return Err(AppError::Validation(format!(
            "Too many join paths (max {})",
            MAX_JOIN_PATHS
        )));
    }
    if query.filters.len() > MAX_FILTERS {
        return Err(AppError::Validation(format!(
            "Too many filters (max {})",
            MAX_FILTERS
        )));
    }
    if query.filter_groups.len() > MAX_FILTER_GROUPS {
        return Err(AppError::Validation(format!(
            "Too many filterGroups (max {})",
            MAX_FILTER_GROUPS
        )));
    }
    let mut total_clauses = query.filters.len();
    for group in &query.filter_groups {
        if group.len() > MAX_FILTERS_PER_OR_GROUP {
            return Err(AppError::Validation(format!(
                "Too many filters in a filter group (max {})",
                MAX_FILTERS_PER_OR_GROUP
            )));
        }
        total_clauses = total_clauses.saturating_add(group.len());
    }
    if total_clauses > MAX_TOTAL_FILTER_CLAUSES {
        return Err(AppError::Validation(format!(
            "Too many filter clauses (filters + filterGroups total max {})",
            MAX_TOTAL_FILTER_CLAUSES
        )));
    }
    if query.select.len() > MAX_SELECT {
        return Err(AppError::Validation(format!(
            "Too many select fields (max {})",
            MAX_SELECT
        )));
    }
    if query.aggregations.len() > MAX_AGGREGATIONS {
        return Err(AppError::Validation(format!(
            "Too many aggregations (max {})",
            MAX_AGGREGATIONS
        )));
    }
    if query.group_by.len() > MAX_GROUP_BY {
        return Err(AppError::Validation(format!(
            "Too many groupBy fields (max {})",
            MAX_GROUP_BY
        )));
    }
    if query.order_by.len() > MAX_ORDER_BY {
        return Err(AppError::Validation(format!(
            "Too many orderBy fields (max {})",
            MAX_ORDER_BY
        )));
    }

    for p in &query.joins {
        if p.len() > 256 {
            return Err(AppError::Validation("Join path too long".into()));
        }
    }

    Ok(())
}
