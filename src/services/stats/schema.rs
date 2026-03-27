//! Whitelist registry for flexible stats queries — only listed tables/columns are reachable.

use std::collections::{HashMap, HashSet};

use once_cell::sync::Lazy;
use serde_json::{json, Value};

use crate::error::AppError;
use crate::models::stats_builder::StatsBuilderBody;

/// Supported `UNION ALL` roots: active loans + archived loans (same projected columns).
pub static LOANS_UNION_GROUP: &[&str] = &["loans", "loans_archives"];

/// Validate `entity` + `unionWith` form an allowed multi-root query (currently only the loans group).
pub fn validate_union_branches(entity: &str, union_with: &[String]) -> Result<(), AppError> {
    if union_with.is_empty() {
        return Ok(());
    }
    let mut names: Vec<String> = Vec::with_capacity(1 + union_with.len());
    names.push(entity.to_string());
    names.extend(union_with.iter().cloned());
    let set: HashSet<&str> = names.iter().map(|s| s.as_str()).collect();
    if set.len() != names.len() {
        return Err(AppError::Validation(
            "unionWith must not duplicate entity names".into(),
        ));
    }
    let allowed: HashSet<&str> = LOANS_UNION_GROUP.iter().copied().collect();
    if set != allowed {
        return Err(AppError::Validation(
            "unionWith is only supported for combining loans and loans_archives (exactly both)"
                .into(),
        ));
    }
    Ok(())
}

/// True if `field` references `union_source` / `__union_source` (must be used with unionWith).
pub fn field_references_union_source(field: &str) -> bool {
    let lower = field.to_ascii_lowercase();
    lower.contains("union_source") || lower.contains("__union_source")
}

/// Validates union branch rules and that `union_source` is not used without `unionWith`.
pub fn validate_union_field_usage(query: &StatsBuilderBody) -> Result<(), AppError> {
    validate_union_branches(&query.entity, &query.union_with)?;
    if !query.union_with.is_empty() {
        return Ok(());
    }
    for path in iter_stats_field_paths(query) {
        if field_references_union_source(&path) {
            return Err(AppError::Validation(
                "union_source / __union_source fields require unionWith (e.g. [\"loans_archives\"])"
                    .into(),
            ));
        }
    }
    Ok(())
}

fn iter_stats_field_paths(query: &StatsBuilderBody) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in &query.select {
        out.push(s.field.clone());
    }
    for g in &query.group_by {
        out.push(g.field.clone());
    }
    for f in &query.filters {
        out.push(f.field.clone());
    }
    for g in &query.filter_groups {
        for f in g {
            out.push(f.field.clone());
        }
    }
    if let Some(ref tb) = query.time_bucket {
        out.push(tb.field.clone());
    }
    for a in &query.aggregations {
        out.push(a.field.clone());
    }
    out
}

#[derive(Debug, Clone)]
pub enum FieldKind {
    /// A real column on the entity table (`"{alias}"."column"` in SQL).
    Physical {
        column: &'static str,
    },
    /// Server-defined SQL; `{alias}` is replaced with the quoted table alias (e.g. `"users"`).
    Computed {
        sql_template: &'static str,
    },
}

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub kind: FieldKind,
    pub data_type: &'static str,
    pub label: &'static str,
}

#[derive(Debug, Clone)]
pub struct EntityDef {
    pub table: &'static str,
    pub label: &'static str,
    pub fields: HashMap<&'static str, FieldDef>,
    pub relations: HashMap<&'static str, RelationDef>,
}

#[derive(Debug, Clone)]
pub struct RelationDef {
    pub target_entity: &'static str,
    pub from_column: &'static str,
    pub to_column: &'static str,
    pub label: &'static str,
}

fn f(
    column: &'static str,
    data_type: &'static str,
    label: &'static str,
) -> FieldDef {
    FieldDef {
        kind: FieldKind::Physical { column },
        data_type,
        label,
    }
}

fn c(
    sql_template: &'static str,
    data_type: &'static str,
    label: &'static str,
) -> FieldDef {
    FieldDef {
        kind: FieldKind::Computed { sql_template },
        data_type,
        label,
    }
}

fn r(
    target_entity: &'static str,
    from_column: &'static str,
    to_column: &'static str,
    label: &'static str,
) -> RelationDef {
    RelationDef {
        target_entity,
        from_column,
        to_column,
        label,
    }
}

/// Single source of truth for allowed entities, fields, and joins.
pub static SCHEMA: Lazy<HashMap<&'static str, EntityDef>> = Lazy::new(|| {
    let mut m = HashMap::new();

    m.insert(
        "loans",
        EntityDef {
            table: "loans",
            label: "Loans",
            fields: HashMap::from([
                ("id", f("id", "bigint", "Loan id")),
                ("user_id", f("user_id", "bigint", "User id")),
                ("item_id", f("item_id", "bigint", "Item id")),
                ("date", f("date", "timestamptz", "Loan date")),
                ("expiry_at", f("expiry_at", "timestamptz", "Due date")),
                ("returned_at", f("returned_at", "timestamptz", "Return date")),
                ("nb_renews", f("nb_renews", "integer", "Renewals")),
                (
                    "union_source",
                    c(
                        "{alias}.__union_source",
                        "text",
                        "Union branch (loans | loans_archives); only when using unionWith",
                    ),
                ),
            ]),
            relations: HashMap::from([
                ("users", r("users", "user_id", "id", "Borrower")),
                ("items", r("items", "item_id", "id", "Item copy")),
            ]),
        },
    );

    m.insert(
        "users",
        EntityDef {
            table: "users",
            label: "Patrons",
            fields: HashMap::from([
                ("id", f("id", "bigint", "User id")),
                ("firstname", f("firstname", "text", "First name")),
                ("lastname", f("lastname", "text", "Last name")),
                ("addr_city", f("addr_city", "text", "City")),
                ("sex", f("sex", "text", "Sex (m/f)")),
                (
                    "age_band",
                    c(
                        "CASE WHEN {alias}.birthdate IS NULL THEN NULL WHEN EXTRACT(YEAR FROM AGE(CURRENT_DATE, {alias}.birthdate)) < 18 THEN '0-17' WHEN EXTRACT(YEAR FROM AGE(CURRENT_DATE, {alias}.birthdate)) < 30 THEN '18-29' WHEN EXTRACT(YEAR FROM AGE(CURRENT_DATE, {alias}.birthdate)) < 50 THEN '30-49' WHEN EXTRACT(YEAR FROM AGE(CURRENT_DATE, {alias}.birthdate)) < 65 THEN '50-64' ELSE '65+' END",
                        "text",
                        "Age band (from birthdate)",
                    ),
                ),
                (
                    "sex_label",
                    c(
                        "CASE {alias}.sex WHEN 'm' THEN 'male' WHEN 'f' THEN 'female' ELSE 'unknown' END",
                        "text",
                        "Sex label",
                    ),
                ),
                (
                    "age_band_3",
                    c(
                        "CASE WHEN {alias}.birthdate IS NULL THEN NULL WHEN EXTRACT(YEAR FROM AGE(CURRENT_DATE, {alias}.birthdate)) <= 14 THEN '0-14' WHEN EXTRACT(YEAR FROM AGE(CURRENT_DATE, {alias}.birthdate)) <= 64 THEN '15-64' ELSE '65+' END",
                        "text",
                        "Age band: 0–14, 15–64, 65+",
                    ),
                ),
                (
                    "active_membership_calendar_year",
                    c(
                        "CASE WHEN {alias}.expiry_at IS NULL OR {alias}.expiry_at >= date_trunc('year', CURRENT_TIMESTAMP) THEN 'yes' ELSE 'no' END",
                        "text",
                        "Membership active for current calendar year (no expiry or expiry on/after Jan 1)",
                    ),
                ),
                ("birthdate", f("birthdate", "date", "Birth date")),
                ("created_at", f("created_at", "timestamptz", "Registration")),
                ("expiry_at", f("expiry_at", "timestamptz", "Membership expiry")),
                ("status", f("status", "text", "Status")),
            ]),
            relations: HashMap::from([
                ("public_types", r("public_types", "public_type", "id", "Audience type")),
                ("account_types", r("account_types", "account_type", "code", "Account type")),
            ]),
        },
    );

    m.insert(
        "items",
        EntityDef {
            table: "items",
            label: "Item copies",
            fields: HashMap::from([
                ("id", f("id", "bigint", "Item id")),
                ("biblio_id", f("biblio_id", "bigint", "Biblio id")),
                ("source_id", f("source_id", "bigint", "Catalog source id")),
                ("barcode", f("barcode", "text", "Barcode")),
                ("call_number", f("call_number", "text", "Call number")),
                ("created_at", f("created_at", "timestamptz", "Created at")),
            ]),
            relations: HashMap::from([
                ("biblios", r("biblios", "biblio_id", "id", "Biblio")),
                ("sources", r("sources", "source_id", "id", "Catalog source")),
            ]),
        },
    );

    m.insert(
        "sources",
        EntityDef {
            table: "sources",
            label: "Catalog sources",
            fields: HashMap::from([
                ("id", f("id", "bigint", "Source id")),
                ("name", f("name", "text", "Source name")),
            ]),
            relations: HashMap::new(),
        },
    );

    m.insert(
        "biblios",
        EntityDef {
            table: "biblios",
            label: "Bibliographic records",
            fields: HashMap::from([
                ("id", f("id", "bigint", "Biblio id")),
                ("title", f("title", "text", "Title")),
                ("media_type", f("media_type", "text", "Media type")),
                ("audience_type", f("audience_type", "text", "Audience")),
                ("lang", f("lang", "text", "Language")),
                ("publication_date", f("publication_date", "text", "Publication date")),
            ]),
            relations: HashMap::new(),
        },
    );

    m.insert(
        "public_types",
        EntityDef {
            table: "public_types",
            label: "Audience types",
            fields: HashMap::from([
                ("id", f("id", "bigint", "Id")),
                ("name", f("name", "text", "Code")),
                ("label", f("label", "text", "Label")),
            ]),
            relations: HashMap::new(),
        },
    );

    m.insert(
        "account_types",
        EntityDef {
            table: "account_types",
            label: "Account types",
            fields: HashMap::from([
                ("code", f("code", "text", "Code")),
                ("name", f("name", "text", "Name")),
            ]),
            relations: HashMap::new(),
        },
    );

    m.insert(
        "visitor_counts",
        EntityDef {
            table: "visitor_counts",
            label: "Visitor counts",
            fields: HashMap::from([
                ("id", f("id", "bigint", "Id")),
                ("count_date", f("count_date", "date", "Date")),
                ("count", f("count", "integer", "Visitors")),
                ("source", f("source", "text", "Source")),
            ]),
            relations: HashMap::new(),
        },
    );

    m.insert(
        "events",
        EntityDef {
            table: "events",
            label: "Events",
            fields: HashMap::from([
                ("id", f("id", "bigint", "Id")),
                ("name", f("name", "text", "Name")),
                ("event_type", f("event_type", "integer", "Type")),
                ("event_date", f("event_date", "date", "Date")),
                ("attendees_count", f("attendees_count", "integer", "Attendees")),
                ("target_public", f("target_public", "integer", "Target audience")),
                ("school_name", f("school_name", "text", "School")),
                ("students_count", f("students_count", "integer", "Students")),
            ]),
            relations: HashMap::new(),
        },
    );

    m.insert(
        "loans_archives",
        EntityDef {
            table: "loans_archives",
            label: "Archived loans",
            fields: HashMap::from([
                ("id", f("id", "bigint", "Id")),
                ("user_id", f("user_id", "bigint", "User id")),
                ("item_id", f("item_id", "bigint", "Item id")),
                ("date", f("date", "timestamptz", "Date")),
                ("expiry_at", f("expiry_at", "timestamptz", "Due date")),
                ("returned_at", f("returned_at", "timestamptz", "Return")),
                ("nb_renews", f("nb_renews", "integer", "Renewals")),
                ("addr_city", f("addr_city", "text", "Borrower city")),
            ]),
            relations: HashMap::from([
                ("users", r("users", "user_id", "id", "Borrower")),
                ("public_types", r("public_types", "borrower_public_type", "id", "Audience type")),
                ("items", r("items", "item_id", "id", "Item copy")),
            ]),
        },
    );

    m
});

/// OpenAPI / frontend discovery payload (camelCase keys).
pub fn discovery_json() -> Value {
    let mut entities = serde_json::Map::new();
    for (key, def) in SCHEMA.iter() {
        let mut fields = serde_json::Map::new();
        for (fname, fd) in &def.fields {
            let mut obj = serde_json::Map::new();
            obj.insert("type".to_string(), json!(fd.data_type));
            obj.insert("label".to_string(), json!(fd.label));
            if matches!(fd.kind, FieldKind::Computed { .. }) {
                obj.insert("computed".to_string(), json!(true));
            }
            fields.insert((*fname).to_string(), Value::Object(obj));
        }
        let mut relations = serde_json::Map::new();
        for (rname, rd) in &def.relations {
            relations.insert(
                (*rname).to_string(),
                json!({
                    "join": [
                        format!("{}.{}", key, rd.from_column),
                        format!("{}.{}", rd.target_entity, rd.to_column),
                    ],
                    "label": rd.label,
                }),
            );
        }
        let mut entity_obj = serde_json::Map::new();
        entity_obj.insert("label".to_string(), json!(def.label));
        entity_obj.insert("fields".to_string(), Value::Object(fields));
        entity_obj.insert("relations".to_string(), Value::Object(relations));
        if *key == "loans" {
            entity_obj.insert(
                "unionWith".to_string(),
                json!(["loans_archives"]),
            );
        }
        entities.insert((*key).to_string(), Value::Object(entity_obj));
    }

    json!({
        "entities": Value::Object(entities),
        "aggregationFunctions": ["count", "countDistinct", "sum", "avg", "min", "max"],
        "operators": ["eq", "neq", "gt", "gte", "lt", "lte", "in", "notIn", "isNull", "isNotNull"],
        "timeGranularities": ["day", "week", "month", "quarter", "year"],
        "filterGroupsSemantics": "Top-level `filters` are AND'd together. If `filterGroups` is non-empty, the WHERE clause is: (AND of `filters`) AND (OR of each inner group, where each inner group is the AND of its filters). Computed fields cannot be used in `aggregations` or `timeBucket`.",
        "unionWithSemantics": "When `unionWith` is set on the request body, the root `FROM` is a UNION ALL of the canonical `entity` and listed peers (same columns). Joins are resolved from `entity` only; use paths valid for that entity (e.g. users, items). Field `loans.union_source` refers to the branch label.",
    })
}
