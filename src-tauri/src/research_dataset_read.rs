use crate::{
    database::Database,
    research_dataset::{
        DatasetAuthorStatedConfidence, DatasetLocation, DatasetRecordKind, DatasetRecordReference,
        DatasetReviewMetadata, DatasetTimePeriod, DiscrepancyStatus, EvidenceKind,
        ReferenceConvention, RelationshipKind, RelationshipNote, SourceAccess, SourceMetadata,
    },
    research_records::{
        ClaimCategory, RecordActor, ReviewMetadata, SourceKind, SourceLocator,
        SourceVerificationStatus,
    },
};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::{HashMap, HashSet};
use tauri::State;

const BUNDLED_DATASET_ID: &str = "xenoscope.case.al-taqaddum-2017";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResearchDatasetMetadataDto {
    pub reference_convention: ReferenceConvention,
    pub source_access: Vec<SourceAccess>,
    pub source_metadata: Vec<SourceMetadata>,
    pub relationship_notes: Vec<RelationshipNote>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CaseRecordReferenceDto {
    pub dataset_key: String,
    pub record_id: String,
    pub record_kind: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CaseRecordMetadataDto {
    pub id: String,
    pub dataset_key: String,
    pub record_kind: String,
    pub created_at: String,
    pub created_by: RecordActor,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResearchIncidentDto {
    pub record: CaseRecordMetadataDto,
    pub label: String,
    pub description: Option<String>,
    pub location: Option<DatasetLocation>,
    pub time_period: Option<DatasetTimePeriod>,
    pub sources: Option<Vec<ResearchSourceDto>>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResearchSourceDto {
    pub record: CaseRecordMetadataDto,
    pub source_kind: SourceKind,
    pub title: Option<String>,
    pub publisher: Option<String>,
    pub author: Option<String>,
    pub publication_date: Option<String>,
    pub url: Option<String>,
    pub other_locator: Option<String>,
    pub verification_status: SourceVerificationStatus,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResearchEvidenceItemDto {
    pub record: CaseRecordMetadataDto,
    pub source: CaseRecordReferenceDto,
    pub locator: SourceLocator,
    pub evidence_kind: crate::research_dataset::EvidenceKind,
    pub description: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResearchClaimDto {
    pub record: CaseRecordMetadataDto,
    pub assertion_text: String,
    pub category: ClaimCategory,
    pub source: CaseRecordReferenceDto,
    pub source_locator: SourceLocator,
    pub incident: Option<CaseRecordReferenceDto>,
    pub author_stated_confidence: Option<CaseAuthorStatedConfidenceDto>,
    pub review: ReviewMetadata,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CaseAuthorStatedConfidenceDto {
    pub stated_by: String,
    pub value_as_stated: String,
    pub source: CaseRecordReferenceDto,
    pub locator: Option<SourceLocator>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResearchRelationshipDto {
    pub record: CaseRecordMetadataDto,
    pub relationship_kind: RelationshipKind,
    pub from: CaseRecordReferenceDto,
    pub to: CaseRecordReferenceDto,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResearchDiscrepancyDto {
    pub record: CaseRecordMetadataDto,
    pub claim_references: Vec<CaseRecordReferenceDto>,
    pub description: String,
    pub status: DiscrepancyStatus,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CaseFindingReferenceDto {
    pub record: CaseRecordReferenceDto,
    pub role: crate::research_dataset::FindingReferenceRole,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResearchFindingDto {
    pub record: CaseRecordMetadataDto,
    pub assessment: String,
    pub attribution: RecordActor,
    pub claim_references: Vec<CaseFindingReferenceDto>,
    pub evidence_references: Vec<CaseFindingReferenceDto>,
    pub review: ReviewMetadata,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportedResearchCaseDto {
    pub dataset_id: String,
    pub dataset_version: String,
    pub import_sequence: i64,
    pub imported_at: String,
    pub metadata: ResearchDatasetMetadataDto,
    pub incidents: Vec<ResearchIncidentDto>,
    pub sources: Vec<ResearchSourceDto>,
    pub evidence_items: Vec<ResearchEvidenceItemDto>,
    pub claims: Vec<ResearchClaimDto>,
    pub relationships: Vec<ResearchRelationshipDto>,
    pub discrepancies: Vec<ResearchDiscrepancyDto>,
    pub findings: Vec<ResearchFindingDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResearchCaseRecordCountsDto {
    pub incidents: usize,
    pub sources: usize,
    pub evidence_items: usize,
    pub claims: usize,
    pub relationships: usize,
    pub discrepancies: usize,
    pub findings: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResearchCaseDiscrepancyCountsDto {
    pub open: usize,
    pub investigating: usize,
    pub documented: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResearchCaseListEntryDto {
    pub dataset_id: String,
    pub dataset_version: String,
    pub import_sequence: i64,
    pub imported_at: String,
    pub incident_labels: Vec<String>,
    pub record_counts: ResearchCaseRecordCountsDto,
    pub discrepancy_status_counts: ResearchCaseDiscrepancyCountsDto,
}

#[derive(Clone, Debug)]
struct StableRecord {
    dataset_key: String,
    record_id: String,
    record_kind: String,
    content_snapshot_json: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IncidentContentSnapshot {
    label: String,
    description: Option<String>,
    location: Option<DatasetLocation>,
    time_period: Option<DatasetTimePeriod>,
    source_keys: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceContentSnapshot {
    source_kind: SourceKind,
    title: Option<String>,
    publisher: Option<String>,
    author: Option<String>,
    publication_date: Option<String>,
    url: Option<String>,
    other_locator: Option<String>,
    verification_status: SourceVerificationStatus,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ClaimContentSnapshot {
    assertion_text: String,
    category: ClaimCategory,
    source_key: String,
    source_locator: SourceLocator,
    incident_key: Option<String>,
    author_stated_confidence: Option<DatasetAuthorStatedConfidence>,
    #[serde(rename = "review")]
    _review: DatasetReviewMetadata,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvidenceContentSnapshot {
    source_key: String,
    locator: SourceLocator,
    evidence_kind: EvidenceKind,
    description: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RelationshipContentSnapshot {
    kind: RelationshipKind,
    from: DatasetRecordReference,
    to: DatasetRecordReference,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DiscrepancyContentSnapshot {
    claim_keys: Vec<String>,
    description: String,
    #[serde(rename = "status")]
    _status: DiscrepancyStatus,
}

impl StableRecord {
    fn reference(&self) -> CaseRecordReferenceDto {
        CaseRecordReferenceDto {
            dataset_key: self.dataset_key.clone(),
            record_id: self.record_id.clone(),
            record_kind: self.record_kind.clone(),
        }
    }

    fn metadata(&self, transaction: &Transaction<'_>) -> Result<CaseRecordMetadataDto, String> {
        let row: Option<(String, String, Option<String>)> = transaction
            .query_row(
                "SELECT created_at, created_by_kind, created_by_name
                 FROM records
                 WHERE record_id = ?1 AND record_kind = ?2",
                params![self.record_id, self.record_kind],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|error| {
                format!(
                    "Could not read identity for dataset record {:?}: {error}",
                    self.dataset_key
                )
            })?;
        let (created_at, actor_kind, actor_name) = row.ok_or_else(|| {
            format!(
                "Dataset record {:?} maps to missing {} record identity",
                self.dataset_key, self.record_kind
            )
        })?;
        Ok(CaseRecordMetadataDto {
            id: self.record_id.clone(),
            dataset_key: self.dataset_key.clone(),
            record_kind: self.record_kind.clone(),
            created_at,
            created_by: RecordActor {
                kind: parse_enum(&actor_kind, &format!("{}.createdBy.kind", self.dataset_key))?,
                display_name: actor_name,
            },
        })
    }
}

fn read_research_case(
    database: &Database,
    dataset_id: &str,
) -> Result<Option<ImportedResearchCaseDto>, String> {
    let mut connection = database.connection()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(|error| format!("Could not begin research case read: {error}"))?;

    let ledger: Option<(String, i64, String, i64, String, String)> = transaction
        .query_row(
            "SELECT dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json
             FROM dataset_imports
             WHERE dataset_id = ?1
             ORDER BY import_sequence DESC
             LIMIT 1",
            [dataset_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()
        .map_err(|error| format!("Could not read bundled research case import history: {error}"))?;

    let Some((
        dataset_version,
        import_sequence,
        format,
        format_version,
        imported_at,
        metadata_json,
    )) = ledger
    else {
        transaction
            .commit()
            .map_err(|error| format!("Could not finish empty research case read: {error}"))?;
        return Ok(None);
    };

    let case = assemble_research_case(
        &transaction,
        dataset_id,
        dataset_version,
        import_sequence,
        &format,
        format_version,
        imported_at,
        &metadata_json,
    )?;
    transaction
        .commit()
        .map_err(|error| format!("Could not finish research case read: {error}"))?;
    Ok(Some(case))
}

pub(crate) fn assemble_research_case(
    transaction: &Transaction<'_>,
    dataset_id: &str,
    dataset_version: String,
    import_sequence: i64,
    format: &str,
    format_version: i64,
    imported_at: String,
    metadata_json: &str,
) -> Result<ImportedResearchCaseDto, String> {
    let metadata: ResearchDatasetMetadataDto = serde_json::from_str(metadata_json)
        .map_err(|error| format!("Stored dataset metadata is malformed: {error}"))?;
    if format != "xenoscope-research-dataset" || !matches!(format_version, 1 | 2) {
        return Err(format!(
            "Dataset {dataset_id:?} has unsupported stored format {format:?} version {format_version}"
        ));
    }
    let records = load_stable_records(transaction, dataset_id, import_sequence)?;
    let by_key = records
        .iter()
        .cloned()
        .map(|record| (record.dataset_key.clone(), record))
        .collect::<HashMap<_, _>>();
    if by_key.len() != records.len() {
        return Err("Stored dataset mappings contain duplicate stable keys".to_string());
    }
    let manifest = load_manifest(transaction, dataset_id, import_sequence)?;
    validate_manifest(&manifest, &by_key)?;
    validate_metadata_references(&metadata, &by_key)?;
    let sources = load_sources(transaction, &manifest, &by_key)?;
    let incidents = load_incidents(transaction, &manifest, &by_key, format_version, &sources)?;

    Ok(ImportedResearchCaseDto {
        dataset_id: dataset_id.to_string(),
        dataset_version,
        import_sequence,
        imported_at,
        metadata,
        incidents,
        sources,
        evidence_items: load_evidence_items(transaction, &manifest, &by_key)?,
        claims: load_claims(transaction, &manifest, &by_key)?,
        relationships: load_relationships(transaction, &manifest, &by_key)?,
        discrepancies: load_discrepancies(transaction, &manifest, &by_key)?,
        findings: load_findings(transaction, &manifest, &by_key)?,
    })
}

#[tauri::command]
pub(crate) fn get_bundled_research_case(
    database: State<'_, Database>,
) -> Result<Option<ImportedResearchCaseDto>, String> {
    read_research_case(&database, BUNDLED_DATASET_ID)
}

#[tauri::command]
pub(crate) fn get_research_case(
    database: State<'_, Database>,
    dataset_id: String,
) -> Result<Option<ImportedResearchCaseDto>, String> {
    read_persisted_research_case(&database, &dataset_id).map(Some)
}

fn read_persisted_research_case(
    database: &Database,
    dataset_id: &str,
) -> Result<ImportedResearchCaseDto, String> {
    read_research_case(database, dataset_id)?
        .ok_or_else(|| format!("Dataset {dataset_id:?} has no persisted imports"))
}

#[tauri::command]
pub(crate) fn get_research_case_library(
    database: State<'_, Database>,
) -> Result<Vec<ResearchCaseListEntryDto>, String> {
    read_research_case_library(&database)
}

fn read_research_case_library(
    database: &Database,
) -> Result<Vec<ResearchCaseListEntryDto>, String> {
    let mut connection = database.connection()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(|error| format!("Could not begin research case library read: {error}"))?;

    let mut statement = transaction
        .prepare(
            "SELECT dataset_id, dataset_version, import_sequence, format,
                    format_version, imported_at, metadata_json
             FROM dataset_imports current
             WHERE NOT EXISTS (
                 SELECT 1 FROM dataset_imports newer
                 WHERE newer.dataset_id = current.dataset_id
                   AND newer.import_sequence > current.import_sequence
             )
             ORDER BY dataset_id",
        )
        .map_err(|error| format!("Could not prepare research case library query: {error}"))?;
    let imports = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })
        .map_err(|error| format!("Could not query research case library: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Could not read research case library rows: {error}"))?;
    drop(statement);

    let mut entries = Vec::with_capacity(imports.len());
    for (
        dataset_id,
        dataset_version,
        sequence,
        format,
        format_version,
        imported_at,
        metadata_json,
    ) in imports
    {
        let case = assemble_research_case(
            &transaction,
            &dataset_id,
            dataset_version.clone(),
            sequence,
            &format,
            format_version,
            imported_at.clone(),
            &metadata_json,
        )?;
        let mut discrepancy_status_counts = ResearchCaseDiscrepancyCountsDto {
            open: 0,
            investigating: 0,
            documented: 0,
        };
        for discrepancy in &case.discrepancies {
            match &discrepancy.status {
                DiscrepancyStatus::Open => discrepancy_status_counts.open += 1,
                DiscrepancyStatus::Investigating => discrepancy_status_counts.investigating += 1,
                DiscrepancyStatus::Documented => discrepancy_status_counts.documented += 1,
            }
        }
        let incident_labels = case
            .incidents
            .iter()
            .map(|incident| incident.label.clone())
            .collect();

        entries.push(ResearchCaseListEntryDto {
            dataset_id,
            dataset_version,
            import_sequence: sequence,
            imported_at,
            incident_labels,
            record_counts: ResearchCaseRecordCountsDto {
                incidents: case.incidents.len(),
                sources: case.sources.len(),
                evidence_items: case.evidence_items.len(),
                claims: case.claims.len(),
                relationships: case.relationships.len(),
                discrepancies: case.discrepancies.len(),
                findings: case.findings.len(),
            },
            discrepancy_status_counts,
        });
    }

    transaction
        .commit()
        .map_err(|error| format!("Could not finish research case library read: {error}"))?;
    Ok(entries)
}

fn load_stable_records(
    transaction: &Transaction<'_>,
    dataset_id: &str,
    import_sequence: i64,
) -> Result<Vec<StableRecord>, String> {
    let manifest = load_manifest(transaction, dataset_id, import_sequence)?;
    let mut records = Vec::with_capacity(manifest.len());
    for key in manifest {
        let mapping: (String, String, Option<String>, String) = transaction
            .query_row(
                "SELECT m.record_kind, m.record_id, r.record_kind, m.content_snapshot_json
                 FROM dataset_record_mappings m
                 LEFT JOIN records r ON r.record_id = m.record_id
                 WHERE m.dataset_id = ?1 AND m.record_key = ?2",
                params![dataset_id, key],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(|error| format!("Could not read mapping for stable key {key:?}: {error}"))?
            .ok_or_else(|| format!("Import manifest references missing mapping {key:?}"))?;
        let (record_kind, record_id, identity_kind, content_snapshot_json) = mapping;
        if identity_kind.as_deref() != Some(record_kind.as_str()) {
            return Err(format!(
                "Mapping for stable key {key:?} has inconsistent record identity kind"
            ));
        }
        records.push(StableRecord {
            dataset_key: key,
            record_id,
            record_kind,
            content_snapshot_json,
        });
    }
    Ok(records)
}

fn load_manifest(
    transaction: &Transaction<'_>,
    dataset_id: &str,
    import_sequence: i64,
) -> Result<Vec<String>, String> {
    let manifest_json: String = transaction
        .query_row(
            "SELECT record_keys_json FROM dataset_imports
             WHERE dataset_id = ?1 AND import_sequence = ?2",
            params![dataset_id, import_sequence],
            |row| row.get(0),
        )
        .map_err(|error| {
            format!("Could not read manifest for import sequence {import_sequence}: {error}")
        })?;
    let manifest: Vec<String> = serde_json::from_str(&manifest_json)
        .map_err(|error| format!("Stored dataset manifest is malformed: {error}"))?;
    let unique = manifest.iter().collect::<HashSet<_>>();
    if unique.len() != manifest.len() {
        return Err("Stored dataset manifest contains duplicate stable keys".to_string());
    }
    Ok(manifest)
}

fn validate_manifest(
    manifest: &[String],
    mappings: &HashMap<String, StableRecord>,
) -> Result<(), String> {
    let manifest_keys = manifest.iter().collect::<HashSet<_>>();
    if manifest_keys.len() != mappings.len()
        || mappings.keys().any(|key| !manifest_keys.contains(key))
    {
        return Err("Stored dataset mappings do not match the import manifest".to_string());
    }
    Ok(())
}

fn require_record<'a>(
    key: &str,
    expected_kind: &str,
    mappings: &'a HashMap<String, StableRecord>,
) -> Result<&'a StableRecord, String> {
    let record = mappings
        .get(key)
        .ok_or_else(|| format!("Stored reference points to missing stable key {key:?}"))?;
    if record.record_kind != expected_kind {
        return Err(format!(
            "Stable key {key:?} resolves to {}, expected {expected_kind}",
            record.record_kind
        ));
    }
    Ok(record)
}

fn load_incidents(
    transaction: &Transaction<'_>,
    manifest: &[String],
    mappings: &HashMap<String, StableRecord>,
    format_version: i64,
    sources: &[ResearchSourceDto],
) -> Result<Vec<ResearchIncidentDto>, String> {
    let mut result = Vec::new();
    for key in keys_of_kind(manifest, mappings, "incident") {
        let record = require_record(&key, "incident", mappings)?;
        let snapshot: IncidentContentSnapshot = serde_json::from_str(&record.content_snapshot_json)
            .map_err(|error| {
                format!("Stored incident content snapshot for {key:?} is malformed: {error}")
            })?;
        let incident_exists: bool = transaction
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM incidents
                    WHERE record_id = ?1 AND record_kind = 'incident'
                 )",
                [&record.record_id],
                |row| row.get(0),
            )
            .map_err(|error| {
                format!("Could not verify incident domain data for {key:?}: {error}")
            })?;
        if !incident_exists {
            return Err(format!(
                "Missing incident domain data for stable key {key:?}"
            ));
        }

        let (description, location, time_period, incident_sources) = match format_version {
            1 => {
                if snapshot.description.is_some()
                    || snapshot.location.is_some()
                    || snapshot.time_period.is_some()
                    || snapshot.source_keys.is_some()
                {
                    return Err(format!(
                        "Stored v1 incident snapshot for {key:?} contains v2 admission fields"
                    ));
                }
                (None, None, None, None)
            }
            2 => {
                let description = snapshot.description.ok_or_else(|| {
                    format!("Stored v2 incident snapshot for {key:?} is missing description")
                })?;
                let location = snapshot.location.ok_or_else(|| {
                    format!("Stored v2 incident snapshot for {key:?} is missing location")
                })?;
                let time_period = snapshot.time_period.ok_or_else(|| {
                    format!("Stored v2 incident snapshot for {key:?} is missing timePeriod")
                })?;
                let source_keys = snapshot.source_keys.ok_or_else(|| {
                    format!("Stored v2 incident snapshot for {key:?} is missing sourceKeys")
                })?;
                if source_keys.is_empty() {
                    return Err(format!(
                        "Stored v2 incident snapshot for {key:?} has no sourceKeys"
                    ));
                }
                if source_keys.iter().collect::<HashSet<_>>().len() != source_keys.len() {
                    return Err(format!(
                        "Stored v2 incident snapshot for {key:?} has duplicate sourceKeys"
                    ));
                }
                let mut incident_sources = Vec::with_capacity(source_keys.len());
                for source_key in source_keys {
                    let source_record =
                        require_record(&source_key, "source", mappings).map_err(|error| {
                            format!(
                                "Stored incident {key:?} has an invalid source reference: {error}"
                            )
                        })?;
                    let source = sources
                        .iter()
                        .find(|source| source.record.dataset_key == source_record.dataset_key)
                        .ok_or_else(|| {
                            format!(
                                "Stored incident {key:?} references source {source_key:?} missing from the selected import manifest"
                            )
                        })?;
                    incident_sources.push(source.clone());
                }
                (
                    Some(description),
                    Some(location),
                    Some(time_period),
                    Some(incident_sources),
                )
            }
            _ => {
                return Err(format!(
                    "Unsupported stored dataset format version {format_version}"
                ));
            }
        };
        result.push(ResearchIncidentDto {
            record: record.metadata(transaction)?,
            label: snapshot.label,
            description,
            location,
            time_period,
            sources: incident_sources,
        });
    }
    Ok(result)
}

fn load_sources(
    transaction: &Transaction<'_>,
    manifest: &[String],
    mappings: &HashMap<String, StableRecord>,
) -> Result<Vec<ResearchSourceDto>, String> {
    let mut result = Vec::new();
    for key in keys_of_kind(manifest, mappings, "source") {
        let record = require_record(&key, "source", mappings)?;
        let snapshot: SourceContentSnapshot = serde_json::from_str(&record.content_snapshot_json)
            .map_err(|error| {
            format!("Stored source content snapshot for {key:?} is malformed: {error}")
        })?;
        let source_exists: bool = transaction
            .query_row(
                "SELECT EXISTS(
                SELECT 1 FROM sources
                WHERE record_id = ?1 AND record_kind = 'source'
             )",
                [&record.record_id],
                |row| row.get(0),
            )
            .map_err(|error| format!("Could not verify source domain data for {key:?}: {error}"))?;
        if !source_exists {
            return Err(format!("Missing source domain data for stable key {key:?}"));
        }
        result.push(ResearchSourceDto {
            record: record.metadata(transaction)?,
            source_kind: snapshot.source_kind,
            title: snapshot.title,
            publisher: snapshot.publisher,
            author: snapshot.author,
            publication_date: snapshot.publication_date,
            url: snapshot.url,
            other_locator: snapshot.other_locator,
            verification_status: snapshot.verification_status,
        });
    }
    Ok(result)
}

fn load_evidence_items(
    transaction: &Transaction<'_>,
    manifest: &[String],
    mappings: &HashMap<String, StableRecord>,
) -> Result<Vec<ResearchEvidenceItemDto>, String> {
    let mut result = Vec::new();
    for key in keys_of_kind(manifest, mappings, "evidence_item") {
        let record = require_record(&key, "evidence_item", mappings)?;
        let snapshot: EvidenceContentSnapshot = serde_json::from_str(&record.content_snapshot_json)
            .map_err(|error| {
                format!("Stored evidence content snapshot for {key:?} is malformed: {error}")
            })?;
        validate_snapshot_locator(&snapshot.locator, &format!("{key}.locator"))?;
        let exists: bool = transaction
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM evidence_items
                    WHERE record_id = ?1 AND record_kind = 'evidence_item'
                 )",
                [&record.record_id],
                |row| row.get(0),
            )
            .map_err(|error| {
                format!("Could not verify evidence domain data for {key:?}: {error}")
            })?;
        if !exists {
            return Err(format!(
                "Missing evidence domain data for stable key {key:?}"
            ));
        }
        let source = resolve_snapshot_reference(
            &snapshot.source_key,
            "source",
            &key,
            "sourceKey",
            mappings,
        )?;
        result.push(ResearchEvidenceItemDto {
            record: record.metadata(transaction)?,
            source,
            locator: snapshot.locator,
            evidence_kind: snapshot.evidence_kind,
            description: snapshot.description,
        });
    }
    Ok(result)
}

fn load_claims(
    transaction: &Transaction<'_>,
    manifest: &[String],
    mappings: &HashMap<String, StableRecord>,
) -> Result<Vec<ResearchClaimDto>, String> {
    let mut result = Vec::new();
    for key in keys_of_kind(manifest, mappings, "claim") {
        let record = require_record(&key, "claim", mappings)?;
        let snapshot: ClaimContentSnapshot = serde_json::from_str(&record.content_snapshot_json)
            .map_err(|error| {
                format!("Stored claim content snapshot for {key:?} is malformed: {error}")
            })?;
        validate_snapshot_locator(&snapshot.source_locator, &format!("{key}.sourceLocator"))?;
        let review_row: Option<(String, Option<String>, Option<String>, Option<String>)> =
            transaction
                .query_row(
                    "SELECT review_status, reviewed_at, reviewed_by_kind, reviewed_by_name
                 FROM claims
                 WHERE record_id = ?1 AND record_kind = 'claim'",
                    [&record.record_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()
                .map_err(|error| format!("Could not read claim {key:?}: {error}"))?;
        let (review_status, reviewed_at, reviewed_by_kind, reviewed_by_name) = review_row
            .ok_or_else(|| format!("Missing claim domain data for stable key {key:?}"))?;
        let source = resolve_snapshot_reference(
            &snapshot.source_key,
            "source",
            &key,
            "sourceKey",
            mappings,
        )?;
        let incident = snapshot
            .incident_key
            .as_deref()
            .map(|incident_key| {
                resolve_snapshot_reference(incident_key, "incident", &key, "incidentKey", mappings)
            })
            .transpose()?;
        let confidence = snapshot
            .author_stated_confidence
            .map(|confidence| {
                let source = resolve_snapshot_reference(
                    &confidence.source_key,
                    "source",
                    &key,
                    "authorStatedConfidence.sourceKey",
                    mappings,
                )?;
                if let Some(locator) = &confidence.locator {
                    validate_snapshot_locator(
                        locator,
                        &format!("{key}.authorStatedConfidence.locator"),
                    )?;
                }
                Ok::<_, String>(CaseAuthorStatedConfidenceDto {
                    stated_by: confidence.stated_by,
                    value_as_stated: confidence.value_as_stated,
                    source,
                    locator: confidence.locator,
                })
            })
            .transpose()?;
        let review = ReviewMetadata {
            status: parse_enum(&review_status, &format!("{key}.review.status"))?,
            reviewed_at,
            reviewed_by: reviewed_by_kind
                .map(|kind| {
                    Ok::<RecordActor, String>(RecordActor {
                        kind: parse_enum(&kind, &format!("{key}.review.reviewedBy.kind"))?,
                        display_name: reviewed_by_name,
                    })
                })
                .transpose()?,
        };
        result.push(ResearchClaimDto {
            record: record.metadata(transaction)?,
            assertion_text: snapshot.assertion_text,
            category: snapshot.category,
            source,
            source_locator: snapshot.source_locator,
            incident,
            author_stated_confidence: confidence,
            review,
        });
    }
    Ok(result)
}

fn resolve_snapshot_reference(
    reference_key: &str,
    expected_kind: &str,
    owner_key: &str,
    field: &str,
    mappings: &HashMap<String, StableRecord>,
) -> Result<CaseRecordReferenceDto, String> {
    require_record(reference_key, expected_kind, mappings)
        .map(StableRecord::reference)
        .map_err(|error| {
            format!("Stored record {owner_key:?} has invalid {field} reference: {error}")
        })
}

fn validate_snapshot_locator(locator: &SourceLocator, field: &str) -> Result<(), String> {
    locator
        .validate()
        .map_err(|error| format!("Stored {field} locator is invalid: {error}"))
}

fn load_relationships(
    transaction: &Transaction<'_>,
    manifest: &[String],
    mappings: &HashMap<String, StableRecord>,
) -> Result<Vec<ResearchRelationshipDto>, String> {
    let mut result = Vec::new();
    for key in keys_of_kind(manifest, mappings, "relationship") {
        let record = require_record(&key, "relationship", mappings)?;
        let snapshot: RelationshipContentSnapshot =
            serde_json::from_str(&record.content_snapshot_json).map_err(|error| {
                format!("Stored relationship content snapshot for {key:?} is malformed: {error}")
            })?;
        if snapshot.from.record_key == snapshot.to.record_key {
            return Err(format!(
                "Stored relationship snapshot for {key:?} references the same record at both endpoints"
            ));
        }
        let from = resolve_snapshot_reference(
            &snapshot.from.record_key,
            snapshot_record_kind(snapshot.from.record_kind)?,
            &key,
            "from",
            mappings,
        )?;
        let to = resolve_snapshot_reference(
            &snapshot.to.record_key,
            snapshot_record_kind(snapshot.to.record_kind)?,
            &key,
            "to",
            mappings,
        )?;
        result.push(ResearchRelationshipDto {
            record: record.metadata(transaction)?,
            relationship_kind: snapshot.kind,
            from,
            to,
        });
    }
    Ok(result)
}

fn load_discrepancies(
    transaction: &Transaction<'_>,
    manifest: &[String],
    mappings: &HashMap<String, StableRecord>,
) -> Result<Vec<ResearchDiscrepancyDto>, String> {
    let mut result = Vec::new();
    for key in keys_of_kind(manifest, mappings, "discrepancy") {
        let record = require_record(&key, "discrepancy", mappings)?;
        let snapshot: DiscrepancyContentSnapshot =
            serde_json::from_str(&record.content_snapshot_json).map_err(|error| {
                format!("Stored discrepancy content snapshot for {key:?} is malformed: {error}")
            })?;
        if snapshot.claim_keys.len() < 2 {
            return Err(format!(
                "Stored discrepancy snapshot for {key:?} must reference at least two claims"
            ));
        }
        if snapshot.claim_keys.iter().collect::<HashSet<_>>().len() != snapshot.claim_keys.len() {
            return Err(format!(
                "Stored discrepancy snapshot for {key:?} contains duplicate claim keys"
            ));
        }
        let status: Option<String> = transaction
            .query_row(
                "SELECT status FROM discrepancies
                 WHERE record_id = ?1 AND record_kind = 'discrepancy'",
                [&record.record_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| format!("Could not read discrepancy {key:?}: {error}"))?;
        let status = status
            .ok_or_else(|| format!("Missing discrepancy domain data for stable key {key:?}"))?;
        let claim_references = snapshot
            .claim_keys
            .iter()
            .map(|claim_key| {
                resolve_snapshot_reference(claim_key, "claim", &key, "claimKeys", mappings)
            })
            .collect::<Result<Vec<_>, _>>()?;
        result.push(ResearchDiscrepancyDto {
            record: record.metadata(transaction)?,
            claim_references,
            description: snapshot.description,
            status: parse_enum(&status, &format!("{key}.status"))?,
        });
    }
    Ok(result)
}

fn snapshot_record_kind(kind: DatasetRecordKind) -> Result<&'static str, String> {
    match kind {
        DatasetRecordKind::Incident => Ok("incident"),
        DatasetRecordKind::Source => Ok("source"),
        DatasetRecordKind::EvidenceItem => Ok("evidence"),
        DatasetRecordKind::Claim => Ok("claim"),
        DatasetRecordKind::Relationship => Ok("relationship"),
        DatasetRecordKind::Discrepancy => Ok("discrepancy"),
        DatasetRecordKind::Finding => Ok("finding"),
        DatasetRecordKind::History => {
            Err("Relationship endpoints cannot reference history records".to_string())
        }
    }
}

fn load_findings(
    transaction: &Transaction<'_>,
    manifest: &[String],
    mappings: &HashMap<String, StableRecord>,
) -> Result<Vec<ResearchFindingDto>, String> {
    let mut result = Vec::new();
    for key in keys_of_kind(manifest, mappings, "finding") {
        let record = require_record(&key, "finding", mappings)?;
        let row: Option<(
            String,
            String,
            Option<String>,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
        )> = transaction
            .query_row(
                "SELECT assessment, attribution_kind, attribution_name, review_status,
                        reviewed_at, reviewed_by_kind, reviewed_by_name
                 FROM findings WHERE record_id = ?1 AND record_kind = 'finding'",
                [&record.record_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| format!("Could not read finding {key:?}: {error}"))?;
        let (
            assessment,
            attribution_kind,
            attribution_name,
            review_status,
            reviewed_at,
            reviewer_kind,
            reviewer_name,
        ) = row.ok_or_else(|| format!("Missing finding domain data for stable key {key:?}"))?;
        let attribution = RecordActor {
            kind: parse_enum(&attribution_kind, &format!("{key}.attribution.kind"))?,
            display_name: attribution_name,
        };
        let review = ReviewMetadata {
            status: parse_enum(&review_status, &format!("{key}.review.status"))?,
            reviewed_at,
            reviewed_by: reviewer_kind
                .map(|kind| {
                    Ok::<RecordActor, String>(RecordActor {
                        kind: parse_enum(&kind, &format!("{key}.review.reviewedBy.kind"))?,
                        display_name: reviewer_name,
                    })
                })
                .transpose()?,
        };
        let claim_references = load_finding_references(
            transaction,
            &key,
            &record.record_id,
            "finding_claim_references",
            "claim_id",
            mappings,
        )?;
        let evidence_references = load_finding_references(
            transaction,
            &key,
            &record.record_id,
            "finding_evidence_references",
            "evidence_item_id",
            mappings,
        )?;
        result.push(ResearchFindingDto {
            record: record.metadata(transaction)?,
            assessment,
            attribution,
            claim_references,
            evidence_references,
            review,
        });
    }
    Ok(result)
}

fn load_finding_references(
    transaction: &Transaction<'_>,
    finding_key: &str,
    finding_id: &str,
    table: &str,
    reference_column: &str,
    mappings: &HashMap<String, StableRecord>,
) -> Result<Vec<CaseFindingReferenceDto>, String> {
    let mut statement = transaction
        .prepare(&format!(
            "SELECT {reference_column}, reference_role
             FROM {table}
             WHERE finding_id = ?1
             ORDER BY {reference_column}, reference_role"
        ))
        .map_err(|error| format!("Could not prepare {table} read: {error}"))?;
    let rows = statement
        .query_map([finding_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| format!("Could not query {table} for {finding_key:?}: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Could not read {table} for {finding_key:?}: {error}"))?;
    rows.into_iter()
        .map(|(record_id, role)| {
            let expected_kind = if reference_column == "claim_id" {
                "claim"
            } else {
                "evidence_item"
            };
            let reference = reference_by_id(&record_id, expected_kind, mappings)?;
            Ok(CaseFindingReferenceDto {
                record: reference,
                role: parse_enum(&role, &format!("{finding_key}.{table}.role"))?,
            })
        })
        .collect()
}

fn reference_by_id(
    id: &str,
    expected_kind: &str,
    mappings: &HashMap<String, StableRecord>,
) -> Result<CaseRecordReferenceDto, String> {
    let record = mappings
        .values()
        .find(|record| record.record_id == id)
        .ok_or_else(|| format!("Stored reference points to unmapped record ID {id:?}"))?;
    if record.record_kind != expected_kind {
        return Err(format!(
            "Stored reference ID {id:?} has kind {}, expected {expected_kind}",
            record.record_kind
        ));
    }
    Ok(record.reference())
}

fn validate_metadata_references(
    metadata: &ResearchDatasetMetadataDto,
    mappings: &HashMap<String, StableRecord>,
) -> Result<(), String> {
    for (index, source_access) in metadata.source_access.iter().enumerate() {
        require_record(&source_access.source_key, "source", mappings).map_err(|error| {
            format!("Stored metadata sourceAccess[{index}] has invalid reference: {error}")
        })?;
    }
    for (index, source_metadata) in metadata.source_metadata.iter().enumerate() {
        require_record(&source_metadata.source_key, "source", mappings).map_err(|error| {
            format!("Stored metadata sourceMetadata[{index}] has invalid reference: {error}")
        })?;
    }
    for (index, note) in metadata.relationship_notes.iter().enumerate() {
        require_record(&note.relationship_key, "relationship", mappings).map_err(|error| {
            format!("Stored metadata relationshipNotes[{index}] has invalid reference: {error}")
        })?;
    }
    Ok(())
}

fn keys_of_kind(
    manifest: &[String],
    mappings: &HashMap<String, StableRecord>,
    kind: &str,
) -> Vec<String> {
    let mut keys = manifest
        .iter()
        .filter(|key| {
            mappings
                .get(*key)
                .is_some_and(|record| record.record_kind == kind)
        })
        .cloned()
        .collect::<Vec<_>>();
    keys.sort_unstable();
    keys
}

fn parse_enum<T: DeserializeOwned>(value: &str, field: &str) -> Result<T, String> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .map_err(|error| format!("Stored {field} is invalid: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research_records::ClaimReviewStatus;
    use crate::{
        database::Database,
        research_dataset_import::{import_bundled_dataset, seed_bundled_legacy_dataset_for_tests},
    };
    use std::{fs, path::PathBuf};
    use uuid::Uuid;

    fn scalar(database: &Database, sql: &str) -> i64 {
        database
            .connection()
            .expect("lock isolated database")
            .query_row(sql, [], |row| row.get(0))
            .expect("query scalar")
    }

    fn clone_bundled_dataset(database: &Database, dataset_id: &str) {
        let connection = database.connection().expect("lock isolated database");
        let transaction = connection
            .unchecked_transaction()
            .expect("begin isolated dataset clone");
        transaction
            .execute(
                "INSERT INTO research_datasets (dataset_id) VALUES (?1)",
                [dataset_id],
            )
            .expect("insert isolated dataset identity");
        transaction
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 )
                 SELECT ?1, dataset_version, import_sequence, format, format_version,
                        imported_at, metadata_json, record_keys_json
                 FROM dataset_imports
                 WHERE dataset_id = ?2 AND import_sequence = 1",
                params![dataset_id, BUNDLED_DATASET_ID],
            )
            .expect("copy bundled import ledger to isolated identity");
        transaction
            .execute(
                "INSERT INTO dataset_record_mappings (
                    dataset_id, record_key, record_kind, record_id, content_snapshot_json
                 )
                 SELECT ?1, record_key, record_kind, record_id, content_snapshot_json
                 FROM dataset_record_mappings WHERE dataset_id = ?2",
                params![dataset_id, BUNDLED_DATASET_ID],
            )
            .expect("copy stable mappings to isolated identity");
        transaction.commit().expect("commit isolated dataset clone");
    }

    fn mutate_snapshot(
        database: &Database,
        record_key: &str,
        mutate: impl FnOnce(&mut serde_json::Value),
    ) {
        let mut connection = database.connection().expect("lock isolated database");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("begin test snapshot mutation");
        let snapshot_json: String = transaction
            .query_row(
                "SELECT content_snapshot_json FROM dataset_record_mappings
                 WHERE dataset_id = ?1 AND record_key = ?2",
                params![BUNDLED_DATASET_ID, record_key],
                |row| row.get(0),
            )
            .expect("read canonical snapshot");
        let mut snapshot: serde_json::Value =
            serde_json::from_str(&snapshot_json).expect("parse canonical snapshot");
        mutate(&mut snapshot);
        transaction
            .execute(
                "UPDATE dataset_record_mappings SET content_snapshot_json = ?1
                 WHERE dataset_id = ?2 AND record_key = ?3",
                params![
                    serde_json::to_string(&snapshot).expect("serialize changed snapshot"),
                    BUNDLED_DATASET_ID,
                    record_key
                ],
            )
            .expect("persist changed snapshot");
        transaction.commit().expect("commit changed snapshot");
    }

    #[test]
    fn no_imported_bundled_dataset_returns_none() {
        let database = Database::in_memory().expect("initialize isolated database");
        assert!(
            read_research_case(&database, BUNDLED_DATASET_ID)
                .expect("read empty database")
                .is_none()
        );
    }

    #[test]
    fn reads_latest_v2_manifest_after_legacy_dataset_upgrade() {
        let database = Database::in_memory().expect("initialize isolated database");
        seed_bundled_legacy_dataset_for_tests(&database).expect("seed historical v1 dataset");

        let legacy_case = read_research_case(&database, BUNDLED_DATASET_ID)
            .expect("read v1 case")
            .expect("v1 case exists");
        assert_eq!(legacy_case.dataset_version, "1.0.0");
        assert_eq!(
            legacy_case.incidents[0].record.dataset_key,
            "xenoscope.case.al-taqaddum-2017:incident:al-taqaddum-object-report"
        );

        let upgrade = import_bundled_dataset(&database).expect("import production v2 bundle");
        assert_eq!(upgrade.dataset_version, "2.0.0");
        assert_eq!(upgrade.import_sequence, 2);

        let current_case = read_research_case(&database, BUNDLED_DATASET_ID)
            .expect("read current case")
            .expect("current case exists");
        assert_eq!(current_case.dataset_version, "2.0.0");
        assert_eq!(current_case.import_sequence, 2);
        assert_eq!(
            current_case.incidents[0].record.dataset_key,
            "xenoscope.case.al-taqaddum-2017:incident:al-taqaddum-object-report-v2"
        );
        let incident = &current_case.incidents[0];
        assert_eq!(
            incident.description.as_deref(),
            Some(
                "AARO and DVIDS report that infrared footage from an aerostat near Al Taqaddum Air Base showed an unidentified object. The video itself was not independently inspected for this dataset."
            )
        );
        let location = incident.location.as_ref().expect("v2 incident location");
        assert_eq!(location.description, "Near Al Taqaddum Air Base, Iraq");
        assert_eq!(
            location.precision,
            crate::research_dataset::DatasetLocationPrecision::ApproximateArea
        );
        assert_eq!(
            location.original_wording.as_deref(),
            Some("near Al Taqaddum Air Base, Iraq")
        );
        let time_period = incident
            .time_period
            .as_ref()
            .expect("v2 incident time period");
        assert_eq!(time_period.start_date, "2017-10-01");
        assert_eq!(time_period.end_date, "2017-10-31");
        assert_eq!(
            time_period.precision,
            crate::research_dataset::DatasetTimePrecision::Month
        );
        assert_eq!(
            time_period.original_wording.as_deref(),
            Some("October 2017")
        );
        let incident_sources = incident.sources.as_ref().expect("v2 incident sources");
        assert_eq!(incident_sources.len(), 2);
        assert_eq!(
            incident_sources[0].record.dataset_key,
            "xenoscope.case.al-taqaddum-2017:source:aaro-report"
        );
        assert_eq!(incident_sources[0].record.record_kind, "source");
        assert_eq!(
            incident_sources[0].record.id,
            current_case.sources[0].record.id
        );
        assert_eq!(
            incident_sources[0].title.as_deref(),
            Some("The Al Taqaddum Object")
        );
        assert_eq!(
            incident_sources[1].record.dataset_key,
            "xenoscope.case.al-taqaddum-2017:source:dvids-record"
        );
        assert_eq!(incident_sources[1].record.record_kind, "source");
        assert_eq!(
            incident_sources[1].record.id,
            current_case.sources[1].record.id
        );
        assert_eq!(
            incident_sources[1].title.as_deref(),
            Some("Al Taqaddum Object")
        );
        let incident_json = serde_json::to_value(incident).expect("serialize v2 incident DTO");
        assert_eq!(incident_json["location"]["precision"], "approximate_area");
        assert_eq!(incident_json["timePeriod"]["precision"], "month");
        assert_eq!(
            incident_json["timePeriod"]["originalWording"],
            "October 2017"
        );
        assert_eq!(
            incident_json["sources"][0]["record"]["recordKind"],
            "source"
        );
        assert_eq!(
            incident_json["sources"][0]["title"],
            "The Al Taqaddum Object"
        );
        assert_eq!(current_case.claims.len(), 10);
        assert!(
            current_case
                .claims
                .iter()
                .all(|claim| { claim.record.dataset_key.ends_with("-v2") })
        );
        assert_eq!(
            current_case.evidence_items[0].record.dataset_key,
            "xenoscope.case.al-taqaddum-2017:evidence:dvids-video-960331"
        );
    }

    #[test]
    fn reads_incident_label_from_manifest_snapshot_after_live_label_edit() {
        let database = Database::in_memory().expect("initialize isolated database");
        import_bundled_dataset(&database).expect("import v2 fixture");
        let incident_key = "xenoscope.case.al-taqaddum-2017:incident:al-taqaddum-object-report-v2";

        {
            let connection = database.connection().expect("lock isolated database");
            let incident_id: String = connection
                .query_row(
                    "SELECT record_id FROM dataset_record_mappings
                     WHERE dataset_id = ?1 AND record_key = ?2",
                    params![BUNDLED_DATASET_ID, incident_key],
                    |row| row.get(0),
                )
                .expect("find mapped incident");
            connection
                .execute(
                    "UPDATE incidents SET label = 'Edited live label' WHERE record_id = ?1",
                    [incident_id],
                )
                .expect("edit live incident label");
        }

        let case = read_research_case(&database, BUNDLED_DATASET_ID)
            .expect("read imported case")
            .expect("case exists");
        assert_eq!(
            case.incidents[0].label,
            "Al Taqaddum object report, October 2017"
        );
    }

    #[test]
    fn reads_source_content_from_manifest_snapshots_after_live_source_edits() {
        let database = Database::in_memory().expect("initialize isolated database");
        import_bundled_dataset(&database).expect("import v2 fixture");
        let source_key = "xenoscope.case.al-taqaddum-2017:source:aaro-report";

        {
            let connection = database.connection().expect("lock isolated database");
            let source_id: String = connection
                .query_row(
                    "SELECT record_id FROM dataset_record_mappings
                     WHERE dataset_id = ?1 AND record_key = ?2",
                    params![BUNDLED_DATASET_ID, source_key],
                    |row| row.get(0),
                )
                .expect("find mapped source");
            connection
                .execute(
                    "UPDATE sources SET title = 'Edited live title',
                         publisher = 'Edited live publisher'
                     WHERE record_id = ?1",
                    [source_id],
                )
                .expect("edit live source content");
        }

        let case = read_research_case(&database, BUNDLED_DATASET_ID)
            .expect("read imported case")
            .expect("case exists");
        let source = case
            .sources
            .iter()
            .find(|source| source.record.dataset_key == source_key)
            .expect("source exists in selected manifest");
        assert_eq!(source.title.as_deref(), Some("The Al Taqaddum Object"));
        assert_eq!(
            source.publisher.as_deref(),
            Some("All-domain Anomaly Resolution Office (AARO)")
        );
        let incident_source = case.incidents[0]
            .sources
            .as_ref()
            .expect("incident sources")
            .iter()
            .find(|source| source.record.dataset_key == source_key)
            .expect("incident references source");
        assert_eq!(incident_source, source);
        assert_eq!(
            incident_source.title.as_deref(),
            Some("The Al Taqaddum Object")
        );
        assert_eq!(
            incident_source.publisher.as_deref(),
            Some("All-domain Anomaly Resolution Office (AARO)")
        );
    }

    #[test]
    fn malformed_source_content_snapshot_fails_explicitly() {
        let database = Database::in_memory().expect("initialize isolated database");
        import_bundled_dataset(&database).expect("import v2 fixture");
        let source_key = "xenoscope.case.al-taqaddum-2017:source:aaro-report";

        {
            let connection = database.connection().expect("lock isolated database");
            connection
                .execute(
                    "UPDATE dataset_record_mappings
                     SET content_snapshot_json = '{\"title\":\"missing required fields\"}'
                     WHERE dataset_id = ?1 AND record_key = ?2",
                    params![BUNDLED_DATASET_ID, source_key],
                )
                .expect("replace source snapshot with incompatible content");
        }

        let error = read_research_case(&database, BUNDLED_DATASET_ID)
            .expect_err("incompatible source snapshot should fail");
        assert!(error.contains("source content snapshot") && error.contains("malformed"));
    }

    #[test]
    fn reads_claim_content_from_snapshot_and_review_state_from_live_row() {
        let database = Database::in_memory().expect("initialize isolated database");
        import_bundled_dataset(&database).expect("import v2 fixture");
        let claim_key = "xenoscope.case.al-taqaddum-2017:claim:aaro-balloon-assessment-v2";
        let aaro_source_key = "xenoscope.case.al-taqaddum-2017:source:aaro-report";
        let dvids_source_key = "xenoscope.case.al-taqaddum-2017:source:dvids-record";
        let incident_key = "xenoscope.case.al-taqaddum-2017:incident:al-taqaddum-object-report-v2";

        {
            let connection = database.connection().expect("lock isolated database");
            let ids = |record_key: &str| {
                connection
                    .query_row(
                        "SELECT record_id FROM dataset_record_mappings
                         WHERE dataset_id = ?1 AND record_key = ?2",
                        params![BUNDLED_DATASET_ID, record_key],
                        |row| row.get::<_, String>(0),
                    )
                    .expect("find mapped record")
            };
            let claim_id = ids(claim_key);
            let alternate_source_id = ids(dvids_source_key);
            connection
                .execute(
                    "UPDATE claims
                     SET assertion_text = 'Edited live assertion',
                         category = 'context',
                         source_id = ?1,
                         source_locator_kind = 'page',
                         source_locator_json = '{\"kind\":\"page\",\"page\":99}',
                         incident_id = NULL,
                         review_status = 'in_review',
                         reviewed_at = '2026-11-01T00:00:00Z',
                         reviewed_by_kind = 'researcher',
                         reviewed_by_name = 'Test Reviewer'
                     WHERE record_id = ?2",
                    params![alternate_source_id, claim_id],
                )
                .expect("edit live claim and review fields");
            connection
                .execute(
                    "UPDATE claim_author_confidence
                     SET stated_by = 'Edited live author',
                         value_as_stated = 'edited confidence',
                         source_id = ?1,
                         locator_kind = 'page',
                         locator_json = '{\"kind\":\"page\",\"page\":99}'
                     WHERE claim_id = ?2",
                    params![alternate_source_id, claim_id],
                )
                .expect("edit live author-stated confidence");
        }

        let case = read_research_case(&database, BUNDLED_DATASET_ID)
            .expect("read imported case")
            .expect("case exists");
        let claim = case
            .claims
            .iter()
            .find(|claim| claim.record.dataset_key == claim_key)
            .expect("claim exists");
        assert_eq!(
            claim.assertion_text,
            "AARO assesses with high confidence that the object was consistent with a cluster of fully and partially inflated balloons (Figure 1)."
        );
        assert_eq!(claim.category, ClaimCategory::Identification);
        assert_eq!(claim.source.dataset_key, aaro_source_key);
        assert_eq!(claim.source.record_id, case.sources[0].record.id);
        assert_eq!(claim.source.record_kind, "source");
        assert_eq!(claim.incident.as_ref().unwrap().dataset_key, incident_key);
        assert_eq!(claim.incident.as_ref().unwrap().record_kind, "incident");
        assert!(matches!(
            claim.source_locator,
            SourceLocator::Other { ref description }
                if description == "PDF page 1, AARO Assessment and Case Status."
        ));
        let confidence = claim
            .author_stated_confidence
            .as_ref()
            .expect("imported confidence exists");
        assert_eq!(
            confidence.stated_by,
            "All-domain Anomaly Resolution Office (AARO)"
        );
        assert_eq!(confidence.value_as_stated, "high confidence");
        assert_eq!(confidence.source.dataset_key, aaro_source_key);
        assert_eq!(confidence.source.record_kind, "source");
        assert!(matches!(
            confidence.locator,
            Some(SourceLocator::Other { ref description })
                if description == "PDF page 1, AARO Assessment and Case Status."
        ));
        assert_eq!(claim.review.status, ClaimReviewStatus::InReview);
        assert_eq!(
            claim.review.reviewed_at.as_deref(),
            Some("2026-11-01T00:00:00Z")
        );
        assert_eq!(
            claim
                .review
                .reviewed_by
                .as_ref()
                .unwrap()
                .display_name
                .as_deref(),
            Some("Test Reviewer")
        );
    }

    #[test]
    fn reads_evidence_content_and_source_from_snapshot_after_live_edits() {
        let database = Database::in_memory().expect("initialize isolated database");
        import_bundled_dataset(&database).expect("import v2 fixture");
        let evidence_key = "xenoscope.case.al-taqaddum-2017:evidence:dvids-video-960331";
        let dvids_source_key = "xenoscope.case.al-taqaddum-2017:source:dvids-record";
        let aaro_source_key = "xenoscope.case.al-taqaddum-2017:source:aaro-report";

        {
            let connection = database.connection().expect("lock isolated database");
            let evidence_id: String = connection
                .query_row(
                    "SELECT record_id FROM dataset_record_mappings
                     WHERE dataset_id = ?1 AND record_key = ?2",
                    params![BUNDLED_DATASET_ID, evidence_key],
                    |row| row.get(0),
                )
                .expect("find mapped evidence");
            let alternate_source_id: String = connection
                .query_row(
                    "SELECT record_id FROM dataset_record_mappings
                     WHERE dataset_id = ?1 AND record_key = ?2",
                    params![BUNDLED_DATASET_ID, aaro_source_key],
                    |row| row.get(0),
                )
                .expect("find alternate source");
            connection
                .execute(
                    "UPDATE evidence_items
                     SET source_id = ?1, locator_kind = 'page',
                         locator_json = '{\"kind\":\"page\",\"page\":99}',
                         evidence_kind = 'document', description = 'Edited live evidence'
                     WHERE record_id = ?2",
                    params![alternate_source_id, evidence_id],
                )
                .expect("edit live evidence");
        }

        let case = read_research_case(&database, BUNDLED_DATASET_ID)
            .expect("read imported case")
            .expect("case exists");
        let evidence = &case.evidence_items[0];
        assert_eq!(evidence.record.dataset_key, evidence_key);
        assert_eq!(evidence.source.dataset_key, dvids_source_key);
        assert_eq!(evidence.source.record_kind, "source");
        assert!(case.sources.iter().any(|source| {
            source.record.dataset_key == dvids_source_key
                && source.record.id == evidence.source.record_id
        }));
        assert!(matches!(
            evidence.locator,
            SourceLocator::Other { ref description }
                if description == "DVIDS video ID 960331; page-listed length 00:17:27."
        ));
        assert_eq!(evidence.evidence_kind, EvidenceKind::Video);
        assert_eq!(
            evidence.description,
            "Video referenced by the DVIDS record. The video itself was not played or independently inspected for this dataset."
        );
    }

    #[test]
    fn claim_and_evidence_snapshot_references_must_resolve_in_selected_manifest() {
        let claim_key = "xenoscope.case.al-taqaddum-2017:claim:aaro-balloon-assessment-v2";
        let evidence_key = "xenoscope.case.al-taqaddum-2017:evidence:dvids-video-960331";
        let cases = [
            (
                claim_key,
                "sourceKey",
                "xenoscope.case.al-taqaddum-2017:source:missing",
                "missing stable key",
            ),
            (
                claim_key,
                "sourceKey",
                "xenoscope.case.al-taqaddum-2017:claim:aaro-event-date-v2",
                "expected source",
            ),
            (
                claim_key,
                "incidentKey",
                "xenoscope.case.al-taqaddum-2017:incident:missing",
                "missing stable key",
            ),
            (
                claim_key,
                "incidentKey",
                "xenoscope.case.al-taqaddum-2017:source:aaro-report",
                "expected incident",
            ),
            (
                claim_key,
                "authorStatedConfidence.sourceKey",
                "xenoscope.case.al-taqaddum-2017:source:missing",
                "missing stable key",
            ),
            (
                claim_key,
                "authorStatedConfidence.sourceKey",
                "xenoscope.case.al-taqaddum-2017:claim:aaro-event-date-v2",
                "expected source",
            ),
            (
                evidence_key,
                "sourceKey",
                "xenoscope.case.al-taqaddum-2017:source:missing",
                "missing stable key",
            ),
            (
                evidence_key,
                "sourceKey",
                "xenoscope.case.al-taqaddum-2017:claim:aaro-event-date-v2",
                "expected source",
            ),
        ];

        for (record_key, reference_path, reference, expected_error) in cases {
            let database = Database::in_memory().expect("initialize isolated database");
            import_bundled_dataset(&database).expect("import v2 fixture");
            mutate_snapshot(&database, record_key, |snapshot| match reference_path {
                "authorStatedConfidence.sourceKey" => {
                    snapshot["authorStatedConfidence"]["sourceKey"] = serde_json::json!(reference);
                }
                field => snapshot[field] = serde_json::json!(reference),
            });

            let error = read_research_case(&database, BUNDLED_DATASET_ID)
                .expect_err("invalid snapshot reference should fail");
            assert!(
                error.contains(expected_error),
                "unexpected error for {record_key} {reference_path}: {error}"
            );
        }
    }

    #[test]
    fn malformed_claim_and_evidence_snapshots_fail_explicitly() {
        let claim_key = "xenoscope.case.al-taqaddum-2017:claim:aaro-balloon-assessment-v2";
        let evidence_key = "xenoscope.case.al-taqaddum-2017:evidence:dvids-video-960331";
        for (record_key, malformed_snapshot) in [
            (
                claim_key,
                serde_json::json!({ "assertionText": "incomplete" }),
            ),
            (
                evidence_key,
                serde_json::json!({ "description": "incomplete" }),
            ),
        ] {
            let database = Database::in_memory().expect("initialize isolated database");
            import_bundled_dataset(&database).expect("import v2 fixture");
            {
                let connection = database.connection().expect("lock isolated database");
                connection
                    .execute(
                        "UPDATE dataset_record_mappings SET content_snapshot_json = ?1
                         WHERE dataset_id = ?2 AND record_key = ?3",
                        params![
                            malformed_snapshot.to_string(),
                            BUNDLED_DATASET_ID,
                            record_key
                        ],
                    )
                    .expect("persist incomplete snapshot");
            }
            let error = read_research_case(&database, BUNDLED_DATASET_ID)
                .expect_err("incomplete snapshot should fail");
            assert!(error.contains("content snapshot") && error.contains("malformed"));
        }
    }

    #[test]
    fn incomplete_snapshot_author_confidence_fails_explicitly() {
        let database = Database::in_memory().expect("initialize isolated database");
        import_bundled_dataset(&database).expect("import v2 fixture");
        let claim_key = "xenoscope.case.al-taqaddum-2017:claim:aaro-balloon-assessment-v2";
        mutate_snapshot(&database, claim_key, |snapshot| {
            snapshot["authorStatedConfidence"] = serde_json::json!({
                "statedBy": "AARO",
                "sourceKey": "xenoscope.case.al-taqaddum-2017:source:aaro-report"
            });
        });

        let error = read_research_case(&database, BUNDLED_DATASET_ID)
            .expect_err("incomplete confidence snapshot should fail");
        assert!(error.contains("claim content snapshot") && error.contains("malformed"));
    }

    #[test]
    fn malformed_v2_incident_snapshots_and_source_references_fail_explicitly() {
        for mutation in ["malformed_snapshot", "missing_source", "wrong_kind"] {
            let database = Database::in_memory().expect("initialize isolated database");
            import_bundled_dataset(&database).expect("import v2 fixture");
            let incident_key =
                "xenoscope.case.al-taqaddum-2017:incident:al-taqaddum-object-report-v2";

            let mut connection = database.connection().expect("lock isolated database");
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .expect("begin test snapshot mutation");
            let mut snapshot_json: String = transaction
                .query_row(
                    "SELECT content_snapshot_json FROM dataset_record_mappings
                     WHERE dataset_id = ?1 AND record_key = ?2",
                    params![BUNDLED_DATASET_ID, incident_key],
                    |row| row.get(0),
                )
                .expect("read incident snapshot");
            if mutation == "malformed_snapshot" {
                snapshot_json = serde_json::json!({"label": 42}).to_string();
            } else {
                let mut snapshot: serde_json::Value =
                    serde_json::from_str(&snapshot_json).expect("parse incident snapshot");
                match mutation {
                    "missing_source" => {
                        snapshot["sourceKeys"][0] =
                            serde_json::json!("xenoscope.case.al-taqaddum-2017:source:missing");
                    }
                    "wrong_kind" => {
                        snapshot["sourceKeys"][0] = serde_json::json!(
                            "xenoscope.case.al-taqaddum-2017:claim:aaro-event-date-v2"
                        );
                    }
                    _ => unreachable!(),
                }
                snapshot_json =
                    serde_json::to_string(&snapshot).expect("serialize incident mutation");
            }
            transaction
                .execute(
                    "UPDATE dataset_record_mappings SET content_snapshot_json = ?1
                     WHERE dataset_id = ?2 AND record_key = ?3",
                    params![snapshot_json, BUNDLED_DATASET_ID, incident_key],
                )
                .expect("persist malformed incident snapshot");
            transaction.commit().expect("commit test snapshot mutation");
            drop(connection);

            let error = read_research_case(&database, BUNDLED_DATASET_ID)
                .expect_err("invalid incident snapshot/reference should fail");
            match mutation {
                "malformed_snapshot" => {
                    assert!(error.contains("content snapshot") && error.contains("malformed"))
                }
                "missing_source" => assert!(error.contains("missing stable key")),
                "wrong_kind" => {
                    assert!(error.contains("resolves to claim"));
                    assert!(error.contains("expected source"));
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn reads_imported_fixture_records_references_and_serializes_camel_case() {
        let database = Database::in_memory().expect("initialize isolated database");
        seed_bundled_legacy_dataset_for_tests(&database).expect("seed legacy fixture");
        let case = read_research_case(&database, BUNDLED_DATASET_ID)
            .expect("read imported case")
            .expect("case exists");

        assert_eq!(case.dataset_id, BUNDLED_DATASET_ID);
        assert_eq!(case.dataset_version, "1.0.0");
        assert_eq!(case.import_sequence, 1);
        assert_eq!(case.incidents.len(), 1);
        assert_eq!(case.sources.len(), 2);
        assert_eq!(case.evidence_items.len(), 1);
        assert_eq!(case.claims.len(), 10);
        assert_eq!(case.relationships.len(), 2);
        assert_eq!(case.discrepancies.len(), 2);
        assert!(case.findings.is_empty());

        let incident = &case.incidents[0];
        assert_eq!(
            incident.record.dataset_key,
            "xenoscope.case.al-taqaddum-2017:incident:al-taqaddum-object-report"
        );
        assert_eq!(incident.description, None);
        assert_eq!(incident.location, None);
        assert_eq!(incident.time_period, None);
        assert_eq!(incident.sources, None);
        assert_eq!(incident.record.record_kind, "incident");
        assert_ne!(incident.record.dataset_key, incident.record.id);

        let confidence_claim = case
            .claims
            .iter()
            .find(|claim| claim.author_stated_confidence.is_some())
            .expect("fixture has author-stated confidence");
        assert!(case.claims.iter().any(|claim| {
            claim.author_stated_confidence.is_none() && claim.source.record_kind == "source"
        }));
        assert_eq!(
            confidence_claim.review.status,
            ClaimReviewStatus::Unreviewed
        );
        assert!(
            case.evidence_items
                .iter()
                .all(|evidence| evidence.source.record_kind == "source")
        );
        assert_eq!(
            confidence_claim
                .author_stated_confidence
                .as_ref()
                .unwrap()
                .source
                .record_kind,
            "source"
        );
        assert!(
            case.claims.iter().any(|claim| {
                matches!(claim.source_locator, SourceLocator::TextFragment { .. })
            })
        );
        assert!(matches!(
            case.evidence_items[0].locator,
            SourceLocator::Other { .. }
        ));
        assert!(case.discrepancies.iter().all(|discrepancy| {
            discrepancy.status == DiscrepancyStatus::Open && discrepancy.claim_references.len() == 2
        }));
        assert_eq!(case.metadata.source_access.len(), 2);
        assert_eq!(case.metadata.relationship_notes.len(), 2);

        let json = serde_json::to_value(&case).expect("serialize case DTO");
        assert!(json.get("datasetId").is_some());
        assert!(json.get("datasetVersion").is_some());
        assert!(json.get("importSequence").is_some());
        assert!(json.get("evidenceItems").is_some());
        assert!(json["incidents"][0]["description"].is_null());
        assert!(json["incidents"][0]["location"].is_null());
        assert!(json["incidents"][0]["timePeriod"].is_null());
        assert!(json["incidents"][0]["sources"].is_null());
        assert!(json["claims"][0].get("sourceLocator").is_some());
        assert!(json["metadata"].get("sourceAccess").is_some());
        assert!(json["metadata"].get("relationshipNotes").is_some());
        assert!(json["claims"][0].get("source_locator").is_none());
    }

    #[test]
    fn reads_test_only_finding_and_its_linked_claim() {
        let database = Database::in_memory().expect("initialize isolated database");
        seed_bundled_legacy_dataset_for_tests(&database).expect("seed legacy fixture");
        let finding_id = Uuid::new_v4().to_string();
        let finding_key = format!("{BUNDLED_DATASET_ID}:finding:test-only");
        let mut connection = database.connection().expect("lock isolated database");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("begin test-only finding transaction");
        let claim_id: String = transaction
            .query_row(
                "SELECT record_id FROM dataset_record_mappings
                 WHERE dataset_id = ?1 AND record_kind = 'claim'
                 ORDER BY record_key LIMIT 1",
                [BUNDLED_DATASET_ID],
                |row| row.get(0),
            )
            .expect("select a real imported claim");
        transaction
            .execute(
                "INSERT INTO records (record_id, record_kind, created_at, created_by_kind)
                 VALUES (?1, 'finding', '2025-01-01T00:00:00Z', 'system')",
                [&finding_id],
            )
            .expect("insert test-only finding identity");
        transaction
            .execute(
                "INSERT INTO findings (record_id, assessment, attribution_kind, review_status)
                 VALUES (?1, 'Test-only assessment', 'system', 'unreviewed')",
                [&finding_id],
            )
            .expect("insert test-only finding");
        transaction
            .execute(
                "INSERT INTO finding_claim_references (finding_id, claim_id, reference_role)
                 VALUES (?1, ?2, 'supporting')",
                params![finding_id, claim_id],
            )
            .expect("link test-only finding to imported claim");
        transaction
            .execute(
                "INSERT INTO dataset_record_mappings
                    (dataset_id, record_key, record_kind, record_id, content_snapshot_json)
                 VALUES (?1, ?2, 'finding', ?3, '{}')",
                params![BUNDLED_DATASET_ID, finding_key, finding_id],
            )
            .expect("map test-only finding");
        let mut manifest =
            load_manifest(&transaction, BUNDLED_DATASET_ID, 1).expect("load actual manifest");
        manifest.push(finding_key);
        manifest.sort_unstable();
        let manifest_json = serde_json::to_string(&manifest).expect("serialize test manifest");
        transaction
            .execute(
                "UPDATE dataset_imports SET record_keys_json = ?1
                 WHERE dataset_id = ?2 AND import_sequence = 1",
                params![manifest_json, BUNDLED_DATASET_ID],
            )
            .expect("update isolated test manifest");
        transaction.commit().expect("commit test-only finding");
        drop(connection);

        let case = read_research_case(&database, BUNDLED_DATASET_ID)
            .expect("read test finding")
            .expect("case exists");
        assert_eq!(case.findings.len(), 1);
        let finding = &case.findings[0];
        assert_eq!(finding.record.id, finding_id);
        assert_eq!(finding.assessment, "Test-only assessment");
        assert_eq!(finding.claim_references.len(), 1);
        assert_eq!(finding.claim_references[0].record.record_id, claim_id);
        assert_eq!(
            finding.claim_references[0].role,
            crate::research_dataset::FindingReferenceRole::Supporting
        );
    }

    #[test]
    fn relationships_and_all_stable_references_resolve_to_database_ids_and_kinds() {
        let database = Database::in_memory().expect("initialize isolated database");
        seed_bundled_legacy_dataset_for_tests(&database).expect("seed legacy fixture");
        let case = read_research_case(&database, BUNDLED_DATASET_ID)
            .expect("read imported case")
            .expect("case exists");
        let known_ids = case
            .incidents
            .iter()
            .map(|record| record.record.id.as_str())
            .chain(case.sources.iter().map(|record| record.record.id.as_str()))
            .chain(
                case.evidence_items
                    .iter()
                    .map(|record| record.record.id.as_str()),
            )
            .chain(case.claims.iter().map(|record| record.record.id.as_str()))
            .chain(
                case.relationships
                    .iter()
                    .map(|record| record.record.id.as_str()),
            )
            .chain(
                case.discrepancies
                    .iter()
                    .map(|record| record.record.id.as_str()),
            )
            .collect::<HashSet<_>>();
        for id in &known_ids {
            Uuid::parse_str(id).expect("persisted record id is a UUID");
        }
        for relationship in &case.relationships {
            assert!(known_ids.contains(relationship.from.record_id.as_str()));
            assert!(known_ids.contains(relationship.to.record_id.as_str()));
            assert_ne!(relationship.from.record_kind, "");
            assert_ne!(relationship.to.record_kind, "");
        }
        for claim in &case.claims {
            assert!(known_ids.contains(claim.source.record_id.as_str()));
            if let Some(incident) = &claim.incident {
                assert!(known_ids.contains(incident.record_id.as_str()));
            }
        }
        for evidence in &case.evidence_items {
            assert!(known_ids.contains(evidence.source.record_id.as_str()));
        }
        for discrepancy in &case.discrepancies {
            assert!(
                discrepancy
                    .claim_references
                    .iter()
                    .all(|reference| known_ids.contains(reference.record_id.as_str()))
            );
        }
    }

    #[test]
    fn relationship_and_discrepancy_content_comes_from_snapshots_after_live_edits() {
        let database = Database::in_memory().expect("initialize isolated database");
        import_bundled_dataset(&database).expect("import v2 fixture");
        let original = read_research_case(&database, BUNDLED_DATASET_ID)
            .expect("read original case")
            .expect("case exists");
        let relationship = &original.relationships[0];
        let discrepancy = &original.discrepancies[0];
        let relationship_id = relationship.record.id.clone();
        let discrepancy_id = discrepancy.record.id.clone();
        let expected_relationship_from = "xenoscope.case.al-taqaddum-2017:source:aaro-report";
        let expected_relationship_to = "xenoscope.case.al-taqaddum-2017:source:dvids-record";
        let expected_discrepancy_claims = [
            "xenoscope.case.al-taqaddum-2017:claim:aaro-event-date-v2",
            "xenoscope.case.al-taqaddum-2017:claim:dvids-date-taken-v2",
        ];
        let expected_discrepancy_description = "AARO identifies the event date as 23 October 2017. DVIDS displays the literal Date Taken value 10.01.2017. The notation and meaning of the DVIDS field have not been established here; the difference is unresolved and is not silently interpreted as a confirmed event date.";
        let alternate_from_id = original.incidents[0].record.id.clone();
        let alternate_to_id = original.claims[0].record.id.clone();
        let excluded_claim_ids = discrepancy
            .claim_references
            .iter()
            .map(|reference| reference.record_id.clone())
            .collect::<HashSet<_>>();
        let alternate_discrepancy_claim_id = original
            .claims
            .iter()
            .find(|claim| !excluded_claim_ids.contains(&claim.record.id))
            .expect("fixture has another claim")
            .record
            .id
            .clone();

        {
            let connection = database.connection().expect("lock isolated database");
            connection
                .execute(
                    "UPDATE relationships
                     SET relationship_kind = 'corrects',
                         from_record_id = ?1, from_record_kind = 'incident',
                         to_record_id = ?2, to_record_kind = 'claim'
                     WHERE record_id = ?3",
                    params![alternate_from_id, alternate_to_id, relationship_id],
                )
                .expect("edit live relationship content");
            connection
                .execute(
                    "UPDATE discrepancies SET description = 'Edited live description',
                         status = 'investigating'
                     WHERE record_id = ?1",
                    [&discrepancy_id],
                )
                .expect("edit live discrepancy description and status");
            connection
                .execute(
                    "DELETE FROM discrepancy_claims WHERE discrepancy_id = ?1",
                    [&discrepancy_id],
                )
                .expect("remove live discrepancy claim links");
            connection
                .execute(
                    "INSERT INTO discrepancy_claims (discrepancy_id, claim_id)
                     VALUES (?1, ?2)",
                    params![discrepancy_id, alternate_discrepancy_claim_id],
                )
                .expect("add different live discrepancy claim link");
        }

        let updated = read_research_case(&database, BUNDLED_DATASET_ID)
            .expect("read case after live edits")
            .expect("case exists");
        let updated_relationship = updated
            .relationships
            .iter()
            .find(|item| item.record.id == relationship_id)
            .expect("relationship remains available");
        assert_eq!(
            updated_relationship.relationship_kind,
            RelationshipKind::Analyses
        );
        assert_eq!(
            updated_relationship.from.dataset_key,
            expected_relationship_from
        );
        assert_eq!(
            updated_relationship.to.dataset_key,
            expected_relationship_to
        );
        let updated_discrepancy = updated
            .discrepancies
            .iter()
            .find(|item| item.record.id == discrepancy_id)
            .expect("discrepancy remains available");
        assert_eq!(
            updated_discrepancy.description,
            expected_discrepancy_description
        );
        assert_eq!(
            updated_discrepancy
                .claim_references
                .iter()
                .map(|reference| reference.dataset_key.as_str())
                .collect::<Vec<_>>(),
            expected_discrepancy_claims
        );
        assert_eq!(updated_discrepancy.status, DiscrepancyStatus::Investigating);
    }

    #[test]
    fn relationship_and_discrepancy_snapshots_reject_malformed_or_invalid_references() {
        let relationship_key =
            "xenoscope.case.al-taqaddum-2017:relationship:aaro-analyses-dvids-video";
        let discrepancy_key = "xenoscope.case.al-taqaddum-2017:discrepancy:event-date-v2";
        let cases = [
            (relationship_key, "malformed"),
            (relationship_key, "missing_endpoint"),
            (relationship_key, "wrong_kind_endpoint"),
            (relationship_key, "self_reference"),
            (discrepancy_key, "malformed"),
            (discrepancy_key, "missing_claim"),
            (discrepancy_key, "wrong_kind_claim"),
            (discrepancy_key, "no_claims"),
            (discrepancy_key, "one_claim"),
            (discrepancy_key, "duplicate_claim"),
        ];

        for (record_key, mutation) in cases {
            let database = Database::in_memory().expect("initialize isolated database");
            import_bundled_dataset(&database).expect("import v2 fixture");
            match mutation {
                "malformed" => {
                    let connection = database.connection().expect("lock isolated database");
                    connection
                        .execute(
                            "UPDATE dataset_record_mappings SET content_snapshot_json = '{}'
                             WHERE dataset_id = ?1 AND record_key = ?2",
                            params![BUNDLED_DATASET_ID, record_key],
                        )
                        .expect("persist malformed snapshot");
                }
                "missing_endpoint" => {
                    mutate_snapshot(&database, record_key, |snapshot| {
                        snapshot["from"]["recordKey"] =
                            serde_json::json!(format!("{BUNDLED_DATASET_ID}:source:missing"));
                    });
                }
                "wrong_kind_endpoint" => {
                    mutate_snapshot(&database, record_key, |snapshot| {
                        snapshot["from"]["recordKind"] = serde_json::json!("source");
                        snapshot["from"]["recordKey"] = serde_json::json!(
                            "xenoscope.case.al-taqaddum-2017:claim:aaro-event-date-v2"
                        );
                    });
                }
                "self_reference" => {
                    mutate_snapshot(&database, record_key, |snapshot| {
                        snapshot["to"] = snapshot["from"].clone();
                    });
                }
                "missing_claim" => {
                    mutate_snapshot(&database, record_key, |snapshot| {
                        snapshot["claimKeys"][0] =
                            serde_json::json!(format!("{BUNDLED_DATASET_ID}:claim:missing"));
                    });
                }
                "wrong_kind_claim" => {
                    mutate_snapshot(&database, record_key, |snapshot| {
                        snapshot["claimKeys"][0] =
                            serde_json::json!("xenoscope.case.al-taqaddum-2017:source:aaro-report");
                    });
                }
                "no_claims" => {
                    mutate_snapshot(&database, record_key, |snapshot| {
                        snapshot["claimKeys"] = serde_json::json!([]);
                    });
                }
                "one_claim" => {
                    mutate_snapshot(&database, record_key, |snapshot| {
                        snapshot["claimKeys"] = serde_json::json!([
                            "xenoscope.case.al-taqaddum-2017:claim:aaro-event-date-v2"
                        ]);
                    });
                }
                "duplicate_claim" => {
                    mutate_snapshot(&database, record_key, |snapshot| {
                        snapshot["claimKeys"][1] = snapshot["claimKeys"][0].clone();
                    });
                }
                _ => unreachable!(),
            }

            let error = read_research_case(&database, BUNDLED_DATASET_ID)
                .expect_err("invalid snapshot or reference should fail");
            match mutation {
                "malformed" => {
                    assert!(
                        error.contains("content snapshot") && error.contains("malformed"),
                        "unexpected error: {error}"
                    );
                }
                "missing_endpoint" | "missing_claim" => {
                    assert!(
                        error.contains("missing stable key"),
                        "unexpected error: {error}"
                    );
                }
                "wrong_kind_endpoint" => {
                    assert!(
                        error.contains("resolves to claim") && error.contains("expected source"),
                        "unexpected error: {error}"
                    );
                }
                "self_reference" => {
                    assert!(
                        error.contains("same record at both endpoints"),
                        "unexpected error: {error}"
                    );
                }
                "wrong_kind_claim" => {
                    assert!(
                        error.contains("resolves to source") && error.contains("expected claim"),
                        "unexpected error: {error}"
                    );
                }
                "no_claims" | "one_claim" => {
                    assert!(
                        error.contains("at least two claims"),
                        "unexpected error: {error}"
                    );
                }
                "duplicate_claim" => {
                    assert!(
                        error.contains("duplicate claim keys"),
                        "unexpected error: {error}"
                    );
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn relationship_and_discrepancy_references_are_limited_to_selected_manifest() {
        let original_database = Database::in_memory().expect("initialize isolated database");
        import_bundled_dataset(&original_database).expect("import v2 fixture");
        let original = read_research_case(&original_database, BUNDLED_DATASET_ID)
            .expect("read original case")
            .expect("case exists");
        assert!(!original.relationships.is_empty());
        assert!(!original.discrepancies.is_empty());

        let relationship_with_claim_endpoint = original
            .relationships
            .iter()
            .find(|relationship| relationship.to.record_kind == "claim")
            .expect("fixture has a relationship with a claim endpoint");
        let exclusions = [
            (
                original.discrepancies[0].claim_references[0]
                    .dataset_key
                    .clone(),
                original.discrepancies[0].record.dataset_key.clone(),
            ),
            (
                relationship_with_claim_endpoint.to.dataset_key.clone(),
                relationship_with_claim_endpoint.record.dataset_key.clone(),
            ),
        ];

        for (excluded_key, expected_owner_key) in exclusions {
            let database = Database::in_memory().expect("initialize isolated database");
            import_bundled_dataset(&database).expect("import v2 fixture");
            let mut connection = database.connection().expect("lock isolated database");
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .expect("begin isolated later-manifest transaction");
            let mut later_manifest =
                load_manifest(&transaction, BUNDLED_DATASET_ID, original.import_sequence)
                    .expect("load original manifest");
            later_manifest.retain(|key| key != &excluded_key);
            transaction
                .execute(
                    "INSERT INTO dataset_imports (
                        dataset_id, dataset_version, import_sequence, format, format_version,
                        imported_at, metadata_json, record_keys_json
                     )
                     SELECT dataset_id, '2.1.0', import_sequence + 1, format, format_version,
                            '2026-01-01T00:00:00Z', metadata_json, ?1
                     FROM dataset_imports
                     WHERE dataset_id = ?2 AND import_sequence = ?3",
                    params![
                        serde_json::to_string(&later_manifest).expect("serialize later manifest"),
                        BUNDLED_DATASET_ID,
                        original.import_sequence
                    ],
                )
                .expect("insert later manifest omitting a referenced record");
            transaction
                .commit()
                .expect("commit isolated later-manifest transaction");
            drop(connection);

            let error = read_research_case(&database, BUNDLED_DATASET_ID)
                .expect_err("later manifest must not resolve an excluded record");
            assert!(error.contains(&format!("missing stable key {excluded_key:?}")));
            assert!(error.contains(&expected_owner_key));
        }
    }

    #[test]
    fn selects_latest_import_by_sequence_and_persists_case_across_reopen() {
        let path: PathBuf =
            std::env::temp_dir().join(format!("xenoscope-case-read-{}.sqlite3", Uuid::new_v4()));
        let original_case = {
            let database = Database::open(&path).expect("open isolated temporary database");
            seed_bundled_legacy_dataset_for_tests(&database).expect("seed legacy fixture");
            let mut connection = database.connection().expect("lock isolated database");
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .expect("begin test-only version transaction");
            transaction
                .execute(
                    "INSERT INTO dataset_imports (
                        dataset_id, dataset_version, import_sequence, format, format_version,
                        imported_at, metadata_json, record_keys_json
                     )
                     SELECT dataset_id, '0.1.0', import_sequence + 1, format, format_version,
                            '1900-01-01T00:00:00Z', metadata_json, record_keys_json
                     FROM dataset_imports
                     WHERE dataset_id = ?1 AND import_sequence = 1",
                    [BUNDLED_DATASET_ID],
                )
                .expect("insert test newer sequence with older timestamp");
            transaction.commit().expect("commit test-only version");
            drop(connection);
            read_research_case(&database, BUNDLED_DATASET_ID)
                .expect("read latest test version")
                .expect("latest version exists")
        };

        assert_eq!(original_case.dataset_version, "0.1.0");
        let reopened = Database::open(&path).expect("reopen isolated temporary database");
        let persisted_case = read_research_case(&reopened, BUNDLED_DATASET_ID)
            .expect("read persisted case")
            .expect("persisted case exists");
        assert_eq!(persisted_case, original_case);
        assert_eq!(scalar(&reopened, "SELECT COUNT(*) FROM records"), 36);
        drop(reopened);
        fs::remove_file(path).expect("remove isolated database file");
    }

    #[test]
    fn missing_required_domain_data_is_reported() {
        let database = Database::in_memory().expect("initialize isolated database");
        seed_bundled_legacy_dataset_for_tests(&database).expect("seed legacy fixture");
        {
            let connection = database.connection().expect("lock isolated database");
            connection
                .pragma_update(None, "foreign_keys", "OFF")
                .expect("disable foreign keys for corruption test");
            connection
                .execute(
                    "DELETE FROM sources WHERE record_id = (
                    SELECT record_id FROM dataset_record_mappings
                    WHERE record_key = ?1
                )",
                    ["xenoscope.case.al-taqaddum-2017:source:aaro-report"],
                )
                .expect("remove source domain row");
            connection
                .pragma_update(None, "foreign_keys", "ON")
                .expect("restore foreign keys after corruption test");
        }
        let error = read_research_case(&database, BUNDLED_DATASET_ID)
            .expect_err("missing source row should fail");
        assert!(error.contains("Missing source domain data"));
    }

    #[test]
    fn case_library_uses_latest_sequence_and_counts_only_that_manifest() {
        let database = Database::in_memory().expect("initialize isolated database");
        seed_bundled_legacy_dataset_for_tests(&database).expect("seed legacy fixture");
        let dataset_id = "xenoscope.case.test-library-sequence";
        clone_bundled_dataset(&database, dataset_id);

        let connection = database.connection().expect("lock isolated database");
        connection
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 )
                 SELECT ?1, '0.1.0', 2, format, format_version,
                        '1900-01-01T00:00:00Z', metadata_json, record_keys_json
                 FROM dataset_imports WHERE dataset_id = ?1 AND import_sequence = 1",
                [dataset_id],
            )
            .expect("insert newer sequence with older timestamp and lexically earlier label");
        let mut keys: Vec<String> = connection
            .query_row(
                "SELECT record_keys_json FROM dataset_imports
                 WHERE dataset_id = ?1 AND import_sequence = 2",
                [dataset_id],
                |row| {
                    let json: String = row.get(0)?;
                    serde_json::from_str(&json).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            json.len(),
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })
                },
            )
            .expect("read test version manifest");
        let removed_key = "xenoscope.case.al-taqaddum-2017:discrepancy:event-date";
        keys.retain(|key| key != removed_key);
        connection
            .execute(
                "UPDATE dataset_imports SET record_keys_json = ?1
                 WHERE dataset_id = ?2 AND import_sequence = 2",
                params![
                    serde_json::to_string(&keys).expect("serialize newer manifest"),
                    dataset_id
                ],
            )
            .expect("remove one old-version mapping from latest manifest");
        connection
            .execute(
                "UPDATE discrepancies SET status = 'investigating'
                 WHERE record_id = (
                    SELECT record_id FROM dataset_record_mappings
                    WHERE dataset_id = ?1
                      AND record_key = 'xenoscope.case.al-taqaddum-2017:discrepancy:recording-duration'
                 )",
                [dataset_id],
            )
            .expect("set stored status for remaining discrepancy");
        drop(connection);

        let entry = read_research_case_library(&database)
            .expect("read isolated library")
            .into_iter()
            .find(|entry| entry.dataset_id == dataset_id)
            .expect("find isolated library entry");
        assert_eq!(entry.dataset_version, "0.1.0");
        assert_eq!(entry.import_sequence, 2);
        assert_eq!(entry.imported_at, "1900-01-01T00:00:00Z");
        assert_eq!(entry.record_counts.discrepancies, 1);
        assert_eq!(entry.discrepancy_status_counts.open, 0);
        assert_eq!(entry.discrepancy_status_counts.investigating, 1);
        assert_eq!(entry.discrepancy_status_counts.documented, 0);
        assert_eq!(
            entry.incident_labels,
            vec!["Al Taqaddum object report, October 2017"]
        );
        assert_eq!(entry.record_counts.sources, 2);
        assert_eq!(entry.record_counts.claims, 10);
    }

    #[test]
    fn case_library_resolves_multiple_incident_labels_from_selected_manifest() {
        let database = Database::in_memory().expect("initialize isolated database");
        seed_bundled_legacy_dataset_for_tests(&database).expect("seed legacy fixture");
        let dataset_id = "xenoscope.case.test-library-multiple-incidents";
        clone_bundled_dataset(&database, dataset_id);

        let connection = database.connection().expect("lock isolated database");
        let transaction = connection
            .unchecked_transaction()
            .expect("begin isolated incident transaction");
        let id = Uuid::new_v4().to_string();
        let key = format!("{dataset_id}:incident:second-report");
        transaction
            .execute(
                "INSERT INTO records (record_id, record_kind, created_at, created_by_kind)
                 VALUES (?1, 'incident', '2026-01-01T00:00:00Z', 'system')",
                [&id],
            )
            .expect("insert second incident identity");
        transaction
            .execute(
                "INSERT INTO incidents (record_id, label) VALUES (?1, 'Second stored report label')",
                [&id],
            )
            .expect("insert second incident");
        transaction
            .execute(
                "INSERT INTO dataset_record_mappings
                    (dataset_id, record_key, record_kind, record_id, content_snapshot_json)
                 VALUES (?1, ?2, 'incident', ?3, '{\"label\":\"Second stored report label\"}')",
                params![dataset_id, key, id],
            )
            .expect("map second incident");
        let mut manifest =
            load_manifest(&transaction, dataset_id, 1).expect("load cloned manifest");
        manifest.push(key);
        manifest.sort_unstable();
        transaction
            .execute(
                "UPDATE dataset_imports SET record_keys_json = ?1
                 WHERE dataset_id = ?2 AND import_sequence = 1",
                params![
                    serde_json::to_string(&manifest).expect("serialize manifest"),
                    dataset_id
                ],
            )
            .expect("add second incident to selected manifest");
        transaction.commit().expect("commit second incident");
        drop(connection);

        let entry = read_research_case_library(&database)
            .expect("read isolated library")
            .into_iter()
            .find(|entry| entry.dataset_id == dataset_id)
            .expect("find isolated library entry");
        assert_eq!(entry.record_counts.incidents, 2);
        assert_eq!(
            entry.incident_labels,
            vec![
                "Al Taqaddum object report, October 2017",
                "Second stored report label"
            ]
        );
    }

    #[test]
    fn case_library_uses_dataset_id_when_no_incident_labels_exist() {
        let database = Database::in_memory().expect("initialize isolated database");
        let dataset_id = "xenoscope.case.test-library-empty";
        database
            .connection()
            .expect("lock isolated database")
            .execute_batch(&format!(
                "INSERT INTO research_datasets (dataset_id) VALUES ('{dataset_id}');
                 INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES (
                    '{dataset_id}', 'unlabelled', 1, 'xenoscope-research-dataset', 1,
                    '2026-01-01T00:00:00Z',
                    '{{\"referenceConvention\":{{\"recordIdentity\":\"keys\",\"references\":\"keys\",\"omittedRecordMetadata\":\"none\"}},\"sourceAccess\":[],\"sourceMetadata\":[],\"relationshipNotes\":[]}}',
                    '[]'
                 );"
            ))
            .expect("insert empty persisted dataset");

        let entry = read_research_case_library(&database)
            .expect("read empty-case library")
            .into_iter()
            .next()
            .expect("empty case remains in library");
        assert_eq!(entry.dataset_id, dataset_id);
        assert!(entry.incident_labels.is_empty());
        assert_eq!(entry.record_counts.incidents, 0);
    }

    #[test]
    fn generic_case_reader_reads_persisted_dataset_and_rejects_unknown_identity() {
        let database = Database::in_memory().expect("initialize isolated database");
        seed_bundled_legacy_dataset_for_tests(&database).expect("seed legacy fixture");
        let dataset_id = "xenoscope.case.test-generic-read";
        clone_bundled_dataset(&database, dataset_id);

        let case = read_persisted_research_case(&database, dataset_id)
            .expect("read persisted generic dataset");
        assert_eq!(case.dataset_id, dataset_id);
        assert_eq!(case.incidents.len(), 1);
        assert_eq!(
            read_persisted_research_case(&database, "not-persisted")
                .expect_err("unknown dataset identity should be rejected"),
            "Dataset \"not-persisted\" has no persisted imports"
        );
    }

    #[test]
    fn case_library_rejects_manifest_references_without_mappings() {
        let database = Database::in_memory().expect("initialize isolated database");
        seed_bundled_legacy_dataset_for_tests(&database).expect("seed legacy fixture");
        let connection = database.connection().expect("lock isolated database");
        let mut keys: Vec<String> = connection
            .query_row(
                "SELECT record_keys_json FROM dataset_imports
                 WHERE dataset_id = ?1 AND import_sequence = 1",
                [BUNDLED_DATASET_ID],
                |row| {
                    let json: String = row.get(0)?;
                    serde_json::from_str(&json).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            json.len(),
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })
                },
            )
            .expect("read actual manifest");
        keys.push(format!("{BUNDLED_DATASET_ID}:source:missing-mapping"));
        connection
            .execute(
                "UPDATE dataset_imports SET record_keys_json = ?1
                 WHERE dataset_id = ?2 AND import_sequence = 1",
                params![
                    serde_json::to_string(&keys).expect("serialize corrupted manifest"),
                    BUNDLED_DATASET_ID
                ],
            )
            .expect("corrupt only isolated manifest");
        drop(connection);

        let error = read_research_case_library(&database)
            .expect_err("missing manifest mapping should fail library read");
        assert!(error.contains("manifest references missing mapping"));
    }
}
