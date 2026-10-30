use crate::research_records::{
    ClaimCategory, ClaimReviewStatus, SourceKind, SourceLocator, SourceVerificationStatus,
};
use serde::de::{self, DeserializeOwned, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::fmt;

const FORMAT_IDENTIFIER: &str = "xenoscope-research-dataset";
const SUPPORTED_FORMAT_VERSIONS: [u32; 2] = [1, 2];
const MAX_INPUT_BYTES: usize = 1024 * 1024;
const MAX_RECORDS: usize = 1_000;
const MAX_STRING_CHARS: usize = 20_000;
const MAX_DATASET_ID_CHARS: usize = 200;
const MAX_RECORD_KEY_CHARS: usize = 300;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResearchDataset {
    pub format: String,
    #[serde(rename = "formatVersion")]
    pub format_version: u32,
    #[serde(rename = "datasetId")]
    pub dataset_id: String,
    #[serde(rename = "datasetVersion")]
    pub dataset_version: String,
    #[serde(rename = "referenceConvention")]
    pub reference_convention: ReferenceConvention,
    #[serde(rename = "sourceAccess")]
    pub source_access: Vec<SourceAccess>,
    #[serde(rename = "sourceMetadata")]
    pub source_metadata: Vec<SourceMetadata>,
    pub records: DatasetRecords,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReferenceConvention {
    #[serde(rename = "recordIdentity")]
    pub record_identity: String,
    pub references: String,
    #[serde(rename = "omittedRecordMetadata")]
    pub omitted_record_metadata: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceAccess {
    #[serde(rename = "sourceKey")]
    pub source_key: String,
    #[serde(rename = "canonicalUrl")]
    pub canonical_url: String,
    #[serde(rename = "inspectedCopyUrl")]
    pub inspected_copy_url: Option<String>,
    #[serde(rename = "accessNote")]
    pub access_note: Option<String>,
    #[serde(rename = "equivalenceLimitation")]
    pub equivalence_limitation: Option<String>,
    #[serde(rename = "inspectionNote")]
    pub inspection_note: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceMetadata {
    #[serde(rename = "sourceKey")]
    pub source_key: String,
    #[serde(rename = "fieldsAsPrinted")]
    pub fields_as_printed: Option<HashMap<String, String>>,
    #[serde(rename = "fieldsAsDisplayed")]
    pub fields_as_displayed: Option<HashMap<String, String>>,
    #[serde(rename = "dateInterpretation")]
    pub date_interpretation: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DatasetRecords {
    pub incidents: Vec<DatasetIncident>,
    pub sources: Vec<DatasetSource>,
    #[serde(rename = "evidenceItems")]
    pub evidence_items: Vec<DatasetEvidenceItem>,
    pub claims: Vec<DatasetClaim>,
    pub relationships: Vec<DatasetRelationship>,
    #[serde(rename = "relationshipNotes")]
    pub relationship_notes: Vec<RelationshipNote>,
    pub discrepancies: Vec<DatasetDiscrepancy>,
    pub findings: Vec<DatasetFinding>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DatasetIncident {
    pub key: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<DatasetLocation>,
    #[serde(rename = "timePeriod", skip_serializing_if = "Option::is_none")]
    pub time_period: Option<DatasetTimePeriod>,
    #[serde(rename = "sourceKeys", skip_serializing_if = "Option::is_none")]
    pub source_keys: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DatasetLocation {
    pub description: String,
    pub precision: DatasetLocationPrecision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_wording: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DatasetLocationPrecision {
    NamedPlace,
    ApproximateArea,
    BoundedRegion,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DatasetTimePeriod {
    #[serde(rename = "startDate")]
    pub start_date: String,
    #[serde(rename = "endDate")]
    pub end_date: String,
    pub precision: DatasetTimePrecision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_wording: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DatasetTimePrecision {
    ExactDay,
    ApproximateDay,
    Month,
    Year,
    BoundedPeriod,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DatasetSource {
    pub key: String,
    #[serde(rename = "sourceKind")]
    pub source_kind: SourceKind,
    pub title: Option<String>,
    pub publisher: Option<String>,
    pub author: Option<String>,
    #[serde(rename = "publicationDate")]
    pub publication_date: Option<String>,
    pub url: Option<String>,
    #[serde(rename = "otherLocator")]
    pub other_locator: Option<String>,
    #[serde(rename = "verificationStatus")]
    pub verification_status: SourceVerificationStatus,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Document,
    Image,
    Video,
    Audio,
    Dataset,
    PhysicalItem,
    Other,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DatasetEvidenceItem {
    pub key: String,
    #[serde(rename = "sourceKey")]
    pub source_key: String,
    pub locator: SourceLocator,
    #[serde(rename = "evidenceKind")]
    pub evidence_kind: EvidenceKind,
    pub description: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DatasetReviewMetadata {
    pub status: ClaimReviewStatus,
    #[serde(rename = "reviewedAt")]
    pub reviewed_at: Option<String>,
    #[serde(rename = "reviewedBy")]
    pub reviewed_by: Option<DatasetActor>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    Researcher,
    System,
    Ai,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DatasetActor {
    pub kind: ActorKind,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DatasetAuthorStatedConfidence {
    #[serde(rename = "statedBy")]
    pub stated_by: String,
    #[serde(rename = "valueAsStated")]
    pub value_as_stated: String,
    #[serde(rename = "sourceKey")]
    pub source_key: String,
    pub locator: Option<SourceLocator>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DatasetClaim {
    pub key: String,
    #[serde(rename = "assertionText")]
    pub assertion_text: String,
    pub category: ClaimCategory,
    #[serde(rename = "sourceKey")]
    pub source_key: String,
    #[serde(rename = "sourceLocator")]
    pub source_locator: SourceLocator,
    #[serde(rename = "incidentKey")]
    pub incident_key: Option<String>,
    #[serde(rename = "authorStatedConfidence")]
    pub author_stated_confidence: Option<DatasetAuthorStatedConfidence>,
    pub review: DatasetReviewMetadata,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKind {
    Quotes,
    Repeats,
    DerivedFrom,
    Analyses,
    Supports,
    Contradicts,
    Corrects,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DatasetRecordKind {
    Incident,
    Source,
    EvidenceItem,
    Claim,
    Relationship,
    Discrepancy,
    Finding,
    History,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DatasetRecordReference {
    #[serde(rename = "recordKind")]
    pub record_kind: DatasetRecordKind,
    #[serde(rename = "recordKey")]
    pub record_key: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DatasetRelationship {
    pub key: String,
    pub kind: RelationshipKind,
    pub from: DatasetRecordReference,
    pub to: DatasetRecordReference,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RelationshipNote {
    #[serde(rename = "relationshipKey")]
    pub relationship_key: String,
    pub basis: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiscrepancyStatus {
    Open,
    Investigating,
    Documented,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DatasetDiscrepancy {
    pub key: String,
    #[serde(rename = "claimKeys")]
    pub claim_keys: Vec<String>,
    pub description: String,
    pub status: DiscrepancyStatus,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindingReferenceRole {
    Supporting,
    Conflicting,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DatasetFindingReference {
    #[serde(rename = "recordKey")]
    pub record_key: String,
    pub role: FindingReferenceRole,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DatasetFinding {
    pub key: String,
    pub assessment: String,
    pub attribution: DatasetActor,
    #[serde(rename = "claimReferences")]
    pub claim_references: Vec<DatasetFindingReference>,
    #[serde(rename = "evidenceReferences")]
    pub evidence_references: Vec<DatasetFindingReference>,
    pub review: DatasetReviewMetadata,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecordKind {
    Incident,
    Source,
    EvidenceItem,
    Claim,
    Relationship,
    Discrepancy,
    Finding,
}

impl RecordKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Incident => "incident",
            Self::Source => "source",
            Self::EvidenceItem => "evidence",
            Self::Claim => "claim",
            Self::Relationship => "relationship",
            Self::Discrepancy => "discrepancy",
            Self::Finding => "finding",
        }
    }

    fn domain_kind(self) -> &'static str {
        match self {
            Self::Incident => "incident",
            Self::Source => "source",
            Self::EvidenceItem => "evidence_item",
            Self::Claim => "claim",
            Self::Relationship => "relationship",
            Self::Discrepancy => "discrepancy",
            Self::Finding => "finding",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalRecordSnapshot {
    pub record_kind: String,
    pub content_json: String,
}

pub fn canonical_json(value: &Value) -> Result<String, String> {
    let canonical_value = sort_json_object_keys(value);
    serde_json::to_string(&canonical_value)
        .map_err(|error| format!("Could not serialize canonical JSON: {error}"))
}

fn sort_json_object_keys(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(sort_json_object_keys).collect()),
        Value::Object(object) => {
            let mut keys: Vec<_> = object.keys().collect();
            keys.sort_unstable();
            let sorted = keys
                .into_iter()
                .map(|key| (key.clone(), sort_json_object_keys(&object[key])))
                .collect();
            Value::Object(sorted)
        }
        scalar => scalar.clone(),
    }
}

impl ResearchDataset {
    pub fn canonical_record_snapshot(
        &self,
        record_key: &str,
    ) -> Result<CanonicalRecordSnapshot, String> {
        self.validate()?;
        let mut snapshot = None;
        for record in &self.records.incidents {
            if record.key == record_key {
                set_record_snapshot(
                    &mut snapshot,
                    record,
                    record.key.as_str(),
                    RecordKind::Incident,
                )?;
            }
        }
        for record in &self.records.sources {
            if record.key == record_key {
                set_record_snapshot(
                    &mut snapshot,
                    record,
                    record.key.as_str(),
                    RecordKind::Source,
                )?;
            }
        }
        for record in &self.records.evidence_items {
            if record.key == record_key {
                set_record_snapshot(
                    &mut snapshot,
                    record,
                    record.key.as_str(),
                    RecordKind::EvidenceItem,
                )?;
            }
        }
        for record in &self.records.claims {
            if record.key == record_key {
                set_record_snapshot(
                    &mut snapshot,
                    record,
                    record.key.as_str(),
                    RecordKind::Claim,
                )?;
            }
        }
        for record in &self.records.relationships {
            if record.key == record_key {
                set_record_snapshot(
                    &mut snapshot,
                    record,
                    record.key.as_str(),
                    RecordKind::Relationship,
                )?;
            }
        }
        for record in &self.records.discrepancies {
            if record.key == record_key {
                set_record_snapshot(
                    &mut snapshot,
                    record,
                    record.key.as_str(),
                    RecordKind::Discrepancy,
                )?;
            }
        }
        for record in &self.records.findings {
            if record.key == record_key {
                set_record_snapshot(
                    &mut snapshot,
                    record,
                    record.key.as_str(),
                    RecordKind::Finding,
                )?;
            }
        }
        snapshot.ok_or_else(|| format!("Dataset record {record_key:?} does not exist"))
    }
}

fn set_record_snapshot<T: Serialize>(
    snapshot: &mut Option<CanonicalRecordSnapshot>,
    record: &T,
    key: &str,
    record_kind: RecordKind,
) -> Result<(), String> {
    if snapshot.is_some() {
        return Err(format!("Dataset record key {key:?} is ambiguous"));
    }
    let mut value = serde_json::to_value(record)
        .map_err(|error| format!("Could not serialize record {key:?}: {error}"))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| format!("Serialized record {key:?} is not a JSON object"))?;
    if object.remove("key").is_none() {
        return Err(format!("Serialized record {key:?} has no stable key field"));
    }
    *snapshot = Some(CanonicalRecordSnapshot {
        record_kind: record_kind.domain_kind().to_string(),
        content_json: canonical_json(&value)?,
    });
    Ok(())
}

pub fn parse_and_validate(input: &str) -> Result<ResearchDataset, String> {
    if input.len() > MAX_INPUT_BYTES {
        return Err(format!(
            "Dataset input is {} bytes; maximum allowed size is {MAX_INPUT_BYTES} bytes",
            input.len()
        ));
    }

    let value = parse_json_without_duplicate_keys(input)
        .map_err(|error| format!("Malformed dataset JSON: {error}"))?;
    validate_string_limits(&value, "$")?;
    validate_record_shapes(&value)?;

    let dataset: ResearchDataset = serde_json::from_value(value)
        .map_err(|error| format!("Invalid research dataset fields: {error}"))?;
    dataset.validate()?;
    Ok(dataset)
}

impl ResearchDataset {
    fn validate(&self) -> Result<(), String> {
        if self.format != FORMAT_IDENTIFIER {
            return Err(format!(
                "Unsupported dataset format {:?}; expected {:?}",
                self.format, FORMAT_IDENTIFIER
            ));
        }
        if !SUPPORTED_FORMAT_VERSIONS.contains(&self.format_version) {
            return Err(format!(
                "Unsupported dataset formatVersion {}; supported versions are {} and {}",
                self.format_version, SUPPORTED_FORMAT_VERSIONS[0], SUPPORTED_FORMAT_VERSIONS[1]
            ));
        }
        require_nonblank("datasetId", &self.dataset_id)?;
        if self.dataset_id.contains(':') {
            return Err("datasetId must not contain ':'".to_string());
        }
        if self.dataset_id.chars().count() > MAX_DATASET_ID_CHARS {
            return Err(format!(
                "datasetId exceeds the {MAX_DATASET_ID_CHARS}-character limit"
            ));
        }
        require_nonblank("datasetVersion", &self.dataset_version)?;
        require_nonblank(
            "referenceConvention.recordIdentity",
            &self.reference_convention.record_identity,
        )?;
        require_nonblank(
            "referenceConvention.references",
            &self.reference_convention.references,
        )?;
        require_nonblank(
            "referenceConvention.omittedRecordMetadata",
            &self.reference_convention.omitted_record_metadata,
        )?;

        let record_count = self.records.incidents.len()
            + self.records.sources.len()
            + self.records.evidence_items.len()
            + self.records.claims.len()
            + self.records.relationships.len()
            + self.records.discrepancies.len()
            + self.records.findings.len();
        if record_count > MAX_RECORDS {
            return Err(format!(
                "Dataset contains {record_count} records; maximum allowed is {MAX_RECORDS}"
            ));
        }

        let registry = self.build_registry()?;
        self.validate_record_references(&registry)?;
        self.validate_metadata_references(&registry)?;
        Ok(())
    }

    pub(crate) fn validate_admission(&self) -> Result<(), String> {
        for incident in &self.records.incidents {
            let description = incident.description.as_deref().ok_or_else(|| {
                format!(
                    "Incident {:?} cannot be admitted: description is missing",
                    incident.key
                )
            })?;
            if is_generic_text(
                description,
                &[
                    "unknown",
                    "n a",
                    "ufo sighting",
                    "something happened",
                    "unidentified object",
                ],
            ) {
                return Err(format!(
                    "Incident {:?} cannot be admitted: description is empty or a generic placeholder",
                    incident.key
                ));
            }

            let location = incident.location.as_ref().ok_or_else(|| {
                format!(
                    "Incident {:?} cannot be admitted: location is missing",
                    incident.key
                )
            })?;
            if is_generic_text(
                &location.description,
                &[
                    "unknown",
                    "n a",
                    "somewhere",
                    "somewhere in the world",
                    "unknown location",
                    "undisclosed location",
                    "world",
                ],
            ) || (is_country_only(&location.description)
                && !description_has_specific_place_context(description))
            {
                return Err(format!(
                    "Incident {:?} cannot be admitted: location is empty, generic, or only identifies a country",
                    incident.key
                ));
            }

            let period = incident.time_period.as_ref().ok_or_else(|| {
                format!(
                    "Incident {:?} cannot be admitted: timePeriod is missing",
                    incident.key
                )
            })?;
            if period.precision == DatasetTimePrecision::BoundedPeriod
                && date_span_days(&period.start_date, &period.end_date)? > 31
            {
                return Err(format!(
                    "Incident {:?} cannot be admitted: bounded_period exceeds the 31-day maximum",
                    incident.key
                ));
            }

            let source_keys = incident.source_keys.as_deref().ok_or_else(|| {
                format!(
                    "Incident {:?} cannot be admitted: sourceKeys is missing",
                    incident.key
                )
            })?;
            if !source_keys.iter().any(|key| {
                self.records
                    .sources
                    .iter()
                    .find(|source| source.key == *key)
                    .is_some_and(source_is_traceable)
            }) {
                return Err(format!(
                    "Incident {:?} cannot be admitted: no referenced source has sufficient identifying information",
                    incident.key
                ));
            }
        }
        Ok(())
    }

    fn build_registry(&self) -> Result<HashMap<&str, RecordKind>, String> {
        let mut registry = HashMap::new();
        for incident in &self.records.incidents {
            register_key(
                &mut registry,
                &self.dataset_id,
                &incident.key,
                RecordKind::Incident,
                "records.incidents",
            )?;
            require_nonblank(&format!("{} label", incident.key), &incident.label)?;
            match self.format_version {
                1 => {
                    if incident.description.is_some()
                        || incident.location.is_some()
                        || incident.time_period.is_some()
                        || incident.source_keys.is_some()
                    {
                        return Err(format!(
                            "{}: v2 admission fields are not supported in formatVersion 1",
                            incident.key
                        ));
                    }
                }
                2 => {
                    let description = incident.description.as_deref().ok_or_else(|| {
                        format!(
                            "{}.description is required in formatVersion 2",
                            incident.key
                        )
                    })?;
                    require_nonblank(&format!("{}.description", incident.key), description)?;
                    let location = incident.location.as_ref().ok_or_else(|| {
                        format!("{}.location is required in formatVersion 2", incident.key)
                    })?;
                    require_nonblank(
                        &format!("{}.location.description", incident.key),
                        &location.description,
                    )?;
                    if let Some(wording) = &location.original_wording {
                        require_nonblank(
                            &format!("{}.location.originalWording", incident.key),
                            wording,
                        )?;
                    }
                    let period = incident.time_period.as_ref().ok_or_else(|| {
                        format!("{}.timePeriod is required in formatVersion 2", incident.key)
                    })?;
                    validate_time_period(period, &incident.key)?;
                    let source_keys = incident.source_keys.as_ref().ok_or_else(|| {
                        format!("{}.sourceKeys is required in formatVersion 2", incident.key)
                    })?;
                    if source_keys.is_empty() {
                        return Err(format!(
                            "{}.sourceKeys must contain at least one source key",
                            incident.key
                        ));
                    }
                    let unique_sources = source_keys.iter().collect::<HashSet<_>>();
                    if unique_sources.len() != source_keys.len() {
                        return Err(format!(
                            "{}.sourceKeys must not contain duplicate keys",
                            incident.key
                        ));
                    }
                }
                _ => unreachable!("unsupported versions are rejected before record validation"),
            }
        }
        for source in &self.records.sources {
            register_key(
                &mut registry,
                &self.dataset_id,
                &source.key,
                RecordKind::Source,
                "records.sources",
            )?;
            for (field, value) in [
                ("title", source.title.as_deref()),
                ("publisher", source.publisher.as_deref()),
                ("author", source.author.as_deref()),
                ("publicationDate", source.publication_date.as_deref()),
                ("url", source.url.as_deref()),
                ("otherLocator", source.other_locator.as_deref()),
            ] {
                if let Some(value) = value {
                    require_nonblank(&format!("{}.{}", source.key, field), value)?;
                }
            }
        }
        for evidence in &self.records.evidence_items {
            register_key(
                &mut registry,
                &self.dataset_id,
                &evidence.key,
                RecordKind::EvidenceItem,
                "records.evidenceItems",
            )?;
            require_nonblank(
                &format!("{}.description", evidence.key),
                &evidence.description,
            )?;
            validate_locator(&evidence.locator, &format!("{}.locator", evidence.key))?;
        }
        for claim in &self.records.claims {
            register_key(
                &mut registry,
                &self.dataset_id,
                &claim.key,
                RecordKind::Claim,
                "records.claims",
            )?;
            require_nonblank(
                &format!("{}.assertionText", claim.key),
                &claim.assertion_text,
            )?;
            validate_locator(
                &claim.source_locator,
                &format!("{}.sourceLocator", claim.key),
            )?;
            if let Some(confidence) = &claim.author_stated_confidence {
                require_nonblank(
                    &format!("{}.authorStatedConfidence.statedBy", claim.key),
                    &confidence.stated_by,
                )?;
                require_nonblank(
                    &format!("{}.authorStatedConfidence.valueAsStated", claim.key),
                    &confidence.value_as_stated,
                )?;
                if let Some(locator) = &confidence.locator {
                    validate_locator(
                        locator,
                        &format!("{}.authorStatedConfidence.locator", claim.key),
                    )?;
                }
            }
            validate_review(&claim.review, &claim.key)?;
        }
        for relationship in &self.records.relationships {
            register_key(
                &mut registry,
                &self.dataset_id,
                &relationship.key,
                RecordKind::Relationship,
                "records.relationships",
            )?;
        }
        for discrepancy in &self.records.discrepancies {
            register_key(
                &mut registry,
                &self.dataset_id,
                &discrepancy.key,
                RecordKind::Discrepancy,
                "records.discrepancies",
            )?;
            require_nonblank(
                &format!("{}.description", discrepancy.key),
                &discrepancy.description,
            )?;
            if discrepancy.claim_keys.len() < 2 {
                return Err(format!(
                    "{}.claimKeys must reference at least two claims",
                    discrepancy.key
                ));
            }
        }
        for finding in &self.records.findings {
            register_key(
                &mut registry,
                &self.dataset_id,
                &finding.key,
                RecordKind::Finding,
                "records.findings",
            )?;
            require_nonblank(&format!("{}.assessment", finding.key), &finding.assessment)?;
            if finding.claim_references.is_empty() && finding.evidence_references.is_empty() {
                return Err(format!(
                    "{} must reference at least one claim or evidence item",
                    finding.key
                ));
            }
            validate_review(&finding.review, &finding.key)?;
        }
        Ok(registry)
    }

    fn validate_record_references(
        &self,
        registry: &HashMap<&str, RecordKind>,
    ) -> Result<(), String> {
        if self.format_version == 2 {
            for incident in &self.records.incidents {
                for (index, source_key) in incident
                    .source_keys
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .enumerate()
                {
                    require_reference(
                        registry,
                        &self.dataset_id,
                        source_key,
                        RecordKind::Source,
                        &format!("{}.sourceKeys[{index}]", incident.key),
                    )?;
                }
            }
        }
        for evidence in &self.records.evidence_items {
            require_reference(
                registry,
                &self.dataset_id,
                &evidence.source_key,
                RecordKind::Source,
                &format!("{}.sourceKey", evidence.key),
            )?;
        }
        for claim in &self.records.claims {
            require_reference(
                registry,
                &self.dataset_id,
                &claim.source_key,
                RecordKind::Source,
                &format!("{}.sourceKey", claim.key),
            )?;
            if let Some(incident_key) = &claim.incident_key {
                require_reference(
                    registry,
                    &self.dataset_id,
                    incident_key,
                    RecordKind::Incident,
                    &format!("{}.incidentKey", claim.key),
                )?;
            }
            if let Some(confidence) = &claim.author_stated_confidence {
                require_reference(
                    registry,
                    &self.dataset_id,
                    &confidence.source_key,
                    RecordKind::Source,
                    &format!("{}.authorStatedConfidence.sourceKey", claim.key),
                )?;
            }
        }
        for relationship in &self.records.relationships {
            require_typed_reference(
                registry,
                &self.dataset_id,
                &relationship.from,
                &format!("{}.from", relationship.key),
            )?;
            require_typed_reference(
                registry,
                &self.dataset_id,
                &relationship.to,
                &format!("{}.to", relationship.key),
            )?;
            if relationship.from.record_key == relationship.to.record_key {
                return Err(format!(
                    "{} cannot relate a record to itself",
                    relationship.key
                ));
            }
        }
        for discrepancy in &self.records.discrepancies {
            for (index, claim_key) in discrepancy.claim_keys.iter().enumerate() {
                require_reference(
                    registry,
                    &self.dataset_id,
                    claim_key,
                    RecordKind::Claim,
                    &format!("{}.claimKeys[{index}]", discrepancy.key),
                )?;
            }
            let unique_claim_count = discrepancy
                .claim_keys
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len();
            if unique_claim_count < 2 {
                return Err(format!(
                    "{}.claimKeys must contain at least two distinct claims",
                    discrepancy.key
                ));
            }
        }
        for finding in &self.records.findings {
            for (index, reference) in finding.claim_references.iter().enumerate() {
                require_reference(
                    registry,
                    &self.dataset_id,
                    &reference.record_key,
                    RecordKind::Claim,
                    &format!("{}.claimReferences[{index}]", finding.key),
                )?;
            }
            for (index, reference) in finding.evidence_references.iter().enumerate() {
                require_reference(
                    registry,
                    &self.dataset_id,
                    &reference.record_key,
                    RecordKind::EvidenceItem,
                    &format!("{}.evidenceReferences[{index}]", finding.key),
                )?;
            }
        }
        Ok(())
    }

    fn validate_metadata_references(
        &self,
        registry: &HashMap<&str, RecordKind>,
    ) -> Result<(), String> {
        for (index, source_access) in self.source_access.iter().enumerate() {
            require_reference(
                registry,
                &self.dataset_id,
                &source_access.source_key,
                RecordKind::Source,
                &format!("sourceAccess[{index}].sourceKey"),
            )?;
            require_nonblank(
                &format!("sourceAccess[{index}].canonicalUrl"),
                &source_access.canonical_url,
            )?;
        }
        for (index, metadata) in self.source_metadata.iter().enumerate() {
            require_reference(
                registry,
                &self.dataset_id,
                &metadata.source_key,
                RecordKind::Source,
                &format!("sourceMetadata[{index}].sourceKey"),
            )?;
            require_nonblank(
                &format!("sourceMetadata[{index}].dateInterpretation"),
                &metadata.date_interpretation,
            )?;
        }
        for (index, note) in self.records.relationship_notes.iter().enumerate() {
            require_reference(
                registry,
                &self.dataset_id,
                &note.relationship_key,
                RecordKind::Relationship,
                &format!("records.relationshipNotes[{index}].relationshipKey"),
            )?;
            require_nonblank(
                &format!("records.relationshipNotes[{index}].basis"),
                &note.basis,
            )?;
        }
        Ok(())
    }
}

fn register_key<'a>(
    registry: &mut HashMap<&'a str, RecordKind>,
    dataset_id: &str,
    key: &'a str,
    expected_kind: RecordKind,
    field: &str,
) -> Result<(), String> {
    validate_key(key, dataset_id, expected_kind, field)?;
    if registry.insert(key, expected_kind).is_some() {
        return Err(format!("{field}.{key}: duplicate record key"));
    }
    Ok(())
}

fn validate_key(
    key: &str,
    dataset_id: &str,
    expected_kind: RecordKind,
    field: &str,
) -> Result<(), String> {
    if key.chars().count() > MAX_RECORD_KEY_CHARS {
        return Err(format!(
            "{field}: record key exceeds the {MAX_RECORD_KEY_CHARS}-character limit"
        ));
    }
    let prefix = format!("{dataset_id}:");
    let remainder = key.strip_prefix(&prefix).ok_or_else(|| {
        format!("{field}: key must start with the exact datasetId followed by ':'")
    })?;
    let (kind, slug) = remainder
        .split_once(':')
        .ok_or_else(|| format!("{field}: key must follow <datasetId>:<record-kind>:<slug>"))?;
    if kind != expected_kind.as_str() {
        return Err(format!(
            "{field}: key record kind must be {:?}, not {:?}",
            expected_kind.as_str(),
            kind
        ));
    }
    if !valid_slug(slug) {
        return Err(format!(
            "{field}: key slug must use lowercase ASCII letters/digits separated by single hyphens"
        ));
    }
    Ok(())
}

fn valid_slug(slug: &str) -> bool {
    if slug.is_empty() || slug.starts_with('-') || slug.ends_with('-') {
        return false;
    }
    let mut previous_was_hyphen = false;
    for character in slug.chars() {
        match character {
            'a'..='z' | '0'..='9' => previous_was_hyphen = false,
            '-' if !previous_was_hyphen => previous_was_hyphen = true,
            _ => return false,
        }
    }
    true
}

fn require_reference(
    registry: &HashMap<&str, RecordKind>,
    dataset_id: &str,
    key: &str,
    expected_kind: RecordKind,
    field: &str,
) -> Result<(), String> {
    validate_key(key, dataset_id, expected_kind, field)?;
    match registry.get(key) {
        Some(actual_kind) if *actual_kind == expected_kind => Ok(()),
        Some(actual_kind) => Err(format!(
            "{field}: expected {} record, but key resolves to {}",
            expected_kind.as_str(),
            actual_kind.as_str()
        )),
        None => Err(format!(
            "{field}: referenced {} record {:?} does not exist",
            expected_kind.as_str(),
            key
        )),
    }
}

fn require_typed_reference(
    registry: &HashMap<&str, RecordKind>,
    dataset_id: &str,
    reference: &DatasetRecordReference,
    field: &str,
) -> Result<(), String> {
    let expected_kind = match reference.record_kind {
        DatasetRecordKind::Incident => RecordKind::Incident,
        DatasetRecordKind::Source => RecordKind::Source,
        DatasetRecordKind::EvidenceItem => RecordKind::EvidenceItem,
        DatasetRecordKind::Claim => RecordKind::Claim,
        DatasetRecordKind::Relationship => RecordKind::Relationship,
        DatasetRecordKind::Discrepancy => RecordKind::Discrepancy,
        DatasetRecordKind::Finding => RecordKind::Finding,
        DatasetRecordKind::History => {
            return Err(format!(
                "{field}: dataset relationship endpoints cannot reference history records"
            ));
        }
    };
    require_reference(
        registry,
        dataset_id,
        &reference.record_key,
        expected_kind,
        field,
    )
}

fn validate_locator(locator: &SourceLocator, field: &str) -> Result<(), String> {
    locator
        .validate()
        .map_err(|error| format!("{field}: {error}"))
}

fn validate_review(review: &DatasetReviewMetadata, key: &str) -> Result<(), String> {
    if review.reviewed_at.is_some() != review.reviewed_by.is_some() {
        return Err(format!(
            "{key}.review must provide both reviewedAt and reviewedBy, or neither"
        ));
    }
    Ok(())
}

fn validate_string_limits(value: &Value, path: &str) -> Result<(), String> {
    match value {
        Value::String(text) if text.chars().count() > MAX_STRING_CHARS => Err(format!(
            "{path}: string exceeds the {MAX_STRING_CHARS}-character limit"
        )),
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                validate_string_limits(value, &format!("{path}[{index}]"))?;
            }
            Ok(())
        }
        Value::Object(values) => {
            for (key, value) in values {
                if key.chars().count() > MAX_STRING_CHARS {
                    return Err(format!(
                        "{path}: object field name exceeds the {MAX_STRING_CHARS}-character limit"
                    ));
                }
                validate_string_limits(value, &format!("{path}.{key}"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

struct JsonValueSeed;

impl<'de> DeserializeSeed<'de> for JsonValueSeed {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(JsonValueVisitor)
    }
}

struct JsonValueVisitor;

impl<'de> Visitor<'de> for JsonValueVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("JSON number is not finite"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(Value::String(value))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(JsonValueSeed)? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A>(self, mut object: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = Map::new();
        let mut seen = HashSet::new();
        while let Some(key) = object.next_key::<String>()? {
            if !seen.insert(key.clone()) {
                return Err(de::Error::custom(format!(
                    "duplicate object property {key:?}"
                )));
            }
            values.insert(key, object.next_value_seed(JsonValueSeed)?);
        }
        Ok(Value::Object(values))
    }
}

fn parse_json_without_duplicate_keys(input: &str) -> Result<Value, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_str(input);
    let value = JsonValueSeed.deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(value)
}

fn validate_record_shapes(value: &Value) -> Result<(), String> {
    validate_array_entries::<SourceAccess>(value, None, "sourceAccess", "sourceKey")?;
    validate_array_entries::<SourceMetadata>(value, None, "sourceMetadata", "sourceKey")?;
    validate_array_entries::<DatasetIncident>(value, Some("records"), "incidents", "key")?;
    validate_array_entries::<DatasetSource>(value, Some("records"), "sources", "key")?;
    validate_array_entries::<DatasetEvidenceItem>(value, Some("records"), "evidenceItems", "key")?;
    validate_array_entries::<DatasetClaim>(value, Some("records"), "claims", "key")?;
    validate_array_entries::<DatasetRelationship>(value, Some("records"), "relationships", "key")?;
    validate_array_entries::<RelationshipNote>(
        value,
        Some("records"),
        "relationshipNotes",
        "relationshipKey",
    )?;
    validate_array_entries::<DatasetDiscrepancy>(value, Some("records"), "discrepancies", "key")?;
    validate_array_entries::<DatasetFinding>(value, Some("records"), "findings", "key")?;
    Ok(())
}

fn validate_array_entries<T: DeserializeOwned>(
    value: &Value,
    parent: Option<&str>,
    collection: &str,
    key_field: &str,
) -> Result<(), String> {
    let mut collection_value = value;
    let mut path = String::new();
    if let Some(parent) = parent {
        path.push_str(parent);
        path.push('.');
        collection_value = match collection_value.get(parent) {
            Some(parent_value) => parent_value,
            None => return Ok(()),
        };
    }
    if !path.is_empty() {
        path.push_str(collection);
    } else {
        path.push_str(collection);
    }

    let Some(entries) = collection_value.get(collection).and_then(Value::as_array) else {
        return Ok(());
    };
    for (index, entry) in entries.iter().enumerate() {
        if let Err(error) = serde_json::from_value::<T>(entry.clone()) {
            let key = entry.get(key_field).and_then(Value::as_str);
            let context = key.map_or_else(
                || format!("{path}[{index}]"),
                |key| format!("{path}[{index}] (key {key:?})"),
            );
            return Err(format!("{context}: {error}"));
        }
    }
    Ok(())
}

fn require_nonblank(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field} must not be empty"))
    } else {
        Ok(())
    }
}

fn source_is_traceable(source: &DatasetSource) -> bool {
    let title = source
        .title
        .as_deref()
        .is_some_and(is_meaningful_source_text);
    let publisher = source
        .publisher
        .as_deref()
        .is_some_and(is_meaningful_source_text);
    let author = source
        .author
        .as_deref()
        .is_some_and(is_meaningful_source_text);
    let publication_date = source
        .publication_date
        .as_deref()
        .is_some_and(is_meaningful_source_text);
    let url = source.url.as_deref().is_some_and(valid_source_url);
    let other_locator = source
        .other_locator
        .as_deref()
        .is_some_and(is_meaningful_source_text);

    url || other_locator || (title && publisher) || (title && publication_date && author)
}

fn is_meaningful_source_text(value: &str) -> bool {
    !is_generic_text(
        value,
        &[
            "unknown",
            "n a",
            "none",
            "source",
            "report",
            "not available",
            "not provided",
            "unspecified",
            "placeholder",
            "archive",
            "catalogue",
            "catalog",
            "document id",
            "identifier",
            "id",
        ],
    )
}

fn valid_source_url(value: &str) -> bool {
    if !is_meaningful_source_text(value)
        || value.chars().any(char::is_whitespace)
        || value.chars().any(char::is_control)
    {
        return false;
    }
    let Some((scheme, remainder)) = value.split_once("://") else {
        return false;
    };
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
        return false;
    }
    let authority = remainder.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() || authority.contains('@') {
        return false;
    }
    let mut authority_parts = authority.split(':');
    let host = authority_parts.next().unwrap_or_default();
    let port = authority_parts.next();
    if authority_parts.next().is_some()
        || port.is_some_and(|port| {
            port.is_empty()
                || !port.bytes().all(|byte| byte.is_ascii_digit())
                || port
                    .parse::<u32>()
                    .map_or(true, |port| port == 0 || port > 65_535)
        })
    {
        return false;
    }
    if host.is_empty()
        || !host.contains('.')
        || host.starts_with('.')
        || host.ends_with('.')
        || host.split('.').any(|label| {
            label.is_empty()
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        return false;
    }
    let normalized_host = host.to_ascii_lowercase();
    !matches!(
        normalized_host.as_str(),
        "example.com" | "example.org" | "example.net" | "invalid.example"
    ) && !normalized_host.ends_with(".example.com")
        && !normalized_host.ends_with(".example.org")
        && !normalized_host.ends_with(".example.net")
}

fn is_generic_text(value: &str, placeholders: &[&str]) -> bool {
    let normalized = normalize_admission_text(value);
    normalized.is_empty() || placeholders.contains(&normalized.as_str())
}

fn normalize_admission_text(value: &str) -> String {
    let mut normalized = String::new();
    let mut previous_was_space = true;
    for character in value.chars() {
        if character.is_alphanumeric() {
            normalized.extend(character.to_lowercase());
            previous_was_space = false;
        } else if !previous_was_space {
            normalized.push(' ');
            previous_was_space = true;
        }
    }
    normalized.trim().to_string()
}

fn is_country_only(value: &str) -> bool {
    const COUNTRY_NAMES: &[&str] = &[
        "argentina",
        "australia",
        "austria",
        "belgium",
        "brazil",
        "canada",
        "chile",
        "china",
        "colombia",
        "cuba",
        "denmark",
        "egypt",
        "finland",
        "france",
        "germany",
        "greece",
        "india",
        "indonesia",
        "iran",
        "iraq",
        "ireland",
        "israel",
        "italy",
        "japan",
        "kenya",
        "mexico",
        "netherlands",
        "new zealand",
        "north korea",
        "norway",
        "pakistan",
        "peru",
        "poland",
        "portugal",
        "russia",
        "saudi arabia",
        "south africa",
        "south korea",
        "spain",
        "sweden",
        "switzerland",
        "syria",
        "turkey",
        "ukraine",
        "united arab emirates",
        "united kingdom",
        "united states",
        "usa",
        "uk",
        "vietnam",
    ];
    let normalized = normalize_admission_text(value);
    COUNTRY_NAMES.contains(&normalized.as_str())
}

fn description_has_specific_place_context(value: &str) -> bool {
    let normalized = normalize_admission_text(value);
    [
        "air base", "airfield", "airport", "city", "town", "village", "facility", "landmark",
        "mountain", "river", "lake", "coast", "province", "county", "district",
    ]
    .iter()
    .any(|context| normalized.contains(context))
}

fn validate_time_period(period: &DatasetTimePeriod, incident_key: &str) -> Result<(), String> {
    let start = parse_iso_date(&period.start_date)
        .map_err(|error| format!("{incident_key}.timePeriod.startDate: {error}"))?;
    let end = parse_iso_date(&period.end_date)
        .map_err(|error| format!("{incident_key}.timePeriod.endDate: {error}"))?;
    if start > end {
        return Err(format!(
            "{incident_key}.timePeriod startDate must not be after endDate"
        ));
    }

    match period.precision {
        DatasetTimePrecision::ExactDay => {
            if start != end {
                return Err(format!(
                    "{incident_key}.timePeriod exact_day precision requires identical bounds"
                ));
            }
        }
        DatasetTimePrecision::ApproximateDay => {
            let wording = period.original_wording.as_deref().ok_or_else(|| {
                format!(
                    "{incident_key}.timePeriod.originalWording is required for approximate_day precision"
                )
            })?;
            require_nonblank(
                &format!("{incident_key}.timePeriod.originalWording"),
                wording,
            )?;
        }
        DatasetTimePrecision::Month => {
            if start.0 != end.0
                || start.1 != end.1
                || start.2 != 1
                || end.2 != days_in_month(end.0, end.1)
            {
                return Err(format!(
                    "{incident_key}.timePeriod month precision requires bounds covering one complete calendar month"
                ));
            }
        }
        DatasetTimePrecision::Year => {
            if start.0 != end.0 || start.1 != 1 || start.2 != 1 || end.1 != 12 || end.2 != 31 {
                return Err(format!(
                    "{incident_key}.timePeriod year precision requires bounds covering one complete calendar year"
                ));
            }
        }
        DatasetTimePrecision::BoundedPeriod => {
            if start == end {
                return Err(format!(
                    "{incident_key}.timePeriod bounded_period precision requires a non-zero interval"
                ));
            }
        }
    }

    Ok(())
}

fn parse_iso_date(value: &str) -> Result<(u32, u32, u32), String> {
    let mut parts = value.split('-');
    let (Some(year), Some(month), Some(day), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err("date must use YYYY-MM-DD format".to_string());
    };
    if year.len() != 4
        || month.len() != 2
        || day.len() != 2
        || !year.bytes().all(|byte| byte.is_ascii_digit())
        || !month.bytes().all(|byte| byte.is_ascii_digit())
        || !day.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("date must use YYYY-MM-DD format".to_string());
    }
    let year = year
        .parse::<u32>()
        .map_err(|_| "date contains an invalid year".to_string())?;
    let month = month
        .parse::<u32>()
        .map_err(|_| "date contains an invalid month".to_string())?;
    let day = day
        .parse::<u32>()
        .map_err(|_| "date contains an invalid day".to_string())?;
    if year == 0 || !(1..=12).contains(&month) || !(1..=days_in_month(year, month)).contains(&day) {
        return Err("date is not a valid calendar date".to_string());
    }
    Ok((year, month, day))
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        2 if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn date_span_days(start: &str, end: &str) -> Result<u64, String> {
    let start =
        parse_iso_date(start).map_err(|error| format!("Invalid timePeriod startDate: {error}"))?;
    let end =
        parse_iso_date(end).map_err(|error| format!("Invalid timePeriod endDate: {error}"))?;
    date_ordinal(end)
        .checked_sub(date_ordinal(start))
        .ok_or_else(|| "Invalid timePeriod: endDate precedes startDate".to_string())
}

fn date_ordinal((year, month, day): (u32, u32, u32)) -> u64 {
    let prior_year = u64::from(year - 1);
    let days_before_year = prior_year * 365 + prior_year / 4 - prior_year / 100 + prior_year / 400;
    let days_before_month = (1..month)
        .map(|month| days_in_month(year, month))
        .map(u64::from)
        .sum::<u64>();
    days_before_year + days_before_month + u64::from(day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FIXTURE: &str = include_str!("../../datasets/research/al-taqaddum-2017.json");
    const LEGACY_V1_FIXTURE: &str =
        include_str!("../../datasets/research/test-fixtures/al-taqaddum-2017-v1-legacy.json");

    fn fixture_value() -> Value {
        serde_json::from_str(FIXTURE).expect("parse actual research fixture")
    }

    fn parse_value(value: Value) -> Result<ResearchDataset, String> {
        parse_and_validate(&serde_json::to_string(&value).expect("serialize fixture mutation"))
    }

    fn add_v2_admission_fields(value: &mut Value) {
        value["formatVersion"] = json!(2);
        value["records"]["incidents"][0]["description"] =
            json!("An object was reported over the base and recorded by an airborne sensor.");
        value["records"]["incidents"][0]["location"] = json!({
            "description": "An area near Al Taqaddum Air Base, Iraq",
            "precision": "approximate_area",
            "originalWording": "at Al Taqaddum Air Base, Iraq"
        });
        value["records"]["incidents"][0]["timePeriod"] = json!({
            "startDate": "2017-10-22",
            "endDate": "2017-10-24",
            "precision": "approximate_day",
            "originalWording": "around 23 October 2017"
        });
        value["records"]["incidents"][0]["sourceKeys"] =
            json!(["xenoscope.case.al-taqaddum-2017:source:aaro-report"]);
    }

    fn canonical_record_value<T: Serialize>(record: &T) -> Value {
        let mut value = serde_json::to_value(record).expect("serialize expected record");
        value
            .as_object_mut()
            .expect("record serializes as an object")
            .remove("key")
            .expect("record has stable key");
        value
    }

    #[test]
    fn actual_al_taqaddum_fixture_passes_validation_and_retains_provenance() {
        let dataset = parse_and_validate(FIXTURE).expect("actual fixture should validate");

        assert_eq!(dataset.dataset_id, "xenoscope.case.al-taqaddum-2017");
        assert_eq!(dataset.format_version, 2);
        assert_eq!(dataset.dataset_version, "2.0.0");
        assert_eq!(dataset.records.sources.len(), 2);
        assert_eq!(dataset.records.claims.len(), 10);
        let incident = &dataset.records.incidents[0];
        assert_eq!(
            incident.key,
            "xenoscope.case.al-taqaddum-2017:incident:al-taqaddum-object-report-v2"
        );
        assert_eq!(
            incident
                .time_period
                .as_ref()
                .expect("v2 incident period")
                .precision,
            DatasetTimePrecision::Month
        );
        assert_eq!(
            incident
                .time_period
                .as_ref()
                .unwrap()
                .original_wording
                .as_deref(),
            Some("October 2017")
        );
        assert_eq!(
            incident.source_keys.as_deref(),
            Some(
                [
                    "xenoscope.case.al-taqaddum-2017:source:aaro-report".to_string(),
                    "xenoscope.case.al-taqaddum-2017:source:dvids-record".to_string(),
                ]
                .as_slice()
            )
        );
        let aaro_date = dataset
            .records
            .claims
            .iter()
            .find(|claim| claim.key.ends_with(":claim:aaro-event-date-v2"))
            .expect("AARO event-date claim");
        assert_eq!(
            aaro_date.source_key,
            "xenoscope.case.al-taqaddum-2017:source:aaro-report"
        );
        assert_eq!(aaro_date.assertion_text, "Date: 23 October 2017");
        let dvids_date = dataset
            .records
            .claims
            .iter()
            .find(|claim| claim.key.ends_with(":claim:dvids-date-taken-v2"))
            .expect("DVIDS date claim");
        assert_eq!(
            dvids_date.source_key,
            "xenoscope.case.al-taqaddum-2017:source:dvids-record"
        );
        assert!(dvids_date.assertion_text.contains("10.01.2017"));
        let aaro_duration = dataset
            .records
            .claims
            .iter()
            .find(|claim| claim.key.ends_with(":claim:aaro-overview-duration-v2"))
            .expect("AARO duration claim");
        assert!(
            aaro_duration
                .assertion_text
                .contains("17 minutes and 30 seconds")
        );
        let dvids_duration = dataset
            .records
            .claims
            .iter()
            .find(|claim| claim.key.ends_with(":claim:dvids-listed-duration-v2"))
            .expect("DVIDS duration claim");
        assert!(dvids_duration.assertion_text.contains("00:17:27"));
        let balloon_assessment = dataset
            .records
            .claims
            .iter()
            .find(|claim| claim.key.ends_with(":claim:aaro-balloon-assessment-v2"))
            .expect("AARO-attributed assessment claim");
        assert!(
            balloon_assessment
                .assertion_text
                .starts_with("AARO assesses")
        );
        assert_eq!(
            dataset.records.evidence_items[0].description,
            "Video referenced by the DVIDS record. The video itself was not played or independently inspected for this dataset."
        );
        assert_eq!(dataset.source_access.len(), 2);
        assert_eq!(dataset.source_metadata.len(), 2);
        assert_eq!(dataset.records.relationship_notes.len(), 2);
        assert!(
            dataset.source_access[0]
                .equivalence_limitation
                .as_deref()
                .expect("AARO archive limitation")
                .contains("Byte-for-byte equivalence")
        );
        assert!(
            dataset.source_access[1]
                .inspection_note
                .as_deref()
                .expect("DVIDS inspection note")
                .contains("not played")
        );
        assert!(
            dataset.records.relationship_notes[0]
                .basis
                .contains("does not explicitly identify the DVIDS URL or video ID")
        );
    }

    #[test]
    fn parses_v1_incidents_without_admission_fields_or_snapshot_changes() {
        let dataset = parse_and_validate(LEGACY_V1_FIXTURE).expect("v1 fixture should parse");
        let incident = &dataset.records.incidents[0];
        assert!(incident.description.is_none());
        assert!(incident.location.is_none());
        assert!(incident.time_period.is_none());
        assert!(incident.source_keys.is_none());

        let snapshot = dataset
            .canonical_record_snapshot(&incident.key)
            .expect("legacy incident snapshot");
        assert_eq!(
            snapshot.content_json,
            r#"{"label":"Al Taqaddum object report, October 2017"}"#
        );
        assert!(!snapshot.content_json.contains("description"));
        assert!(!snapshot.content_json.contains("timePeriod"));
    }

    #[test]
    fn parses_v2_incident_fields_and_preserves_temporal_uncertainty() {
        let mut value = fixture_value();
        add_v2_admission_fields(&mut value);
        let dataset = parse_value(value).expect("valid v2 dataset should parse");
        let incident = &dataset.records.incidents[0];
        assert_eq!(
            incident.description.as_deref(),
            Some("An object was reported over the base and recorded by an airborne sensor.")
        );
        assert_eq!(
            incident.location.as_ref().unwrap().precision,
            DatasetLocationPrecision::ApproximateArea
        );
        assert_eq!(
            incident
                .location
                .as_ref()
                .unwrap()
                .original_wording
                .as_deref(),
            Some("at Al Taqaddum Air Base, Iraq")
        );
        let period = incident.time_period.as_ref().unwrap();
        assert_eq!(period.precision, DatasetTimePrecision::ApproximateDay);
        assert_eq!(period.start_date, "2017-10-22");
        assert_eq!(period.end_date, "2017-10-24");
        assert_eq!(
            period.original_wording.as_deref(),
            Some("around 23 October 2017")
        );
    }

    #[test]
    fn v2_requires_all_admission_fields() {
        for field in ["description", "location", "timePeriod", "sourceKeys"] {
            let mut value = fixture_value();
            add_v2_admission_fields(&mut value);
            value["records"]["incidents"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            let error = parse_value(value).expect_err("missing v2 field should fail");
            assert!(
                error.contains(&format!(
                    "incident:al-taqaddum-object-report-v2.{field} is required"
                )),
                "unexpected error for missing {field}: {error}"
            );
        }
    }

    #[test]
    fn rejects_malformed_v2_admission_fields_and_unknown_nested_fields() {
        let mut wrong_description_type = fixture_value();
        add_v2_admission_fields(&mut wrong_description_type);
        wrong_description_type["records"]["incidents"][0]["description"] = json!(42);
        let error = parse_value(wrong_description_type).expect_err("description must be a string");
        assert!(
            error.contains("records.incidents[0]"),
            "malformed description should identify its incident: {error}"
        );

        let mut unknown_location_field = fixture_value();
        add_v2_admission_fields(&mut unknown_location_field);
        unknown_location_field["records"]["incidents"][0]["location"]["coordinates"] =
            json!([33.0, 42.0]);
        assert!(
            parse_value(unknown_location_field)
                .expect_err("unknown location fields remain forbidden")
                .contains("unknown field `coordinates`")
        );

        let mut unknown_precision = fixture_value();
        add_v2_admission_fields(&mut unknown_precision);
        unknown_precision["records"]["incidents"][0]["timePeriod"]["precision"] =
            json!("approximate_month");
        assert!(
            parse_value(unknown_precision)
                .expect_err("unknown time precision should fail")
                .contains("approximate_month")
        );
    }

    #[test]
    fn validates_temporal_bounds_and_precision_without_a_bounded_period_cap() {
        let invalid_periods = [
            (
                json!({
                    "startDate": "2017-10-24",
                    "endDate": "2017-10-22",
                    "precision": "bounded_period"
                }),
                "must not be after",
            ),
            (
                json!({
                    "startDate": "2017-10-22",
                    "endDate": "2017-10-24",
                    "precision": "exact_day"
                }),
                "identical bounds",
            ),
            (
                json!({
                    "startDate": "2017-10-02",
                    "endDate": "2017-10-31",
                    "precision": "month"
                }),
                "complete calendar month",
            ),
            (
                json!({
                    "startDate": "2017-01-01",
                    "endDate": "2017-12-30",
                    "precision": "year"
                }),
                "complete calendar year",
            ),
            (
                json!({
                    "startDate": "2017-02-29",
                    "endDate": "2017-03-01",
                    "precision": "bounded_period"
                }),
                "valid calendar date",
            ),
        ];
        for (period, expected_error) in invalid_periods {
            let mut value = fixture_value();
            add_v2_admission_fields(&mut value);
            value["records"]["incidents"][0]["timePeriod"] = period;
            let error = parse_value(value).expect_err("invalid temporal structure should fail");
            assert!(error.contains(expected_error), "unexpected error: {error}");
        }

        let mut long_period = fixture_value();
        add_v2_admission_fields(&mut long_period);
        long_period["records"]["incidents"][0]["timePeriod"] = json!({
            "startDate": "2017-01-01",
            "endDate": "2017-03-01",
            "precision": "bounded_period",
            "originalWording": "between January and March 2017"
        });
        parse_value(long_period).expect("structural validation does not cap bounded periods");
    }

    #[test]
    fn validates_v2_source_key_shape_and_source_references() {
        let invalid_keys = [
            (
                json!(["xenoscope.case.al-taqaddum-2017:source:Bad_Key"]),
                "lowercase ASCII letters/digits",
            ),
            (
                json!(["xenoscope.case.al-taqaddum-2017:claim:aaro-event-date"]),
                "key record kind must be \"source\"",
            ),
            (
                json!(["xenoscope.case.al-taqaddum-2017:source:missing-source"]),
                "does not exist",
            ),
        ];
        for (source_keys, expected_error) in invalid_keys {
            let mut value = fixture_value();
            add_v2_admission_fields(&mut value);
            value["records"]["incidents"][0]["sourceKeys"] = source_keys;
            let error = parse_value(value).expect_err("invalid source reference should fail");
            assert!(error.contains(expected_error), "unexpected error: {error}");
        }

        let mut empty_keys = fixture_value();
        add_v2_admission_fields(&mut empty_keys);
        empty_keys["records"]["incidents"][0]["sourceKeys"] = json!([]);
        assert!(
            parse_value(empty_keys)
                .expect_err("sourceKeys must not be empty")
                .contains("sourceKeys must contain at least one source key")
        );
    }

    #[test]
    fn admission_accepts_identifiable_unreviewed_incident_without_scoring_evidence() {
        let mut value = fixture_value();
        add_v2_admission_fields(&mut value);
        let dataset = parse_value(value).expect("structurally valid v2 dataset");
        dataset
            .validate_admission()
            .expect("unreviewed and weak evidence do not block admission");
    }

    #[test]
    fn admission_rejects_generic_descriptions_and_locations() {
        for description in [
            "Unknown",
            "N/A",
            "UFO sighting",
            "Something happened",
            "Unidentified object",
        ] {
            let mut value = fixture_value();
            add_v2_admission_fields(&mut value);
            value["records"]["incidents"][0]["description"] = json!(description);
            let dataset = parse_value(value).expect("placeholder remains structurally valid");
            assert!(
                dataset
                    .validate_admission()
                    .unwrap_err()
                    .contains("description is empty or a generic placeholder"),
                "should reject description {description:?}"
            );
        }

        for location in ["Unknown", "somewhere in the world", "Iraq"] {
            let mut value = fixture_value();
            add_v2_admission_fields(&mut value);
            value["records"]["incidents"][0]["location"]["description"] = json!(location);
            let dataset = parse_value(value).expect("location remains structurally valid");
            assert!(
                dataset
                    .validate_admission()
                    .unwrap_err()
                    .contains("location is empty, generic, or only identifies a country"),
                "should reject location {location:?}"
            );
        }

        let mut blank_description = fixture_value();
        add_v2_admission_fields(&mut blank_description);
        blank_description["records"]["incidents"][0]["description"] = json!("");
        assert!(parse_value(blank_description).is_err());

        let mut blank_location = fixture_value();
        add_v2_admission_fields(&mut blank_location);
        blank_location["records"]["incidents"][0]["location"]["description"] = json!("");
        assert!(parse_value(blank_location).is_err());
    }

    #[test]
    fn admission_accepts_country_location_when_description_adds_place_context() {
        let mut value = fixture_value();
        add_v2_admission_fields(&mut value);
        value["records"]["incidents"][0]["location"]["description"] = json!("Iraq");
        value["records"]["incidents"][0]["description"] =
            json!("An object was recorded near Al Taqaddum Air Base in Iraq.");
        let dataset = parse_value(value).expect("structurally valid v2 dataset");
        dataset
            .validate_admission()
            .expect("specific place context can supplement a country location");
    }

    #[test]
    fn admission_limits_only_bounded_periods_to_31_days() {
        let mut accepted = fixture_value();
        add_v2_admission_fields(&mut accepted);
        accepted["records"]["incidents"][0]["timePeriod"] = json!({
            "startDate": "2017-01-01",
            "endDate": "2017-02-01",
            "precision": "bounded_period",
            "originalWording": "between 1 January and 1 February 2017"
        });
        parse_value(accepted)
            .unwrap()
            .validate_admission()
            .expect("31 elapsed days are allowed");

        let mut rejected = fixture_value();
        add_v2_admission_fields(&mut rejected);
        rejected["records"]["incidents"][0]["timePeriod"] = json!({
            "startDate": "2017-01-01",
            "endDate": "2017-02-02",
            "precision": "bounded_period",
            "originalWording": "between 1 January and 2 February 2017"
        });
        assert!(
            parse_value(rejected)
                .unwrap()
                .validate_admission()
                .unwrap_err()
                .contains("bounded_period exceeds the 31-day maximum")
        );
    }

    #[test]
    fn admission_requires_traceable_source_fields_without_checking_network_availability() {
        let mut untraceable = fixture_value();
        add_v2_admission_fields(&mut untraceable);
        for field in [
            "title",
            "publisher",
            "author",
            "publicationDate",
            "url",
            "otherLocator",
        ] {
            untraceable["records"]["sources"][0][field] = Value::Null;
        }
        let dataset = parse_value(untraceable).expect("untraceable source is structurally valid");
        assert!(
            dataset
                .validate_admission()
                .unwrap_err()
                .contains("no referenced source has sufficient identifying information")
        );

        let mut title_only = fixture_value();
        add_v2_admission_fields(&mut title_only);
        title_only["records"]["sources"][0]["publisher"] = Value::Null;
        title_only["records"]["sources"][0]["url"] = Value::Null;
        title_only["records"]["sources"][0]["otherLocator"] = Value::Null;
        title_only["records"]["sources"][0]["author"] = Value::Null;
        title_only["records"]["sources"][0]["publicationDate"] = Value::Null;
        assert!(
            parse_value(title_only)
                .unwrap()
                .validate_admission()
                .unwrap_err()
                .contains("no referenced source has sufficient identifying information")
        );

        let mut locator = fixture_value();
        add_v2_admission_fields(&mut locator);
        locator["records"]["sources"][0]["url"] = Value::Null;
        locator["records"]["sources"][0]["publisher"] = Value::Null;
        locator["records"]["sources"][0]["otherLocator"] =
            json!("Archive catalogue identifier: AARO-2017-23");
        parse_value(locator)
            .unwrap()
            .validate_admission()
            .expect("meaningful otherLocator is sufficient");

        let mut citation = fixture_value();
        add_v2_admission_fields(&mut citation);
        citation["records"]["sources"][0]["url"] = Value::Null;
        citation["records"]["sources"][0]["otherLocator"] = Value::Null;
        parse_value(citation)
            .unwrap()
            .validate_admission()
            .expect("title and publisher citation is sufficient");

        let mut malformed_url = fixture_value();
        add_v2_admission_fields(&mut malformed_url);
        malformed_url["records"]["sources"][0]["publisher"] = Value::Null;
        malformed_url["records"]["sources"][0]["url"] = json!("ftp://aaro.mil/report");
        malformed_url["records"]["sources"][0]["otherLocator"] = Value::Null;
        malformed_url["records"]["sources"][0]["title"] = Value::Null;
        assert!(
            parse_value(malformed_url)
                .unwrap()
                .validate_admission()
                .unwrap_err()
                .contains("no referenced source has sufficient identifying information")
        );
    }

    #[test]
    fn legacy_v1_dataset_is_readable_but_has_no_inferred_admission_fields() {
        let dataset = parse_and_validate(LEGACY_V1_FIXTURE).expect("legacy v1 remains readable");
        assert!(
            dataset
                .validate_admission()
                .unwrap_err()
                .contains("description is missing")
        );
    }

    #[test]
    fn canonical_json_sorts_top_level_and_nested_objects() {
        let first: Value = serde_json::from_str(r#"{"z":1,"nested":{"y":2,"x":3},"a":4}"#).unwrap();
        let second: Value =
            serde_json::from_str(r#"{"a":4,"nested":{"x":3,"y":2},"z":1}"#).unwrap();

        assert_eq!(
            canonical_json(&first).unwrap(),
            canonical_json(&second).unwrap()
        );
        assert_eq!(
            canonical_json(&first).unwrap(),
            r#"{"a":4,"nested":{"x":3,"y":2},"z":1}"#
        );
    }

    #[test]
    fn canonical_json_preserves_array_order_values_and_json_types() {
        let first: Value = serde_json::from_str(r#"{"items":[1,2],"value":1}"#).unwrap();
        let reordered_array: Value = serde_json::from_str(r#"{"items":[2,1],"value":1}"#).unwrap();
        let different_value: Value = serde_json::from_str(r#"{"items":[1,2],"value":2}"#).unwrap();
        let different_type: Value = serde_json::from_str(r#"{"items":[1,2],"value":"1"}"#).unwrap();

        let canonical_first = canonical_json(&first).unwrap();
        assert_ne!(canonical_first, canonical_json(&reordered_array).unwrap());
        assert_ne!(canonical_first, canonical_json(&different_value).unwrap());
        assert_ne!(canonical_first, canonical_json(&different_type).unwrap());
    }

    #[test]
    fn canonical_snapshots_cover_all_seven_record_types_without_dataset_metadata() {
        let mut dataset = parse_and_validate(FIXTURE).expect("actual fixture should validate");
        let finding: DatasetFinding = serde_json::from_value(json!({
            "key": "xenoscope.case.al-taqaddum-2017:finding:test-finding",
            "assessment": "A test-only finding used to validate snapshot coverage.",
            "attribution": { "kind": "researcher" },
            "claimReferences": [{
                "recordKey": "xenoscope.case.al-taqaddum-2017:claim:aaro-balloon-assessment-v2",
                "role": "supporting"
            }],
            "evidenceReferences": [{
                "recordKey": "xenoscope.case.al-taqaddum-2017:evidence:dvids-video-960331",
                "role": "conflicting"
            }],
            "review": { "status": "unreviewed" }
        }))
        .expect("deserialize test finding");
        dataset.records.findings.push(finding);

        let expected_records = vec![
            (
                dataset.records.incidents[0].key.as_str(),
                canonical_record_value(&dataset.records.incidents[0]),
                "incident",
            ),
            (
                dataset.records.sources[0].key.as_str(),
                canonical_record_value(&dataset.records.sources[0]),
                "source",
            ),
            (
                dataset.records.evidence_items[0].key.as_str(),
                canonical_record_value(&dataset.records.evidence_items[0]),
                "evidence_item",
            ),
            (
                dataset.records.claims[2].key.as_str(),
                canonical_record_value(&dataset.records.claims[2]),
                "claim",
            ),
            (
                dataset.records.relationships[0].key.as_str(),
                canonical_record_value(&dataset.records.relationships[0]),
                "relationship",
            ),
            (
                dataset.records.discrepancies[0].key.as_str(),
                canonical_record_value(&dataset.records.discrepancies[0]),
                "discrepancy",
            ),
            (
                dataset.records.findings[0].key.as_str(),
                canonical_record_value(&dataset.records.findings[0]),
                "finding",
            ),
        ];

        for (key, expected_content, expected_kind) in expected_records {
            let snapshot = dataset
                .canonical_record_snapshot(key)
                .expect("record snapshot");
            let snapshot_value: Value =
                serde_json::from_str(&snapshot.content_json).expect("snapshot JSON");
            assert_eq!(snapshot.record_kind, expected_kind);
            assert_eq!(
                snapshot.content_json,
                canonical_json(&expected_content).expect("canonical expected record")
            );
            assert!(
                snapshot_value.get("key").is_none(),
                "stable key is stored separately from snapshot content"
            );
        }

        let source_key = dataset.records.sources[0].key.clone();
        let original = dataset
            .canonical_record_snapshot(&source_key)
            .expect("source snapshot");
        dataset.dataset_version = "2.0.0".to_string();
        dataset.reference_convention.references = "Changed version-level metadata".to_string();
        dataset.source_access[0].access_note = Some("Changed access metadata".to_string());
        dataset.records.relationship_notes[0].basis = "Changed version-level rationale".to_string();
        assert_eq!(
            dataset
                .canonical_record_snapshot(&source_key)
                .expect("source snapshot after metadata change"),
            original
        );
    }

    #[test]
    fn rejects_malformed_json() {
        let error = parse_and_validate("{").expect_err("malformed JSON should fail");
        assert!(error.starts_with("Malformed dataset JSON:"));
    }

    #[test]
    fn rejects_duplicate_top_level_and_nested_properties() {
        let top_level = parse_and_validate(r#"{"format":"first","format":"second"}"#)
            .expect_err("duplicate top-level property should fail");
        assert!(top_level.contains("duplicate object property \"format\""));

        let nested = FIXTURE.replace(
            "\"title\": \"The Al Taqaddum Object\"",
            "\"title\": \"first\", \"title\": \"second\"",
        );
        assert_ne!(
            nested, FIXTURE,
            "nested duplicate test must mutate the fixture"
        );
        let nested_error =
            parse_and_validate(&nested).expect_err("duplicate nested property should fail");
        assert!(nested_error.contains("duplicate object property \"title\""));
    }

    #[test]
    fn rejects_unsupported_format_version() {
        let mut value = fixture_value();
        value["formatVersion"] = json!(3);
        let error = parse_value(value).expect_err("unsupported version should fail");
        assert!(error.contains("Unsupported dataset formatVersion 3"));
    }

    #[test]
    fn rejects_dataset_ids_containing_the_key_delimiter() {
        let mut value = fixture_value();
        value["datasetId"] = json!("xenoscope.case:al-taqaddum-2017");
        let error = parse_value(value).expect_err("dataset ID delimiter should fail");
        assert!(error.contains("datasetId must not contain ':'"));
    }

    #[test]
    fn accepts_fixture_key_structure_and_rejects_foreign_or_misleading_dataset_prefixes() {
        let dataset_id = "xenoscope.case.al-taqaddum-2017";
        assert!(
            validate_key(
                "xenoscope.case.al-taqaddum-2017:source:aaro-report",
                dataset_id,
                RecordKind::Source,
                "test key"
            )
            .is_ok()
        );

        for key in [
            "other.dataset:source:aaro-report",
            "prefix.xenoscope.case.al-taqaddum-2017:source:aaro-report",
            "xenoscope.case.al-taqaddum-2017-invalid:source:aaro-report",
        ] {
            assert!(
                validate_key(key, dataset_id, RecordKind::Source, "test key").is_err(),
                "should reject misleading key {key}"
            );
        }
    }

    #[test]
    fn rejects_malformed_key_slugs_and_wrong_collection_kind() {
        let dataset_id = "xenoscope.case.al-taqaddum-2017";
        for key in [
            "xenoscope.case.al-taqaddum-2017:source:",
            "xenoscope.case.al-taqaddum-2017:source:-leading",
            "xenoscope.case.al-taqaddum-2017:source:trailing-",
            "xenoscope.case.al-taqaddum-2017:source:two--hyphens",
            "xenoscope.case.al-taqaddum-2017:source:Uppercase",
            "xenoscope.case.al-taqaddum-2017:source:has_underscore",
            "xenoscope.case.al-taqaddum-2017::aaro-report",
            "xenoscope.case.al-taqaddum-2017:source::aaro-report",
            "xenoscope.case.al-taqaddum-2017:source:aaro:report",
        ] {
            assert!(
                validate_key(key, dataset_id, RecordKind::Source, "test key").is_err(),
                "should reject malformed key {key}"
            );
        }
        let error = validate_key(
            "xenoscope.case.al-taqaddum-2017:claim:aaro-event-date",
            dataset_id,
            RecordKind::Source,
            "test key",
        )
        .expect_err("key kind must match collection");
        assert!(error.contains("key record kind must be \"source\""));
    }

    #[test]
    fn rejects_duplicate_record_keys() {
        let mut value = fixture_value();
        let source_key = value["records"]["sources"][0]["key"]
            .as_str()
            .expect("fixture source key")
            .to_string();
        value["records"]["sources"][1]["key"] = json!(source_key);
        let error = parse_value(value).expect_err("duplicate key should fail");
        assert!(error.contains("duplicate record key"));
    }

    #[test]
    fn rejects_missing_references() {
        let mut value = fixture_value();
        value["records"]["evidenceItems"][0]["sourceKey"] =
            json!("xenoscope.case.al-taqaddum-2017:source:missing");
        let error = parse_value(value).expect_err("missing source should fail");
        assert!(error.contains(":evidence:dvids-video-960331.sourceKey"));
        assert!(error.contains("does not exist"));
    }

    #[test]
    fn rejects_relationship_endpoint_kind_mismatch() {
        let mut value = fixture_value();
        value["records"]["relationships"][0]["from"]["recordKind"] = json!("claim");
        let error = parse_value(value).expect_err("endpoint kind mismatch should fail");
        assert!(error.contains(".from"));
        assert!(error.contains("key record kind must be \"claim\""));
    }

    #[test]
    fn rejects_invalid_enum_values() {
        let mut value = fixture_value();
        value["records"]["sources"][0]["sourceKind"] = json!("made_up_kind");
        let error = parse_value(value).expect_err("invalid enum should fail");
        assert!(error.contains("records.sources[0]"));
        assert!(error.contains("aaro-report"));
    }

    #[test]
    fn rejects_invalid_locator_shapes() {
        let mut value = fixture_value();
        value["records"]["claims"][0]["sourceLocator"] = json!({
            "kind": "page",
            "page": 0
        });
        let error = parse_value(value).expect_err("page zero should fail");
        assert!(error.contains("sourceLocator"));
        assert!(error.contains("greater than zero"));
    }

    #[test]
    fn rejects_oversized_input_and_excessive_record_count() {
        let oversized = " ".repeat(MAX_INPUT_BYTES + 1);
        let error = parse_and_validate(&oversized).expect_err("oversized input should fail");
        assert!(error.contains("maximum allowed size is 1048576 bytes"));

        let mut value = fixture_value();
        let incidents = value["records"]["incidents"]
            .as_array_mut()
            .expect("incident collection");
        for index in 0..(MAX_RECORDS - incidents.len() + 1) {
            incidents.push(json!({
                "key": format!(
                    "xenoscope.case.al-taqaddum-2017:incident:excess-{index}"
                ),
                "label": "Extra test incident"
            }));
        }
        let error = parse_value(value).expect_err("excessive record count should fail");
        assert!(error.contains("maximum allowed is 1000"));
    }

    #[test]
    fn rejects_strings_over_the_individual_limit() {
        let mut value = fixture_value();
        value["referenceConvention"]["recordIdentity"] = json!("x".repeat(MAX_STRING_CHARS + 1));
        let error = parse_value(value).expect_err("oversized string should fail");
        assert!(error.contains("referenceConvention.recordIdentity"));
        assert!(error.contains("20000-character limit"));
    }

    #[test]
    fn rejects_invalid_author_confidence_source_reference() {
        let mut value = fixture_value();
        value["records"]["claims"][2]["authorStatedConfidence"]["sourceKey"] =
            json!("xenoscope.case.al-taqaddum-2017:claim:aaro-balloon-assessment");
        let error = parse_value(value).expect_err("confidence must refer to a source");
        assert!(error.contains("authorStatedConfidence.sourceKey"));
        assert!(error.contains("key record kind must be \"source\""));
    }

    #[test]
    fn reports_missing_fields_unknown_fields_and_wrong_types_with_record_context() {
        let mut missing = fixture_value();
        missing["records"]["incidents"][0]
            .as_object_mut()
            .expect("incident object")
            .remove("label");
        let error = parse_value(missing).expect_err("missing required field should fail");
        assert!(error.contains("records.incidents[0]"));
        assert!(error.contains("al-taqaddum-object-report"));
        assert!(error.contains("missing field `label`"));

        let mut unknown = fixture_value();
        unknown["records"]["sources"][0]["unexpected"] = json!(true);
        let error = parse_value(unknown).expect_err("unknown field should fail");
        assert!(error.contains("records.sources[0]"));
        assert!(error.contains("aaro-report"));
        assert!(error.contains("unknown field `unexpected`"));

        let mut wrong_type = fixture_value();
        wrong_type["records"]["sources"][0]["sourceKind"] = json!(7);
        let error = parse_value(wrong_type).expect_err("wrong JSON type should fail");
        assert!(error.contains("records.sources[0]"));
        assert!(error.contains("aaro-report"));
        assert!(error.contains("expected"));
    }

    #[test]
    fn rejects_dangling_source_metadata_and_relationship_note_references() {
        let mut metadata = fixture_value();
        metadata["sourceMetadata"][0]["sourceKey"] =
            json!("xenoscope.case.al-taqaddum-2017:source:missing");
        let error = parse_value(metadata).expect_err("dangling metadata source should fail");
        assert!(error.contains("sourceMetadata[0].sourceKey"));
        assert!(error.contains("does not exist"));

        let mut note = fixture_value();
        note["records"]["relationshipNotes"][0]["relationshipKey"] =
            json!("xenoscope.case.al-taqaddum-2017:relationship:missing");
        let error = parse_value(note).expect_err("dangling note relationship should fail");
        assert!(error.contains("relationshipNotes[0].relationshipKey"));
        assert!(error.contains("does not exist"));
    }

    fn fixture_with_finding(claim_key: &str, evidence_key: &str) -> Value {
        let mut value = fixture_value();
        value["records"]["findings"]
            .as_array_mut()
            .expect("finding collection")
            .push(json!({
                "key": "xenoscope.case.al-taqaddum-2017:finding:test-finding",
                "assessment": "Test finding for reference validation.",
                "attribution": { "kind": "researcher" },
                "claimReferences": [
                    { "recordKey": claim_key, "role": "supporting" }
                ],
                "evidenceReferences": [
                    { "recordKey": evidence_key, "role": "conflicting" }
                ],
                "review": { "status": "unreviewed" }
            }));
        value
    }

    #[test]
    fn rejects_finding_references_with_wrong_record_kinds() {
        let value = fixture_with_finding(
            "xenoscope.case.al-taqaddum-2017:evidence:dvids-video-960331",
            "xenoscope.case.al-taqaddum-2017:evidence:dvids-video-960331",
        );
        let error = parse_value(value).expect_err("claim reference to evidence should fail");
        assert!(error.contains("claimReferences[0]"));
        assert!(error.contains("key record kind must be \"claim\""));
    }

    #[test]
    fn rejects_missing_finding_records() {
        let value = fixture_with_finding(
            "xenoscope.case.al-taqaddum-2017:claim:aaro-event-date-v2",
            "xenoscope.case.al-taqaddum-2017:evidence:missing",
        );
        let error = parse_value(value).expect_err("missing evidence reference should fail");
        assert!(error.contains("evidenceReferences[0]"));
        assert!(error.contains("does not exist"));
    }
}
