# Elidune — Design d'une API REST flexible pour les statistiques

## Contexte

Le serveur Elidune est écrit en **Rust** avec la stack suivante :

- **Axum 0.7** — framework HTTP
- **SQLx 0.7** — accès PostgreSQL (requêtes compilées ou dynamiques)
- **Meilisearch** — recherche full-text
- **Utoipa** — documentation OpenAPI
- **Redis** — cache (via `redis` crate, déjà dans les dépendances)
- **tower_governor** — rate limiting (déjà dans les dépendances)
- Architecture : `api/` → `services/` → `repository/`

L'objectif est de permettre aux bibliothécaires de construire leurs propres statistiques (fréquentation, emprunts, types de public, etc.) **sans coder en dur** des endpoints par indicateur, et de résister aux évolutions du schéma DB.

---

## Principe — Query Builder structuré

Un endpoint unique `POST /api/v1/stats/query` accepte un JSON déclaratif décrivant la requête souhaitée. Le serveur :

1. Valide la requête contre un **registre de schéma** (whitelist)
2. Résout le **graphe de jointures** (DFS topologique, aliases uniques)
3. Construit le SQL dynamiquement via SQLx (incluant `HAVING`, pagination)
4. Exécute avec **timeout** et **cache Redis**
5. Retourne les résultats en JSON tabulaire paginé

Le frontend présente un "Query Builder" visuel qui s'auto-configure via `GET /api/v1/stats/schema`.

---

## Schéma de la requête

```json
POST /api/v1/stats/query

{
  "entity": "loans",
  "joins": ["users", "items.biblios", "users.public_types"],
  "select": [
    { "field": "public_types.label", "alias": "type_public" },
    { "field": "biblios.media_type", "alias": "media" }
  ],
  "filters": [
    { "field": "loans.date", "op": "gte", "value": "2025-01-01" },
    { "field": "public_types.name", "op": "eq", "value": "adult" }
  ],
  "aggregations": [
    { "fn": "count", "field": "loans.id", "alias": "total_loans" },
    { "fn": "count_distinct", "field": "loans.user_id", "alias": "unique_borrowers" }
  ],
  "group_by": [
    { "field": "public_types.label", "alias": "type_public" },
    { "field": "biblios.media_type", "alias": "media" }
  ],
  "having": [
    { "field": "total_loans", "op": "gt", "value": 50 }
  ],
  "time_bucket": {
    "field": "loans.date",
    "granularity": "month",
    "alias": "mois"
  },
  "order_by": [{ "field": "total_loans", "dir": "desc" }],
  "limit": 100,
  "offset": 0
}
```

### Changements par rapport à la v1

| Ajout | Raison |
|-------|--------|
| `having` | Filtrer sur agrégations (ex. : "seulement les types avec > 50 emprunts") |
| `alias` dans `select`, `group_by`, `time_bucket` | Éviter les conflits de noms, références cohérentes dans ORDER BY/HAVING |
| `offset` | Pagination réelle (OFFSET + LIMIT) |
| Joins en graphe | `["users", "items.biblios", "users.public_types"]` — deux chemins depuis `loans`, gérés proprement |

---

## Endpoint de découverte du schéma

```json
GET /api/v1/stats/schema

{
  "entities": {
    "loans": {
      "label": "Emprunts",
      "fields": {
        "date":        { "type": "timestamptz", "label": "Date d'emprunt" },
        "returned_at": { "type": "timestamptz", "label": "Date de retour" },
        "nb_renews":   { "type": "integer",     "label": "Renouvellements" }
      },
      "relations": {
        "users": { "join": ["loans.user_id", "users.id"], "label": "Usager" },
        "items": { "join": ["loans.item_id", "items.id"], "label": "Exemplaire" }
      }
    },
    "users": {
      "label": "Usagers",
      "fields": {
        "firstname":  { "type": "text",    "label": "Prénom" },
        "lastname":   { "type": "text",    "label": "Nom" },
        "addr_city":  { "type": "text",    "label": "Ville" },
        "sex":        { "type": "integer", "label": "Sexe" },
        "birthdate":  { "type": "text",    "label": "Date de naissance" },
        "created_at": { "type": "timestamptz", "label": "Inscription" }
      },
      "relations": {
        "public_types":  { "join": ["users.public_type", "public_types.id"], "label": "Type de public" },
        "account_types": { "join": ["users.account_type", "account_types.code"], "label": "Type de compte" }
      }
    },
    "items": {
      "label": "Exemplaires",
      "fields": {
        "barcode":     { "type": "text", "label": "Code-barres" },
        "call_number": { "type": "text", "label": "Cote" },
        "created_at":  { "type": "timestamptz", "label": "Date de création" }
      },
      "relations": {
        "biblios": { "join": ["items.biblio_id", "biblios.id"], "label": "Notice" }
      }
    },
    "biblios": {
      "label": "Notices bibliographiques",
      "fields": {
        "title":            { "type": "text", "label": "Titre" },
        "media_type":       { "type": "text", "label": "Type de média" },
        "audience_type":    { "type": "text", "label": "Public cible" },
        "lang":             { "type": "text", "label": "Langue" },
        "publication_date": { "type": "text", "label": "Date de publication" }
      },
      "relations": {}
    },
    "public_types": {
      "label": "Types de public",
      "fields": {
        "name":  { "type": "text", "label": "Code" },
        "label": { "type": "text", "label": "Libellé" }
      },
      "relations": {}
    },
    "visitor_counts": {
      "label": "Fréquentation",
      "fields": {
        "count_date": { "type": "date",    "label": "Date" },
        "count":      { "type": "integer", "label": "Nombre de visiteurs" },
        "source":     { "type": "text",    "label": "Source" }
      },
      "relations": {}
    },
    "events": {
      "label": "Événements / animations",
      "fields": {
        "name":             { "type": "text",    "label": "Nom" },
        "event_type":       { "type": "integer", "label": "Type" },
        "event_date":       { "type": "date",    "label": "Date" },
        "attendees_count":  { "type": "integer", "label": "Participants" },
        "target_public":    { "type": "integer", "label": "Public cible" },
        "school_name":      { "type": "text",    "label": "École" },
        "students_count":   { "type": "integer", "label": "Élèves" }
      },
      "relations": {}
    },
    "loans_archives": {
      "label": "Emprunts archivés",
      "fields": {
        "date":        { "type": "timestamptz", "label": "Date" },
        "returned_at": { "type": "timestamptz", "label": "Retour" },
        "addr_city":   { "type": "text",        "label": "Ville emprunteur" }
      },
      "relations": {
        "public_types": { "join": ["loans_archives.borrower_public_type", "public_types.id"], "label": "Type de public" },
        "items":        { "join": ["loans_archives.item_id", "items.id"], "label": "Exemplaire" }
      }
    }
  },
  "aggregation_functions": ["count", "count_distinct", "sum", "avg", "min", "max"],
  "operators": ["eq", "neq", "gt", "gte", "lt", "lte", "in", "not_in", "is_null", "is_not_null"],
  "time_granularities": ["day", "week", "month", "quarter", "year"]
}
```

---

## Implémentation Rust

### Structure du module

```
src/
├── api/
│   └── stats.rs                # Handlers Axum + rate limiting + timeout
├── services/
│   └── stats/
│       ├── mod.rs
│       ├── schema.rs            # Registre de schéma (whitelist)
│       ├── join_graph.rs        # Résolution de jointures par DFS + aliases
│       ├── query_builder.rs     # Construction SQL (SELECT, WHERE, HAVING, pagination)
│       ├── validator.rs         # Validation de la requête
│       ├── executor.rs          # Exécution via SQLx + mapping PG→JSON robuste
│       └── cache.rs             # Cache Redis
├── models/
│   └── stats.rs                 # Structs de requête/réponse
```

---

### Modèles (models/stats.rs)

```rust
use serde::{Deserialize, Serialize};

// ─── Requête ────────────────────────────────────────────────────────

/// Requête de statistiques envoyée par le frontend.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct StatsQuery {
    pub entity: String,
    #[serde(default)]
    pub joins: Vec<String>,
    #[serde(default)]
    pub select: Vec<SelectField>,
    #[serde(default)]
    pub filters: Vec<StatsFilter>,
    #[serde(default)]
    pub aggregations: Vec<StatsAggregation>,
    #[serde(default)]
    pub group_by: Vec<GroupByField>,
    #[serde(default)]
    pub having: Vec<HavingFilter>,
    pub time_bucket: Option<TimeBucket>,
    #[serde(default)]
    pub order_by: Vec<StatsOrderBy>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Champ sélectionné avec alias optionnel.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SelectField {
    pub field: String,
    pub alias: Option<String>,
}

/// Champ de groupement avec alias optionnel.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct GroupByField {
    pub field: String,
    pub alias: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct StatsFilter {
    pub field: String,
    pub op: FilterOperator,
    pub value: serde_json::Value,
}

/// Filtre post-agrégation (HAVING).
/// `field` référence un alias d'agrégation (ex. "total_loans").
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct HavingFilter {
    pub field: String,
    pub op: FilterOperator,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FilterOperator {
    Eq, Neq, Gt, Gte, Lt, Lte,
    In, NotIn,
    IsNull, IsNotNull,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct StatsAggregation {
    pub fn_name: AggregateFunction,
    pub field: String,
    pub alias: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AggregateFunction {
    Count, CountDistinct, Sum, Avg, Min, Max,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct TimeBucket {
    pub field: String,
    pub granularity: TimeGranularity,
    pub alias: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimeGranularity {
    Day, Week, Month, Quarter, Year,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct StatsOrderBy {
    pub field: String,
    pub dir: Option<SortDirection>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    Asc, Desc,
}

// ─── Réponse ────────────────────────────────────────────────────────

/// Réponse tabulaire paginée.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StatsResponse {
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<serde_json::Map<String, serde_json::Value>>,
    pub total_rows: u64,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ColumnMeta {
    pub name: String,
    pub label: String,
    pub data_type: String,
}
```

---

### Registre de schéma (services/stats/schema.rs)

C'est le cœur de la sécurité et de l'adaptabilité. Ce fichier est la **seule chose à modifier** quand la DB évolue.

```rust
use std::collections::HashMap;
use once_cell::sync::Lazy;

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

/// LE REGISTRE — seul point de vérité entre la DB et l'API stats.
/// Quand le schéma DB change, on met à jour ici et le frontend s'adapte
/// automatiquement via GET /api/v1/stats/schema.
pub static SCHEMA: Lazy<HashMap<&'static str, EntityDef>> = Lazy::new(|| {
    let mut m = HashMap::new();

    m.insert("loans", EntityDef {
        table: "loans",
        label: "Emprunts",
        fields: fields![
            "date"        => ("date",        "timestamptz", "Date d'emprunt"),
            "returned_at" => ("returned_at", "timestamptz", "Date de retour"),
            "nb_renews"   => ("nb_renews",   "integer",     "Renouvellements"),
        ],
        relations: relations![
            "users" => ("users", "user_id", "id", "Usager"),
            "items" => ("items", "item_id", "id", "Exemplaire"),
        ],
    });

    // NOTE : password, totp_secret, recovery_codes EXCLUS
    m.insert("users", EntityDef {
        table: "users",
        label: "Usagers",
        fields: fields![
            "firstname"  => ("firstname",  "text",        "Prénom"),
            "lastname"   => ("lastname",   "text",        "Nom"),
            "addr_city"  => ("addr_city",  "text",        "Ville"),
            "sex"        => ("sex",        "integer",     "Sexe"),
            "birthdate"  => ("birthdate",  "text",        "Date de naissance"),
            "created_at" => ("created_at", "timestamptz", "Date d'inscription"),
        ],
        relations: relations![
            "public_types"  => ("public_types",  "public_type",  "id",   "Type de public"),
            "account_types" => ("account_types", "account_type", "code", "Type de compte"),
        ],
    });

    m.insert("items", EntityDef {
        table: "items",
        label: "Exemplaires",
        fields: fields![
            "barcode"     => ("barcode",     "text",        "Code-barres"),
            "call_number" => ("call_number", "text",        "Cote"),
            "created_at"  => ("created_at",  "timestamptz", "Création"),
        ],
        relations: relations![
            "biblios" => ("biblios", "biblio_id", "id", "Notice"),
        ],
    });

    m.insert("biblios", EntityDef {
        table: "biblios",
        label: "Notices",
        fields: fields![
            "title"            => ("title",            "text", "Titre"),
            "media_type"       => ("media_type",       "text", "Type de média"),
            "audience_type"    => ("audience_type",     "text", "Public cible"),
            "lang"             => ("lang",             "text", "Langue"),
            "publication_date" => ("publication_date", "text", "Date de publication"),
        ],
        relations: relations![],
    });

    m.insert("public_types", EntityDef {
        table: "public_types",
        label: "Types de public",
        fields: fields![
            "name"  => ("name",  "text", "Code"),
            "label" => ("label", "text", "Libellé"),
        ],
        relations: relations![],
    });

    m.insert("visitor_counts", EntityDef {
        table: "visitor_counts",
        label: "Fréquentation",
        fields: fields![
            "count_date" => ("count_date", "date",    "Date"),
            "count"      => ("count",      "integer", "Visiteurs"),
            "source"     => ("source",     "text",    "Source"),
        ],
        relations: relations![],
    });

    m.insert("events", EntityDef {
        table: "events",
        label: "Animations",
        fields: fields![
            "name"            => ("name",            "text",    "Nom"),
            "event_type"      => ("event_type",      "integer", "Type"),
            "event_date"      => ("event_date",      "date",    "Date"),
            "attendees_count" => ("attendees_count",  "integer", "Participants"),
            "target_public"   => ("target_public",   "integer", "Public cible"),
            "school_name"     => ("school_name",     "text",    "École"),
            "students_count"  => ("students_count",  "integer", "Élèves"),
        ],
        relations: relations![],
    });

    m.insert("loans_archives", EntityDef {
        table: "loans_archives",
        label: "Emprunts archivés",
        fields: fields![
            "date"        => ("date",        "timestamptz", "Date"),
            "returned_at" => ("returned_at", "timestamptz", "Retour"),
            "addr_city"   => ("addr_city",   "text",        "Ville emprunteur"),
        ],
        relations: relations![
            "public_types" => ("public_types", "borrower_public_type", "id", "Type de public"),
            "items"        => ("items",        "item_id",              "id", "Exemplaire"),
        ],
    });

    m
});

// Macros utilitaires
macro_rules! fields {
    ($($name:literal => ($col:literal, $dt:literal, $label:literal)),* $(,)?) => {{
        let mut f = HashMap::new();
        $(f.insert($name, FieldDef {
            column: $col, data_type: $dt, label: $label
        });)*
        f
    }};
}

macro_rules! relations {
    ($($name:literal => ($target:literal, $from:literal, $to:literal, $label:literal)),* $(,)?) => {{
        let mut r = HashMap::new();
        $(r.insert($name, RelationDef {
            target_entity: $target,
            from_column: $from,
            to_column: $to,
            label: $label,
        });)*
        r
    }};
}

pub(crate) use fields;
pub(crate) use relations;
```

---

### Résolution de jointures par graphe (services/stats/join_graph.rs)

L'ancien `resolve_join` supposait une chaîne linéaire de LEFT JOIN. Problème : quand la requête contient
`["users", "items.biblios", "users.public_types"]`, on a **deux chemins** depuis l'entité racine `loans`,
et `users` est atteint par deux routes. L'ancien code produisait des doublons ou des aliases en conflit.

Le nouveau système :

1. **Parse** chaque chemin de join en arêtes `(source_entity, relation_name, target_entity)`
2. **Dé-duplique** dans un `IndexMap` (insertion-order preserved, pas de doublon)
3. **Attribue un alias unique** à chaque nœud visité, en tenant compte du chemin d'accès (ex. `public_types` atteint via `users` → alias `users__public_types`)
4. **Génère** les clauses LEFT JOIN dans l'ordre topologique naturel (ordre de parcours DFS)

```rust
use indexmap::IndexMap;
use super::schema::{SCHEMA, RelationDef};
use crate::error::AppError;

/// Un nœud résolu dans le graphe de jointures.
#[derive(Debug, Clone)]
pub struct JoinNode {
    /// Alias SQL unique pour cette occurrence (ex. "users__public_types")
    pub alias: String,
    /// Nom de la table physique
    pub table: String,
    /// Nom de l'entité dans le registre
    pub entity_name: String,
    /// Clause ON — None pour l'entité racine
    pub join_on: Option<JoinOn>,
}

#[derive(Debug, Clone)]
pub struct JoinOn {
    pub from_alias: String,
    pub from_column: String,
    pub to_alias: String,
    pub to_column: String,
}

/// Table d'alias : clé = chemin canonique (ex. "users", "users.public_types"),
/// valeur = JoinNode avec alias unique.
pub type AliasMap = IndexMap<String, JoinNode>;

/// Résout TOUS les chemins de join et retourne une table d'aliases ordonnée.
///
/// Entrée : entity racine + liste de chemins (["users", "items.biblios", "users.public_types"])
/// Sortie : IndexMap ordonné (DFS) avec les JoinNodes, prêt à émettre du SQL.
///
/// Propriétés :
///   - Chaque entité jointe a un alias UNIQUE dérivé du chemin
///   - Pas de doublon (même si "users" apparaît dans deux chemins)
///   - L'ordre d'insertion respecte la dépendance (parent avant enfant)
pub fn resolve_joins(
    root_entity: &str,
    join_paths: &[String],
) -> Result<AliasMap, AppError> {
    let root_def = SCHEMA.get(root_entity)
        .ok_or_else(|| AppError::BadRequest(
            format!("Entité racine inconnue: {}", root_entity)
        ))?;

    let mut alias_map = IndexMap::new();

    // Insérer la racine
    alias_map.insert(root_entity.to_string(), JoinNode {
        alias: root_entity.to_string(),
        table: root_def.table.to_string(),
        entity_name: root_entity.to_string(),
        join_on: None,
    });

    // Parcourir chaque chemin de join
    for path in join_paths {
        let segments: Vec<&str> = path.split('.').collect();
        let mut current_path = String::new();
        let mut current_entity = root_entity;
        let mut current_alias = root_entity.to_string();

        for (i, segment) in segments.iter().enumerate() {
            // Construire le chemin canonique cumulé
            if i == 0 {
                current_path = segment.to_string();
            } else {
                current_path = format!("{}.{}", current_path, segment);
            }

            // Si ce chemin est déjà résolu, on avance sans re-joindre
            if let Some(existing) = alias_map.get(&current_path) {
                current_alias = existing.alias.clone();
                current_entity = &existing.entity_name;
                continue;
            }

            // Résoudre la relation depuis l'entité courante
            let entity_def = SCHEMA.get(current_entity)
                .ok_or_else(|| AppError::BadRequest(
                    format!("Entité inconnue dans le graphe: {}", current_entity)
                ))?;

            let relation = entity_def.relations.get(*segment)
                .ok_or_else(|| AppError::BadRequest(
                    format!("Relation inconnue: {}.{}", current_entity, segment)
                ))?;

            let target_def = SCHEMA.get(relation.target_entity)
                .ok_or_else(|| AppError::BadRequest(
                    format!("Cible de relation inconnue: {}", relation.target_entity)
                ))?;

            // Alias unique = chemin avec séparateur "__"
            // Ex: "users" → "users", "users.public_types" → "users__public_types"
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
            current_entity = relation.target_entity;
        }
    }

    Ok(alias_map)
}

/// Génère les clauses SQL LEFT JOIN depuis la table d'aliases.
/// Saute l'entrée racine (qui n'a pas de join_on).
pub fn emit_join_sql(alias_map: &AliasMap) -> String {
    let mut sql = String::new();
    for node in alias_map.values() {
        if let Some(ref on) = node.join_on {
            sql.push_str(&format!(
                " LEFT JOIN {} AS \"{}\" ON \"{}\".{} = \"{}\".{}",
                node.table, node.alias,
                on.from_alias, on.from_column,
                on.to_alias, on.to_column,
            ));
        }
    }
    sql
}

/// Résout "entity.field" → (alias SQL, column_name) en utilisant la table d'aliases.
///
/// Logique :
///   1. "loans.date"         → cherche l'entité "loans" dans alias_map → alias + colonne
///   2. "public_types.label" → cherche TOUS les chemins dont l'entity_name == "public_types"
///                              → s'il y en a un seul → OK, sinon → ambiguïté, erreur
///   3. "field_only"         → implicitement l'entité racine
pub fn resolve_field(
    field_path: &str,
    root_entity: &str,
    alias_map: &AliasMap,
) -> Result<(String, String), AppError> {
    let parts: Vec<&str> = field_path.splitn(2, '.').collect();

    let (entity_name, field_name) = if parts.len() == 2 {
        (parts[0], parts[1])
    } else {
        (root_entity, parts[0])
    };

    // Valider le champ dans le registre
    let entity_def = SCHEMA.get(entity_name)
        .ok_or_else(|| AppError::BadRequest(
            format!("Entité inconnue: {}", entity_name)
        ))?;

    let field_def = entity_def.fields.get(field_name)
        .ok_or_else(|| AppError::BadRequest(
            format!("Champ non autorisé: {}.{}", entity_name, field_name)
        ))?;

    // Trouver l'alias dans la map
    // Cas 1 : chemin direct (ex. "users" ou "loans")
    if let Some(node) = alias_map.get(entity_name) {
        return Ok((node.alias.clone(), field_def.column.to_string()));
    }

    // Cas 2 : chercher par entity_name dans toutes les entrées
    let candidates: Vec<&JoinNode> = alias_map.values()
        .filter(|n| n.entity_name == entity_name)
        .collect();

    match candidates.len() {
        0 => Err(AppError::BadRequest(
            format!("L'entité '{}' n'est pas jointe. Ajoutez-la dans 'joins'.", entity_name)
        )),
        1 => Ok((candidates[0].alias.clone(), field_def.column.to_string())),
        _ => Err(AppError::BadRequest(
            format!(
                "Ambiguïté : l'entité '{}' est jointe par {} chemins. \
                 Utilisez le chemin complet (ex. 'users.public_types.label').",
                entity_name, candidates.len()
            )
        )),
    }
}
```

**Dépendance à ajouter dans `Cargo.toml` :**

```toml
indexmap = { version = "2", features = ["serde"] }
```

#### Illustration du graphe de jointures

Pour la requête `joins: ["users", "items.biblios", "users.public_types"]` avec l'entité racine `loans` :

```
                    loans  (alias: "loans")
                   /     \
                  /       \
     [user_id]  /         \  [item_id]
               v           v
           users            items  (alias: "items")
      (alias: "users")        |
              |                |  [biblio_id]
              |  [public_type] v
              v            biblios  (alias: "items__biblios")
        public_types
  (alias: "users__public_types")
```

L'`IndexMap` contient, dans l'ordre d'insertion :

| Clé (chemin canonique) | Alias SQL | Table physique | JOIN ON |
|------------------------|-----------|----------------|---------|
| `loans` | `loans` | `loans` | *(racine)* |
| `users` | `users` | `users` | `loans.user_id = users.id` |
| `items` | `items` | `items` | `loans.item_id = items.id` |
| `items.biblios` | `items__biblios` | `biblios` | `items.biblio_id = items__biblios.id` |
| `users.public_types` | `users__public_types` | `public_types` | `users.public_type = users__public_types.id` |

---

### Construction SQL (services/stats/query_builder.rs)

```rust
use super::join_graph::{self, AliasMap, resolve_field, resolve_joins, emit_join_sql};
use super::schema::SCHEMA;
use crate::models::stats::*;
use crate::error::AppError;

/// Résultat de la construction : SQL principal, SQL de comptage, et valeurs à binder.
pub struct BuiltQuery {
    pub data_sql: String,
    pub count_sql: String,
    pub binds: Vec<serde_json::Value>,
}

/// Construit la requête SQL paramétrée complète.
pub fn build_sql(query: &StatsQuery) -> Result<BuiltQuery, AppError> {
    let entity = SCHEMA.get(query.entity.as_str())
        .ok_or_else(|| AppError::BadRequest(
            format!("Entité inconnue: {}", query.entity)
        ))?;

    // 1. Résoudre le graphe de jointures
    let alias_map = resolve_joins(&query.entity, &query.joins)?;

    let mut binds: Vec<serde_json::Value> = Vec::new();
    let mut bind_idx = 1usize;

    // ─── SELECT ─────────────────────────────────────────────
    let mut select_parts: Vec<String> = Vec::new();

    for sf in &query.select {
        let (tbl_alias, col) = resolve_field(&sf.field, &query.entity, &alias_map)?;
        let alias = sf.alias.as_deref().unwrap_or(&sf.field);
        select_parts.push(format!("\"{}\".{} AS \"{}\"", tbl_alias, col, alias));
    }

    // Time bucket
    if let Some(ref tb) = query.time_bucket {
        let (tbl_alias, col) = resolve_field(&tb.field, &query.entity, &alias_map)?;
        let trunc = granularity_to_pg(&tb.granularity);
        let default_alias = format!("{}_{}", tb.field, trunc);
        let alias = tb.alias.as_deref().unwrap_or(&default_alias);
        select_parts.push(
            format!("DATE_TRUNC('{}', \"{}\".{}) AS \"{}\"", trunc, tbl_alias, col, alias)
        );
    }

    // Agrégations
    for agg in &query.aggregations {
        let (tbl_alias, col) = resolve_field(&agg.field, &query.entity, &alias_map)?;
        let expr = build_agg_expr(&agg.fn_name, &tbl_alias, &col);
        select_parts.push(format!("{} AS \"{}\"", expr, agg.alias));
    }

    let select_clause = select_parts.join(", ");

    // ─── FROM + JOINs ───────────────────────────────────────
    let from_clause = format!("{} AS \"{}\"", entity.table, query.entity);
    let join_clause = emit_join_sql(&alias_map);

    // ─── WHERE ──────────────────────────────────────────────
    let where_clause = if !query.filters.is_empty() {
        let conditions = build_filter_conditions(
            &query.filters, &query.entity, &alias_map, &mut binds, &mut bind_idx
        )?;
        format!(" WHERE {}", conditions.join(" AND "))
    } else {
        String::new()
    };

    // ─── GROUP BY ───────────────────────────────────────────
    let group_by_clause = {
        let mut gb_parts: Vec<String> = Vec::new();

        for gbf in &query.group_by {
            let (tbl_alias, col) = resolve_field(&gbf.field, &query.entity, &alias_map)?;
            gb_parts.push(format!("\"{}\".{}", tbl_alias, col));
        }

        // Ajouter le time_bucket au GROUP BY automatiquement
        if let Some(ref tb) = query.time_bucket {
            let (tbl_alias, col) = resolve_field(&tb.field, &query.entity, &alias_map)?;
            let trunc = granularity_to_pg(&tb.granularity);
            let expr = format!("DATE_TRUNC('{}', \"{}\".{})", trunc, tbl_alias, col);
            if !gb_parts.contains(&expr) {
                gb_parts.push(expr);
            }
        }

        if gb_parts.is_empty() {
            String::new()
        } else {
            format!(" GROUP BY {}", gb_parts.join(", "))
        }
    };

    // ─── HAVING ─────────────────────────────────────────────
    let having_clause = if !query.having.is_empty() {
        let conditions = build_having_conditions(
            &query.having, &query.aggregations, &query.entity, &alias_map,
            &mut binds, &mut bind_idx,
        )?;
        format!(" HAVING {}", conditions.join(" AND "))
    } else {
        String::new()
    };

    // ─── ORDER BY ───────────────────────────────────────────
    let order_by_clause = if !query.order_by.is_empty() {
        let ob_parts: Vec<String> = query.order_by.iter().map(|ob| {
            let dir = match ob.dir.as_ref().unwrap_or(&SortDirection::Asc) {
                SortDirection::Asc => "ASC",
                SortDirection::Desc => "DESC",
            };
            format!("\"{}\" {}", ob.field, dir)
        }).collect();
        format!(" ORDER BY {}", ob_parts.join(", "))
    } else {
        String::new()
    };

    // ─── LIMIT + OFFSET ────────────────────────────────────
    let limit = query.limit.unwrap_or(1000).min(10_000);
    let offset = query.offset.unwrap_or(0);
    let pagination = format!(" LIMIT {} OFFSET {}", limit, offset);

    // ─── Assemblage ─────────────────────────────────────────
    let core_sql = format!(
        "SELECT {} FROM {}{}{}{}{}",
        select_clause, from_clause, join_clause, where_clause, group_by_clause, having_clause
    );

    let data_sql = format!("{}{}{}", core_sql, order_by_clause, pagination);

    // Requête de comptage total (pour la pagination)
    let count_sql = format!(
        "SELECT COUNT(*) AS \"__total\" FROM ({}) AS __sub",
        core_sql
    );

    Ok(BuiltQuery { data_sql, count_sql, binds })
}

// ─── Helpers privés ─────────────────────────────────────────────────

fn granularity_to_pg(g: &TimeGranularity) -> &'static str {
    match g {
        TimeGranularity::Day     => "day",
        TimeGranularity::Week    => "week",
        TimeGranularity::Month   => "month",
        TimeGranularity::Quarter => "quarter",
        TimeGranularity::Year    => "year",
    }
}

fn build_agg_expr(func: &AggregateFunction, alias: &str, col: &str) -> String {
    match func {
        AggregateFunction::Count        => format!("COUNT(\"{}\".{})", alias, col),
        AggregateFunction::CountDistinct => format!("COUNT(DISTINCT \"{}\".{})", alias, col),
        AggregateFunction::Sum          => format!("SUM(\"{}\".{})", alias, col),
        AggregateFunction::Avg          => format!("AVG(\"{}\".{})", alias, col),
        AggregateFunction::Min          => format!("MIN(\"{}\".{})", alias, col),
        AggregateFunction::Max          => format!("MAX(\"{}\".{})", alias, col),
    }
}

/// Construit les conditions WHERE.
fn build_filter_conditions(
    filters: &[StatsFilter],
    root_entity: &str,
    alias_map: &AliasMap,
    binds: &mut Vec<serde_json::Value>,
    bind_idx: &mut usize,
) -> Result<Vec<String>, AppError> {
    filters.iter().map(|f| {
        let (tbl_alias, col) = resolve_field(&f.field, root_entity, alias_map)?;
        let qualified = format!("\"{}\".{}", tbl_alias, col);
        build_condition_sql(&qualified, &f.op, &f.value, binds, bind_idx)
    }).collect()
}

/// Construit les conditions HAVING.
/// Les champs référencent des ALIAS d'agrégation, pas des colonnes de table.
/// PostgreSQL n'accepte pas les alias dans HAVING → on reconstruit l'expression.
fn build_having_conditions(
    having: &[HavingFilter],
    aggregations: &[StatsAggregation],
    root_entity: &str,
    alias_map: &AliasMap,
    binds: &mut Vec<serde_json::Value>,
    bind_idx: &mut usize,
) -> Result<Vec<String>, AppError> {
    having.iter().map(|h| {
        let agg = aggregations.iter()
            .find(|a| a.alias == h.field)
            .ok_or_else(|| AppError::BadRequest(
                format!(
                    "HAVING '{}' ne correspond à aucune agrégation. Disponibles : {:?}",
                    h.field,
                    aggregations.iter().map(|a| &a.alias).collect::<Vec<_>>()
                )
            ))?;

        let (tbl_alias, col) = resolve_field(&agg.field, root_entity, alias_map)?;
        let agg_expr = build_agg_expr(&agg.fn_name, &tbl_alias, &col);

        build_condition_sql(&agg_expr, &h.op, &h.value, binds, bind_idx)
    }).collect()
}

/// Construit une condition SQL pour une expression (colonne qualifiée ou agrégation).
fn build_condition_sql(
    expr: &str,
    op: &FilterOperator,
    value: &serde_json::Value,
    binds: &mut Vec<serde_json::Value>,
    bind_idx: &mut usize,
) -> Result<String, AppError> {
    match op {
        FilterOperator::IsNull    => Ok(format!("{} IS NULL", expr)),
        FilterOperator::IsNotNull => Ok(format!("{} IS NOT NULL", expr)),
        FilterOperator::In | FilterOperator::NotIn => {
            let arr = value.as_array()
                .ok_or_else(|| AppError::BadRequest(
                    "L'opérateur IN/NOT_IN attend un tableau".into()
                ))?;
            let placeholders: Vec<String> = arr.iter().map(|v| {
                binds.push(v.clone());
                let p = format!("${}", *bind_idx);
                *bind_idx += 1;
                p
            }).collect();
            let kw = if matches!(op, FilterOperator::In) { "IN" } else { "NOT IN" };
            Ok(format!("{} {} ({})", expr, kw, placeholders.join(", ")))
        }
        _ => {
            let op_str = match op {
                FilterOperator::Eq  => "=",
                FilterOperator::Neq => "!=",
                FilterOperator::Gt  => ">",
                FilterOperator::Gte => ">=",
                FilterOperator::Lt  => "<",
                FilterOperator::Lte => "<=",
                _ => unreachable!(),
            };
            binds.push(value.clone());
            let c = format!("{} {} ${}", expr, op_str, *bind_idx);
            *bind_idx += 1;
            Ok(c)
        }
    }
}
```

---

### Exécution dynamique (repository/stats/executor.rs)

Mapping PostgreSQL → JSON robuste via `PgTypeInfo`, avec timeout `tokio::time::timeout`.

```rust
use sqlx::{PgPool, Row, Column, TypeInfo};
use sqlx::postgres::PgRow;
use crate::models::stats::*;
use crate::error::AppError;
use std::time::Duration;

/// Timeout par défaut (configurable via settings).
const QUERY_TIMEOUT: Duration = Duration::from_secs(30);

/// Exécute la requête de données + la requête de comptage, avec timeout global.
pub async fn execute(
    pool: &PgPool,
    data_sql: &str,
    count_sql: &str,
    binds: &[serde_json::Value],
    limit: u32,
    offset: u32,
) -> Result<StatsResponse, AppError> {
    let (data_result, count_result) = tokio::time::timeout(
        QUERY_TIMEOUT,
        async {
            let data_fut = execute_raw(pool, data_sql, binds);
            let count_fut = execute_count(pool, count_sql, binds);
            tokio::join!(data_fut, count_fut)
        }
    )
    .await
    .map_err(|_| AppError::Internal(
        format!("Timeout : la requête a dépassé {}s", QUERY_TIMEOUT.as_secs())
    ))?;

    let rows = data_result?;
    let total_rows = count_result?;

    let columns: Vec<ColumnMeta> = if let Some(first) = rows.first() {
        first.columns().iter().map(|col| ColumnMeta {
            name: col.name().to_string(),
            label: col.name().to_string(),
            data_type: col.type_info().name().to_string(),
        }).collect()
    } else {
        Vec::new()
    };

    let json_rows: Vec<serde_json::Map<String, serde_json::Value>> = rows.iter()
        .map(|row| row_to_json(row))
        .collect();

    Ok(StatsResponse { columns, rows: json_rows, total_rows, limit, offset })
}

async fn execute_raw(
    pool: &PgPool,
    sql: &str,
    binds: &[serde_json::Value],
) -> Result<Vec<PgRow>, AppError> {
    let mut q = sqlx::query(sql);
    q = bind_values(q, binds);
    q.fetch_all(pool).await
        .map_err(|e| AppError::Internal(format!("Erreur SQL: {}", e)))
}

async fn execute_count(
    pool: &PgPool,
    count_sql: &str,
    binds: &[serde_json::Value],
) -> Result<u64, AppError> {
    let mut q = sqlx::query(count_sql);
    q = bind_values(q, binds);
    let row = q.fetch_one(pool).await
        .map_err(|e| AppError::Internal(format!("Erreur SQL (count): {}", e)))?;
    let total: i64 = row.try_get("__total")
        .map_err(|e| AppError::Internal(format!("Erreur lecture total: {}", e)))?;
    Ok(total as u64)
}

fn bind_values<'q>(
    mut q: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    binds: &'q [serde_json::Value],
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    for bind in binds {
        q = match bind {
            serde_json::Value::String(s) => q.bind(s.as_str()),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() { q.bind(i) }
                else if let Some(f) = n.as_f64() { q.bind(f) }
                else { q.bind(n.to_string()) }
            }
            serde_json::Value::Bool(b) => q.bind(*b),
            serde_json::Value::Null => q.bind(Option::<String>::None),
            _ => q.bind(bind.to_string()),
        };
    }
    q
}

/// Convertit une PgRow en Map JSON en utilisant le type PG de chaque colonne.
fn row_to_json(row: &PgRow) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();

    for col in row.columns() {
        let name = col.name();
        let type_name = col.type_info().name();

        let val: serde_json::Value = match type_name {
            // Entiers
            "INT2" => row.try_get::<i16, _>(name)
                .map(|v| serde_json::Value::from(v as i64))
                .unwrap_or(serde_json::Value::Null),
            "INT4" => row.try_get::<i32, _>(name)
                .map(|v| serde_json::Value::from(v as i64))
                .unwrap_or(serde_json::Value::Null),
            "INT8" => row.try_get::<i64, _>(name)
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null),

            // Flottants
            "FLOAT4" => row.try_get::<f32, _>(name)
                .ok()
                .and_then(|v| serde_json::Number::from_f64(v as f64))
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            "FLOAT8" | "NUMERIC" => row.try_get::<f64, _>(name)
                .ok()
                .and_then(|v| serde_json::Number::from_f64(v))
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),

            // Booléens
            "BOOL" => row.try_get::<bool, _>(name)
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null),

            // Texte
            "TEXT" | "VARCHAR" | "CHAR" | "NAME" | "BPCHAR" =>
                row.try_get::<String, _>(name)
                    .map(serde_json::Value::from)
                    .unwrap_or(serde_json::Value::Null),

            // Dates & timestamps
            "DATE" => row.try_get::<chrono::NaiveDate, _>(name)
                .map(|v| v.to_string().into())
                .unwrap_or(serde_json::Value::Null),
            "TIME" => row.try_get::<chrono::NaiveTime, _>(name)
                .map(|v| v.to_string().into())
                .unwrap_or(serde_json::Value::Null),
            "TIMESTAMP" => row.try_get::<chrono::NaiveDateTime, _>(name)
                .map(|v| v.to_string().into())
                .unwrap_or(serde_json::Value::Null),
            "TIMESTAMPTZ" => row.try_get::<chrono::DateTime<chrono::Utc>, _>(name)
                .map(|v| v.to_rfc3339().into())
                .unwrap_or(serde_json::Value::Null),

            // JSON
            "JSON" | "JSONB" =>
                row.try_get::<serde_json::Value, _>(name)
                    .unwrap_or(serde_json::Value::Null),

            // UUID
            "UUID" => row.try_get::<uuid::Uuid, _>(name)
                .map(|v| v.to_string().into())
                .unwrap_or(serde_json::Value::Null),

            // Fallback
            _ => row.try_get::<String, _>(name)
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null),
        };

        map.insert(name.to_string(), val);
    }

    map
}
```

---

### Cache Redis (services/stats/cache.rs)

Le cache est optionnel (si Redis n'est pas configuré, on passe directement à l'exécution).
La clé est un hash SHA-256 du JSON de la requête.

```rust
use redis::AsyncCommands;
use sha2::{Sha256, Digest};
use crate::models::stats::{StatsQuery, StatsResponse};

/// TTL : 5 minutes — les données de bibliothèque ne changent pas à la seconde.
const CACHE_TTL_SECS: u64 = 300;
const CACHE_PREFIX: &str = "elidune:stats:";

pub fn cache_key(query: &StatsQuery) -> String {
    let json = serde_json::to_string(query).unwrap_or_default();
    let hash = hex::encode(Sha256::digest(json.as_bytes()));
    format!("{}{}", CACHE_PREFIX, hash)
}

pub async fn get(
    redis: &redis::aio::ConnectionManager,
    key: &str,
) -> Option<StatsResponse> {
    let data: Option<String> = redis.clone().get(key).await.ok()?;
    data.and_then(|s| serde_json::from_str(&s).ok())
}

pub async fn set(
    redis: &redis::aio::ConnectionManager,
    key: &str,
    response: &StatsResponse,
) {
    if let Ok(json) = serde_json::to_string(response) {
        let _: Result<(), _> = redis.clone()
            .set_ex(key, json, CACHE_TTL_SECS)
            .await;
    }
}

/// Invalide tout le cache stats (après un import massif, par ex.).
pub async fn invalidate_all(redis: &redis::aio::ConnectionManager) {
    let pattern = format!("{}*", CACHE_PREFIX);
    let keys: Vec<String> = redis::cmd("KEYS")
        .arg(&pattern)
        .query_async(&mut redis.clone())
        .await
        .unwrap_or_default();

    if !keys.is_empty() {
        let _: Result<(), _> = redis.clone().del(keys).await;
    }
}
```

**Dépendances à ajouter dans `Cargo.toml` :**

```toml
sha2 = "0.10"
hex = "0.4"
indexmap = { version = "2", features = ["serde"] }
```

---

### Handler Axum (api/stats.rs)

Intègre le cache, le timeout, et un rate limiting spécifique aux stats.

```rust
use axum::{
    extract::State,
    Json,
    routing::{get, post},
    Router,
};
use tower_governor::governor::GovernorConfigBuilder;
use crate::models::stats::*;
use crate::services::stats::{schema::SCHEMA, query_builder, executor, cache, validator};
use crate::AppState;

pub fn router() -> Router<AppState> {
    // Rate limit stats : 30 req/min par IP (plus restrictif que le CRUD)
    let stats_governor = GovernorConfigBuilder::default()
        .per_second(2)
        .burst_size(30)
        .finish()
        .expect("governor config");

    Router::new()
        .route("/stats/schema", get(get_schema))
        .route("/stats/query", post(execute_query))
        .route("/stats/saved", get(list_saved_queries))
        .route("/stats/saved", post(save_query))
        .route("/stats/saved/:id/run", get(run_saved_query))
        .layer(tower_governor::GovernorLayer {
            config: std::sync::Arc::new(stats_governor),
        })
}

/// GET /api/v1/stats/schema
async fn get_schema() -> Json<serde_json::Value> {
    let schema_json = serialize_schema(&SCHEMA);
    Json(schema_json)
}

/// POST /api/v1/stats/query
async fn execute_query(
    State(state): State<AppState>,
    Json(query): Json<StatsQuery>,
) -> Result<Json<StatsResponse>, crate::error::AppError> {
    // 1. Valider
    validator::validate(&query)?;

    // 2. Cache lookup
    let key = cache::cache_key(&query);
    if let Some(ref redis) = state.redis {
        if let Some(cached) = cache::get(redis, &key).await {
            return Ok(Json(cached));
        }
    }

    // 3. Construire le SQL
    let built = query_builder::build_sql(&query)?;
    let limit = query.limit.unwrap_or(1000).min(10_000);
    let offset = query.offset.unwrap_or(0);

    // 4. Exécuter (timeout inclus dans executor)
    let response = executor::execute(
        &state.db, &built.data_sql, &built.count_sql, &built.binds, limit, offset,
    ).await?;

    // 5. Cache store
    if let Some(ref redis) = state.redis {
        cache::set(redis, &key, &response).await;
    }

    Ok(Json(response))
}
```

---

### Requêtes sauvegardées

```sql
CREATE TABLE IF NOT EXISTS saved_queries (
    id          BIGSERIAL   PRIMARY KEY,
    name        VARCHAR(200) NOT NULL,
    description TEXT,
    query_json  JSONB       NOT NULL,
    user_id     BIGINT      REFERENCES users(id),
    is_shared   BOOLEAN     DEFAULT FALSE,
    created_at  TIMESTAMPTZ DEFAULT NOW(),
    updated_at  TIMESTAMPTZ DEFAULT NOW()
);
```

Endpoints :
- `GET  /api/v1/stats/saved` — lister (les siennes + les partagées)
- `POST /api/v1/stats/saved` — sauvegarder une requête
- `GET  /api/v1/stats/saved/:id/run` — exécuter une requête sauvegardée

---

## Sécurité

Le registre de schéma fait office de **whitelist stricte** :

- Les champs sensibles (`password`, `totp_secret`, `recovery_codes`) ne sont jamais exposés
- Seuls les champs listés dans `SCHEMA` sont accessibles
- Les identifiants de table/colonne sont résolus par le registre, jamais injectés depuis l'input utilisateur
- Les valeurs sont toujours passées via des paramètres bindés (`$1`, `$2`…)
- `LIMIT` plafond (10 000) + `OFFSET` pour la pagination
- **Timeout** (30s) sur chaque requête via `tokio::time::timeout`
- **Rate limiting dédié** (30 req/min) via `tower_governor`
- **Cache Redis** (TTL 5min) pour absorber les requêtes répétitives
- Auth JWT existante + rôle minimum (`librarian` ou `admin`)

---

## Exemples de requêtes concrètes

### 1. Emprunts par mois et type de public (seuil > 50)

```json
{
  "entity": "loans",
  "joins": ["users.public_types", "items.biblios"],
  "select": [
    { "field": "public_types.label", "alias": "type_public" },
    { "field": "biblios.media_type", "alias": "media" }
  ],
  "aggregations": [
    { "fn": "count", "field": "loans.id", "alias": "nb_emprunts" },
    { "fn": "count_distinct", "field": "loans.user_id", "alias": "lecteurs_uniques" }
  ],
  "group_by": [
    { "field": "public_types.label", "alias": "type_public" },
    { "field": "biblios.media_type", "alias": "media" }
  ],
  "having": [
    { "field": "nb_emprunts", "op": "gt", "value": 50 }
  ],
  "time_bucket": { "field": "loans.date", "granularity": "month", "alias": "mois" },
  "filters": [
    { "field": "loans.date", "op": "gte", "value": "2025-01-01" }
  ],
  "order_by": [{ "field": "nb_emprunts", "dir": "desc" }],
  "limit": 200,
  "offset": 0
}
```

**SQL généré :**

```sql
SELECT
    "users__public_types".label AS "type_public",
    "items__biblios".media_type AS "media",
    DATE_TRUNC('month', "loans".date) AS "mois",
    COUNT("loans".id) AS "nb_emprunts",
    COUNT(DISTINCT "loans".user_id) AS "lecteurs_uniques"
FROM loans AS "loans"
    LEFT JOIN users AS "users" ON "loans".user_id = "users".id
    LEFT JOIN public_types AS "users__public_types"
        ON "users".public_type = "users__public_types".id
    LEFT JOIN items AS "items" ON "loans".item_id = "items".id
    LEFT JOIN biblios AS "items__biblios"
        ON "items".biblio_id = "items__biblios".id
WHERE "loans".date >= $1
GROUP BY "users__public_types".label, "items__biblios".media_type,
         DATE_TRUNC('month', "loans".date)
HAVING COUNT("loans".id) > $2
ORDER BY "nb_emprunts" DESC
LIMIT 200 OFFSET 0
```

### 2. Fréquentation mensuelle

```json
{
  "entity": "visitor_counts",
  "joins": [],
  "select": [],
  "aggregations": [
    { "fn": "sum", "field": "visitor_counts.count", "alias": "total_visiteurs" },
    { "fn": "avg", "field": "visitor_counts.count", "alias": "moyenne_jour" }
  ],
  "group_by": [],
  "time_bucket": { "field": "visitor_counts.count_date", "granularity": "month", "alias": "mois" },
  "filters": [
    { "field": "visitor_counts.count_date", "op": "gte", "value": "2025-01-01" }
  ],
  "order_by": [{ "field": "mois", "dir": "asc" }],
  "limit": 50
}
```

### 3. Top 10 des titres les plus empruntés

```json
{
  "entity": "loans",
  "joins": ["items.biblios"],
  "select": [{ "field": "biblios.title" }],
  "aggregations": [
    { "fn": "count", "field": "loans.id", "alias": "nb_emprunts" }
  ],
  "group_by": [{ "field": "biblios.title" }],
  "order_by": [{ "field": "nb_emprunts", "dir": "desc" }],
  "limit": 10
}
```

---

## Plan d'implémentation

### Phase 1 — Fondations (1 semaine)

1. `schema.rs` — le registre de schéma
2. `join_graph.rs` — résolution des jointures par graphe (DFS + `IndexMap`)
3. `query_builder.rs` — construction SQL complète (WHERE, HAVING, GROUP BY, pagination)
4. `executor.rs` — exécution + mapping PG→JSON robuste par `TypeInfo`
5. Routes Axum dans `api/stats.rs`
6. `GET /stats/schema` pour la découverte frontend

### Phase 2 — Robustesse (1 semaine)

1. `validator.rs` — validation exhaustive (champs autorisés, profondeur max de join, taille de requête)
2. `cache.rs` — cache Redis + invalidation
3. Rate limiting dédié via `tower_governor`
4. Timeout par requête via `tokio::time::timeout`
5. Table `saved_queries` + endpoints CRUD
6. Tests d'intégration (requêtes valides, invalides, timeout, cache hit/miss)

### Phase 3 — Frontend (2 semaines)

1. Composant React "Stats Builder" alimenté par `GET /stats/schema`
2. Rendu en tableau + graphiques (Recharts)
3. Support HAVING dans l'UI (filtres post-agrégation)
4. Pagination (offset/limit + total affiché)
5. Sauvegarde/chargement des requêtes favorites
6. Export CSV/Excel
