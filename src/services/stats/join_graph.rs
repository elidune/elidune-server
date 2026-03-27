//! Resolve join paths into unique SQL aliases (insertion-ordered).

use indexmap::IndexMap;

use crate::error::AppError;

use super::schema::{FieldKind, FieldDef, SCHEMA};

/// Resolved field: physical column or a computed expression (server whitelist template + `{alias}`).
#[derive(Debug, Clone)]
pub enum ResolvedField {
    Physical {
        table_alias: String,
        column: String,
    },
    Computed {
        expression: String,
    },
}

impl ResolvedField {
    /// SQL fragment for filters, GROUP BY, and HAVING (aggregate expressions use physical only).
    pub fn sql_expr(&self) -> String {
        match self {
            ResolvedField::Physical {
                table_alias,
                column,
            } => format!(r#""{}"."{}""#, table_alias, column),
            ResolvedField::Computed { expression } => expression.clone(),
        }
    }
}

/// Resolved join node with optional ON clause (root has no ON).
#[derive(Debug, Clone)]
pub struct JoinNode {
    pub alias: String,
    pub table: String,
    pub entity_name: String,
    pub join_on: Option<JoinOn>,
}

#[derive(Debug, Clone)]
pub struct JoinOn {
    pub from_alias: String,
    pub from_column: String,
    pub to_alias: String,
    pub to_column: String,
}

pub type AliasMap = IndexMap<String, JoinNode>;

/// Build alias map from root entity and dot-separated join paths (e.g. `items.biblios`).
pub fn resolve_joins(root_entity: &str, join_paths: &[String]) -> Result<AliasMap, AppError> {
    SCHEMA
        .get(root_entity)
        .ok_or_else(|| AppError::BadRequest(format!("Unknown root entity: {}", root_entity)))?;

    let mut alias_map = IndexMap::new();
    alias_map.insert(
        root_entity.to_string(),
        JoinNode {
            alias: root_entity.to_string(),
            table: SCHEMA[root_entity].table.to_string(),
            entity_name: root_entity.to_string(),
            join_on: None,
        },
    );

    for path in join_paths {
        if path.is_empty() {
            return Err(AppError::Validation("Empty join path".into()));
        }
        let segments: Vec<&str> = path.split('.').collect();
        let mut current_path = String::new();
        let mut current_entity = root_entity.to_string();
        let mut current_alias = root_entity.to_string();

        for (i, segment) in segments.iter().enumerate() {
            if i == 0 {
                current_path = (*segment).to_string();
            } else {
                current_path = format!("{}.{}", current_path, segment);
            }

            if let Some(existing) = alias_map.get(&current_path) {
                current_alias = existing.alias.clone();
                current_entity = existing.entity_name.clone();
                continue;
            }

            let entity_def = SCHEMA.get(current_entity.as_str()).ok_or_else(|| {
                AppError::BadRequest(format!("Unknown entity in join graph: {}", current_entity))
            })?;

            let relation = entity_def.relations.get(*segment).ok_or_else(|| {
                AppError::BadRequest(format!(
                    "Unknown relation: {}.{}",
                    current_entity, segment
                ))
            })?;

            let target_def = SCHEMA.get(relation.target_entity).ok_or_else(|| {
                AppError::BadRequest(format!(
                    "Relation target entity not registered: {}",
                    relation.target_entity
                ))
            })?;

            let new_alias = current_path.replace('.', "__");
            let node = JoinNode {
                alias: new_alias.clone(),
                table: target_def.table.to_string(),
                entity_name: relation.target_entity.to_string(),
                join_on: Some(JoinOn {
                    from_alias: current_alias.clone(),
                    from_column: relation.from_column.to_string(),
                    to_alias: new_alias.clone(),
                    to_column: relation.to_column.to_string(),
                }),
            };

            alias_map.insert(current_path.clone(), node);
            current_alias = new_alias;
            current_entity = relation.target_entity.to_string();
        }
    }

    Ok(alias_map)
}

pub fn emit_join_sql(alias_map: &AliasMap) -> String {
    let mut sql = String::new();
    for node in alias_map.values() {
        if let Some(ref on) = node.join_on {
            sql.push_str(&format!(
                " LEFT JOIN {} AS \"{}\" ON \"{}\".{} = \"{}\".{}",
                node.table, node.alias, on.from_alias, on.from_column, on.to_alias, on.to_column
            ));
        }
    }
    sql
}

fn substitute_alias(template: &str, table_alias: &str) -> String {
    let quoted = format!(r#""{}""#, table_alias);
    template.replace("{alias}", &quoted)
}

/// Resolve `entity.field` or `field` (implicit root) to a physical or computed SQL expression.
pub fn resolve_field(
    field_path: &str,
    root_entity: &str,
    alias_map: &AliasMap,
) -> Result<ResolvedField, AppError> {
    let parts: Vec<&str> = field_path.splitn(2, '.').collect();
    let (entity_name, field_name) = if parts.len() == 2 {
        (parts[0], parts[1])
    } else {
        (root_entity, parts[0])
    };

    let entity_def = SCHEMA.get(entity_name).ok_or_else(|| {
        AppError::BadRequest(format!("Unknown entity in field: {}", entity_name))
    })?;

    let field_def: &FieldDef = entity_def.fields.get(field_name).ok_or_else(|| {
        AppError::BadRequest(format!(
            "Field not allowed: {}.{}",
            entity_name, field_name
        ))
    })?;

    let table_alias = if let Some(node) = alias_map.get(entity_name) {
        node.alias.clone()
    } else {
        let candidates: Vec<&JoinNode> = alias_map
            .values()
            .filter(|n| n.entity_name == entity_name)
            .collect();

        match candidates.len() {
            0 => {
                return Err(AppError::BadRequest(format!(
                    "Entity '{}' is not joined; add it to joins",
                    entity_name
                )));
            }
            1 => candidates[0].alias.clone(),
            _ => {
                return Err(AppError::BadRequest(format!(
                    "Ambiguous entity '{}'; qualify joins so only one path exists",
                    entity_name
                )));
            }
        }
    };

    match &field_def.kind {
        FieldKind::Physical { column } => Ok(ResolvedField::Physical {
            table_alias,
            column: (*column).to_string(),
        }),
        FieldKind::Computed { sql_template } => Ok(ResolvedField::Computed {
            expression: substitute_alias(sql_template, &table_alias),
        }),
    }
}
