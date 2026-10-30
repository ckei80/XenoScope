use crate::database::{Database, current_timestamp, enum_value, researcher_actor};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

const SNAPSHOT_FORMAT_VERSION: u32 = 1;
const UNSCORED_PROFILE_VERSION: &str = "not_scored";
const UNSCORED_ALGORITHM_VERSION: &str = "not_scored";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AssessmentScopeRole {
    Core,
    Context,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ClaimOutcome {
    Supported,
    Contradicted,
    Mixed,
    Unexamined,
    Indeterminate,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EvidenceAssessmentState {
    Supports,
    Conflicts,
    NotExamined,
    Unavailable,
    IrrelevantToScope,
    Indeterminate,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceDependencyState {
    Dependent,
    AssessedIndependent,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AssessmentFactor {
    EvidenceQuality,
    Provenance,
    IndependentCorroboration,
    Consistency,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AssessmentClaimInput {
    pub claim_key: String,
    pub scope_role: AssessmentScopeRole,
    pub outcome: ClaimOutcome,
    pub rationale: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AssessmentClaimEvidenceInput {
    pub claim_key: String,
    pub evidence_key: Option<String>,
    pub state: EvidenceAssessmentState,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AssessmentSourceDependencyInput {
    pub source_a_key: String,
    pub source_b_key: String,
    pub state: SourceDependencyState,
    pub rationale: Option<String>,
    #[serde(default)]
    pub basis_references: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AssessmentFactorRatingInput {
    pub factor: AssessmentFactor,
    pub rating: Option<Value>,
    pub rationale: Option<String>,
    #[serde(default)]
    pub references: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CreateAssessmentInput {
    pub dataset_id: String,
    pub dataset_version: String,
    pub import_sequence: i64,
    #[serde(default)]
    pub claims: Vec<AssessmentClaimInput>,
    #[serde(default)]
    pub claim_evidence: Vec<AssessmentClaimEvidenceInput>,
    #[serde(default)]
    pub source_dependencies: Vec<AssessmentSourceDependencyInput>,
    #[serde(default)]
    pub factor_ratings: Vec<AssessmentFactorRatingInput>,
    #[serde(default)]
    pub supersedes_assessment_id: Option<String>,
    #[serde(default)]
    pub revision_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssessmentClaimDto {
    pub claim_key: String,
    pub claim_record_id: String,
    pub scope_role: AssessmentScopeRole,
    pub outcome: ClaimOutcome,
    pub rationale: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssessmentClaimEvidenceDto {
    pub claim_key: String,
    pub evidence_key: Option<String>,
    pub evidence_record_id: Option<String>,
    pub state: EvidenceAssessmentState,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssessmentSourceDependencyDto {
    pub source_a_key: String,
    pub source_a_id: String,
    pub source_b_key: String,
    pub source_b_id: String,
    pub state: SourceDependencyState,
    pub rationale: Option<String>,
    pub basis_references: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssessmentFactorRatingDto {
    pub factor: AssessmentFactor,
    pub rating: Option<Value>,
    pub rationale: Option<String>,
    pub references: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResearchAssessmentDto {
    pub assessment_id: String,
    pub dataset_id: String,
    pub dataset_version: String,
    pub import_sequence: i64,
    pub created_at: String,
    pub created_by_kind: String,
    pub created_by_name: Option<String>,
    pub status: String,
    pub score: Option<i64>,
    pub input_snapshot: Value,
    pub supersedes_assessment_id: Option<String>,
    pub revision_reason: Option<String>,
    pub claims: Vec<AssessmentClaimDto>,
    pub claim_evidence: Vec<AssessmentClaimEvidenceDto>,
    pub source_dependencies: Vec<AssessmentSourceDependencyDto>,
    pub factor_ratings: Vec<AssessmentFactorRatingDto>,
}

#[derive(Clone, Debug)]
struct ImportedRecord {
    key: String,
    kind: String,
    id: String,
    content_snapshot: Value,
    created_at: String,
    created_by_kind: String,
    created_by_name: Option<String>,
}

struct SelectedImport {
    imported_at: String,
    format: String,
    format_version: i64,
    metadata_json: String,
    metadata: Value,
    records: HashMap<String, ImportedRecord>,
}

pub(crate) fn create_preliminary_assessment(
    database: &Database,
    mut input: CreateAssessmentInput,
) -> Result<String, String> {
    if let Some(predecessor_id) = &input.supersedes_assessment_id {
        input.supersedes_assessment_id = Some(canonical_assessment_id(predecessor_id)?);
    }
    validate_input_shape(&input)?;

    let mut connection = database.connection()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| format!("Could not begin assessment creation: {error}"))?;

    let selected_import = load_selected_import(
        &transaction,
        &input.dataset_id,
        &input.dataset_version,
        input.import_sequence,
    )?;
    let claim_records = resolve_inputs(&input, &selected_import)?;
    validate_predecessor(&transaction, &input)?;
    let persisted_case = crate::research_dataset_read::assemble_research_case(
        &transaction,
        &input.dataset_id,
        input.dataset_version.clone(),
        input.import_sequence,
        &selected_import.format,
        selected_import.format_version,
        selected_import.imported_at.clone(),
        &selected_import.metadata_json,
    )?;

    let assessment_id = Uuid::new_v4().hyphenated().to_string();
    let created_at = current_timestamp(&transaction)?;
    let actor = researcher_actor();
    let actor_kind = enum_value(&actor.kind)?;
    let snapshot = build_input_snapshot(&input, &selected_import, &persisted_case)?;
    let snapshot_json = serde_json::to_string(&snapshot)
        .map_err(|error| format!("Could not encode assessment input snapshot: {error}"))?;
    let revision_reason = input
        .revision_reason
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    transaction
        .execute(
            "INSERT INTO research_assessments (
                assessment_id, dataset_id, dataset_version, import_sequence,
                created_at, created_by_kind, created_by_name, status, score,
                scoring_profile_version, scoring_profile_json, algorithm_version,
                input_snapshot_json, supersedes_assessment_id, revision_reason
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'preliminary', NULL,
                       ?8, '{}', ?9, ?10, ?11, ?12)",
            params![
                assessment_id,
                input.dataset_id,
                input.dataset_version,
                input.import_sequence,
                created_at,
                actor_kind,
                actor.display_name,
                UNSCORED_PROFILE_VERSION,
                UNSCORED_ALGORITHM_VERSION,
                snapshot_json,
                input.supersedes_assessment_id,
                revision_reason,
            ],
        )
        .map_err(|error| format!("Could not create preliminary assessment: {error}"))?;

    insert_claims(
        &transaction,
        &assessment_id,
        &input.dataset_id,
        &input.claims,
        &claim_records,
    )?;
    insert_claim_evidence(
        &transaction,
        &assessment_id,
        &input.dataset_id,
        &input.claim_evidence,
        &claim_records,
        &selected_import.records,
    )?;
    insert_source_dependencies(
        &transaction,
        &assessment_id,
        &input.dataset_id,
        &input.source_dependencies,
        &selected_import.records,
    )?;
    insert_factor_ratings(&transaction, &assessment_id, &input.factor_ratings)?;

    let changed = transaction
        .execute(
            "UPDATE research_assessments SET is_sealed = 1 WHERE assessment_id = ?1",
            [&assessment_id],
        )
        .map_err(|error| format!("Could not seal assessment: {error}"))?;
    if changed != 1 {
        return Err("Assessment could not be sealed because it was not created".to_string());
    }

    transaction
        .commit()
        .map_err(|error| format!("Could not commit assessment: {error}"))?;
    Ok(assessment_id)
}

pub(crate) fn create_successor_assessment(
    database: &Database,
    predecessor_id: &str,
    revision_reason: &str,
    mut input: CreateAssessmentInput,
) -> Result<String, String> {
    let predecessor_id = canonical_assessment_id(predecessor_id)?;
    if revision_reason.trim().is_empty() {
        return Err("Assessment revision reason must not be blank".to_string());
    }
    if input.supersedes_assessment_id.is_some() || input.revision_reason.is_some() {
        return Err("Successor input must not already specify a predecessor".to_string());
    }
    input.supersedes_assessment_id = Some(predecessor_id);
    input.revision_reason = Some(revision_reason.trim().to_string());
    create_preliminary_assessment(database, input)
}

pub(crate) fn read_sealed_assessment(
    database: &Database,
    assessment_id: &str,
) -> Result<ResearchAssessmentDto, String> {
    let assessment_id = canonical_assessment_id(assessment_id)?;
    let mut connection = database.connection()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(|error| format!("Could not begin assessment read: {error}"))?;
    let row = transaction
        .query_row(
            "SELECT assessment_id, dataset_id, dataset_version, import_sequence,
                    created_at, created_by_kind, created_by_name, status, score,
                    input_snapshot_json, supersedes_assessment_id, revision_reason,
                    is_sealed
             FROM research_assessments WHERE assessment_id = ?1",
            [&assessment_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, Option<i64>>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, i64>(12)?,
                ))
            },
        )
        .optional()
        .map_err(|error| format!("Could not read assessment {assessment_id}: {error}"))?
        .ok_or_else(|| format!("Assessment {assessment_id} does not exist"))?;
    if row.12 != 1 {
        return Err(format!("Assessment {assessment_id} is not sealed"));
    }
    let input_snapshot: Value = serde_json::from_str(&row.9).map_err(|error| {
        format!("Assessment {assessment_id} has invalid snapshot JSON: {error}")
    })?;
    let assessment = ResearchAssessmentDto {
        assessment_id: row.0,
        dataset_id: row.1,
        dataset_version: row.2,
        import_sequence: row.3,
        created_at: row.4,
        created_by_kind: row.5,
        created_by_name: row.6,
        status: row.7,
        score: row.8,
        input_snapshot,
        supersedes_assessment_id: row.10,
        revision_reason: row.11,
        claims: read_claims(&transaction, &assessment_id)?,
        claim_evidence: read_claim_evidence(&transaction, &assessment_id)?,
        source_dependencies: read_source_dependencies(&transaction, &assessment_id)?,
        factor_ratings: read_factor_ratings(&transaction, &assessment_id)?,
    };
    transaction
        .commit()
        .map_err(|error| format!("Could not finish assessment read: {error}"))?;
    Ok(assessment)
}

fn validate_input_shape(input: &CreateAssessmentInput) -> Result<(), String> {
    require_nonblank("Dataset ID", &input.dataset_id)?;
    require_nonblank("Dataset version", &input.dataset_version)?;
    if input.import_sequence <= 0 {
        return Err("Import sequence must be greater than zero".to_string());
    }
    match (&input.supersedes_assessment_id, &input.revision_reason) {
        (None, None) => {}
        (Some(id), Some(reason)) => {
            canonical_assessment_id(id)?;
            require_nonblank("Assessment revision reason", reason)?;
        }
        _ => {
            return Err(
                "Assessment predecessor and revision reason must be supplied together".to_string(),
            );
        }
    }

    let mut claims = HashSet::new();
    for claim in &input.claims {
        require_nonblank("Claim stable key", &claim.claim_key)?;
        validate_optional_text("Claim rationale", claim.rationale.as_deref())?;
        if !claims.insert(claim.claim_key.as_str()) {
            return Err(format!(
                "Claim {:?} is selected more than once",
                claim.claim_key
            ));
        }
    }
    for evidence in &input.claim_evidence {
        require_nonblank("Evidence assessment claim key", &evidence.claim_key)?;
        if let Some(key) = &evidence.evidence_key {
            require_nonblank("Evidence stable key or reference", key)?;
        }
        validate_optional_text("Evidence note", evidence.note.as_deref())?;
    }
    for dependency in &input.source_dependencies {
        require_nonblank("First source stable key", &dependency.source_a_key)?;
        require_nonblank("Second source stable key", &dependency.source_b_key)?;
        if dependency.source_a_key == dependency.source_b_key {
            return Err("A source cannot depend on itself".to_string());
        }
        validate_optional_text(
            "Source dependency rationale",
            dependency.rationale.as_deref(),
        )?;
        validate_references(&dependency.basis_references, "Source dependency reference")?;
    }
    let mut factors = HashSet::new();
    for factor in &input.factor_ratings {
        let key = enum_value(&factor.factor)?;
        if !factors.insert(key.clone()) {
            return Err(format!("Factor {key:?} is supplied more than once"));
        }
        validate_optional_text("Factor rationale", factor.rationale.as_deref())?;
        validate_references(&factor.references, "Factor reference")?;
        if let Some(rating) = &factor.rating {
            serde_json::to_string(rating)
                .map_err(|error| format!("Could not encode factor rating: {error}"))?;
        }
    }
    Ok(())
}

fn validate_references(references: &[String], field: &str) -> Result<(), String> {
    let mut seen = HashSet::new();
    for reference in references {
        require_nonblank(field, reference)?;
        if !seen.insert(reference.as_str()) {
            return Err(format!("{field} {reference:?} is duplicated"));
        }
    }
    Ok(())
}

fn validate_optional_text(field: &str, value: Option<&str>) -> Result<(), String> {
    if let Some(value) = value
        && value.trim().is_empty()
    {
        return Err(format!("{field} must not be blank when supplied"));
    }
    Ok(())
}

fn require_nonblank(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field} must not be blank"))
    } else {
        Ok(())
    }
}

fn canonical_assessment_id(value: &str) -> Result<String, String> {
    Uuid::parse_str(value)
        .map(|uuid| uuid.hyphenated().to_string())
        .map_err(|_| "Assessment ID must be a valid UUID".to_string())
}

fn validate_predecessor(
    transaction: &Transaction<'_>,
    input: &CreateAssessmentInput,
) -> Result<(), String> {
    let Some(predecessor_id) = &input.supersedes_assessment_id else {
        return Ok(());
    };
    let valid: bool = transaction
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM research_assessments
                WHERE assessment_id = ?1 AND dataset_id = ?2 AND is_sealed = 1
             )",
            params![predecessor_id, input.dataset_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("Could not validate predecessor assessment: {error}"))?;
    if valid {
        Ok(())
    } else {
        Err(format!(
            "Predecessor assessment {predecessor_id} is missing, unsealed, or belongs to another dataset"
        ))
    }
}

fn load_selected_import(
    transaction: &Transaction<'_>,
    dataset_id: &str,
    dataset_version: &str,
    import_sequence: i64,
) -> Result<SelectedImport, String> {
    let import = transaction
        .query_row(
            "SELECT imported_at, format, format_version, metadata_json, record_keys_json
             FROM dataset_imports
             WHERE dataset_id = ?1 AND dataset_version = ?2 AND import_sequence = ?3",
            params![dataset_id, dataset_version, import_sequence],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| format!("Could not look up exact dataset import: {error}"))?
        .ok_or_else(|| {
            format!(
                "Dataset {dataset_id:?} version {dataset_version:?} import sequence {import_sequence} does not exist"
            )
        })?;
    let metadata = parse_json_object(&import.3, "dataset metadata")?;
    let manifest: Vec<String> = serde_json::from_str(&import.4)
        .map_err(|error| format!("Stored import manifest is invalid: {error}"))?;
    let manifest_set = manifest.iter().collect::<HashSet<_>>();
    if manifest_set.len() != manifest.len() {
        return Err("Stored import manifest contains duplicate stable keys".to_string());
    }

    let mut statement = transaction
        .prepare(
            "SELECT m.record_key, m.record_kind, m.record_id, m.content_snapshot_json,
                    r.record_kind, r.created_at, r.created_by_kind, r.created_by_name
             FROM dataset_record_mappings m
             LEFT JOIN records r ON r.record_id = m.record_id
             WHERE m.dataset_id = ?1",
        )
        .map_err(|error| format!("Could not prepare exact import mapping query: {error}"))?;
    let mapping_rows = statement
        .query_map([dataset_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })
        .map_err(|error| format!("Could not query exact import mappings: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Could not read exact import mappings: {error}"))?;
    drop(statement);

    let mut records = HashMap::with_capacity(manifest.len());
    for (
        key,
        kind,
        id,
        content_json,
        identity_kind,
        created_at,
        created_by_kind,
        created_by_name,
    ) in mapping_rows
    {
        if !manifest_set.contains(&key) {
            continue;
        }
        if identity_kind.as_deref() != Some(kind.as_str()) {
            return Err(format!(
                "Dataset key {key:?} has a missing or inconsistent record identity"
            ));
        }
        let (created_at, created_by_kind) = created_at
            .zip(created_by_kind)
            .ok_or_else(|| format!("Dataset key {key:?} has incomplete record provenance"))?;
        let content_snapshot = parse_json_object(&content_json, "record content snapshot")?;
        if records
            .insert(
                key.clone(),
                ImportedRecord {
                    key: key.clone(),
                    kind,
                    id,
                    content_snapshot,
                    created_at,
                    created_by_kind,
                    created_by_name,
                },
            )
            .is_some()
        {
            return Err(format!(
                "Import has duplicate mapping for dataset key {key:?}"
            ));
        }
    }
    if records.len() != manifest.len() {
        return Err(
            "Stored dataset mappings do not match the selected import manifest".to_string(),
        );
    }
    Ok(SelectedImport {
        imported_at: import.0,
        format: import.1,
        format_version: import.2,
        metadata_json: import.3,
        metadata,
        records,
    })
}

fn parse_json_object(value: &str, field: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(value)
        .map_err(|error| format!("Stored {field} is invalid: {error}"))?;
    if !value.is_object() {
        return Err(format!("Stored {field} must be a JSON object"));
    }
    Ok(value)
}

fn resolve_inputs(
    input: &CreateAssessmentInput,
    selected_import: &SelectedImport,
) -> Result<HashMap<String, ImportedRecord>, String> {
    let mut claims = HashMap::new();
    let selected_claim_keys = input
        .claims
        .iter()
        .map(|claim| claim.claim_key.as_str())
        .collect::<HashSet<_>>();
    for claim in &input.claims {
        let record = require_record(selected_import, &claim.claim_key, "claim")?;
        claims.insert(claim.claim_key.clone(), record.clone());
    }

    let mut seen_evidence = HashSet::new();
    for evidence in &input.claim_evidence {
        if !selected_claim_keys.contains(evidence.claim_key.as_str()) {
            return Err(format!(
                "Evidence assessment references unselected claim {:?}",
                evidence.claim_key
            ));
        }
        let evidence_record = if let Some(key) = evidence.evidence_key.as_deref() {
            match selected_import.records.get(key) {
                Some(record) => {
                    if record.kind != "evidence_item" {
                        return Err(format!("Dataset key {key:?} is not an evidence item"));
                    }
                    Some(record)
                }
                None if evidence.state == EvidenceAssessmentState::Unavailable => None,
                None => {
                    return Err(format!(
                        "Evidence key {key:?} is not in the exact import manifest"
                    ));
                }
            }
        } else {
            None
        };
        if evidence_record.is_none() {
            if evidence.state != EvidenceAssessmentState::Unavailable {
                return Err("Evidence without a record ID must have unavailable state".to_string());
            }
            if evidence
                .note
                .as_deref()
                .is_none_or(|note| note.trim().is_empty())
                && evidence
                    .evidence_key
                    .as_deref()
                    .is_none_or(|reference| reference.trim().is_empty())
            {
                return Err(
                    "Unavailable evidence requires a descriptive note or stable reference"
                        .to_string(),
                );
            }
        }
        let duplicate_key = (
            evidence.claim_key.as_str(),
            evidence_record.map(|record| record.id.as_str()),
            evidence.evidence_key.as_deref(),
        );
        if !seen_evidence.insert(duplicate_key) {
            return Err("Duplicate evidence relationship for an assessment claim".to_string());
        }
    }

    let mut dependencies = HashSet::new();
    for dependency in &input.source_dependencies {
        let source_a = require_record(selected_import, &dependency.source_a_key, "source")?;
        let source_b = require_record(selected_import, &dependency.source_b_key, "source")?;
        if source_a.id >= source_b.id {
            return Err(
                "Source dependency pair must be ordered by ascending source record ID".to_string(),
            );
        }
        if !dependencies.insert((source_a.id.as_str(), source_b.id.as_str())) {
            return Err("Duplicate source dependency pair".to_string());
        }
        validate_manifest_references(
            selected_import,
            &dependency.basis_references,
            "Source dependency reference",
        )?;
    }
    for factor in &input.factor_ratings {
        validate_manifest_references(selected_import, &factor.references, "Factor reference")?;
    }
    Ok(claims)
}

fn require_record<'a>(
    selected_import: &'a SelectedImport,
    key: &str,
    expected_kind: &str,
) -> Result<&'a ImportedRecord, String> {
    let record = selected_import
        .records
        .get(key)
        .ok_or_else(|| format!("Dataset key {key:?} is not in the exact import manifest"))?;
    if record.kind != expected_kind {
        return Err(format!(
            "Dataset key {key:?} has kind {:?}, expected {expected_kind:?}",
            record.kind
        ));
    }
    Ok(record)
}

fn validate_manifest_references(
    selected_import: &SelectedImport,
    references: &[String],
    field: &str,
) -> Result<(), String> {
    for reference in references {
        if !selected_import.records.contains_key(reference) {
            return Err(format!(
                "{field} {reference:?} is not in the exact import manifest"
            ));
        }
    }
    Ok(())
}

fn build_input_snapshot(
    input: &CreateAssessmentInput,
    selected_import: &SelectedImport,
    persisted_case: &crate::research_dataset_read::ImportedResearchCaseDto,
) -> Result<Value, String> {
    let mut included_keys = HashSet::new();
    for claim in &input.claims {
        included_keys.insert(claim.claim_key.clone());
    }
    for evidence in &input.claim_evidence {
        if let Some(key) = &evidence.evidence_key
            && selected_import.records.contains_key(key)
        {
            included_keys.insert(key.clone());
        }
    }
    for dependency in &input.source_dependencies {
        included_keys.insert(dependency.source_a_key.clone());
        included_keys.insert(dependency.source_b_key.clone());
        included_keys.extend(dependency.basis_references.iter().cloned());
    }
    for factor in &input.factor_ratings {
        included_keys.extend(factor.references.iter().cloned());
    }

    let mut pending = included_keys.iter().cloned().collect::<Vec<_>>();
    while let Some(key) = pending.pop() {
        let record = selected_import
            .records
            .get(&key)
            .ok_or_else(|| format!("Snapshot record {key:?} is absent from the import"))?;
        for referenced_key in referenced_dataset_keys(&record.content_snapshot) {
            if selected_import.records.contains_key(&referenced_key)
                && included_keys.insert(referenced_key.clone())
            {
                pending.push(referenced_key);
            }
        }
    }

    let mut record_snapshots = included_keys
        .iter()
        .map(|key| {
            let record = &selected_import.records[key];
            json!({
                "stableKey": record.key,
                "recordId": record.id,
                "recordKind": record.kind,
                "createdAt": record.created_at,
                "createdBy": {
                    "kind": record.created_by_kind,
                    "displayName": record.created_by_name,
                },
                "content": record.content_snapshot,
            })
        })
        .collect::<Vec<_>>();
    record_snapshots
        .sort_by(|left, right| left["stableKey"].as_str().cmp(&right["stableKey"].as_str()));
    let persisted_case = serde_json::to_value(persisted_case)
        .map_err(|error| format!("Could not encode persisted research case snapshot: {error}"))?;

    let claims = input
        .claims
        .iter()
        .map(|claim| {
            Ok::<_, String>(json!({
                "claimKey": claim.claim_key,
                "scopeRole": enum_value(&claim.scope_role)?,
                "outcome": enum_value(&claim.outcome)?,
                "rationale": claim.rationale,
            }))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let evidence = input
        .claim_evidence
        .iter()
        .map(|item| {
            Ok::<_, String>(json!({
                "claimKey": item.claim_key,
                "evidenceKey": item.evidence_key,
                "state": enum_value(&item.state)?,
                "note": item.note,
            }))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let dependencies = input
        .source_dependencies
        .iter()
        .map(|item| {
            Ok::<_, String>(json!({
                "sourceAKey": item.source_a_key,
                "sourceBKey": item.source_b_key,
                "state": enum_value(&item.state)?,
                "rationale": item.rationale,
                "basisReferences": item.basis_references,
            }))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let factors = input
        .factor_ratings
        .iter()
        .map(|item| {
            Ok::<_, String>(json!({
                "factor": enum_value(&item.factor)?,
                "rating": item.rating,
                "rationale": item.rationale,
                "references": item.references,
            }))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(json!({
        "snapshotFormatVersion": SNAPSHOT_FORMAT_VERSION,
        "dataset": {
            "datasetId": input.dataset_id,
            "datasetVersion": input.dataset_version,
            "importSequence": input.import_sequence,
            "importedAt": selected_import.imported_at,
            "format": selected_import.format,
            "formatVersion": selected_import.format_version,
            "metadata": selected_import.metadata,
        },
        "importRecordSnapshots": record_snapshots,
        "persistedCase": persisted_case,
        "assessmentInputs": {
            "claims": claims,
            "claimEvidence": evidence,
            "sourceDependencies": dependencies,
            "factorRatings": factors,
        },
    }))
}

fn referenced_dataset_keys(value: &Value) -> Vec<String> {
    let mut keys = Vec::new();
    collect_referenced_dataset_keys(value, &mut keys);
    keys
}

fn collect_referenced_dataset_keys(value: &Value, keys: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            for (field, child) in object {
                if field.ends_with("Key")
                    && let Some(key) = child.as_str()
                {
                    keys.push(key.to_string());
                }
                collect_referenced_dataset_keys(child, keys);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_referenced_dataset_keys(child, keys);
            }
        }
        _ => {}
    }
}

fn insert_claims(
    transaction: &Transaction<'_>,
    assessment_id: &str,
    dataset_id: &str,
    claims: &[AssessmentClaimInput],
    claim_records: &HashMap<String, ImportedRecord>,
) -> Result<(), String> {
    for claim in claims {
        let record = &claim_records[&claim.claim_key];
        transaction
            .execute(
                "INSERT INTO assessment_claims (
                    assessment_id, dataset_id, claim_key, claim_record_id,
                    scope_role, outcome, rationale
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    assessment_id,
                    dataset_id,
                    claim.claim_key,
                    record.id,
                    enum_value(&claim.scope_role)?,
                    enum_value(&claim.outcome)?,
                    claim.rationale,
                ],
            )
            .map_err(|error| {
                format!(
                    "Could not save claim assessment {:?}: {error}",
                    claim.claim_key
                )
            })?;
    }
    Ok(())
}

fn insert_claim_evidence(
    transaction: &Transaction<'_>,
    assessment_id: &str,
    dataset_id: &str,
    evidence_inputs: &[AssessmentClaimEvidenceInput],
    claim_records: &HashMap<String, ImportedRecord>,
    records: &HashMap<String, ImportedRecord>,
) -> Result<(), String> {
    for input in evidence_inputs {
        let claim = &claim_records[&input.claim_key];
        let evidence_record = input
            .evidence_key
            .as_ref()
            .and_then(|key| records.get(key))
            .filter(|record| record.kind == "evidence_item");
        let evidence_id = evidence_record.map(|record| record.id.as_str());
        transaction
            .execute(
                "INSERT INTO assessment_claim_evidence (
                    assessment_evidence_id, assessment_id, dataset_id, claim_record_id,
                    evidence_key, evidence_record_id, state, note
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    Uuid::new_v4().hyphenated().to_string(),
                    assessment_id,
                    dataset_id,
                    claim.id,
                    input.evidence_key,
                    evidence_id,
                    enum_value(&input.state)?,
                    input.note,
                ],
            )
            .map_err(|error| format!("Could not save evidence assessment: {error}"))?;
    }
    Ok(())
}

fn insert_source_dependencies(
    transaction: &Transaction<'_>,
    assessment_id: &str,
    dataset_id: &str,
    dependencies: &[AssessmentSourceDependencyInput],
    records: &HashMap<String, ImportedRecord>,
) -> Result<(), String> {
    for input in dependencies {
        let source_a = &records[&input.source_a_key];
        let source_b = &records[&input.source_b_key];
        let basis_references = serde_json::to_string(&input.basis_references)
            .map_err(|error| format!("Could not encode source-dependency references: {error}"))?;
        transaction
            .execute(
                "INSERT INTO assessment_source_dependencies (
                    dependency_id, assessment_id, dataset_id, source_a_key, source_a_id,
                    source_b_key, source_b_id, state, rationale, basis_references_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    Uuid::new_v4().hyphenated().to_string(),
                    assessment_id,
                    dataset_id,
                    input.source_a_key,
                    source_a.id,
                    input.source_b_key,
                    source_b.id,
                    enum_value(&input.state)?,
                    input.rationale,
                    basis_references,
                ],
            )
            .map_err(|error| format!("Could not save source dependency assessment: {error}"))?;
    }
    Ok(())
}

fn insert_factor_ratings(
    transaction: &Transaction<'_>,
    assessment_id: &str,
    factors: &[AssessmentFactorRatingInput],
) -> Result<(), String> {
    for factor in factors {
        let rating = factor
            .rating
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| format!("Could not encode factor rating: {error}"))?;
        let references = serde_json::to_string(&factor.references)
            .map_err(|error| format!("Could not encode factor references: {error}"))?;
        transaction
            .execute(
                "INSERT INTO assessment_factor_ratings (
                    assessment_id, factor_key, rating_json, rationale, references_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    assessment_id,
                    enum_value(&factor.factor)?,
                    rating,
                    factor.rationale,
                    references,
                ],
            )
            .map_err(|error| format!("Could not save factor rating: {error}"))?;
    }
    Ok(())
}

fn read_claims(
    transaction: &Transaction<'_>,
    assessment_id: &str,
) -> Result<Vec<AssessmentClaimDto>, String> {
    let mut statement = transaction
        .prepare(
            "SELECT claim_key, claim_record_id, scope_role, outcome, rationale
             FROM assessment_claims WHERE assessment_id = ?1 ORDER BY claim_key",
        )
        .map_err(|error| format!("Could not prepare assessment claim read: {error}"))?;
    let rows = statement
        .query_map([assessment_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })
        .map_err(|error| format!("Could not query assessment claims: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Could not read assessment claims: {error}"))?;
    rows.into_iter()
        .map(
            |(claim_key, claim_record_id, scope_role, outcome, rationale)| {
                Ok(AssessmentClaimDto {
                    claim_key,
                    claim_record_id,
                    scope_role: parse_enum(&scope_role, "claim scope role")?,
                    outcome: parse_enum(&outcome, "claim outcome")?,
                    rationale,
                })
            },
        )
        .collect()
}

fn read_claim_evidence(
    transaction: &Transaction<'_>,
    assessment_id: &str,
) -> Result<Vec<AssessmentClaimEvidenceDto>, String> {
    let mut statement = transaction
        .prepare(
            "SELECT c.claim_key, e.evidence_key, e.evidence_record_id, e.state, e.note
             FROM assessment_claim_evidence e
             JOIN assessment_claims c
               ON c.assessment_id = e.assessment_id
              AND c.claim_record_id = e.claim_record_id
             WHERE e.assessment_id = ?1
             ORDER BY c.claim_key, e.evidence_key, e.assessment_evidence_id",
        )
        .map_err(|error| format!("Could not prepare assessment evidence read: {error}"))?;
    let rows = statement
        .query_map([assessment_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })
        .map_err(|error| format!("Could not query assessment evidence: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Could not read assessment evidence: {error}"))?;
    rows.into_iter()
        .map(
            |(claim_key, evidence_key, evidence_record_id, state, note)| {
                Ok(AssessmentClaimEvidenceDto {
                    claim_key,
                    evidence_key,
                    evidence_record_id,
                    state: parse_enum(&state, "evidence assessment state")?,
                    note,
                })
            },
        )
        .collect()
}

fn read_source_dependencies(
    transaction: &Transaction<'_>,
    assessment_id: &str,
) -> Result<Vec<AssessmentSourceDependencyDto>, String> {
    let mut statement = transaction
        .prepare(
            "SELECT source_a_key, source_a_id, source_b_key, source_b_id, state,
                    rationale, basis_references_json
             FROM assessment_source_dependencies
             WHERE assessment_id = ?1
             ORDER BY source_a_key, source_b_key",
        )
        .map_err(|error| format!("Could not prepare source dependency read: {error}"))?;
    let rows = statement
        .query_map([assessment_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
            ))
        })
        .map_err(|error| format!("Could not query source dependencies: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Could not read source dependencies: {error}"))?;
    rows.into_iter()
        .map(
            |(source_a_key, source_a_id, source_b_key, source_b_id, state, rationale, refs)| {
                Ok(AssessmentSourceDependencyDto {
                    source_a_key,
                    source_a_id,
                    source_b_key,
                    source_b_id,
                    state: parse_enum(&state, "source dependency state")?,
                    rationale,
                    basis_references: serde_json::from_str(&refs).map_err(|error| {
                        format!("Stored source dependency references are invalid: {error}")
                    })?,
                })
            },
        )
        .collect()
}

fn read_factor_ratings(
    transaction: &Transaction<'_>,
    assessment_id: &str,
) -> Result<Vec<AssessmentFactorRatingDto>, String> {
    let mut statement = transaction
        .prepare(
            "SELECT factor_key, rating_json, rationale, references_json
             FROM assessment_factor_ratings
             WHERE assessment_id = ?1 ORDER BY factor_key",
        )
        .map_err(|error| format!("Could not prepare factor ratings read: {error}"))?;
    let rows = statement
        .query_map([assessment_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|error| format!("Could not query factor ratings: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Could not read factor ratings: {error}"))?;
    rows.into_iter()
        .map(|(factor, rating, rationale, references)| {
            Ok(AssessmentFactorRatingDto {
                factor: parse_enum(&factor, "assessment factor")?,
                rating: rating
                    .map(|value| {
                        serde_json::from_str(&value)
                            .map_err(|error| format!("Stored factor rating is invalid: {error}"))
                    })
                    .transpose()?,
                rationale,
                references: serde_json::from_str(&references)
                    .map_err(|error| format!("Stored factor references are invalid: {error}"))?,
            })
        })
        .collect()
}

fn parse_enum<T: serde::de::DeserializeOwned>(value: &str, field: &str) -> Result<T, String> {
    serde_json::from_value(Value::String(value.to_string()))
        .map_err(|error| format!("Stored {field} is invalid: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        database::Database,
        research_dataset_import::{import_bundled_dataset, seed_bundled_legacy_dataset_for_tests},
    };

    const DATASET_ID: &str = "xenoscope.case.al-taqaddum-2017";
    const DATASET_VERSION: &str = "1.0.0";

    fn setup() -> Database {
        let database = Database::in_memory().expect("create test database");
        seed_bundled_legacy_dataset_for_tests(&database).expect("seed legacy bundled dataset");
        database
    }

    fn dataset_key(database: &Database, kind: &str, index: usize) -> String {
        let connection = database.connection().expect("lock database");
        let mut statement = connection
            .prepare(
                "SELECT record_key FROM dataset_record_mappings
                 WHERE dataset_id = ?1 AND record_kind = ?2 ORDER BY record_key",
            )
            .expect("prepare stable-key query");
        let keys = statement
            .query_map(params![DATASET_ID, kind], |row| row.get(0))
            .expect("query stable keys")
            .collect::<Result<Vec<String>, _>>()
            .expect("read stable keys");
        keys.get(index)
            .unwrap_or_else(|| {
                panic!(
                    "fixture has requested {kind} record kind at index {index}; found keys {keys:?}"
                )
            })
            .clone()
    }

    fn ordered_sources(database: &Database) -> (String, String) {
        let connection = database.connection().expect("lock database");
        let mut statement = connection
            .prepare(
                "SELECT m.record_key
                 FROM dataset_record_mappings m
                 JOIN sources s ON s.record_id = m.record_id
                 WHERE m.dataset_id = ?1
                 ORDER BY m.record_id
                 LIMIT 2",
            )
            .expect("prepare source query");
        let keys = statement
            .query_map([DATASET_ID], |row| row.get(0))
            .expect("query sources")
            .collect::<Result<Vec<String>, _>>()
            .expect("read sources");
        (keys[0].clone(), keys[1].clone())
    }

    fn blank_input() -> CreateAssessmentInput {
        CreateAssessmentInput {
            dataset_id: DATASET_ID.to_string(),
            dataset_version: DATASET_VERSION.to_string(),
            import_sequence: 1,
            claims: Vec::new(),
            claim_evidence: Vec::new(),
            source_dependencies: Vec::new(),
            factor_ratings: Vec::new(),
            supersedes_assessment_id: None,
            revision_reason: None,
        }
    }

    fn valid_input(database: &Database) -> CreateAssessmentInput {
        let claim_key = dataset_key(database, "claim", 0);
        let evidence_key = dataset_key(database, "evidence_item", 0);
        CreateAssessmentInput {
            claims: vec![AssessmentClaimInput {
                claim_key,
                scope_role: AssessmentScopeRole::Core,
                outcome: ClaimOutcome::Supported,
                rationale: Some("Supported by the cited record.".to_string()),
            }],
            claim_evidence: vec![AssessmentClaimEvidenceInput {
                claim_key: dataset_key(database, "claim", 0),
                evidence_key: Some(evidence_key),
                state: EvidenceAssessmentState::Supports,
                note: None,
            }],
            ..blank_input()
        }
    }

    #[test]
    fn creates_preliminary_assessment_with_null_score_and_reads_sealed_inputs() {
        let database = setup();
        let input = valid_input(&database);
        let expected_claim = input.claims[0].clone();
        let expected_evidence = input.claim_evidence[0].clone();
        let (dataset_id, dataset_version, import_sequence, format, format_version): (
            String,
            String,
            i64,
            String,
            i64,
        ) = database
            .connection()
            .expect("lock database")
            .query_row(
                "SELECT dataset_id, dataset_version, import_sequence, format, format_version
                 FROM dataset_imports
                 WHERE dataset_id = ?1 AND dataset_version = ?2 AND import_sequence = ?3",
                params![
                    input.dataset_id,
                    input.dataset_version,
                    input.import_sequence
                ],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .expect("read exact imported dataset metadata");
        let id = create_preliminary_assessment(&database, input).expect("create assessment");
        let read = read_sealed_assessment(&database, &id).expect("read sealed assessment");

        assert_eq!(read.status, "preliminary");
        assert_eq!(read.score, None);
        assert_eq!(read.claims.len(), 1);
        assert_eq!(read.claims[0].claim_key, expected_claim.claim_key);
        assert_eq!(read.claims[0].scope_role, expected_claim.scope_role);
        assert_eq!(read.claims[0].outcome, expected_claim.outcome);
        assert_eq!(read.claims[0].rationale, expected_claim.rationale);
        assert_eq!(read.claim_evidence.len(), 1);
        assert_eq!(
            read.claim_evidence[0].claim_key,
            expected_evidence.claim_key
        );
        assert_eq!(
            read.claim_evidence[0].evidence_key,
            expected_evidence.evidence_key
        );
        assert_eq!(read.claim_evidence[0].state, expected_evidence.state);

        assert_eq!(
            read.input_snapshot["dataset"]["datasetId"],
            json!(dataset_id)
        );
        assert_eq!(
            read.input_snapshot["dataset"]["datasetVersion"],
            json!(dataset_version)
        );
        assert_eq!(
            read.input_snapshot["dataset"]["importSequence"],
            json!(import_sequence)
        );
        assert_eq!(read.input_snapshot["dataset"]["format"], json!(format));
        assert_eq!(
            read.input_snapshot["dataset"]["formatVersion"],
            json!(format_version)
        );
        assert_eq!(
            read.input_snapshot["snapshotFormatVersion"],
            json!(SNAPSHOT_FORMAT_VERSION)
        );
        assert_eq!(
            read.input_snapshot["assessmentInputs"]["claims"][0]["claimKey"],
            json!(expected_claim.claim_key)
        );
        assert_eq!(
            read.input_snapshot["assessmentInputs"]["claims"][0]["scopeRole"],
            json!("core")
        );
        assert_eq!(
            read.input_snapshot["assessmentInputs"]["claims"][0]["outcome"],
            json!("supported")
        );
        assert_eq!(
            read.input_snapshot["assessmentInputs"]["claims"][0]["rationale"],
            json!(expected_claim.rationale)
        );
        assert_eq!(
            read.input_snapshot["assessmentInputs"]["claimEvidence"][0]["claimKey"],
            json!(expected_evidence.claim_key)
        );
        assert_eq!(
            read.input_snapshot["assessmentInputs"]["claimEvidence"][0]["evidenceKey"],
            json!(expected_evidence.evidence_key)
        );
        assert_eq!(
            read.input_snapshot["assessmentInputs"]["claimEvidence"][0]["state"],
            json!("supports")
        );
        assert_eq!(
            read.input_snapshot["persistedCase"]["datasetId"],
            json!(dataset_id)
        );
        assert_eq!(
            read.input_snapshot["persistedCase"]["datasetVersion"],
            json!(dataset_version)
        );
        assert_eq!(
            read.input_snapshot["persistedCase"]["importSequence"],
            json!(import_sequence)
        );
    }

    #[test]
    fn requires_exact_dataset_version_and_import_sequence() {
        let database = setup();
        let mut input = blank_input();
        input.dataset_version = "not-the-imported-version".to_string();
        assert!(create_preliminary_assessment(&database, input).is_err());

        let mut input = blank_input();
        input.import_sequence = 2;
        assert!(create_preliminary_assessment(&database, input).is_err());

        let mut input = blank_input();
        input.dataset_id = "unknown.dataset".to_string();
        assert!(create_preliminary_assessment(&database, input).is_err());
    }

    #[test]
    fn rejects_unknown_or_wrong_kind_claim_evidence_and_source_references() {
        let database = setup();
        let mut input = blank_input();
        input.claims.push(AssessmentClaimInput {
            claim_key: "unknown-claim".to_string(),
            scope_role: AssessmentScopeRole::Core,
            outcome: ClaimOutcome::Indeterminate,
            rationale: None,
        });
        assert!(create_preliminary_assessment(&database, input).is_err());

        let wrong_kind_key = dataset_key(&database, "source", 0);
        let mut input = blank_input();
        input.claims.push(AssessmentClaimInput {
            claim_key: wrong_kind_key,
            scope_role: AssessmentScopeRole::Core,
            outcome: ClaimOutcome::Indeterminate,
            rationale: None,
        });
        assert!(create_preliminary_assessment(&database, input).is_err());

        let claim_key = dataset_key(&database, "claim", 0);
        let mut input = blank_input();
        input.claims.push(AssessmentClaimInput {
            claim_key: claim_key.clone(),
            scope_role: AssessmentScopeRole::Core,
            outcome: ClaimOutcome::Indeterminate,
            rationale: None,
        });
        input.claim_evidence.push(AssessmentClaimEvidenceInput {
            claim_key,
            evidence_key: Some(dataset_key(&database, "claim", 1)),
            state: EvidenceAssessmentState::Supports,
            note: None,
        });
        assert!(create_preliminary_assessment(&database, input).is_err());

        let (source_a_key, _) = ordered_sources(&database);
        let mut input = blank_input();
        input
            .source_dependencies
            .push(AssessmentSourceDependencyInput {
                source_a_key,
                source_b_key: dataset_key(&database, "claim", 0),
                state: SourceDependencyState::Unknown,
                rationale: None,
                basis_references: Vec::new(),
            });
        assert!(create_preliminary_assessment(&database, input).is_err());

        let (_, source_b_key) = ordered_sources(&database);
        let mut input = blank_input();
        input
            .source_dependencies
            .push(AssessmentSourceDependencyInput {
                source_a_key: "missing-source".to_string(),
                source_b_key,
                state: SourceDependencyState::Unknown,
                rationale: None,
                basis_references: Vec::new(),
            });
        assert!(create_preliminary_assessment(&database, input).is_err());
    }

    #[test]
    fn evidence_must_reference_a_selected_claim_and_unavailable_items_need_context() {
        let database = setup();
        let claim_key = dataset_key(&database, "claim", 0);
        let mut input = blank_input();
        input.claim_evidence.push(AssessmentClaimEvidenceInput {
            claim_key: claim_key.clone(),
            evidence_key: None,
            state: EvidenceAssessmentState::Unavailable,
            note: Some("No copy was available for examination.".to_string()),
        });
        assert!(create_preliminary_assessment(&database, input.clone()).is_err());

        input.claims.push(AssessmentClaimInput {
            claim_key: claim_key.clone(),
            scope_role: AssessmentScopeRole::Core,
            outcome: ClaimOutcome::Unexamined,
            rationale: None,
        });
        assert!(create_preliminary_assessment(&database, input.clone()).is_ok());

        let mut unknown_evidence = valid_input(&database);
        unknown_evidence.claim_evidence[0].evidence_key = Some("missing-evidence".to_string());
        assert!(create_preliminary_assessment(&database, unknown_evidence).is_err());

        input.claim_evidence[0].note = None;
        assert!(create_preliminary_assessment(&database, input).is_err());

        let mut referenced = valid_input(&database);
        referenced.claim_evidence[0].evidence_key = Some("external-archive-ref".to_string());
        referenced.claim_evidence[0].state = EvidenceAssessmentState::Unavailable;
        referenced.claim_evidence[0].note = None;
        assert!(create_preliminary_assessment(&database, referenced).is_ok());
    }

    #[test]
    fn validates_source_pairs_and_persists_unrated_factors() {
        let database = setup();
        let (source_a_key, source_b_key) = ordered_sources(&database);
        let mut input = valid_input(&database);
        input
            .source_dependencies
            .push(AssessmentSourceDependencyInput {
                source_a_key: source_a_key.clone(),
                source_b_key: source_b_key.clone(),
                state: SourceDependencyState::Unknown,
                rationale: Some("Insufficient publication history.".to_string()),
                basis_references: vec![input.claims[0].claim_key.clone()],
            });
        input.factor_ratings = vec![
            AssessmentFactor::EvidenceQuality,
            AssessmentFactor::Provenance,
            AssessmentFactor::IndependentCorroboration,
            AssessmentFactor::Consistency,
        ]
        .into_iter()
        .map(|factor| AssessmentFactorRatingInput {
            factor,
            rating: None,
            rationale: None,
            references: Vec::new(),
        })
        .collect();
        let id = create_preliminary_assessment(&database, input.clone()).expect("create");
        let read = read_sealed_assessment(&database, &id).expect("read");
        assert_eq!(read.source_dependencies.len(), 1);
        assert_eq!(
            read.source_dependencies[0].state,
            SourceDependencyState::Unknown
        );
        assert_eq!(read.factor_ratings.len(), 4);
        assert!(
            read.factor_ratings
                .iter()
                .all(|factor| factor.rating.is_none())
        );

        for state in [
            SourceDependencyState::Dependent,
            SourceDependencyState::AssessedIndependent,
        ] {
            let mut state_input = blank_input();
            state_input
                .source_dependencies
                .push(AssessmentSourceDependencyInput {
                    source_a_key: source_a_key.clone(),
                    source_b_key: source_b_key.clone(),
                    state: state.clone(),
                    rationale: None,
                    basis_references: Vec::new(),
                });
            let state_id =
                create_preliminary_assessment(&database, state_input).expect("create state");
            let state_read = read_sealed_assessment(&database, &state_id).expect("read state");
            assert_eq!(state_read.source_dependencies[0].state, state);
        }

        input
            .source_dependencies
            .push(input.source_dependencies[0].clone());
        assert!(create_preliminary_assessment(&database, input).is_err());

        let mut reversed = valid_input(&database);
        reversed
            .source_dependencies
            .push(AssessmentSourceDependencyInput {
                source_a_key: source_b_key,
                source_b_key: source_a_key,
                state: SourceDependencyState::Dependent,
                rationale: None,
                basis_references: Vec::new(),
            });
        assert!(create_preliminary_assessment(&database, reversed).is_err());
    }

    #[test]
    fn invalid_child_input_rolls_back_assessment_and_successor_is_immutable() {
        let database = setup();
        let mut interrupted = valid_input(&database);
        interrupted
            .factor_ratings
            .push(AssessmentFactorRatingInput {
                factor: AssessmentFactor::EvidenceQuality,
                rating: None,
                rationale: None,
                references: Vec::new(),
            });
        database
            .connection()
            .expect("lock database")
            .execute_batch(
                "CREATE TRIGGER fail_factor_insert_for_test
                 BEFORE INSERT ON assessment_factor_ratings
                 BEGIN
                     SELECT RAISE(ABORT, 'deliberate test failure');
                 END;",
            )
            .expect("create test failure trigger");
        assert!(create_preliminary_assessment(&database, interrupted).is_err());
        let count: i64 = database
            .connection()
            .expect("lock database")
            .query_row("SELECT COUNT(*) FROM research_assessments", [], |row| {
                row.get(0)
            })
            .expect("count assessments");
        assert_eq!(count, 0);
        let claim_count: i64 = database
            .connection()
            .expect("lock database")
            .query_row("SELECT COUNT(*) FROM assessment_claims", [], |row| {
                row.get(0)
            })
            .expect("count claim inputs");
        assert_eq!(claim_count, 0);
        database
            .connection()
            .expect("lock database")
            .execute_batch("DROP TRIGGER fail_factor_insert_for_test")
            .expect("remove test failure trigger");

        let predecessor_id =
            create_preliminary_assessment(&database, valid_input(&database)).expect("predecessor");
        let predecessor_before =
            read_sealed_assessment(&database, &predecessor_id).expect("read predecessor");
        let mut successor_input = blank_input();
        successor_input.claims.push(AssessmentClaimInput {
            claim_key: dataset_key(&database, "claim", 1),
            scope_role: AssessmentScopeRole::Context,
            outcome: ClaimOutcome::Unexamined,
            rationale: None,
        });
        let successor_id = create_successor_assessment(
            &database,
            &predecessor_id,
            "A later review selected a broader claim scope.",
            successor_input,
        )
        .expect("create successor");
        let successor = read_sealed_assessment(&database, &successor_id).expect("read successor");
        let predecessor_after =
            read_sealed_assessment(&database, &predecessor_id).expect("read predecessor again");
        assert_eq!(
            successor.supersedes_assessment_id.as_deref(),
            Some(predecessor_id.as_str())
        );
        assert_eq!(predecessor_before, predecessor_after);
    }

    #[test]
    fn rejects_invalid_predecessors_and_unsealed_reads() {
        let database = setup();
        let invalid_id = Uuid::new_v4().hyphenated().to_string();
        let mut input = blank_input();
        input.supersedes_assessment_id = Some(invalid_id);
        input.revision_reason = Some("Revision".to_string());
        assert!(create_preliminary_assessment(&database, input).is_err());

        let assessment_id = Uuid::new_v4().hyphenated().to_string();
        let dataset_version: String = database
            .connection()
            .expect("lock database")
            .query_row(
                "SELECT dataset_version FROM dataset_imports
                 WHERE dataset_id = ?1 AND import_sequence = 1",
                [DATASET_ID],
                |row| row.get(0),
            )
            .expect("read imported dataset version");
        {
            let connection = database.connection().expect("lock database");
            connection
                .execute(
                    "INSERT INTO research_assessments (
                        assessment_id, dataset_id, dataset_version, import_sequence,
                        created_at, created_by_kind, status, scoring_profile_version,
                        scoring_profile_json, algorithm_version, input_snapshot_json
                     ) VALUES (?1, ?2, ?3, 1, 'now', 'researcher', 'preliminary',
                               'not_scored', '{}', 'not_scored', '{}')",
                    params![assessment_id, DATASET_ID, dataset_version],
                )
                .expect("insert test unsealed assessment");
        }
        assert!(
            create_successor_assessment(
                &database,
                &assessment_id,
                "Attempt to use an unsealed predecessor.",
                blank_input(),
            )
            .is_err()
        );
        assert!(read_sealed_assessment(&database, &assessment_id).is_err());

        let other_dataset_id = "xenoscope.case.other";
        let other_predecessor_id = Uuid::new_v4().hyphenated().to_string();
        {
            let connection = database.connection().expect("lock database");
            connection
                .execute(
                    "INSERT INTO research_datasets (dataset_id) VALUES (?1)",
                    [other_dataset_id],
                )
                .expect("insert other dataset");
            connection
                .execute(
                    "INSERT INTO dataset_imports (
                        dataset_id, dataset_version, import_sequence, format,
                        format_version, imported_at, metadata_json, record_keys_json
                     ) VALUES (?1, '1.0.0', 1, 'xenoscope-research-dataset',
                               1, 'now', '{}', '[]')",
                    [other_dataset_id],
                )
                .expect("insert other dataset import");
            connection
                .execute(
                    "INSERT INTO research_assessments (
                        assessment_id, dataset_id, dataset_version, import_sequence,
                        created_at, created_by_kind, status, scoring_profile_version,
                        scoring_profile_json, algorithm_version, input_snapshot_json
                     ) VALUES (?1, ?2, '1.0.0', 1, 'now', 'researcher', 'preliminary',
                               'not_scored', '{}', 'not_scored', '{}')",
                    params![other_predecessor_id, other_dataset_id],
                )
                .expect("insert other dataset predecessor");
            connection
                .execute(
                    "UPDATE research_assessments SET is_sealed = 1
                     WHERE assessment_id = ?1",
                    [&other_predecessor_id],
                )
                .expect("seal other dataset predecessor");
        }
        assert!(
            create_successor_assessment(
                &database,
                &other_predecessor_id,
                "A predecessor from another dataset must be rejected.",
                blank_input(),
            )
            .is_err()
        );
    }

    #[test]
    fn snapshot_keeps_imported_content_when_current_record_is_edited() {
        let database = setup();
        let input = valid_input(&database);
        let claim_key = input.claims[0].claim_key.clone();
        let evidence_key = input.claim_evidence[0]
            .evidence_key
            .as_ref()
            .expect("selected evidence key")
            .clone();
        let original_assertion: String = database
            .connection()
            .expect("lock database")
            .query_row(
                "SELECT c.assertion_text
                 FROM claims c
                 JOIN dataset_record_mappings m ON m.record_id = c.record_id
                 WHERE m.dataset_id = ?1 AND m.record_key = ?2",
                params![DATASET_ID, claim_key],
                |row| row.get(0),
            )
            .expect("read original claim assertion");
        let (
            original_description,
            original_locator,
            original_source_id,
            original_created_at,
            original_created_by_kind,
            original_created_by_name,
        ): (String, String, String, String, String, Option<String>) = database
            .connection()
            .expect("lock database")
            .query_row(
                "SELECT e.description, e.locator_json, e.source_id,
                        r.created_at, r.created_by_kind, r.created_by_name
                 FROM evidence_items e
                 JOIN records r ON r.record_id = e.record_id
                 JOIN dataset_record_mappings m ON m.record_id = e.record_id
                 WHERE m.dataset_id = ?1 AND m.record_key = ?2",
                params![DATASET_ID, evidence_key],
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
            .expect("read original evidence and provenance");
        let original_locator: Value =
            serde_json::from_str(&original_locator).expect("parse original evidence locator");
        let original_source_key: String = database
            .connection()
            .expect("lock database")
            .query_row(
                "SELECT record_key FROM dataset_record_mappings
                 WHERE dataset_id = ?1 AND record_id = ?2",
                params![DATASET_ID, original_source_id],
                |row| row.get(0),
            )
            .expect("resolve original evidence source key");
        let (_, alternate_source_key) = ordered_sources(&database);
        let alternate_source_id: String = database
            .connection()
            .expect("lock database")
            .query_row(
                "SELECT record_id FROM dataset_record_mappings
                 WHERE dataset_id = ?1 AND record_key = ?2",
                params![DATASET_ID, alternate_source_key],
                |row| row.get(0),
            )
            .expect("lookup alternate source id");
        let assessment_id =
            create_preliminary_assessment(&database, input).expect("create assessment");
        let before = read_sealed_assessment(&database, &assessment_id).expect("read assessment");
        let snapshot_claim = before.input_snapshot["persistedCase"]["claims"]
            .as_array()
            .expect("persisted case claims")
            .iter()
            .find(|claim| claim["record"]["datasetKey"] == claim_key)
            .expect("snapshot selected claim")
            .clone();
        assert_eq!(snapshot_claim["assertionText"], json!(original_assertion));
        let snapshot_evidence = before.input_snapshot["persistedCase"]["evidenceItems"]
            .as_array()
            .expect("persisted case evidence items")
            .iter()
            .find(|evidence| evidence["record"]["datasetKey"] == evidence_key)
            .expect("snapshot selected evidence item")
            .clone();
        assert_eq!(
            snapshot_evidence["description"],
            json!(original_description)
        );
        assert_eq!(snapshot_evidence["locator"], original_locator);
        assert_eq!(
            snapshot_evidence["source"]["datasetKey"],
            json!(original_source_key)
        );
        assert_eq!(
            snapshot_evidence["record"]["createdAt"],
            json!(original_created_at)
        );
        assert_eq!(
            snapshot_evidence["record"]["createdBy"]["kind"],
            json!(original_created_by_kind)
        );
        assert_eq!(
            snapshot_evidence["record"]["createdBy"]["displayName"],
            json!(original_created_by_name)
        );
        assert_eq!(
            before.input_snapshot["assessmentInputs"]["claimEvidence"][0]["claimKey"],
            json!(claim_key)
        );
        assert_eq!(
            before.input_snapshot["assessmentInputs"]["claimEvidence"][0]["evidenceKey"],
            json!(evidence_key)
        );
        assert_eq!(
            before.input_snapshot["assessmentInputs"]["claimEvidence"][0]["state"],
            json!("supports")
        );
        {
            let connection = database.connection().expect("lock database");
            let claim_id: String = connection
                .query_row(
                    "SELECT record_id FROM dataset_record_mappings
                     WHERE dataset_id = ?1 AND record_key = ?2",
                    params![DATASET_ID, claim_key],
                    |row| row.get(0),
                )
                .expect("lookup claim id");
            let evidence_id: String = connection
                .query_row(
                    "SELECT record_id FROM dataset_record_mappings
                     WHERE dataset_id = ?1 AND record_key = ?2",
                    params![DATASET_ID, evidence_key],
                    |row| row.get(0),
                )
                .expect("lookup evidence id");
            connection
                .execute(
                    "UPDATE claims SET assertion_text = 'Edited after assessment'
                     WHERE record_id = ?1",
                    [claim_id],
                )
                .expect("edit live claim");
            connection
                .execute(
                    "UPDATE evidence_items
                     SET description = 'Edited after assessment',
                         locator_json = '{\"kind\":\"page\",\"page\":99}',
                         source_id = ?1
                     WHERE record_id = ?2",
                    params![alternate_source_id, evidence_id],
                )
                .expect("edit live evidence fields");
            connection
                .execute(
                    "UPDATE records
                     SET created_at = '2099-01-01T00:00:00Z',
                         created_by_kind = 'system',
                         created_by_name = 'Edited provenance'
                     WHERE record_id = ?1",
                    [evidence_id],
                )
                .expect("edit live evidence provenance");
        }

        let after = read_sealed_assessment(&database, &assessment_id).expect("read again");
        let after_claim = after.input_snapshot["persistedCase"]["claims"]
            .as_array()
            .expect("persisted case claims")
            .iter()
            .find(|claim| claim["record"]["datasetKey"] == claim_key)
            .expect("snapshot selected claim");
        assert_eq!(snapshot_claim, *after_claim);
        assert_ne!(original_assertion, "Edited after assessment");
        let after_evidence = after.input_snapshot["persistedCase"]["evidenceItems"]
            .as_array()
            .expect("persisted case evidence items")
            .iter()
            .find(|evidence| evidence["record"]["datasetKey"] == evidence_key)
            .expect("snapshot selected evidence item");
        assert_eq!(snapshot_evidence, *after_evidence);
        assert_eq!(after_evidence["description"], json!(original_description));
        assert_eq!(after_evidence["locator"], original_locator);
        assert_eq!(
            after_evidence["source"]["datasetKey"],
            json!(original_source_key)
        );
        assert_eq!(
            after_evidence["record"]["createdAt"],
            json!(original_created_at)
        );
        assert_eq!(
            after_evidence["record"]["createdBy"]["kind"],
            json!(original_created_by_kind)
        );
        assert_eq!(
            after_evidence["record"]["createdBy"]["displayName"],
            json!(original_created_by_name)
        );
    }

    #[test]
    fn sealed_v1_assessment_snapshot_is_unchanged_after_v2_dataset_upgrade() {
        let database = setup();
        let assessment_id = create_preliminary_assessment(&database, valid_input(&database))
            .expect("create sealed assessment for v1 import");
        let before_read =
            read_sealed_assessment(&database, &assessment_id).expect("read v1 assessment");
        assert_eq!(before_read.dataset_version, "1.0.0");
        let before_snapshot_json: String = database
            .connection()
            .expect("lock database")
            .query_row(
                "SELECT input_snapshot_json FROM research_assessments
                 WHERE assessment_id = ?1",
                [&assessment_id],
                |row| row.get(0),
            )
            .expect("read original sealed snapshot JSON");

        let upgrade = import_bundled_dataset(&database).expect("import v2 dataset");
        assert_eq!(upgrade.dataset_version, "2.0.0");
        assert_eq!(upgrade.import_sequence, 2);

        let after_snapshot_json: String = database
            .connection()
            .expect("lock database")
            .query_row(
                "SELECT input_snapshot_json FROM research_assessments
                 WHERE assessment_id = ?1",
                [&assessment_id],
                |row| row.get(0),
            )
            .expect("read sealed snapshot after upgrade");
        assert_eq!(after_snapshot_json, before_snapshot_json);

        let after_read =
            read_sealed_assessment(&database, &assessment_id).expect("read historical assessment");
        assert_eq!(after_read.dataset_version, "1.0.0");
        assert_eq!(after_read.input_snapshot, before_read.input_snapshot);
    }

    #[test]
    fn snapshot_keeps_source_content_and_dependency_when_source_is_edited() {
        let database = setup();
        let (source_a_key, source_b_key) = ordered_sources(&database);
        let basis_reference = dataset_key(&database, "claim", 0);
        let mut input = blank_input();
        input
            .source_dependencies
            .push(AssessmentSourceDependencyInput {
                source_a_key: source_a_key.clone(),
                source_b_key: source_b_key.clone(),
                state: SourceDependencyState::AssessedIndependent,
                rationale: Some("Distinct original reporting records.".to_string()),
                basis_references: vec![basis_reference.clone()],
            });

        let read_source = |key: &str| {
            database
                .connection()
                .expect("lock database")
                .query_row(
                    "SELECT s.source_kind, s.title, s.publisher, s.author,
                            s.publication_date, s.url, s.other_locator,
                            s.verification_status, r.created_at, r.created_by_kind,
                            r.created_by_name
                     FROM sources s
                     JOIN records r ON r.record_id = s.record_id
                     JOIN dataset_record_mappings m ON m.record_id = s.record_id
                     WHERE m.dataset_id = ?1 AND m.record_key = ?2",
                    params![DATASET_ID, key],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, Option<String>>(5)?,
                            row.get::<_, Option<String>>(6)?,
                            row.get::<_, String>(7)?,
                            row.get::<_, String>(8)?,
                            row.get::<_, String>(9)?,
                            row.get::<_, Option<String>>(10)?,
                        ))
                    },
                )
                .expect("read source fields and provenance")
        };
        let original_a = read_source(&source_a_key);
        let original_b = read_source(&source_b_key);
        let id = create_preliminary_assessment(&database, input).expect("create assessment");
        let before = read_sealed_assessment(&database, &id).expect("read assessment");

        let source_snapshot = |case: &Value, key: &str| {
            case["sources"]
                .as_array()
                .expect("persisted case sources")
                .iter()
                .find(|source| source["record"]["datasetKey"] == key)
                .expect("snapshot source")
                .clone()
        };
        let source_a_snapshot =
            source_snapshot(&before.input_snapshot["persistedCase"], &source_a_key);
        let source_b_snapshot =
            source_snapshot(&before.input_snapshot["persistedCase"], &source_b_key);
        let assert_source = |snapshot: &Value,
                             key: &str,
                             values: &(
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            String,
            String,
            Option<String>,
        )| {
            assert_eq!(snapshot["record"]["datasetKey"], json!(key));
            assert_eq!(snapshot["sourceKind"], json!(values.0));
            assert_eq!(snapshot["title"], json!(values.1));
            assert_eq!(snapshot["publisher"], json!(values.2));
            assert_eq!(snapshot["author"], json!(values.3));
            assert_eq!(snapshot["publicationDate"], json!(values.4));
            assert_eq!(snapshot["url"], json!(values.5));
            assert_eq!(snapshot["otherLocator"], json!(values.6));
            assert_eq!(snapshot["verificationStatus"], json!(values.7));
            assert_eq!(snapshot["record"]["createdAt"], json!(values.8));
            assert_eq!(snapshot["record"]["createdBy"]["kind"], json!(values.9));
            assert_eq!(
                snapshot["record"]["createdBy"]["displayName"],
                json!(values.10)
            );
        };
        assert_source(&source_a_snapshot, &source_a_key, &original_a);
        assert_source(&source_b_snapshot, &source_b_key, &original_b);

        let dependency = &before.input_snapshot["assessmentInputs"]["sourceDependencies"][0];
        assert_eq!(dependency["sourceAKey"], json!(source_a_key));
        assert_eq!(dependency["sourceBKey"], json!(source_b_key));
        assert_eq!(dependency["state"], json!("assessed_independent"));
        assert_eq!(
            dependency["rationale"],
            json!("Distinct original reporting records.")
        );
        assert_eq!(dependency["basisReferences"], json!([basis_reference]));
        let dependency_read = &before.source_dependencies[0];
        assert_eq!(dependency_read.source_a_key, source_a_key);
        assert_eq!(dependency_read.source_b_key, source_b_key);
        assert_eq!(
            dependency_read.state,
            SourceDependencyState::AssessedIndependent
        );
        assert_eq!(
            dependency_read.rationale.as_deref(),
            Some("Distinct original reporting records.")
        );
        assert_eq!(
            dependency_read.basis_references,
            vec![basis_reference.clone()]
        );

        {
            let connection = database.connection().expect("lock database");
            for key in [&source_a_key, &source_b_key] {
                let source_id: String = connection
                    .query_row(
                        "SELECT record_id FROM dataset_record_mappings
                         WHERE dataset_id = ?1 AND record_key = ?2",
                        params![DATASET_ID, key],
                        |row| row.get(0),
                    )
                    .expect("lookup source id");
                connection
                    .execute(
                        "UPDATE sources
                         SET title = 'Edited title', publisher = 'Edited publisher',
                             author = 'Edited author', publication_date = '2099',
                             url = 'https://example.invalid/edited',
                             other_locator = 'Edited locator',
                             verification_status = 'needs_review'
                         WHERE record_id = ?1",
                        [&source_id],
                    )
                    .expect("edit live source fields");
                connection
                    .execute(
                        "UPDATE records
                         SET created_at = '2099-01-01T00:00:00Z',
                             created_by_kind = 'system',
                             created_by_name = 'Edited provenance'
                         WHERE record_id = ?1",
                        [&source_id],
                    )
                    .expect("edit live source provenance");
            }
        }
        let after = read_sealed_assessment(&database, &id).expect("read assessment after edits");
        let after_source_a = source_snapshot(&after.input_snapshot["persistedCase"], &source_a_key);
        let after_source_b = source_snapshot(&after.input_snapshot["persistedCase"], &source_b_key);
        assert_eq!(source_a_snapshot, after_source_a);
        assert_eq!(source_b_snapshot, after_source_b);
        assert_source(&after_source_a, &source_a_key, &original_a);
        assert_source(&after_source_b, &source_b_key, &original_b);
        assert_eq!(
            after.input_snapshot["assessmentInputs"]["sourceDependencies"][0],
            *dependency
        );
        assert_eq!(after.source_dependencies, before.source_dependencies);
    }
}
