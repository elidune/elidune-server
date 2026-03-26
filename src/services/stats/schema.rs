//! Whitelist registry for flexible stats queries — only listed tables/columns are reachable.

use std::collections::HashMap;

use once_cell::sync::Lazy;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct EntityDef {
    pub table: &'static str,
    pub label: &'static str,
    pub fields: HashMap<&'static str, FieldDef>,
    pub relations: HashMap<&'static str, RelationDef>,
}

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub column: &'static str,
    pub data_type: &'static str,
    pub label: &'static str,
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
        column,
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
                ("sex", f("sex", "integer", "Sex")),
                ("birthdate", f("birthdate", "text", "Birth date")),
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
                ("barcode", f("barcode", "text", "Barcode")),
                ("call_number", f("call_number", "text", "Call number")),
                ("created_at", f("created_at", "timestamptz", "Created at")),
            ]),
            relations: HashMap::from([("biblios", r("biblios", "biblio_id", "id", "Biblio"))]),
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
                ("date", f("date", "timestamptz", "Date")),
                ("expiry_at", f("expiry_at", "timestamptz", "Due date")),
                ("returned_at", f("returned_at", "timestamptz", "Return")),
                ("addr_city", f("addr_city", "text", "Borrower city")),
            ]),
            relations: HashMap::from([
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
            fields.insert(
                (*fname).to_string(),
                json!({
                    "type": fd.data_type,
                    "label": fd.label,
                }),
            );
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
        entities.insert(
            (*key).to_string(),
            json!({
                "label": def.label,
                "fields": Value::Object(fields),
                "relations": Value::Object(relations),
            }),
        );
    }

    json!({
        "entities": Value::Object(entities),
        "aggregationFunctions": ["count", "countDistinct", "sum", "avg", "min", "max"],
        "operators": ["eq", "neq", "gt", "gte", "lt", "lte", "in", "notIn", "isNull", "isNotNull"],
        "timeGranularities": ["day", "week", "month", "quarter", "year"],
    })
}
