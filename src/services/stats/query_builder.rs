//! Build parameterized SQL from a validated [`StatsBuilderBody`].

use crate::error::AppError;
use crate::models::stats_builder::{
    AggregateFunction, FilterOperator, SortDirection, StatsBuilderBody, StatsFilter, TimeGranularity,
};

use super::join_graph::{emit_join_sql, resolve_field, resolve_joins, AliasMap};

/// Built SQL with bind values in order ($1, $2, …).
pub struct BuiltQuery {
    pub data_sql: String,
    pub count_sql: String,
    pub binds: Vec<serde_json::Value>,
}

pub fn build_sql(query: &StatsBuilderBody) -> Result<BuiltQuery, AppError> {
    let entity = root_entity_def(query)?;

    let alias_map = resolve_joins(&query.entity, &query.joins)?;

    let mut binds: Vec<serde_json::Value> = Vec::new();
    let mut bind_idx = 1usize;

    let mut select_parts: Vec<String> = Vec::new();

    for sf in &query.select {
        let (tbl_alias, col) = resolve_field(&sf.field, &query.entity, &alias_map)?;
        let alias = sf.alias.as_deref().unwrap_or(&sf.field);
        select_parts.push(format!(r#""{}"."{}" AS "{}""#, tbl_alias, col, alias));
    }

    if let Some(ref tb) = query.time_bucket {
        let (tbl_alias, col) = resolve_field(&tb.field, &query.entity, &alias_map)?;
        let trunc = granularity_to_pg(&tb.granularity);
        let default_alias = format!("{}_{}", tb.field.replace('.', "_"), trunc);
        let alias = tb.alias.as_deref().unwrap_or(&default_alias);
        select_parts.push(format!(
            r#"DATE_TRUNC('{}', "{}"."{}") AS "{}""#,
            trunc, tbl_alias, col, alias
        ));
    }

    for agg in &query.aggregations {
        let (tbl_alias, col) = resolve_field(&agg.field, &query.entity, &alias_map)?;
        let expr = build_agg_expr(&agg.function, &tbl_alias, &col);
        select_parts.push(format!(r#"{} AS "{}""#, expr, agg.alias));
    }

    if select_parts.is_empty() {
        return Err(AppError::Validation(
            "Query must include at least one of: select, timeBucket, aggregations".into(),
        ));
    }

    let select_clause = select_parts.join(", ");
    let from_clause = format!(r#"{} AS "{}""#, entity.table, query.entity);
    let join_clause = emit_join_sql(&alias_map);

    let where_clause = if !query.filters.is_empty() {
        let conditions = build_filter_conditions(
            &query.filters,
            &query.entity,
            &alias_map,
            &mut binds,
            &mut bind_idx,
        )?;
        format!(" WHERE {}", conditions.join(" AND "))
    } else {
        String::new()
    };

    let group_by_clause = build_group_by_clause(query, &alias_map)?;

    let having_clause = if !query.having.is_empty() {
        let conditions = build_having_conditions(
            &query.having,
            &query.aggregations,
            &query.entity,
            &alias_map,
            &mut binds,
            &mut bind_idx,
        )?;
        format!(" HAVING {}", conditions.join(" AND "))
    } else {
        String::new()
    };

    let order_by_clause = if !query.order_by.is_empty() {
        let ob_parts: Vec<String> = query
            .order_by
            .iter()
            .map(|ob| {
                let dir = match ob.dir.unwrap_or(SortDirection::Asc) {
                    SortDirection::Asc => "ASC",
                    SortDirection::Desc => "DESC",
                };
                format!(r#""{}" {}"#, ob.field, dir)
            })
            .collect();
        format!(" ORDER BY {}", ob_parts.join(", "))
    } else {
        String::new()
    };

    let limit = query.limit.unwrap_or(1000).min(10_000);
    let offset = query.offset.unwrap_or(0);
    let pagination = format!(" LIMIT {} OFFSET {}", limit, offset);

    let core_sql = format!(
        "SELECT {} FROM {}{}{}{}{}",
        select_clause,
        from_clause,
        join_clause,
        where_clause,
        group_by_clause,
        having_clause
    );

    let data_sql = format!("{}{}{}", core_sql, order_by_clause, pagination);

    let count_sql = format!(
        r#"SELECT COUNT(*) AS "__total" FROM ({}) AS __sub"#,
        core_sql
    );

    Ok(BuiltQuery {
        data_sql,
        count_sql,
        binds,
    })
}

fn root_entity_def(query: &StatsBuilderBody) -> Result<&'static super::schema::EntityDef, AppError> {
    super::schema::SCHEMA
        .get(query.entity.as_str())
        .ok_or_else(|| AppError::BadRequest(format!("Unknown entity: {}", query.entity)))
}

fn granularity_to_pg(g: &TimeGranularity) -> &'static str {
    match g {
        TimeGranularity::Day => "day",
        TimeGranularity::Week => "week",
        TimeGranularity::Month => "month",
        TimeGranularity::Quarter => "quarter",
        TimeGranularity::Year => "year",
    }
}

fn build_agg_expr(func: &AggregateFunction, alias: &str, col: &str) -> String {
    match func {
        AggregateFunction::Count => format!(r#"COUNT("{}"."{}")"#, alias, col),
        AggregateFunction::CountDistinct => format!(r#"COUNT(DISTINCT "{}"."{}")"#, alias, col),
        AggregateFunction::Sum => format!(r#"SUM("{}"."{}")"#, alias, col),
        AggregateFunction::Avg => format!(r#"AVG("{}"."{}")"#, alias, col),
        AggregateFunction::Min => format!(r#"MIN("{}"."{}")"#, alias, col),
        AggregateFunction::Max => format!(r#"MAX("{}"."{}")"#, alias, col),
    }
}

fn build_group_by_clause(query: &StatsBuilderBody, alias_map: &AliasMap) -> Result<String, AppError> {
    let mut gb_parts: Vec<String> = Vec::new();

    for gbf in &query.group_by {
        let (tbl_alias, col) = resolve_field(&gbf.field, &query.entity, alias_map)?;
        gb_parts.push(format!(r#""{}"."{}""#, tbl_alias, col));
    }

    if let Some(ref tb) = query.time_bucket {
        let (tbl_alias, col) = resolve_field(&tb.field, &query.entity, alias_map)?;
        let trunc = granularity_to_pg(&tb.granularity);
        let expr = format!(r#"DATE_TRUNC('{}', "{}"."{}")"#, trunc, tbl_alias, col);
        if !gb_parts.contains(&expr) {
            gb_parts.push(expr);
        }
    }

    if gb_parts.is_empty() {
        Ok(String::new())
    } else {
        Ok(format!(" GROUP BY {}", gb_parts.join(", ")))
    }
}

fn build_filter_conditions(
    filters: &[StatsFilter],
    root_entity: &str,
    alias_map: &AliasMap,
    binds: &mut Vec<serde_json::Value>,
    bind_idx: &mut usize,
) -> Result<Vec<String>, AppError> {
    filters
        .iter()
        .map(|f| {
            let (tbl_alias, col) = resolve_field(&f.field, root_entity, alias_map)?;
            let qualified = format!(r#""{}"."{}""#, tbl_alias, col);
            build_condition_sql(&qualified, &f.op, &f.value, binds, bind_idx)
        })
        .collect()
}

fn build_having_conditions(
    having: &[crate::models::stats_builder::HavingFilter],
    aggregations: &[crate::models::stats_builder::StatsAggregation],
    root_entity: &str,
    alias_map: &AliasMap,
    binds: &mut Vec<serde_json::Value>,
    bind_idx: &mut usize,
) -> Result<Vec<String>, AppError> {
    having
        .iter()
        .map(|h| {
            let agg = aggregations
                .iter()
                .find(|a| a.alias == h.field)
                .ok_or_else(|| {
                    AppError::BadRequest(format!(
                        "HAVING references unknown aggregation alias '{}'",
                        h.field
                    ))
                })?;
            let (tbl_alias, col) = resolve_field(&agg.field, root_entity, alias_map)?;
            let agg_expr = build_agg_expr(&agg.function, &tbl_alias, &col);
            build_condition_sql(&agg_expr, &h.op, &h.value, binds, bind_idx)
        })
        .collect()
}

fn build_condition_sql(
    expr: &str,
    op: &FilterOperator,
    value: &serde_json::Value,
    binds: &mut Vec<serde_json::Value>,
    bind_idx: &mut usize,
) -> Result<String, AppError> {
    match op {
        FilterOperator::IsNull => Ok(format!("{} IS NULL", expr)),
        FilterOperator::IsNotNull => Ok(format!("{} IS NOT NULL", expr)),
        FilterOperator::In | FilterOperator::NotIn => {
            let arr = value.as_array().ok_or_else(|| {
                AppError::Validation("Operator 'in' / 'notIn' expects a JSON array value".into())
            })?;
            let placeholders: Vec<String> = arr
                .iter()
                .map(|v| {
                    binds.push(v.clone());
                    let p = format!("${}", *bind_idx);
                    *bind_idx += 1;
                    p
                })
                .collect();
            let kw = if matches!(op, FilterOperator::In) {
                "IN"
            } else {
                "NOT IN"
            };
            Ok(format!("{} {} ({})", expr, kw, placeholders.join(", ")))
        }
        _ => {
            let op_str = match op {
                FilterOperator::Eq => "=",
                FilterOperator::Neq => "!=",
                FilterOperator::Gt => ">",
                FilterOperator::Gte => ">=",
                FilterOperator::Lt => "<",
                FilterOperator::Lte => "<=",
                _ => {
                    return Err(AppError::Internal(
                        "Unexpected filter operator in build_condition_sql".into(),
                    ))
                }
            };
            binds.push(value.clone());
            let c = format!("{} {} ${}", expr, op_str, *bind_idx);
            *bind_idx += 1;
            Ok(c)
        }
    }
}
