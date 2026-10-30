use crate::{
    database::{
        Database, current_timestamp, enum_value, insert_author_confidence,
        insert_creation_history_with_actor, insert_record,
    },
    research_dataset::{DatasetActor, ResearchDataset, canonical_json, parse_and_validate},
    research_records::{ActorKind, AuthorStatedConfidence, RecordActor},
};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use tauri::State;
use uuid::Uuid;

const BUNDLED_DATASET: &str = include_str!("../../datasets/research/al-taqaddum-2017.json");
#[cfg(test)]
const LEGACY_V1_DATASET: &str =
    include_str!("../../datasets/research/test-fixtures/al-taqaddum-2017-v1-legacy.json");

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ImportStatus {
    Imported,
    AlreadyImported,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportResult {
    pub dataset_id: String,
    pub dataset_version: String,
    pub status: ImportStatus,
    /// For an identical no-op this is the already-recorded sequence; no sequence is consumed.
    pub import_sequence: i64,
    pub removed_keys: Vec<String>,
}

#[tauri::command]
pub(crate) fn import_bundled_research_dataset(
    database: State<'_, Database>,
) -> Result<ImportResult, String> {
    import_bundled_dataset(&database)
}

#[derive(Clone, Debug)]
struct IncomingRecord {
    key: String,
    kind: String,
    content_snapshot_json: String,
}

#[derive(Clone, Debug)]
struct RecordPlan {
    kind: String,
    record_id: String,
    content_snapshot_json: String,
    is_new: bool,
}

#[derive(Debug)]
struct ExistingImport {
    format: String,
    format_version: i64,
    metadata_json: String,
    record_keys_json: String,
    import_sequence: i64,
}

#[derive(Debug)]
struct ExistingMapping {
    record_kind: String,
    record_id: String,
    content_snapshot_json: String,
    identity_kind: Option<String>,
}

pub(crate) fn import_bundled_dataset(database: &Database) -> Result<ImportResult, String> {
    import_dataset_json(database, BUNDLED_DATASET)
}

/// Seeds the preserved v1 fixture as pre-existing data for legacy reader and assessment tests.
#[cfg(test)]
pub(crate) fn seed_bundled_legacy_dataset_for_tests(
    database: &Database,
) -> Result<ImportResult, String> {
    let dataset = parse_and_validate(LEGACY_V1_DATASET)?;
    if dataset.format_version != 1 {
        return Err(format!(
            "Legacy test seeder requires formatVersion 1, found {}",
            dataset.format_version
        ));
    }
    let metadata_json = metadata_snapshot(&dataset)?;
    let incoming_records = incoming_records(&dataset)?;
    import_validated_dataset(database, &dataset, &metadata_json, &incoming_records)
}

fn import_dataset_json(database: &Database, input: &str) -> Result<ImportResult, String> {
    let dataset = parse_and_validate(input)?;
    dataset.validate_admission()?;
    let metadata_json = metadata_snapshot(&dataset)?;
    let incoming_records = incoming_records(&dataset)?;
    import_validated_dataset(database, &dataset, &metadata_json, &incoming_records)
}

fn import_validated_dataset(
    database: &Database,
    dataset: &ResearchDataset,
    metadata_json: &str,
    incoming_records: &[IncomingRecord],
) -> Result<ImportResult, String> {
    let mut connection = database.connection()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| format!("Could not begin dataset import: {error}"))?;

    if let Some(existing_import) =
        find_import(&transaction, &dataset.dataset_id, &dataset.dataset_version)?
    {
        verify_same_version(
            &transaction,
            dataset,
            &existing_import,
            metadata_json,
            incoming_records,
        )?;
        transaction
            .commit()
            .map_err(|error| format!("Could not finish idempotent dataset import: {error}"))?;
        return Ok(ImportResult {
            dataset_id: dataset.dataset_id.clone(),
            dataset_version: dataset.dataset_version.clone(),
            status: ImportStatus::AlreadyImported,
            import_sequence: existing_import.import_sequence,
            removed_keys: Vec::new(),
        });
    }

    let (plans, manifest) =
        resolve_record_plans(&transaction, &dataset.dataset_id, incoming_records, false)?;
    let (previous_sequence, previous_manifest) =
        previous_import(&transaction, &dataset.dataset_id)?;
    let removed_keys = removed_keys(&previous_manifest, &manifest);
    let import_sequence = previous_sequence
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| {
            format!(
                "Import sequence overflow for dataset {:?}",
                dataset.dataset_id
            )
        })?;

    transaction
        .execute(
            "INSERT INTO research_datasets (dataset_id)
             VALUES (?1)
             ON CONFLICT(dataset_id) DO NOTHING",
            [&dataset.dataset_id],
        )
        .map_err(|error| {
            format!(
                "Could not register dataset {:?}: {error}",
                dataset.dataset_id
            )
        })?;

    insert_incidents_and_sources(&transaction, dataset, &plans)?;
    insert_evidence_items(&transaction, dataset, &plans)?;
    insert_claims(&transaction, dataset, &plans)?;
    insert_discrepancies_and_findings(&transaction, dataset, &plans)?;
    insert_relationships(&transaction, dataset, &plans)?;
    insert_links(&transaction, dataset, &plans)?;

    // The history contract supports system actors, so automated imports do not imply
    // human authorship.
    for (key, plan) in &plans {
        if plan.is_new {
            insert_creation_history_with_actor(
                &transaction,
                &plan.record_id,
                &plan.kind,
                &format!(
                    "Imported from dataset {} version {} ({key})",
                    dataset.dataset_id, dataset.dataset_version
                ),
                &import_actor(),
            )?;
        }
    }

    for record in incoming_records {
        let plan = plans
            .get(&record.key)
            .ok_or_else(|| format!("Missing import plan for record {:?}", record.key))?;
        if plan.is_new {
            transaction
                .execute(
                    "INSERT INTO dataset_record_mappings (
                        dataset_id, record_key, record_kind, record_id, content_snapshot_json
                     ) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        dataset.dataset_id,
                        record.key,
                        plan.kind,
                        plan.record_id,
                        plan.content_snapshot_json
                    ],
                )
                .map_err(|error| {
                    format!(
                        "Could not save mapping for dataset record {:?}: {error}",
                        record.key
                    )
                })?;
        }
    }

    let imported_at = current_timestamp(&transaction)?;
    let record_keys_json = serde_json::to_string(&manifest)
        .map_err(|error| format!("Could not encode dataset record manifest: {error}"))?;
    transaction
        .execute(
            "INSERT INTO dataset_imports (
                dataset_id, dataset_version, import_sequence, format, format_version,
                imported_at, metadata_json, record_keys_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                dataset.dataset_id,
                dataset.dataset_version,
                import_sequence,
                dataset.format,
                i64::from(dataset.format_version),
                imported_at,
                metadata_json,
                record_keys_json,
            ],
        )
        .map_err(|error| {
            format!(
                "Could not save import ledger entry for dataset {:?} version {:?}: {error}",
                dataset.dataset_id, dataset.dataset_version
            )
        })?;

    transaction
        .commit()
        .map_err(|error| format!("Could not commit dataset import: {error}"))?;

    Ok(ImportResult {
        dataset_id: dataset.dataset_id.clone(),
        dataset_version: dataset.dataset_version.clone(),
        status: ImportStatus::Imported,
        import_sequence,
        removed_keys,
    })
}

fn metadata_snapshot(dataset: &ResearchDataset) -> Result<String, String> {
    let metadata = serde_json::json!({
        "referenceConvention": dataset.reference_convention,
        "sourceAccess": dataset.source_access,
        "sourceMetadata": dataset.source_metadata,
        "relationshipNotes": dataset.records.relationship_notes,
    });
    canonical_json(&metadata)
}

fn incoming_records(dataset: &ResearchDataset) -> Result<Vec<IncomingRecord>, String> {
    let mut keys = Vec::new();
    keys.extend(
        dataset
            .records
            .incidents
            .iter()
            .map(|record| record.key.clone()),
    );
    keys.extend(
        dataset
            .records
            .sources
            .iter()
            .map(|record| record.key.clone()),
    );
    keys.extend(
        dataset
            .records
            .evidence_items
            .iter()
            .map(|record| record.key.clone()),
    );
    keys.extend(
        dataset
            .records
            .claims
            .iter()
            .map(|record| record.key.clone()),
    );
    keys.extend(
        dataset
            .records
            .relationships
            .iter()
            .map(|record| record.key.clone()),
    );
    keys.extend(
        dataset
            .records
            .discrepancies
            .iter()
            .map(|record| record.key.clone()),
    );
    keys.extend(
        dataset
            .records
            .findings
            .iter()
            .map(|record| record.key.clone()),
    );
    keys.sort_unstable();
    if keys.windows(2).any(|window| window[0] == window[1]) {
        return Err("Validated dataset unexpectedly contains duplicate stable keys".to_string());
    }

    keys.into_iter()
        .map(|key| {
            let snapshot = dataset.canonical_record_snapshot(&key)?;
            Ok(IncomingRecord {
                key,
                kind: snapshot.record_kind,
                content_snapshot_json: snapshot.content_json,
            })
        })
        .collect()
}

fn find_import(
    transaction: &Transaction<'_>,
    dataset_id: &str,
    dataset_version: &str,
) -> Result<Option<ExistingImport>, String> {
    transaction
        .query_row(
            "SELECT format, format_version, metadata_json,
                    record_keys_json, import_sequence
             FROM dataset_imports
             WHERE dataset_id = ?1 AND dataset_version = ?2",
            params![dataset_id, dataset_version],
            |row| {
                Ok(ExistingImport {
                    format: row.get(0)?,
                    format_version: row.get(1)?,
                    metadata_json: row.get(2)?,
                    record_keys_json: row.get(3)?,
                    import_sequence: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(|error| format!("Could not inspect dataset import history: {error}"))
}

fn verify_same_version(
    transaction: &Transaction<'_>,
    dataset: &ResearchDataset,
    existing: &ExistingImport,
    incoming_metadata_json: &str,
    incoming_records: &[IncomingRecord],
) -> Result<(), String> {
    if existing.format != dataset.format
        || existing.format_version != i64::from(dataset.format_version)
    {
        return Err(format!(
            "Dataset {:?} version {:?} conflicts with its previously imported format",
            dataset.dataset_id, dataset.dataset_version
        ));
    }

    if canonical_stored_json(&existing.metadata_json, "metadata snapshot")?
        != incoming_metadata_json
    {
        return Err(format!(
            "Dataset {:?} version {:?} conflicts with its previously imported metadata",
            dataset.dataset_id, dataset.dataset_version
        ));
    }

    let stored_manifest: Vec<String> = serde_json::from_str(&existing.record_keys_json)
        .map_err(|error| format!("Stored dataset manifest is invalid: {error}"))?;
    let mut stored_manifest_sorted = stored_manifest;
    stored_manifest_sorted.sort_unstable();
    let incoming_manifest: Vec<String> = incoming_records
        .iter()
        .map(|record| record.key.clone())
        .collect();
    if stored_manifest_sorted != incoming_manifest {
        return Err(format!(
            "Dataset {:?} version {:?} conflicts with its previously imported record-key manifest",
            dataset.dataset_id, dataset.dataset_version
        ));
    }

    let plans = resolve_record_plans(transaction, &dataset.dataset_id, incoming_records, true)?.0;
    for record in incoming_records {
        let plan = plans
            .get(&record.key)
            .ok_or_else(|| format!("Missing stored mapping for record {:?}", record.key))?;
        if plan.kind != record.kind
            || canonical_stored_json(&plan.content_snapshot_json, "record snapshot")?
                != record.content_snapshot_json
        {
            return Err(format!(
                "Dataset record {:?} conflicts with its previously imported content",
                record.key
            ));
        }
    }
    Ok(())
}

fn canonical_stored_json(value: &str, description: &str) -> Result<String, String> {
    let parsed: Value = serde_json::from_str(value)
        .map_err(|error| format!("Stored {description} is invalid JSON: {error}"))?;
    if !parsed.is_object() {
        return Err(format!("Stored {description} must be a JSON object"));
    }
    canonical_json(&parsed)
}

fn resolve_record_plans(
    transaction: &Transaction<'_>,
    dataset_id: &str,
    incoming_records: &[IncomingRecord],
    require_all: bool,
) -> Result<(HashMap<String, RecordPlan>, Vec<String>), String> {
    let mut plans = HashMap::with_capacity(incoming_records.len());
    let manifest = incoming_records
        .iter()
        .map(|record| record.key.clone())
        .collect::<Vec<_>>();

    for incoming in incoming_records {
        match find_mapping(transaction, dataset_id, &incoming.key)? {
            Some(mapping) => {
                verify_mapping(transaction, &incoming.key, incoming, &mapping)?;
                plans.insert(
                    incoming.key.clone(),
                    RecordPlan {
                        kind: mapping.record_kind,
                        record_id: mapping.record_id,
                        content_snapshot_json: mapping.content_snapshot_json,
                        is_new: false,
                    },
                );
            }
            None if require_all => {
                return Err(format!(
                    "Dataset record {:?} has no mapping for its previously imported version",
                    incoming.key
                ));
            }
            None => {
                plans.insert(
                    incoming.key.clone(),
                    RecordPlan {
                        kind: incoming.kind.clone(),
                        record_id: Uuid::new_v4().hyphenated().to_string(),
                        content_snapshot_json: incoming.content_snapshot_json.clone(),
                        is_new: true,
                    },
                );
            }
        }
    }
    Ok((plans, manifest))
}

fn find_mapping(
    transaction: &Transaction<'_>,
    dataset_id: &str,
    record_key: &str,
) -> Result<Option<ExistingMapping>, String> {
    transaction
        .query_row(
            "SELECT m.record_kind, m.record_id, m.content_snapshot_json, r.record_kind
             FROM dataset_record_mappings m
             LEFT JOIN records r ON r.record_id = m.record_id
             WHERE m.dataset_id = ?1 AND m.record_key = ?2",
            params![dataset_id, record_key],
            |row| {
                Ok(ExistingMapping {
                    record_kind: row.get(0)?,
                    record_id: row.get(1)?,
                    content_snapshot_json: row.get(2)?,
                    identity_kind: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(|error| format!("Could not inspect mapping for record {record_key:?}: {error}"))
}

fn verify_mapping(
    transaction: &Transaction<'_>,
    record_key: &str,
    incoming: &IncomingRecord,
    mapping: &ExistingMapping,
) -> Result<(), String> {
    if mapping.record_kind != incoming.kind
        || mapping.identity_kind.as_deref() != Some(mapping.record_kind.as_str())
    {
        return Err(format!(
            "Dataset record {record_key:?} conflicts with its existing mapped record kind"
        ));
    }

    if canonical_stored_json(&mapping.content_snapshot_json, "record snapshot")?
        != incoming.content_snapshot_json
    {
        return Err(format!(
            "Dataset record {record_key:?} conflicts because its content has changed and cannot be overwritten"
        ));
    }

    let table = match mapping.record_kind.as_str() {
        "incident" => "incidents",
        "source" => "sources",
        "evidence_item" => "evidence_items",
        "claim" => "claims",
        "relationship" => "relationships",
        "discrepancy" => "discrepancies",
        "finding" => "findings",
        other => {
            return Err(format!(
                "Dataset record {record_key:?} has unsupported mapped kind {other:?}"
            ));
        }
    };
    let exists: bool = transaction
        .query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE record_id = ?1)"),
            [&mapping.record_id],
            |row| row.get(0),
        )
        .map_err(|error| {
            format!("Could not verify mapped domain record {record_key:?}: {error}")
        })?;
    if !exists {
        return Err(format!(
            "Dataset record {record_key:?} maps to missing {table} domain data"
        ));
    }
    Ok(())
}

fn previous_import(
    transaction: &Transaction<'_>,
    dataset_id: &str,
) -> Result<(Option<i64>, Vec<String>), String> {
    let previous: Option<(i64, String)> = transaction
        .query_row(
            "SELECT import_sequence, record_keys_json
             FROM dataset_imports
             WHERE dataset_id = ?1
             ORDER BY import_sequence DESC
             LIMIT 1",
            [dataset_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| format!("Could not inspect previous dataset import: {error}"))?;
    match previous {
        Some((sequence, manifest_json)) => {
            let manifest = serde_json::from_str(&manifest_json)
                .map_err(|error| format!("Stored previous dataset manifest is invalid: {error}"))?;
            Ok((Some(sequence), manifest))
        }
        None => Ok((None, Vec::new())),
    }
}

fn removed_keys(previous: &[String], incoming: &[String]) -> Vec<String> {
    let incoming_set = incoming.iter().collect::<HashSet<_>>();
    let mut removed = previous
        .iter()
        .filter(|key| !incoming_set.contains(key))
        .cloned()
        .collect::<Vec<_>>();
    removed.sort_unstable();
    removed.dedup();
    removed
}

fn id_for(
    plans: &HashMap<String, RecordPlan>,
    key: &str,
    expected_kind: &str,
) -> Result<String, String> {
    let plan = plans
        .get(key)
        .ok_or_else(|| format!("No resolved database identity for dataset key {key:?}"))?;
    if plan.kind != expected_kind {
        return Err(format!(
            "Dataset key {key:?} resolved as {}, expected {expected_kind}",
            plan.kind
        ));
    }
    Ok(plan.record_id.clone())
}

fn insert_domain_identity(transaction: &Transaction<'_>, plan: &RecordPlan) -> Result<(), String> {
    if plan.is_new {
        let created_at = current_timestamp(transaction)?;
        insert_record(
            transaction,
            &plan.record_id,
            &plan.kind,
            &created_at,
            &import_actor(),
        )?;
    }
    Ok(())
}

fn import_actor() -> RecordActor {
    RecordActor {
        kind: ActorKind::System,
        display_name: None,
    }
}

fn insert_incidents_and_sources(
    transaction: &Transaction<'_>,
    dataset: &ResearchDataset,
    plans: &HashMap<String, RecordPlan>,
) -> Result<(), String> {
    for incident in &dataset.records.incidents {
        let plan = &plans[&incident.key];
        if plan.is_new {
            insert_domain_identity(transaction, plan)?;
            transaction
                .execute(
                    "INSERT INTO incidents (record_id, record_kind, label)
                     VALUES (?1, 'incident', ?2)",
                    params![plan.record_id, incident.label],
                )
                .map_err(|error| {
                    format!("Could not insert incident {:?}: {error}", incident.key)
                })?;
        }
    }
    for source in &dataset.records.sources {
        let plan = &plans[&source.key];
        if plan.is_new {
            insert_domain_identity(transaction, plan)?;
            transaction
                .execute(
                    "INSERT INTO sources (
                        record_id, record_kind, source_kind, title, publisher, author,
                        publication_date, url, other_locator, verification_status
                     ) VALUES (?1, 'source', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        plan.record_id,
                        enum_value(&source.source_kind)?,
                        source.title,
                        source.publisher,
                        source.author,
                        source.publication_date,
                        source.url,
                        source.other_locator,
                        enum_value(&source.verification_status)?,
                    ],
                )
                .map_err(|error| format!("Could not insert source {:?}: {error}", source.key))?;
        }
    }
    Ok(())
}

fn insert_evidence_items(
    transaction: &Transaction<'_>,
    dataset: &ResearchDataset,
    plans: &HashMap<String, RecordPlan>,
) -> Result<(), String> {
    for evidence in &dataset.records.evidence_items {
        let plan = &plans[&evidence.key];
        if !plan.is_new {
            continue;
        }
        insert_domain_identity(transaction, plan)?;
        let source_id = id_for(plans, &evidence.source_key, "source")?;
        let locator_json = serde_json::to_string(&evidence.locator)
            .map_err(|error| format!("Could not encode locator for {:?}: {error}", evidence.key))?;
        transaction
            .execute(
                "INSERT INTO evidence_items (
                    record_id, record_kind, source_id, locator_kind, locator_json,
                    evidence_kind, description
                 ) VALUES (?1, 'evidence_item', ?2, ?3, ?4, ?5, ?6)",
                params![
                    plan.record_id,
                    source_id,
                    evidence.locator.kind(),
                    locator_json,
                    enum_value(&evidence.evidence_kind)?,
                    evidence.description,
                ],
            )
            .map_err(|error| {
                format!("Could not insert evidence item {:?}: {error}", evidence.key)
            })?;
    }
    Ok(())
}

fn insert_claims(
    transaction: &Transaction<'_>,
    dataset: &ResearchDataset,
    plans: &HashMap<String, RecordPlan>,
) -> Result<(), String> {
    for claim in &dataset.records.claims {
        let plan = &plans[&claim.key];
        if !plan.is_new {
            continue;
        }
        insert_domain_identity(transaction, plan)?;
        let source_id = id_for(plans, &claim.source_key, "source")?;
        let incident_id = claim
            .incident_key
            .as_deref()
            .map(|key| id_for(plans, key, "incident"))
            .transpose()?;
        let locator_json = serde_json::to_string(&claim.source_locator)
            .map_err(|error| format!("Could not encode claim locator {:?}: {error}", claim.key))?;
        let (reviewed_by_kind, reviewed_by_name) =
            actor_columns(claim.review.reviewed_by.as_ref())?;
        transaction
            .execute(
                "INSERT INTO claims (
                    record_id, record_kind, assertion_text, category, source_id,
                    source_locator_kind, source_locator_json, incident_id, review_status,
                    reviewed_at, reviewed_by_kind, reviewed_by_name
                 ) VALUES (?1, 'claim', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    plan.record_id,
                    claim.assertion_text,
                    enum_value(&claim.category)?,
                    source_id,
                    claim.source_locator.kind(),
                    locator_json,
                    incident_id,
                    enum_value(&claim.review.status)?,
                    claim.review.reviewed_at,
                    reviewed_by_kind,
                    reviewed_by_name,
                ],
            )
            .map_err(|error| format!("Could not insert claim {:?}: {error}", claim.key))?;

        if let Some(confidence) = &claim.author_stated_confidence {
            let source_id = id_for(plans, &confidence.source_key, "source")?;
            insert_author_confidence(
                transaction,
                &plan.record_id,
                &AuthorStatedConfidence {
                    stated_by: confidence.stated_by.clone(),
                    value_as_stated: confidence.value_as_stated.clone(),
                    source_id,
                    locator: confidence.locator.clone(),
                },
            )?;
        }
    }
    Ok(())
}

fn insert_discrepancies_and_findings(
    transaction: &Transaction<'_>,
    dataset: &ResearchDataset,
    plans: &HashMap<String, RecordPlan>,
) -> Result<(), String> {
    for discrepancy in &dataset.records.discrepancies {
        let plan = &plans[&discrepancy.key];
        if !plan.is_new {
            continue;
        }
        insert_domain_identity(transaction, plan)?;
        transaction
            .execute(
                "INSERT INTO discrepancies (record_id, record_kind, description, status)
                 VALUES (?1, 'discrepancy', ?2, ?3)",
                params![
                    plan.record_id,
                    discrepancy.description,
                    enum_value(&discrepancy.status)?
                ],
            )
            .map_err(|error| {
                format!(
                    "Could not insert discrepancy {:?}: {error}",
                    discrepancy.key
                )
            })?;
    }
    for finding in &dataset.records.findings {
        let plan = &plans[&finding.key];
        if !plan.is_new {
            continue;
        }
        insert_domain_identity(transaction, plan)?;
        let (reviewed_by_kind, reviewed_by_name) =
            actor_columns(finding.review.reviewed_by.as_ref())?;
        transaction
            .execute(
                "INSERT INTO findings (
                    record_id, record_kind, assessment, attribution_kind, attribution_name,
                    review_status, reviewed_at, reviewed_by_kind, reviewed_by_name
                 ) VALUES (?1, 'finding', ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    plan.record_id,
                    finding.assessment,
                    enum_value(&finding.attribution.kind)?,
                    finding.attribution.display_name,
                    enum_value(&finding.review.status)?,
                    finding.review.reviewed_at,
                    reviewed_by_kind,
                    reviewed_by_name,
                ],
            )
            .map_err(|error| format!("Could not insert finding {:?}: {error}", finding.key))?;
    }
    Ok(())
}

fn actor_columns(actor: Option<&DatasetActor>) -> Result<(Option<String>, Option<String>), String> {
    actor
        .map(|actor| Ok((Some(enum_value(&actor.kind)?), actor.display_name.clone())))
        .unwrap_or(Ok((None, None)))
}

fn insert_relationships(
    transaction: &Transaction<'_>,
    dataset: &ResearchDataset,
    plans: &HashMap<String, RecordPlan>,
) -> Result<(), String> {
    // Register all relationship identities first so relationships may reference one another.
    for relationship in &dataset.records.relationships {
        let plan = &plans[&relationship.key];
        insert_domain_identity(transaction, plan)?;
    }
    for relationship in &dataset.records.relationships {
        let plan = &plans[&relationship.key];
        if !plan.is_new {
            continue;
        }
        let from_kind = enum_value(&relationship.from.record_kind)?;
        let to_kind = enum_value(&relationship.to.record_kind)?;
        let from_id = id_for(plans, &relationship.from.record_key, &from_kind)?;
        let to_id = id_for(plans, &relationship.to.record_key, &to_kind)?;
        transaction
            .execute(
                "INSERT INTO relationships (
                    record_id, record_kind, relationship_kind,
                    from_record_id, from_record_kind, to_record_id, to_record_kind
                 ) VALUES (?1, 'relationship', ?2, ?3, ?4, ?5, ?6)",
                params![
                    plan.record_id,
                    enum_value(&relationship.kind)?,
                    from_id,
                    from_kind,
                    to_id,
                    to_kind,
                ],
            )
            .map_err(|error| {
                format!(
                    "Could not insert relationship {:?}: {error}",
                    relationship.key
                )
            })?;
    }
    Ok(())
}

fn insert_links(
    transaction: &Transaction<'_>,
    dataset: &ResearchDataset,
    plans: &HashMap<String, RecordPlan>,
) -> Result<(), String> {
    for discrepancy in &dataset.records.discrepancies {
        if !plans[&discrepancy.key].is_new {
            continue;
        }
        let discrepancy_id = id_for(plans, &discrepancy.key, "discrepancy")?;
        for claim_key in &discrepancy.claim_keys {
            let claim_id = id_for(plans, claim_key, "claim")?;
            transaction
                .execute(
                    "INSERT INTO discrepancy_claims (discrepancy_id, claim_id)
                     VALUES (?1, ?2)",
                    params![discrepancy_id, claim_id],
                )
                .map_err(|error| {
                    format!(
                        "Could not link claim {:?} to discrepancy {:?}: {error}",
                        claim_key, discrepancy.key
                    )
                })?;
        }
    }
    for finding in &dataset.records.findings {
        if !plans[&finding.key].is_new {
            continue;
        }
        let finding_id = id_for(plans, &finding.key, "finding")?;
        for reference in &finding.claim_references {
            let claim_id = id_for(plans, &reference.record_key, "claim")?;
            transaction
                .execute(
                    "INSERT INTO finding_claim_references (finding_id, claim_id, reference_role)
                     VALUES (?1, ?2, ?3)",
                    params![finding_id, claim_id, enum_value(&reference.role)?],
                )
                .map_err(|error| {
                    format!(
                        "Could not link claim {:?} to finding {:?}: {error}",
                        reference.record_key, finding.key
                    )
                })?;
        }
        for reference in &finding.evidence_references {
            let evidence_id = id_for(plans, &reference.record_key, "evidence_item")?;
            transaction
                .execute(
                    "INSERT INTO finding_evidence_references (
                        finding_id, evidence_item_id, reference_role
                     ) VALUES (?1, ?2, ?3)",
                    params![finding_id, evidence_id, enum_value(&reference.role)?],
                )
                .map_err(|error| {
                    format!(
                        "Could not link evidence {:?} to finding {:?}: {error}",
                        reference.record_key, finding.key
                    )
                })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = BUNDLED_DATASET;

    fn fixture_value() -> Value {
        serde_json::from_str(FIXTURE).expect("parse actual bundled fixture")
    }

    fn add_admission_fields(incident: &mut Value, source_slug: &str) {
        let dataset_id = "xenoscope.case.al-taqaddum-2017";
        incident["description"] =
            serde_json::json!("An object was reported over the base and recorded by a sensor.");
        incident["location"] = serde_json::json!({
            "description": "Near Al Taqaddum Air Base, Iraq",
            "precision": "approximate_area",
            "originalWording": "at Al Taqaddum Air Base, Iraq"
        });
        incident["timePeriod"] = serde_json::json!({
            "startDate": "2017-10-23",
            "endDate": "2017-10-23",
            "precision": "exact_day"
        });
        incident["sourceKeys"] = serde_json::json!([format!("{dataset_id}:source:{source_slug}")]);
    }

    fn write_dataset(value: &Value) -> String {
        serde_json::to_string(value).expect("serialize test dataset")
    }

    fn import_value(database: &Database, value: &Value) -> Result<ImportResult, String> {
        import_dataset_json(database, &write_dataset(value))
    }

    #[test]
    fn import_result_serializes_status_and_sequence_fields_for_tauri() {
        let imported = ImportResult {
            dataset_id: "dataset.example".to_string(),
            dataset_version: "v2".to_string(),
            status: ImportStatus::Imported,
            import_sequence: 3,
            removed_keys: vec!["old:key".to_string()],
        };
        let imported_json = serde_json::to_value(imported).expect("serialize import result");
        assert_eq!(imported_json["datasetId"], "dataset.example");
        assert_eq!(imported_json["datasetVersion"], "v2");
        assert_eq!(imported_json["status"], "imported");
        assert_eq!(imported_json["importSequence"], 3);
        assert_eq!(imported_json["removedKeys"][0], "old:key");

        let no_op = ImportResult {
            dataset_id: "dataset.example".to_string(),
            dataset_version: "v2".to_string(),
            status: ImportStatus::AlreadyImported,
            import_sequence: 3,
            removed_keys: Vec::new(),
        };
        let no_op_json = serde_json::to_value(no_op).expect("serialize no-op import result");
        assert_eq!(no_op_json["status"], "already_imported");
        assert_eq!(no_op_json["importSequence"], 3);
    }

    fn set_version(value: &mut Value, version: &str) {
        value["datasetVersion"] = Value::String(version.to_string());
    }

    fn add_test_finding(value: &mut Value) {
        let claim_key = value["records"]["claims"][0]["key"]
            .as_str()
            .expect("fixture claim key")
            .to_string();
        let evidence_key = value["records"]["evidenceItems"][0]["key"]
            .as_str()
            .expect("fixture evidence key")
            .to_string();
        value["records"]["findings"] = serde_json::json!([{
            "key": "xenoscope.case.al-taqaddum-2017:finding:importer-test-only",
            "assessment": "Test-only finding used to exercise finding persistence.",
            "attribution": { "kind": "system", "displayName": "Importer test" },
            "claimReferences": [{ "recordKey": claim_key, "role": "supporting" }],
            "evidenceReferences": [{ "recordKey": evidence_key, "role": "conflicting" }],
            "review": { "status": "unreviewed" }
        }]);
    }

    fn table_counts(database: &Database) -> Vec<(&'static str, i64)> {
        const TABLES: &[&str] = &[
            "research_datasets",
            "dataset_imports",
            "dataset_record_mappings",
            "records",
            "incidents",
            "sources",
            "evidence_items",
            "claims",
            "claim_author_confidence",
            "relationships",
            "discrepancies",
            "discrepancy_claims",
            "findings",
            "finding_claim_references",
            "finding_evidence_references",
            "record_history",
            "history_related_records",
        ];
        let connection = database.connection().expect("lock test database");
        TABLES
            .iter()
            .map(|table| {
                let count = connection
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get(0)
                    })
                    .expect("count database table");
                (*table, count)
            })
            .collect()
    }

    fn scalar(database: &Database, sql: &str) -> i64 {
        database
            .connection()
            .expect("lock test database")
            .query_row(sql, [], |row| row.get(0))
            .expect("query scalar")
    }

    fn mapping_id(database: &Database, record_key: &str) -> String {
        database
            .connection()
            .expect("lock test database")
            .query_row(
                "SELECT record_id FROM dataset_record_mappings WHERE record_key = ?1",
                [record_key],
                |row| row.get(0),
            )
            .expect("read stable-key mapping")
    }

    fn import_fixture(database: &Database) -> ImportResult {
        import_value(database, &fixture_value()).expect("import fixture with admission fields")
    }

    #[test]
    fn imports_actual_fixture_and_preserves_every_supported_domain_type_and_link() {
        let database = Database::in_memory().expect("initialize isolated database");
        let mut fixture = fixture_value();
        add_test_finding(&mut fixture);
        let result = import_value(&database, &fixture).expect("import fixture with test finding");

        assert_eq!(result.status, ImportStatus::Imported);
        assert_eq!(result.import_sequence, 1);
        assert!(result.removed_keys.is_empty());
        assert_eq!(scalar(&database, "SELECT COUNT(*) FROM incidents"), 1);
        assert_eq!(scalar(&database, "SELECT COUNT(*) FROM sources"), 2);
        assert_eq!(scalar(&database, "SELECT COUNT(*) FROM evidence_items"), 1);
        assert_eq!(scalar(&database, "SELECT COUNT(*) FROM claims"), 10);
        assert!(
            scalar(&database, "SELECT COUNT(*) FROM claim_author_confidence") > 0,
            "fixture confidence references should be imported"
        );
        assert_eq!(scalar(&database, "SELECT COUNT(*) FROM relationships"), 2);
        assert_eq!(scalar(&database, "SELECT COUNT(*) FROM discrepancies"), 2);
        assert_eq!(scalar(&database, "SELECT COUNT(*) FROM findings"), 1);
        assert_eq!(
            scalar(
                &database,
                "SELECT COUNT(*) FROM relationships rel
                 JOIN records src ON src.record_id = rel.from_record_id
                 JOIN records dst ON dst.record_id = rel.to_record_id
                 WHERE src.record_kind != rel.from_record_kind
                    OR dst.record_kind != rel.to_record_kind"
            ),
            0
        );
        assert_eq!(
            scalar(&database, "SELECT COUNT(*) FROM discrepancy_claims"),
            4
        );
        assert_eq!(
            scalar(&database, "SELECT COUNT(*) FROM finding_claim_references"),
            1
        );
        assert_eq!(
            scalar(
                &database,
                "SELECT COUNT(*) FROM finding_evidence_references"
            ),
            1
        );
        assert_eq!(
            scalar(
                &database,
                "SELECT COUNT(*) FROM record_history WHERE event_kind = 'record_created'"
            ),
            19
        );
        assert_eq!(
            scalar(&database, "SELECT COUNT(*) FROM dataset_record_mappings"),
            19
        );
        assert_eq!(
            scalar(
                &database,
                "SELECT COUNT(*) FROM records WHERE created_by_kind = 'system'"
            ),
            38
        );

        let connection = database.connection().expect("lock test database");
        let (metadata_json, manifest_json, sequence): (String, String, i64) = connection
            .query_row(
                "SELECT metadata_json, record_keys_json, import_sequence FROM dataset_imports",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("read ledger snapshot");
        let metadata: Value = serde_json::from_str(&metadata_json).expect("parse metadata");
        assert_eq!(metadata["relationshipNotes"].as_array().unwrap().len(), 2);
        assert_eq!(metadata["sourceAccess"].as_array().unwrap().len(), 2);
        let manifest: Vec<String> =
            serde_json::from_str(&manifest_json).expect("parse sorted manifest");
        assert!(manifest.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(sequence, 1);
    }

    #[test]
    fn imports_bundled_v2_fixture_and_is_idempotent() {
        let database = Database::in_memory().expect("initialize isolated database");
        let imported = import_bundled_dataset(&database).expect("import bundled v2 fixture");
        assert_eq!(imported.dataset_id, "xenoscope.case.al-taqaddum-2017");
        assert_eq!(imported.dataset_version, "2.0.0");
        assert_eq!(imported.status, ImportStatus::Imported);
        assert_eq!(imported.import_sequence, 1);

        let connection = database.connection().expect("lock test database");
        let (format_version, manifest_json): (i64, String) = connection
            .query_row(
                "SELECT format_version, record_keys_json
                 FROM dataset_imports
                 WHERE dataset_id = ?1 AND dataset_version = ?2",
                params![imported.dataset_id, imported.dataset_version],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read bundled v2 import ledger");
        assert_eq!(format_version, 2);
        let manifest: Vec<String> =
            serde_json::from_str(&manifest_json).expect("parse v2 manifest");
        assert!(manifest.contains(
            &"xenoscope.case.al-taqaddum-2017:incident:al-taqaddum-object-report-v2".to_string()
        ));
        drop(connection);

        let before = table_counts(&database);
        let repeated = import_bundled_dataset(&database).expect("repeat bundled import");
        assert_eq!(repeated.status, ImportStatus::AlreadyImported);
        assert_eq!(repeated.import_sequence, imported.import_sequence);
        assert_eq!(table_counts(&database), before);
    }

    #[test]
    fn rejects_inadmissible_legacy_v1_import_before_any_database_writes() {
        let database = Database::in_memory().expect("initialize isolated database");
        let before = table_counts(&database);
        assert!(
            import_dataset_json(&database, LEGACY_V1_DATASET)
                .unwrap_err()
                .contains("description is missing")
        );
        assert_eq!(table_counts(&database), before);
    }

    #[test]
    fn v1_to_v2_upgrade_preserves_unchanged_mappings_and_selects_v2_manifest() {
        let database = Database::in_memory().expect("initialize isolated database");
        let legacy =
            seed_bundled_legacy_dataset_for_tests(&database).expect("seed historical v1 dataset");
        assert_eq!(legacy.dataset_version, "1.0.0");

        let unchanged_keys = [
            "xenoscope.case.al-taqaddum-2017:source:aaro-report",
            "xenoscope.case.al-taqaddum-2017:source:dvids-record",
            "xenoscope.case.al-taqaddum-2017:evidence:dvids-video-960331",
        ];
        let historical_keys = [
            "xenoscope.case.al-taqaddum-2017:incident:al-taqaddum-object-report",
            "xenoscope.case.al-taqaddum-2017:claim:aaro-event-date",
        ];
        let before = unchanged_keys
            .iter()
            .map(|key| (key.to_string(), mapping_snapshot(&database, key)))
            .collect::<HashMap<_, _>>();
        let historical_before = historical_keys
            .iter()
            .map(|key| (key.to_string(), mapping_snapshot(&database, key)))
            .collect::<HashMap<_, _>>();

        let upgraded = import_bundled_dataset(&database).expect("import v2 upgrade");
        assert_eq!(upgraded.dataset_version, "2.0.0");
        assert_eq!(upgraded.status, ImportStatus::Imported);
        assert_eq!(upgraded.import_sequence, 2);
        assert_eq!(upgraded.removed_keys.len(), 14);
        for key in unchanged_keys {
            assert_eq!(mapping_snapshot(&database, key), before[key]);
        }
        for key in historical_keys {
            assert_eq!(mapping_snapshot(&database, key), historical_before[key]);
        }
        let old_incident_id = &historical_before
            ["xenoscope.case.al-taqaddum-2017:incident:al-taqaddum-object-report"]
            .0;
        let new_incident_id = mapping_snapshot(
            &database,
            "xenoscope.case.al-taqaddum-2017:incident:al-taqaddum-object-report-v2",
        )
        .0;
        assert_ne!(old_incident_id, &new_incident_id);
        for suffix in [
            "claim:aaro-event-date",
            "claim:aaro-overview-duration",
            "claim:aaro-balloon-assessment",
            "claim:aaro-no-anomalous-behavior",
            "claim:aaro-assessment-basis",
            "claim:aaro-video-stills-caption",
            "claim:dvids-date-taken",
            "claim:dvids-recording-description",
            "claim:dvids-listed-duration",
            "claim:dvids-attributed-balloon-assessment",
            "relationship:dvids-repeats-aaro-balloon-assessment",
            "discrepancy:event-date",
            "discrepancy:recording-duration",
        ] {
            let old_key = format!("xenoscope.case.al-taqaddum-2017:{suffix}");
            let new_key = format!("{old_key}-v2");
            let old_id = mapping_snapshot(&database, &old_key).0;
            let new_id = mapping_snapshot(&database, &new_key).0;
            assert_ne!(
                old_id, new_id,
                "changed record key {suffix} gets a new identity"
            );
        }
        for removed_key in &upgraded.removed_keys {
            mapping_snapshot(&database, removed_key);
            let retained_domain_record: bool = database
                .connection()
                .expect("lock test database")
                .query_row(
                    "SELECT EXISTS(
                        SELECT 1 FROM dataset_record_mappings m
                        JOIN records r ON r.record_id = m.record_id
                        WHERE m.dataset_id = ?1 AND m.record_key = ?2
                    )",
                    params!["xenoscope.case.al-taqaddum-2017", removed_key],
                    |row| row.get(0),
                )
                .expect("verify historical mapping and record remain");
            assert!(
                retained_domain_record,
                "historical record {removed_key} remains"
            );
        }

        let connection = database.connection().expect("lock test database");
        let (format_version, latest_manifest_json, import_count): (i64, String, i64) = connection
            .query_row(
                "SELECT format_version, record_keys_json,
                       (SELECT COUNT(*) FROM dataset_imports WHERE dataset_id = ?1)
                 FROM dataset_imports WHERE dataset_id = ?1
                 ORDER BY import_sequence DESC LIMIT 1",
                ["xenoscope.case.al-taqaddum-2017"],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("read latest import ledger entry");
        assert_eq!(format_version, 2);
        assert_eq!(import_count, 2);
        let latest_manifest: Vec<String> =
            serde_json::from_str(&latest_manifest_json).expect("parse latest manifest");
        let expected_manifest = incoming_records(
            &parse_and_validate(BUNDLED_DATASET).expect("parse bundled v2 dataset"),
        )
        .expect("build expected v2 manifest")
        .into_iter()
        .map(|record| record.key)
        .collect::<Vec<_>>();
        assert_eq!(latest_manifest, expected_manifest);
        assert!(!latest_manifest.contains(
            &"xenoscope.case.al-taqaddum-2017:incident:al-taqaddum-object-report".to_string()
        ));
        assert!(latest_manifest.contains(
            &"xenoscope.case.al-taqaddum-2017:incident:al-taqaddum-object-report-v2".to_string()
        ));
    }

    fn mapping_snapshot(database: &Database, record_key: &str) -> (String, String) {
        database
            .connection()
            .expect("lock test database")
            .query_row(
                "SELECT record_id, content_snapshot_json
                 FROM dataset_record_mappings
                 WHERE dataset_id = ?1 AND record_key = ?2",
                params!["xenoscope.case.al-taqaddum-2017", record_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read stable mapping and content snapshot")
    }

    #[test]
    fn identical_repeat_is_a_read_only_no_op_with_existing_sequence() {
        let database = Database::in_memory().expect("initialize isolated database");
        let first = import_fixture(&database);
        let before = table_counts(&database);
        let repeat = import_fixture(&database);

        assert_eq!(repeat.status, ImportStatus::AlreadyImported);
        assert_eq!(repeat.import_sequence, first.import_sequence);
        assert!(repeat.removed_keys.is_empty());
        assert_eq!(table_counts(&database), before);
    }

    #[test]
    fn same_version_metadata_manifest_and_record_changes_conflict() {
        for mutation in ["metadata", "manifest", "content"] {
            let database = Database::in_memory().expect("initialize isolated database");
            import_fixture(&database);
            let mut value = fixture_value();
            match mutation {
                "metadata" => {
                    value["referenceConvention"]["references"] =
                        Value::String("Changed metadata".to_string());
                }
                "manifest" => {
                    value["records"]["incidents"]
                        .as_array_mut()
                        .unwrap()
                        .push(serde_json::json!({
                            "key": "xenoscope.case.al-taqaddum-2017:incident:new-key",
                            "label": "New test record"
                        }));
                    add_admission_fields(
                        value["records"]["incidents"]
                            .as_array_mut()
                            .unwrap()
                            .last_mut()
                            .unwrap(),
                        "aaro-report",
                    );
                }
                "content" => {
                    value["records"]["incidents"][0]["label"] =
                        Value::String("Changed record content".to_string());
                }
                _ => unreachable!(),
            }
            let before = table_counts(&database);
            assert!(
                import_value(&database, &value)
                    .unwrap_err()
                    .contains("conflicts"),
                "{mutation} mutation should conflict"
            );
            assert_eq!(table_counts(&database), before);
        }
    }

    #[test]
    fn same_version_format_fields_conflict_with_corrupt_prior_ledger() {
        for (column, value) in [("format", "'other-format'"), ("format_version", "1")] {
            let database = Database::in_memory().expect("initialize isolated database");
            import_fixture(&database);
            {
                let connection = database.connection().expect("lock test database");
                connection
                    .execute_batch("PRAGMA ignore_check_constraints = ON;")
                    .expect("temporarily allow corrupt test ledger");
                connection
                    .execute(
                        &format!("UPDATE dataset_imports SET {column} = {value}"),
                        [],
                    )
                    .expect("corrupt stored format field");
                connection
                    .execute_batch("PRAGMA ignore_check_constraints = OFF;")
                    .expect("restore constraint checking");
            }
            assert!(
                import_value(&database, &fixture_value())
                    .unwrap_err()
                    .contains("previously imported format")
            );
        }
    }

    #[test]
    fn new_version_allows_metadata_and_new_keys_and_uses_sequence_not_label_order() {
        let database = Database::in_memory().expect("initialize isolated database");
        let first = import_fixture(&database);
        let stable_key = "xenoscope.case.al-taqaddum-2017:source:aaro-report";
        let first_stable_id = mapping_id(&database, stable_key);
        let mut later = fixture_value();
        set_version(&mut later, "9.0.0");
        later["referenceConvention"]["references"] =
            Value::String("Updated version metadata".to_string());
        later["records"]["incidents"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "key": "xenoscope.case.al-taqaddum-2017:incident:added-later",
                "label": "Additional imported record"
            }));
        add_admission_fields(
            later["records"]["incidents"]
                .as_array_mut()
                .unwrap()
                .last_mut()
                .unwrap(),
            "aaro-report",
        );
        let second = import_value(&database, &later).expect("import second version");
        assert_eq!(second.import_sequence, first.import_sequence + 1);
        assert!(second.removed_keys.is_empty());
        assert_eq!(mapping_id(&database, stable_key), first_stable_id);
        let new_key = "xenoscope.case.al-taqaddum-2017:incident:added-later";
        let new_id = mapping_id(&database, new_key);
        assert_ne!(new_id, first_stable_id);
        assert_eq!(
            Uuid::parse_str(&new_id)
                .expect("new mapping uses a UUID")
                .get_version_num(),
            4
        );
        assert_eq!(scalar(&database, "SELECT COUNT(*) FROM incidents"), 2);
        assert_eq!(mapping_id(&database, stable_key), first_stable_id);

        let mut lexical_earlier = fixture_value();
        set_version(&mut lexical_earlier, "0.5.0");
        let third =
            import_value(&database, &lexical_earlier).expect("import lexical-earlier label");
        assert_eq!(third.import_sequence, second.import_sequence + 1);
        assert_eq!(
            scalar(
                &database,
                "SELECT COUNT(*) FROM dataset_imports WHERE import_sequence IN (1, 2, 3)"
            ),
            3
        );
        assert_eq!(scalar(&database, "SELECT COUNT(*) FROM incidents"), 2);
    }

    #[test]
    fn changed_existing_record_and_mapping_kind_mismatch_are_rejected() {
        let database = Database::in_memory().expect("initialize isolated database");
        import_fixture(&database);
        let mut changed = fixture_value();
        set_version(&mut changed, "2.0.0");
        changed["records"]["incidents"][0]["label"] =
            Value::String("Changed existing content".to_string());
        assert!(
            import_value(&database, &changed)
                .unwrap_err()
                .contains("content has changed")
        );

        {
            let connection = database.connection().expect("lock test database");
            connection
                .execute_batch("PRAGMA foreign_keys = OFF;")
                .expect("temporarily disable FK for corruption test");
            connection
                .execute(
                    "UPDATE dataset_record_mappings SET record_kind = 'incident'
                     WHERE record_key = ?1",
                    ["xenoscope.case.al-taqaddum-2017:source:aaro-report"],
                )
                .expect("corrupt mapping kind");
            connection
                .execute_batch("PRAGMA foreign_keys = ON;")
                .expect("restore FK enforcement");
        }
        let mut later = fixture_value();
        set_version(&mut later, "3.0.0");
        assert!(
            import_value(&database, &later)
                .unwrap_err()
                .contains("mapped record kind")
        );
    }

    #[test]
    fn missing_mapped_domain_record_is_rejected() {
        let database = Database::in_memory().expect("initialize isolated database");
        import_fixture(&database);
        database
            .connection()
            .expect("lock test database")
            .execute(
                "DELETE FROM evidence_items WHERE record_id = (
                    SELECT record_id FROM dataset_record_mappings
                    WHERE record_key = ?1
                 )",
                ["xenoscope.case.al-taqaddum-2017:evidence:dvids-video-960331"],
            )
            .expect("remove test-only domain row while retaining mapping");
        assert!(
            import_value(&database, &fixture_value())
                .unwrap_err()
                .contains("missing evidence_items domain data")
        );
    }

    #[test]
    fn removed_keys_are_reported_but_records_and_mappings_remain() {
        let database = Database::in_memory().expect("initialize isolated database");
        import_fixture(&database);
        let removed_key = "xenoscope.case.al-taqaddum-2017:evidence:dvids-video-960331";
        let mut later = fixture_value();
        set_version(&mut later, "later-version");
        later["records"]["evidenceItems"] = serde_json::json!([]);

        let result = import_value(&database, &later).expect("import version without evidence");
        assert_eq!(result.removed_keys, vec![removed_key.to_string()]);
        let connection = database.connection().expect("lock test database");
        let (mapping_exists, domain_record_exists): (bool, bool) = connection
            .query_row(
                "SELECT
                    EXISTS(SELECT 1 FROM dataset_record_mappings WHERE record_key = ?1),
                    EXISTS(SELECT 1 FROM records r JOIN evidence_items e
                           ON e.record_id = r.record_id WHERE r.record_id = (
                               SELECT record_id FROM dataset_record_mappings WHERE record_key = ?1
                           ))",
                [removed_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("check removed key retention");
        assert!(mapping_exists);
        assert!(domain_record_exists);
    }

    #[test]
    fn invalid_dataset_is_rejected_before_any_database_writes() {
        let database = Database::in_memory().expect("initialize isolated database");
        let before = table_counts(&database);
        let mut invalid = fixture_value();
        invalid["records"]["claims"][0]["sourceKey"] =
            Value::String("xenoscope.case.al-taqaddum-2017:source:missing".to_string());
        assert!(
            import_value(&database, &invalid)
                .unwrap_err()
                .contains("does not exist")
        );
        assert_eq!(table_counts(&database), before);
    }

    #[test]
    fn admission_rejects_entire_dataset_before_any_database_writes() {
        let database = Database::in_memory().expect("initialize isolated database");
        let before = table_counts(&database);
        let mut input = fixture_value();
        let mut rejected_incident = serde_json::json!({
            "key": "xenoscope.case.al-taqaddum-2017:incident:insufficient-detail",
            "label": "Insufficiently identified test report"
        });
        add_admission_fields(&mut rejected_incident, "aaro-report");
        rejected_incident["location"]["description"] = serde_json::json!("Unknown location");
        input["records"]["incidents"]
            .as_array_mut()
            .unwrap()
            .push(rejected_incident);

        assert!(
            import_value(&database, &input)
                .unwrap_err()
                .contains("location is empty, generic, or only identifies a country")
        );
        assert_eq!(table_counts(&database), before);
    }

    #[test]
    fn rejected_later_import_preserves_existing_records_and_ledger() {
        let database = Database::in_memory().expect("initialize isolated database");
        import_fixture(&database);
        let before = table_counts(&database);
        let mut later = fixture_value();
        set_version(&mut later, "2.0.0");
        let mut rejected_incident = serde_json::json!({
            "key": "xenoscope.case.al-taqaddum-2017:incident:untraceable-source",
            "label": "Test incident with untraceable source"
        });
        add_admission_fields(&mut rejected_incident, "aaro-report");
        later["records"]["incidents"]
            .as_array_mut()
            .unwrap()
            .push(rejected_incident);
        later["records"]["incidents"][0]["sourceKeys"] =
            serde_json::json!(["xenoscope.case.al-taqaddum-2017:source:dvids-record"]);
        later["records"]["incidents"][1]["sourceKeys"] =
            serde_json::json!(["xenoscope.case.al-taqaddum-2017:source:aaro-report"]);
        later["records"]["sources"][0]["url"] = Value::Null;
        later["records"]["sources"][0]["title"] = Value::Null;
        later["records"]["sources"][0]["publisher"] = Value::Null;
        later["records"]["sources"][0]["author"] = Value::Null;
        later["records"]["sources"][0]["publicationDate"] = Value::Null;
        later["records"]["sources"][0]["otherLocator"] = Value::Null;

        assert!(
            import_value(&database, &later)
                .unwrap_err()
                .contains("no referenced source has sufficient identifying information")
        );
        assert_eq!(table_counts(&database), before);
    }

    #[test]
    fn final_ledger_failure_rolls_back_every_table_and_does_not_consume_sequence() {
        let database = Database::in_memory().expect("initialize isolated database");
        let before = table_counts(&database);
        database
            .connection()
            .expect("lock test database")
            .execute_batch(
                "CREATE TRIGGER reject_test_import
                 BEFORE INSERT ON dataset_imports
                 BEGIN
                     SELECT RAISE(ABORT, 'test late ledger failure');
                 END;",
            )
            .expect("install final-write failure trigger");

        assert!(
            import_value(&database, &fixture_value())
                .unwrap_err()
                .contains("test late ledger failure")
        );
        assert_eq!(table_counts(&database), before);

        database
            .connection()
            .expect("lock test database")
            .execute_batch("DROP TRIGGER reject_test_import;")
            .expect("remove failure trigger");
        let result = import_fixture(&database);
        assert_eq!(result.status, ImportStatus::Imported);
        assert_eq!(result.import_sequence, 1);
    }
}
