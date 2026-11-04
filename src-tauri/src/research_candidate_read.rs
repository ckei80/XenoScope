use crate::{
    database::Database,
    research_candidate::{
        CandidateGenerationInput, ClaimCandidateInput, IncidentCandidateInput,
        IncidentPairCandidate, SourceDependencyInput, SourceDependencyKind, SuppressedIndexBucket,
        generate_incident_candidates,
    },
    research_dataset::{
        DatasetAuthorStatedConfidence, DatasetLocation, DatasetRecordKind, DatasetRecordReference,
        DatasetReviewMetadata, DatasetTimePeriod, RelationshipKind,
    },
    research_records::{ClaimCategory, SourceKind, SourceLocator, SourceVerificationStatus},
};
use rusqlite::{Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const CANDIDATE_GENERATOR_VERSION: &str = "xenoscope-stage2d-1";
pub(crate) const CANDIDATE_EVIDENCE_FORMAT_VERSION: u32 = 1;
const CANDIDATE_EVIDENCE_FINGERPRINT_DOMAIN: &str = "xenoscope-candidate-evidence";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SelectedDatasetImport {
    pub dataset_id: String,
    pub dataset_version: String,
    pub import_sequence: i64,
    pub imported_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CandidateIncidentProvenance {
    pub database_id: String,
    pub dataset_id: String,
    pub dataset_key: String,
    pub dataset_version: String,
    pub import_sequence: i64,
    pub imported_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CandidateClaimProvenance {
    pub database_id: String,
    pub dataset_id: String,
    pub dataset_key: String,
    pub incident_id: String,
    pub source_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CandidateSourceOrigin {
    pub dataset_id: String,
    pub dataset_key: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CandidateSourceDependencyRecord {
    pub relationship_record_id: String,
    pub dataset_id: String,
    pub dataset_key: String,
    pub edge: SourceDependencyInput,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CandidateCorpus {
    pub input: CandidateGenerationInput,
    pub selected_imports: Vec<SelectedDatasetImport>,
    pub incident_provenance: Vec<CandidateIncidentProvenance>,
    pub claim_provenance: Vec<CandidateClaimProvenance>,
    pub source_origins: BTreeMap<String, Vec<CandidateSourceOrigin>>,
    pub source_dependency_records: Vec<CandidateSourceDependencyRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeneratedCandidateCorpus {
    pub candidates: Vec<IncidentPairCandidate>,
    pub suppressed_buckets: Vec<SuppressedIndexBucket>,
    pub selected_imports: Vec<SelectedDatasetImport>,
    pub incident_provenance: Vec<CandidateIncidentProvenance>,
    pub claim_provenance: Vec<CandidateClaimProvenance>,
    pub source_origins: BTreeMap<String, Vec<CandidateSourceOrigin>>,
    pub source_dependency_records: Vec<CandidateSourceDependencyRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CandidateEvidenceSnapshot {
    pub evidence_format_version: u32,
    pub generator_version: String,
    pub candidate: IncidentPairCandidate,
    pub incident_provenance: Vec<CandidateIncidentProvenance>,
    pub claim_provenance: Vec<CandidateClaimProvenance>,
    pub relationship_provenance: Vec<CandidateSourceDependencyRecord>,
    pub source_origins: BTreeMap<String, Vec<CandidateSourceOrigin>>,
    pub selected_imports: Vec<SelectedDatasetImport>,
    pub corpus_suppressions: Vec<SuppressedIndexBucket>,
    pub fingerprint: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CandidateEvidenceFingerprint<'a> {
    domain: &'static str,
    evidence_format_version: u32,
    generator_version: &'a str,
    candidate: &'a IncidentPairCandidate,
    incident_provenance: Vec<IncidentProvenanceFingerprint<'a>>,
    claim_provenance: &'a [CandidateClaimProvenance],
    relationship_provenance: &'a [CandidateSourceDependencyRecord],
    source_origins: &'a BTreeMap<String, Vec<CandidateSourceOrigin>>,
    selected_imports: Vec<SelectedImportFingerprint<'a>>,
    corpus_suppressions: &'a [SuppressedIndexBucket],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IncidentProvenanceFingerprint<'a> {
    database_id: &'a str,
    dataset_id: &'a str,
    dataset_key: &'a str,
    dataset_version: &'a str,
    import_sequence: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SelectedImportFingerprint<'a> {
    dataset_id: &'a str,
    dataset_version: &'a str,
    import_sequence: i64,
}

impl CandidateEvidenceSnapshot {
    pub(crate) fn new(
        mut candidate: IncidentPairCandidate,
        corpus: &GeneratedCandidateCorpus,
        generator_version: impl Into<String>,
    ) -> Result<Self, String> {
        if candidate.left_incident_id >= candidate.right_incident_id {
            return Err(
                "Candidate evidence requires a canonically ordered, distinct incident pair"
                    .to_string(),
            );
        }
        let generator_version = generator_version.into();
        if generator_version.trim().is_empty() {
            return Err("Candidate generator version must not be blank".to_string());
        }

        candidate.supporting_signals.sort();
        candidate.supporting_signals.dedup();
        candidate.tension_signals.sort();
        candidate.tension_signals.dedup();
        candidate.missing_information.sort();
        candidate.missing_information.dedup();
        candidate.generated_by.sort();
        candidate.generated_by.dedup();

        let endpoint_ids = BTreeSet::from([
            candidate.left_incident_id.clone(),
            candidate.right_incident_id.clone(),
        ]);
        let mut incident_provenance = corpus
            .incident_provenance
            .iter()
            .filter(|incident| endpoint_ids.contains(&incident.database_id))
            .cloned()
            .collect::<Vec<_>>();
        incident_provenance.sort_by(|left, right| {
            (
                &left.database_id,
                &left.dataset_id,
                &left.dataset_key,
                &left.dataset_version,
                left.import_sequence,
                &left.imported_at,
            )
                .cmp(&(
                    &right.database_id,
                    &right.dataset_id,
                    &right.dataset_key,
                    &right.dataset_version,
                    right.import_sequence,
                    &right.imported_at,
                ))
        });
        incident_provenance.dedup();
        for incident_id in &endpoint_ids {
            if !incident_provenance
                .iter()
                .any(|provenance| &provenance.database_id == incident_id)
            {
                return Err(format!(
                    "Candidate incident {incident_id:?} has no selected import provenance"
                ));
            }
        }
        let mut claim_provenance = corpus
            .claim_provenance
            .iter()
            .filter(|claim| endpoint_ids.contains(&claim.incident_id))
            .cloned()
            .collect::<Vec<_>>();
        claim_provenance.sort();
        claim_provenance.dedup();

        let relationship_ids = candidate
            .supporting_signals
            .iter()
            .flat_map(|signal| match signal {
                crate::research_candidate::SupportingSignal::RelatedSourceReferences {
                    dependencies,
                    ..
                } => dependencies
                    .iter()
                    .map(|edge| edge.relationship_record_id.as_str())
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            })
            .collect::<BTreeSet<_>>();
        let mut relationship_provenance = corpus
            .source_dependency_records
            .iter()
            .filter(|record| relationship_ids.contains(record.relationship_record_id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        relationship_provenance.sort_by(|left, right| {
            (
                &left.relationship_record_id,
                &left.dataset_id,
                &left.dataset_key,
                &left.edge,
            )
                .cmp(&(
                    &right.relationship_record_id,
                    &right.dataset_id,
                    &right.dataset_key,
                    &right.edge,
                ))
        });
        relationship_provenance.dedup();
        for relationship_id in &relationship_ids {
            let records = relationship_provenance
                .iter()
                .filter(|record| &record.relationship_record_id == relationship_id)
                .collect::<Vec<_>>();
            if records.is_empty() {
                return Err(format!(
                    "Candidate dependency {relationship_id:?} has no persisted relationship provenance"
                ));
            }
            let candidate_edges = candidate
                .supporting_signals
                .iter()
                .flat_map(|signal| match signal {
                    crate::research_candidate::SupportingSignal::RelatedSourceReferences {
                        dependencies,
                        ..
                    } => dependencies
                        .iter()
                        .filter(|edge| &edge.relationship_record_id == relationship_id)
                        .collect::<Vec<_>>(),
                    _ => Vec::new(),
                })
                .collect::<Vec<_>>();
            if records
                .iter()
                .any(|record| !candidate_edges.iter().any(|edge| *edge == &record.edge))
            {
                return Err(format!(
                    "Candidate dependency {relationship_id:?} differs from persisted relationship provenance"
                ));
            }
        }

        let mut relevant_source_ids = BTreeSet::new();
        for signal in &candidate.supporting_signals {
            match signal {
                crate::research_candidate::SupportingSignal::SharedSourceReference {
                    source_id,
                } => {
                    relevant_source_ids.insert(source_id.clone());
                }
                crate::research_candidate::SupportingSignal::RelatedSourceReferences {
                    source_ids,
                    dependencies,
                    ..
                } => {
                    relevant_source_ids.extend(source_ids.iter().cloned());
                    for dependency in dependencies {
                        relevant_source_ids.insert(dependency.from_source_id.clone());
                        relevant_source_ids.insert(dependency.to_source_id.clone());
                    }
                }
                _ => {}
            }
        }
        relevant_source_ids.extend(claim_provenance.iter().map(|claim| claim.source_id.clone()));
        for relationship in &relationship_provenance {
            relevant_source_ids.insert(relationship.edge.from_source_id.clone());
            relevant_source_ids.insert(relationship.edge.to_source_id.clone());
        }
        if let Some(missing_source) = relevant_source_ids
            .iter()
            .find(|source_id| !corpus.source_origins.contains_key(*source_id))
        {
            return Err(format!(
                "Candidate source {missing_source:?} has no selected dataset origin"
            ));
        }
        let mut source_origins = relevant_source_ids
            .into_iter()
            .filter_map(|source_id| {
                corpus
                    .source_origins
                    .get(&source_id)
                    .map(|origins| (source_id, origins.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        for origins in source_origins.values_mut() {
            origins.sort();
            origins.dedup();
        }

        let relevant_dataset_ids = corpus
            .incident_provenance
            .iter()
            .filter(|incident| endpoint_ids.contains(&incident.database_id))
            .map(|incident| incident.dataset_id.clone())
            .chain(
                claim_provenance
                    .iter()
                    .map(|claim| claim.dataset_id.clone()),
            )
            .chain(
                source_origins
                    .values()
                    .flatten()
                    .map(|origin| origin.dataset_id.clone()),
            )
            .chain(
                relationship_provenance
                    .iter()
                    .map(|relationship| relationship.dataset_id.clone()),
            )
            .collect::<BTreeSet<_>>();
        let mut selected_imports = corpus
            .selected_imports
            .iter()
            .filter(|import| relevant_dataset_ids.contains(&import.dataset_id))
            .cloned()
            .collect::<Vec<_>>();
        selected_imports.sort_by(|left, right| {
            (
                &left.dataset_id,
                &left.dataset_version,
                left.import_sequence,
                &left.imported_at,
            )
                .cmp(&(
                    &right.dataset_id,
                    &right.dataset_version,
                    right.import_sequence,
                    &right.imported_at,
                ))
        });
        selected_imports.dedup();

        let mut corpus_suppressions = corpus
            .suppressed_buckets
            .iter()
            .filter(|suppression| is_candidate_relevant_suppression(suppression, &candidate))
            .cloned()
            .collect::<Vec<_>>();
        for suppression in &mut corpus_suppressions {
            suppression.affected_incident_ids.sort();
            suppression.affected_incident_ids.dedup();
        }
        corpus_suppressions.sort();
        corpus_suppressions.dedup();

        let evidence_format_version = CANDIDATE_EVIDENCE_FORMAT_VERSION;
        let fingerprint = candidate_evidence_fingerprint(
            evidence_format_version,
            &generator_version,
            &candidate,
            &incident_provenance,
            &claim_provenance,
            &relationship_provenance,
            &source_origins,
            &selected_imports,
            &corpus_suppressions,
        )?;

        Ok(Self {
            evidence_format_version,
            generator_version,
            candidate,
            incident_provenance,
            claim_provenance,
            relationship_provenance,
            source_origins,
            selected_imports,
            corpus_suppressions,
            fingerprint,
        })
    }

    pub(crate) fn validate_integrity(&self) -> Result<(), String> {
        if self.candidate.left_incident_id >= self.candidate.right_incident_id {
            return Err(
                "Candidate evidence contains a non-canonical or identical incident pair"
                    .to_string(),
            );
        }
        if self.generator_version.trim().is_empty() {
            return Err("Candidate evidence generator version must not be blank".to_string());
        }
        let expected = candidate_evidence_fingerprint(
            self.evidence_format_version,
            &self.generator_version,
            &self.candidate,
            &self.incident_provenance,
            &self.claim_provenance,
            &self.relationship_provenance,
            &self.source_origins,
            &self.selected_imports,
            &self.corpus_suppressions,
        )?;
        if self.fingerprint != expected {
            return Err(format!(
                "Candidate evidence fingerprint mismatch: stored {}, calculated {expected}",
                self.fingerprint
            ));
        }
        Ok(())
    }
}

fn candidate_evidence_fingerprint(
    evidence_format_version: u32,
    generator_version: &str,
    candidate: &IncidentPairCandidate,
    incident_provenance: &[CandidateIncidentProvenance],
    claim_provenance: &[CandidateClaimProvenance],
    relationship_provenance: &[CandidateSourceDependencyRecord],
    source_origins: &BTreeMap<String, Vec<CandidateSourceOrigin>>,
    selected_imports: &[SelectedDatasetImport],
    corpus_suppressions: &[SuppressedIndexBucket],
) -> Result<String, String> {
    let payload = CandidateEvidenceFingerprint {
        domain: CANDIDATE_EVIDENCE_FINGERPRINT_DOMAIN,
        evidence_format_version,
        generator_version,
        candidate,
        incident_provenance: incident_provenance
            .iter()
            .map(|incident| IncidentProvenanceFingerprint {
                database_id: &incident.database_id,
                dataset_id: &incident.dataset_id,
                dataset_key: &incident.dataset_key,
                dataset_version: &incident.dataset_version,
                import_sequence: incident.import_sequence,
            })
            .collect(),
        claim_provenance,
        relationship_provenance,
        source_origins,
        selected_imports: selected_imports
            .iter()
            .map(|import| SelectedImportFingerprint {
                dataset_id: &import.dataset_id,
                dataset_version: &import.dataset_version,
                import_sequence: import.import_sequence,
            })
            .collect(),
        corpus_suppressions,
    };
    let serialized = serde_json::to_vec(&payload)
        .map_err(|error| format!("Could not serialize candidate evidence fingerprint: {error}"))?;
    let digest = Sha256::digest(serialized);
    let mut fingerprint = String::with_capacity("sha256:".len() + digest.len() * 2);
    fingerprint.push_str("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut fingerprint, "{byte:02x}")
            .map_err(|error| format!("Could not encode candidate evidence fingerprint: {error}"))?;
    }
    Ok(fingerprint)
}

fn is_candidate_relevant_suppression(
    suppression: &SuppressedIndexBucket,
    candidate: &IncidentPairCandidate,
) -> bool {
    use crate::research_candidate::{RetrievalPass, SuppressionReason};

    let left_affected = suppression
        .affected_incident_ids
        .contains(&candidate.left_incident_id);
    let right_affected = suppression
        .affected_incident_ids
        .contains(&candidate.right_incident_id);
    match (suppression.pass, &suppression.reason) {
        (RetrievalPass::TimeOverlap, SuppressionReason::LongPeriod { .. }) => {
            left_affected || right_affected
        }
        _ => left_affected && right_affected,
    }
}

#[derive(Debug)]
struct SelectedImport {
    provenance: SelectedDatasetImport,
    format_version: i64,
    manifest: Vec<String>,
}

#[derive(Debug)]
struct StoredRow {
    dataset_id: String,
    dataset_version: String,
    import_sequence: i64,
    imported_at: String,
    key: String,
    kind: Option<String>,
    id: Option<String>,
    identity_kind: Option<String>,
    snapshot: Option<String>,
    incident_label: Option<String>,
    source_exists: bool,
    claim_source_id: Option<String>,
    claim_incident_id: Option<String>,
    relationship_kind: Option<String>,
    from_id: Option<String>,
    from_kind: Option<String>,
    to_id: Option<String>,
    to_kind: Option<String>,
}

#[derive(Clone, Debug)]
struct StoredRecord {
    kind: String,
    id: String,
    snapshot: Option<String>,
    domain: DomainRecord,
}

#[derive(Clone, Debug)]
enum DomainRecord {
    Incident {
        label: String,
    },
    Source,
    Claim {
        source_id: String,
        incident_id: Option<String>,
    },
    Relationship {
        kind: String,
        from_id: String,
        from_kind: String,
        to_id: String,
        to_kind: String,
    },
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IncidentSnapshot {
    label: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    location: Option<DatasetLocation>,
    #[serde(default)]
    time_period: Option<DatasetTimePeriod>,
    #[serde(default)]
    source_keys: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceSnapshot {
    source_kind: SourceKind,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    publisher: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    publication_date: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    other_locator: Option<String>,
    verification_status: SourceVerificationStatus,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ClaimSnapshot {
    assertion_text: String,
    category: ClaimCategory,
    source_key: String,
    source_locator: SourceLocator,
    #[serde(default)]
    incident_key: Option<String>,
    #[serde(default)]
    author_stated_confidence: Option<DatasetAuthorStatedConfidence>,
    review: DatasetReviewMetadata,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RelationshipSnapshot {
    kind: RelationshipKind,
    from: DatasetRecordReference,
    to: DatasetRecordReference,
}

pub(crate) fn read_candidate_corpus(database: &Database) -> Result<CandidateCorpus, String> {
    let mut connection = database.connection()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(|error| format!("Could not begin candidate corpus read: {error}"))?;

    let imports = read_latest_imports(&transaction)?;
    let rows = read_latest_manifest_rows(&transaction)?;
    let mut records_by_dataset = BTreeMap::<String, BTreeMap<String, StoredRecord>>::new();

    for row in rows {
        let context = format!(
            "Dataset {:?} version {:?} import sequence {} record {:?}",
            row.dataset_id, row.dataset_version, row.import_sequence, row.key
        );
        if row.identity_kind != row.kind {
            return Err(format!(
                "{context} has an inconsistent record identity kind"
            ));
        }
        let kind = row
            .kind
            .as_deref()
            .ok_or_else(|| format!("{context} has no record mapping"))?;
        let id = row
            .id
            .clone()
            .ok_or_else(|| format!("{context} has no canonical database identity"))?;

        let domain = match kind {
            "incident" => {
                let snapshot: IncidentSnapshot = decode_snapshot(&row, &context)?;
                let label = row.incident_label.ok_or_else(|| {
                    format!("{context} is missing persisted incident domain data")
                })?;
                if snapshot.label.trim().is_empty() {
                    return Err(format!("{context} has a blank incident label"));
                }
                if snapshot.label != label {
                    return Err(format!(
                        "{context} label differs from persisted incident data"
                    ));
                }
                DomainRecord::Incident { label }
            }
            "source" => {
                if !row.source_exists {
                    return Err(format!("{context} is missing persisted source domain data"));
                }
                let _: SourceSnapshot = decode_snapshot(&row, &context)?;
                DomainRecord::Source
            }
            "claim" => DomainRecord::Claim {
                source_id: row
                    .claim_source_id
                    .ok_or_else(|| format!("{context} is missing persisted claim domain data"))?,
                incident_id: row.claim_incident_id,
            },
            "relationship" => {
                if row.relationship_kind.is_some() {
                    DomainRecord::Relationship {
                        kind: row.relationship_kind.unwrap(),
                        from_id: row.from_id.ok_or_else(|| {
                            format!("{context} is missing persisted relationship endpoints")
                        })?,
                        from_kind: row.from_kind.ok_or_else(|| {
                            format!("{context} is missing persisted relationship endpoint kind")
                        })?,
                        to_id: row.to_id.ok_or_else(|| {
                            format!("{context} is missing persisted relationship endpoints")
                        })?,
                        to_kind: row.to_kind.ok_or_else(|| {
                            format!("{context} is missing persisted relationship endpoint kind")
                        })?,
                    }
                } else {
                    DomainRecord::Other
                }
            }
            _ => DomainRecord::Other,
        };
        let snapshot = match kind {
            "incident" | "source" | "claim" | "relationship" => row.snapshot,
            _ => None,
        };
        records_by_dataset
            .entry(row.dataset_id)
            .or_default()
            .insert(
                row.key,
                StoredRecord {
                    kind: kind.to_string(),
                    id,
                    snapshot,
                    domain,
                },
            );
    }

    let mut incidents = Vec::new();
    let mut incident_provenance = Vec::new();
    let mut claim_provenance = Vec::new();
    let mut source_dependency_records = Vec::new();
    let mut source_origins = BTreeMap::<String, Vec<CandidateSourceOrigin>>::new();
    let mut all_source_ids = BTreeSet::new();
    for import in &imports {
        if let Some(records) = records_by_dataset.get(&import.provenance.dataset_id) {
            for (key, record) in records.iter().filter(|(_, record)| record.kind == "source") {
                source_origins
                    .entry(record.id.clone())
                    .or_default()
                    .push(CandidateSourceOrigin {
                        dataset_id: import.provenance.dataset_id.clone(),
                        dataset_key: key.clone(),
                    });
                all_source_ids.insert(record.id.clone());
            }
        }
    }

    for import in &imports {
        let dataset_records = records_by_dataset
            .entry(import.provenance.dataset_id.clone())
            .or_default();
        validate_manifest(import, dataset_records)?;

        let mut claims_by_incident = BTreeMap::<String, Vec<ClaimCandidateInput>>::new();
        for key in import.manifest.iter().filter(|key| {
            dataset_records
                .get(*key)
                .is_some_and(|record| record.kind == "claim")
        }) {
            let context = record_context(import, key);
            let record = &dataset_records[key];
            let snapshot: ClaimSnapshot = decode_record(record, &context)?;
            snapshot
                .source_locator
                .validate()
                .map_err(|error| format!("{context} has an invalid source locator: {error}"))?;
            let source_id =
                resolve_record_id(dataset_records, import, key, &snapshot.source_key, "source")?;
            let (persisted_source_id, persisted_incident_id) = match &record.domain {
                DomainRecord::Claim {
                    source_id,
                    incident_id,
                } => (source_id, incident_id),
                _ => return Err(format!("{context} has inconsistent claim domain data")),
            };
            if persisted_source_id != &source_id {
                return Err(format!(
                    "{context} source reference differs from persisted claim data"
                ));
            }
            match snapshot.incident_key.as_deref() {
                Some(incident_key) => {
                    let incident_id =
                        resolve_record_id(dataset_records, import, key, incident_key, "incident")?;
                    if persisted_incident_id.as_deref() != Some(incident_id.as_str()) {
                        return Err(format!(
                            "{context} incident reference differs from persisted claim data"
                        ));
                    }
                    claims_by_incident
                        .entry(incident_key.to_string())
                        .or_default()
                        .push(ClaimCandidateInput {
                            record_id: record.id.clone(),
                            dataset_id: import.provenance.dataset_id.clone(),
                            dataset_key: key.clone(),
                            text: snapshot.assertion_text,
                            category: snapshot.category,
                            source_id: source_id.clone(),
                        });
                    claim_provenance.push(CandidateClaimProvenance {
                        database_id: record.id.clone(),
                        dataset_id: import.provenance.dataset_id.clone(),
                        dataset_key: key.clone(),
                        incident_id,
                        source_id,
                    });
                }
                None if persisted_incident_id.is_some() => {
                    return Err(format!(
                        "{context} has a persisted incident reference absent from its snapshot"
                    ));
                }
                None => {}
            }
        }

        for key in import.manifest.iter().filter(|key| {
            dataset_records
                .get(*key)
                .is_some_and(|record| record.kind == "incident")
        }) {
            let context = record_context(import, key);
            let record = &dataset_records[key];
            let snapshot: IncidentSnapshot = decode_record(record, &context)?;
            if import.format_version == 1
                && (snapshot.description.is_some()
                    || snapshot.location.is_some()
                    || snapshot.time_period.is_some()
                    || snapshot.source_keys.is_some())
            {
                return Err(format!(
                    "{context} has v2-only fields in a format version 1 snapshot"
                ));
            }
            if import.format_version == 2 && snapshot.source_keys.is_none() {
                return Err(format!(
                    "{context} is missing required sourceKeys for format version 2"
                ));
            }
            let source_keys = snapshot.source_keys.unwrap_or_default();
            if import.format_version == 2 && source_keys.is_empty() {
                return Err(format!(
                    "{context} has an empty sourceKeys list for format version 2"
                ));
            }
            if source_keys.iter().collect::<BTreeSet<_>>().len() != source_keys.len() {
                return Err(format!("{context} contains duplicate sourceKeys"));
            }
            let source_ids = source_keys
                .iter()
                .map(|source_key| {
                    resolve_record_id(dataset_records, import, key, source_key, "source")
                })
                .collect::<Result<Vec<_>, _>>()?;
            let claims = claims_by_incident.remove(key).unwrap_or_default();
            incidents.push(IncidentCandidateInput {
                identity: record.id.clone(),
                label: snapshot.label,
                description: snapshot.description,
                time_period: snapshot.time_period,
                location: snapshot.location,
                source_ids,
                claims,
            });
            incident_provenance.push(CandidateIncidentProvenance {
                database_id: record.id.clone(),
                dataset_id: import.provenance.dataset_id.clone(),
                dataset_key: key.clone(),
                dataset_version: import.provenance.dataset_version.clone(),
                import_sequence: import.provenance.import_sequence,
                imported_at: import.provenance.imported_at.clone(),
            });
        }

        for key in import.manifest.iter().filter(|key| {
            dataset_records
                .get(*key)
                .is_some_and(|record| record.kind == "relationship")
        }) {
            let context = record_context(import, key);
            let record = &dataset_records[key];
            let persisted_kind = match &record.domain {
                DomainRecord::Relationship { kind, .. } => kind.as_str(),
                _ => {
                    return Err(format!(
                        "{context} is missing persisted relationship domain data"
                    ));
                }
            };
            let snapshot: RelationshipSnapshot = decode_record(record, &context)?;
            if persisted_kind != relationship_kind_name(&snapshot.kind) {
                return Err(format!(
                    "{context} relationship snapshot kind differs from persisted relationship data"
                ));
            }
            if !is_dependency_kind(&snapshot.kind) {
                continue;
            }
            let (from_id, from_kind) =
                resolve_relationship_endpoint(dataset_records, import, key, &snapshot.from)?;
            let (to_id, to_kind) =
                resolve_relationship_endpoint(dataset_records, import, key, &snapshot.to)?;
            validate_relationship_record(record, &snapshot, &from_id, &to_id, &context)?;
            if from_kind != "source"
                || to_kind != "source"
                || !all_source_ids.contains(&from_id)
                || !all_source_ids.contains(&to_id)
            {
                continue;
            }
            source_dependency_records.push(CandidateSourceDependencyRecord {
                relationship_record_id: record.id.clone(),
                dataset_id: import.provenance.dataset_id.clone(),
                dataset_key: key.clone(),
                edge: SourceDependencyInput {
                    relationship_record_id: record.id.clone(),
                    from_source_id: from_id,
                    to_source_id: to_id,
                    kind: dependency_kind(&snapshot.kind)
                        .expect("dependency relationship kind was validated"),
                },
            });
        }
    }

    incidents.sort_by(|left, right| left.identity.cmp(&right.identity));
    incident_provenance.sort_by(|left, right| left.database_id.cmp(&right.database_id));
    claim_provenance.sort();
    for origins in source_origins.values_mut() {
        origins.sort();
        origins.dedup();
    }
    if let Some(duplicate) = incident_provenance
        .windows(2)
        .find(|pair| pair[0].database_id == pair[1].database_id)
    {
        return Err(format!(
            "Canonical incident identity {:?} appears in multiple selected dataset manifests",
            duplicate[0].database_id
        ));
    }
    source_dependency_records.sort_by(|left, right| {
        (
            &left.relationship_record_id,
            &left.edge.from_source_id,
            &left.edge.to_source_id,
            left.edge.kind,
        )
            .cmp(&(
                &right.relationship_record_id,
                &right.edge.from_source_id,
                &right.edge.to_source_id,
                right.edge.kind,
            ))
    });
    let source_dependencies = source_dependency_records
        .iter()
        .map(|record| record.edge.clone())
        .collect();

    transaction
        .commit()
        .map_err(|error| format!("Could not finish candidate corpus read: {error}"))?;
    Ok(CandidateCorpus {
        input: CandidateGenerationInput {
            incidents,
            source_dependencies,
        },
        selected_imports: imports
            .into_iter()
            .map(|import| import.provenance)
            .collect(),
        incident_provenance,
        claim_provenance,
        source_origins,
        source_dependency_records,
    })
}

pub(crate) fn generate_latest_incident_candidates(
    database: &Database,
) -> Result<GeneratedCandidateCorpus, String> {
    let corpus = read_candidate_corpus(database)?;
    let output = generate_incident_candidates(&corpus.input)
        .map_err(|error| format!("Could not generate incident candidates: {error}"))?;
    Ok(GeneratedCandidateCorpus {
        candidates: output.candidates,
        suppressed_buckets: output.suppressed_buckets,
        selected_imports: corpus.selected_imports,
        incident_provenance: corpus.incident_provenance,
        claim_provenance: corpus.claim_provenance,
        source_origins: corpus.source_origins,
        source_dependency_records: corpus.source_dependency_records,
    })
}

fn read_latest_imports(transaction: &Transaction<'_>) -> Result<Vec<SelectedImport>, String> {
    let mut statement = transaction
        .prepare(
            "SELECT current.dataset_id, current.dataset_version, current.import_sequence,
                    current.format, current.format_version, current.imported_at,
                    current.record_keys_json
             FROM dataset_imports current
             WHERE NOT EXISTS (
                 SELECT 1 FROM dataset_imports newer
                 WHERE newer.dataset_id = current.dataset_id
                   AND newer.import_sequence > current.import_sequence
             )
             ORDER BY current.dataset_id",
        )
        .map_err(|error| format!("Could not prepare latest-import query: {error}"))?;
    let rows = statement
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
        .map_err(|error| format!("Could not query latest imports: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Could not read latest imports: {error}"))?;
    drop(statement);

    rows.into_iter()
        .map(
            |(dataset_id, dataset_version, import_sequence, format, format_version, imported_at, manifest_json)| {
                let context = format!(
                    "Dataset {dataset_id:?} version {dataset_version:?} import sequence {import_sequence}"
                );
                if format != "xenoscope-research-dataset" || !matches!(format_version, 1 | 2) {
                    return Err(format!(
                        "{context} has unsupported format {format:?} version {format_version}"
                    ));
                }
                let manifest: Vec<String> = serde_json::from_str(&manifest_json)
                    .map_err(|error| format!("{context} manifest is malformed: {error}"))?;
                if manifest.iter().any(|key| key.trim().is_empty()) {
                    return Err(format!("{context} manifest contains a blank record key"));
                }
                if manifest.iter().collect::<BTreeSet<_>>().len() != manifest.len() {
                    return Err(format!("{context} manifest contains duplicate record keys"));
                }
                Ok(SelectedImport {
                    provenance: SelectedDatasetImport {
                        dataset_id,
                        dataset_version,
                        import_sequence,
                        imported_at,
                    },
                    format_version,
                    manifest,
                })
            },
        )
        .collect()
}

fn read_latest_manifest_rows(transaction: &Transaction<'_>) -> Result<Vec<StoredRow>, String> {
    let mut statement = transaction
        .prepare(
            "WITH latest AS (
                 SELECT current.dataset_id, current.dataset_version,
                        current.import_sequence, current.imported_at,
                        current.record_keys_json
                 FROM dataset_imports current
                 WHERE NOT EXISTS (
                     SELECT 1 FROM dataset_imports newer
                     WHERE newer.dataset_id = current.dataset_id
                       AND newer.import_sequence > current.import_sequence
                 )
             )
             SELECT latest.dataset_id, latest.dataset_version,
                    latest.import_sequence, latest.imported_at, manifest.value,
                    mappings.record_kind, mappings.record_id, records.record_kind,
                    CASE WHEN mappings.record_kind IN
                         ('incident', 'source', 'claim', 'relationship')
                         THEN mappings.content_snapshot_json END,
                    incidents.label, sources.record_id, claims.source_id,
                    claims.incident_id, relationships.relationship_kind,
                    relationships.from_record_id, relationships.from_record_kind,
                    relationships.to_record_id, relationships.to_record_kind
             FROM latest
             JOIN json_each(latest.record_keys_json) manifest
             LEFT JOIN dataset_record_mappings mappings
               ON mappings.dataset_id = latest.dataset_id
              AND mappings.record_key = manifest.value
             LEFT JOIN records
               ON records.record_id = mappings.record_id
             LEFT JOIN incidents
               ON mappings.record_kind = 'incident'
              AND incidents.record_id = mappings.record_id
             LEFT JOIN sources
               ON mappings.record_kind = 'source'
              AND sources.record_id = mappings.record_id
             LEFT JOIN claims
               ON mappings.record_kind = 'claim'
              AND claims.record_id = mappings.record_id
             LEFT JOIN relationships
               ON mappings.record_kind = 'relationship'
              AND relationships.record_id = mappings.record_id
             ORDER BY latest.dataset_id, manifest.value",
        )
        .map_err(|error| format!("Could not prepare latest-manifest record query: {error}"))?;
    statement
        .query_map([], |row| {
            Ok(StoredRow {
                dataset_id: row.get(0)?,
                dataset_version: row.get(1)?,
                import_sequence: row.get(2)?,
                imported_at: row.get(3)?,
                key: row.get(4)?,
                kind: row.get(5)?,
                id: row.get(6)?,
                identity_kind: row.get(7)?,
                snapshot: row.get(8)?,
                incident_label: row.get(9)?,
                source_exists: row.get::<_, Option<String>>(10)?.is_some(),
                claim_source_id: row.get(11)?,
                claim_incident_id: row.get(12)?,
                relationship_kind: row.get(13)?,
                from_id: row.get(14)?,
                from_kind: row.get(15)?,
                to_id: row.get(16)?,
                to_kind: row.get(17)?,
            })
        })
        .map_err(|error| format!("Could not query latest-manifest records: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Could not read latest-manifest records: {error}"))
}

fn validate_manifest(
    import: &SelectedImport,
    records: &BTreeMap<String, StoredRecord>,
) -> Result<(), String> {
    for key in &import.manifest {
        if !records.contains_key(key) {
            return Err(format!(
                "{} has no mapping for manifest record {key:?}",
                import_context(import)
            ));
        }
    }
    Ok(())
}

fn decode_snapshot<'a, T: serde::de::DeserializeOwned>(
    row: &'a StoredRow,
    context: &str,
) -> Result<T, String> {
    let json = row
        .snapshot
        .as_deref()
        .ok_or_else(|| format!("{context} has no content snapshot"))?;
    serde_json::from_str(json)
        .map_err(|error| format!("{context} has a malformed record snapshot: {error}"))
}

fn decode_record<T: serde::de::DeserializeOwned>(
    record: &StoredRecord,
    context: &str,
) -> Result<T, String> {
    let json = record
        .snapshot
        .as_deref()
        .ok_or_else(|| format!("{context} has no content snapshot"))?;
    serde_json::from_str(json)
        .map_err(|error| format!("{context} has a malformed record snapshot: {error}"))
}

fn resolve_record_id(
    records: &BTreeMap<String, StoredRecord>,
    import: &SelectedImport,
    owner_key: &str,
    reference_key: &str,
    expected_kind: &str,
) -> Result<String, String> {
    let context = record_context(import, owner_key);
    let referenced = records.get(reference_key).ok_or_else(|| {
        format!("{context} references missing {expected_kind} record key {reference_key:?}")
    })?;
    if referenced.kind != expected_kind {
        return Err(format!(
            "{context} reference {reference_key:?} resolves to {}, expected {expected_kind}",
            referenced.kind
        ));
    }
    Ok(referenced.id.clone())
}

fn resolve_relationship_endpoint(
    records: &BTreeMap<String, StoredRecord>,
    import: &SelectedImport,
    owner_key: &str,
    reference: &DatasetRecordReference,
) -> Result<(String, &'static str), String> {
    let expected_kind = match reference.record_kind {
        DatasetRecordKind::Incident => "incident",
        DatasetRecordKind::Source => "source",
        DatasetRecordKind::EvidenceItem => "evidence_item",
        DatasetRecordKind::Claim => "claim",
        DatasetRecordKind::Relationship => "relationship",
        DatasetRecordKind::Discrepancy => "discrepancy",
        DatasetRecordKind::Finding => "finding",
        DatasetRecordKind::History => "history",
    };
    let id = resolve_record_id(
        records,
        import,
        owner_key,
        &reference.record_key,
        expected_kind,
    )?;
    Ok((id, expected_kind))
}

fn validate_relationship_record(
    record: &StoredRecord,
    snapshot: &RelationshipSnapshot,
    from_id: &str,
    to_id: &str,
    context: &str,
) -> Result<(), String> {
    let (kind, persisted_from, persisted_from_kind, persisted_to, persisted_to_kind) =
        match &record.domain {
            DomainRecord::Relationship {
                kind,
                from_id,
                from_kind,
                to_id,
                to_kind,
            } => (
                kind.as_str(),
                from_id.as_str(),
                from_kind.as_str(),
                to_id.as_str(),
                to_kind.as_str(),
            ),
            _ => return Err(format!("{context} is missing persisted relationship data")),
        };
    let expected_kind = match snapshot.kind {
        RelationshipKind::Quotes => "quotes",
        RelationshipKind::Repeats => "repeats",
        RelationshipKind::DerivedFrom => "derived_from",
        _ => return Ok(()),
    };
    if kind != expected_kind
        || persisted_from != from_id
        || persisted_from_kind != record_kind_for(&snapshot.from.record_kind)
        || persisted_to != to_id
        || persisted_to_kind != record_kind_for(&snapshot.to.record_kind)
    {
        return Err(format!(
            "{context} relationship snapshot differs from persisted relationship data"
        ));
    }
    Ok(())
}

fn record_kind_for(kind: &DatasetRecordKind) -> &'static str {
    match kind {
        DatasetRecordKind::Incident => "incident",
        DatasetRecordKind::Source => "source",
        DatasetRecordKind::EvidenceItem => "evidence_item",
        DatasetRecordKind::Claim => "claim",
        DatasetRecordKind::Relationship => "relationship",
        DatasetRecordKind::Discrepancy => "discrepancy",
        DatasetRecordKind::Finding => "finding",
        DatasetRecordKind::History => "history",
    }
}

fn is_dependency_kind(kind: &RelationshipKind) -> bool {
    matches!(
        kind,
        RelationshipKind::Quotes | RelationshipKind::Repeats | RelationshipKind::DerivedFrom
    )
}

fn relationship_kind_name(kind: &RelationshipKind) -> &'static str {
    match kind {
        RelationshipKind::Quotes => "quotes",
        RelationshipKind::Repeats => "repeats",
        RelationshipKind::DerivedFrom => "derived_from",
        RelationshipKind::Analyses => "analyses",
        RelationshipKind::Supports => "supports",
        RelationshipKind::Contradicts => "contradicts",
        RelationshipKind::Corrects => "corrects",
    }
}

fn dependency_kind(kind: &RelationshipKind) -> Option<SourceDependencyKind> {
    match kind {
        RelationshipKind::Quotes => Some(SourceDependencyKind::Quotes),
        RelationshipKind::Repeats => Some(SourceDependencyKind::Repeats),
        RelationshipKind::DerivedFrom => Some(SourceDependencyKind::DerivedFrom),
        _ => None,
    }
}

fn import_context(import: &SelectedImport) -> String {
    format!(
        "Dataset {:?} version {:?} import sequence {}",
        import.provenance.dataset_id,
        import.provenance.dataset_version,
        import.provenance.import_sequence
    )
}

fn record_context(import: &SelectedImport, key: &str) -> String {
    format!("{} record {key:?}", import_context(import))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use crate::research_decision::{
        CreateDecisionInput, CreateSuggestionInput, DecisionKind, DecisionOutcome,
        DecisionRecordKind, DecisionTargetPair, RecordTarget, ResolveSuggestionInput,
        SuggestionOrigin, SuggestionOriginKind,
    };
    use crate::research_records::{ActorKind, RecordActor};
    use rusqlite::params;
    use serde_json::{Value, json};
    use std::sync::Arc;

    #[derive(Clone)]
    struct SeedRecord {
        key: String,
        kind: &'static str,
        id: String,
        snapshot: Value,
    }

    fn incident(key: &str, id: &str, source_keys: &[&str]) -> SeedRecord {
        SeedRecord {
            key: key.to_string(),
            kind: "incident",
            id: id.to_string(),
            snapshot: json!({
                "label": format!("Incident {key}"),
                "description": "Unusual lights observed",
                "location": null,
                "timePeriod": null,
                "sourceKeys": source_keys,
            }),
        }
    }

    fn legacy_incident(key: &str, id: &str) -> SeedRecord {
        SeedRecord {
            key: key.to_string(),
            kind: "incident",
            id: id.to_string(),
            snapshot: json!({ "label": format!("Legacy {key}") }),
        }
    }

    fn source(key: &str, id: &str) -> SeedRecord {
        SeedRecord {
            key: key.to_string(),
            kind: "source",
            id: id.to_string(),
            snapshot: json!({
                "sourceKind": "official_record",
                "title": null,
                "publisher": null,
                "author": null,
                "publicationDate": null,
                "url": null,
                "otherLocator": null,
                "verificationStatus": "verified",
            }),
        }
    }

    fn claim(key: &str, id: &str, source_key: &str, incident_key: Option<&str>) -> SeedRecord {
        SeedRecord {
            key: key.to_string(),
            kind: "claim",
            id: id.to_string(),
            snapshot: json!({
                "assertionText": format!("Claim {key}"),
                "category": "observation",
                "sourceKey": source_key,
                "sourceLocator": { "kind": "page", "page": 1 },
                "incidentKey": incident_key,
                "authorStatedConfidence": null,
                "review": { "status": "unreviewed" },
            }),
        }
    }

    fn relationship(
        key: &str,
        id: &str,
        kind: &str,
        from_kind: &str,
        from_key: &str,
        to_kind: &str,
        to_key: &str,
    ) -> SeedRecord {
        SeedRecord {
            key: key.to_string(),
            kind: "relationship",
            id: id.to_string(),
            snapshot: json!({
                "kind": kind,
                "from": { "recordKind": from_kind, "recordKey": from_key },
                "to": { "recordKind": to_kind, "recordKey": to_key },
            }),
        }
    }

    fn seed_import(
        database: &Database,
        dataset_id: &str,
        version: &str,
        sequence: i64,
        format_version: i64,
        records: &[SeedRecord],
    ) {
        let mut connection = database.connection().expect("lock test database");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("begin test import transaction");
        transaction
            .execute(
                "INSERT OR IGNORE INTO research_datasets (dataset_id) VALUES (?1)",
                [dataset_id],
            )
            .expect("insert dataset identity");

        let manifest = records
            .iter()
            .map(|record| record.key.clone())
            .collect::<Vec<_>>();
        transaction
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES (?1, ?2, ?3, 'xenoscope-research-dataset', ?4,
                           ?5, '{}', ?6)",
                params![
                    dataset_id,
                    version,
                    sequence,
                    format_version,
                    format!("2026-01-{sequence:02}T00:00:00Z"),
                    serde_json::to_string(&manifest).expect("encode manifest")
                ],
            )
            .expect("insert import row");

        let ids = records
            .iter()
            .map(|record| (record.key.as_str(), record.id.as_str()))
            .collect::<BTreeMap<_, _>>();
        for record in records {
            let snapshot = serde_json::to_string(&record.snapshot).expect("encode snapshot");
            let created_at = "2026-01-01T00:00:00Z";
            let record_kind = record.kind;
            transaction
                .execute(
                    "INSERT OR IGNORE INTO records (
                        record_id, record_kind, created_at, created_by_kind
                     ) VALUES (?1, ?2, ?3, 'system')",
                    params![record.id, record_kind, created_at],
                )
                .expect("insert canonical record");
            match record.kind {
                "incident" => {
                    transaction
                        .execute(
                            "INSERT OR IGNORE INTO incidents (record_id, record_kind, label)
                             VALUES (?1, 'incident', ?2)",
                            params![record.id, record.snapshot["label"].as_str().unwrap()],
                        )
                        .expect("insert incident domain record");
                }
                "source" => {
                    transaction
                        .execute(
                            "INSERT OR IGNORE INTO sources (
                                record_id, record_kind, source_kind, verification_status
                             ) VALUES (?1, 'source', 'official_record', 'verified')",
                            [&record.id],
                        )
                        .expect("insert source domain record");
                }
                "claim" => {
                    let source_id = ids[record.snapshot["sourceKey"].as_str().unwrap()];
                    let incident_id = record.snapshot["incidentKey"].as_str().map(|key| ids[key]);
                    transaction
                        .execute(
                            "INSERT OR IGNORE INTO claims (
                                record_id, record_kind, assertion_text, category, source_id,
                                source_locator_kind, source_locator_json, incident_id
                             ) VALUES (?1, 'claim', ?2, 'observation', ?3,
                                       'page', '{\"kind\":\"page\",\"page\":1}', ?4)",
                            params![
                                record.id,
                                record.snapshot["assertionText"].as_str().unwrap(),
                                source_id,
                                incident_id
                            ],
                        )
                        .expect("insert claim domain record");
                }
                "relationship" => {
                    let from_key = record.snapshot["from"]["recordKey"].as_str().unwrap();
                    let to_key = record.snapshot["to"]["recordKey"].as_str().unwrap();
                    let from_kind = record.snapshot["from"]["recordKind"].as_str().unwrap();
                    let to_kind = record.snapshot["to"]["recordKind"].as_str().unwrap();
                    let relation_kind = record.snapshot["kind"].as_str().unwrap();
                    transaction
                        .execute(
                            "INSERT OR IGNORE INTO relationships (
                                record_id, record_kind, relationship_kind,
                                from_record_id, from_record_kind, to_record_id, to_record_kind
                             ) VALUES (?1, 'relationship', ?2, ?3, ?4, ?5, ?6)",
                            params![
                                record.id,
                                relation_kind,
                                ids[from_key],
                                from_kind,
                                ids[to_key],
                                to_kind
                            ],
                        )
                        .expect("insert relationship domain record");
                }
                _ => panic!("unsupported test record kind"),
            }
            transaction
                .execute(
                    "INSERT OR IGNORE INTO dataset_record_mappings (
                        dataset_id, record_key, record_kind, record_id, content_snapshot_json
                     ) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![dataset_id, record.key, record_kind, record.id, snapshot],
                )
                .expect("insert dataset mapping");
        }
        transaction.commit().expect("commit test import");
    }

    fn db() -> Database {
        Database::in_memory().expect("create isolated database")
    }

    fn evidence_fixture() -> (GeneratedCandidateCorpus, IncidentPairCandidate) {
        let database = db();
        seed_evidence_fixture(&database);
        let generated = generate_latest_incident_candidates(&database)
            .expect("generate evidence fixture candidates");
        let candidate = generated
            .candidates
            .iter()
            .find(|candidate| {
                candidate.left_incident_id == "incident-id-a"
                    && candidate.right_incident_id == "incident-id-b"
            })
            .cloned()
            .expect("fixture candidate exists");
        (generated, candidate)
    }

    fn seed_evidence_fixture(database: &Database) {
        seed_import(
            &database,
            "evidence-dataset",
            "v1",
            1,
            2,
            &[
                incident("incident-a", "incident-id-a", &["source-a"]),
                incident("incident-b", "incident-id-b", &["source-b"]),
                source("source-a", "source-id-a"),
                source("source-b", "source-id-b"),
                claim("claim-a", "claim-id-a", "source-a", Some("incident-a")),
                claim("claim-b", "claim-id-b", "source-b", Some("incident-b")),
                relationship(
                    "dependency",
                    "relationship-id",
                    "quotes",
                    "source",
                    "source-a",
                    "source",
                    "source-b",
                ),
            ],
        );
    }

    fn create_snapshot_suggestion(
        database: &Database,
        snapshot: &CandidateEvidenceSnapshot,
    ) -> String {
        let target = DecisionTargetPair::new(
            DecisionKind::IncidentPair,
            RecordTarget {
                record_id: snapshot.candidate.left_incident_id.clone(),
                record_kind: crate::research_decision::DecisionRecordKind::Incident,
            },
            RecordTarget {
                record_id: snapshot.candidate.right_incident_id.clone(),
                record_kind: crate::research_decision::DecisionRecordKind::Incident,
            },
            None,
        )
        .expect("valid candidate target");
        database
            .create_research_suggestion(CreateSuggestionInput {
                target,
                proposed_outcome: None,
                origin: SuggestionOrigin {
                    kind: SuggestionOriginKind::System,
                    display_name: Some("XenoScope candidate generator".to_string()),
                    generator_version: Some(snapshot.generator_version.clone()),
                },
                actor: RecordActor {
                    kind: ActorKind::System,
                    display_name: Some("XenoScope".to_string()),
                },
                summary: "Potentially related incident reports".to_string(),
                references: vec![],
            })
            .expect("create test suggestion")
            .suggestion_id
    }

    fn create_researcher_suggestion(
        database: &Database,
        candidate: &IncidentPairCandidate,
    ) -> String {
        let target = DecisionTargetPair::new(
            DecisionKind::IncidentPair,
            RecordTarget {
                record_id: candidate.left_incident_id.clone(),
                record_kind: DecisionRecordKind::Incident,
            },
            RecordTarget {
                record_id: candidate.right_incident_id.clone(),
                record_kind: DecisionRecordKind::Incident,
            },
            None,
        )
        .expect("valid researcher suggestion target");
        database
            .create_research_suggestion(CreateSuggestionInput {
                target,
                proposed_outcome: None,
                origin: SuggestionOrigin {
                    kind: SuggestionOriginKind::Researcher,
                    display_name: Some("Researcher".to_string()),
                    generator_version: None,
                },
                actor: RecordActor {
                    kind: ActorKind::Researcher,
                    display_name: Some("Researcher".to_string()),
                },
                summary: "Manual review proposal".to_string(),
                references: vec![],
            })
            .expect("create researcher proposal")
            .suggestion_id
    }

    fn changed_evidence(
        mut generated: GeneratedCandidateCorpus,
        term: &str,
    ) -> GeneratedCandidateCorpus {
        generated.candidates[0].supporting_signals.push(
            crate::research_candidate::SupportingSignal::SharedTextTerms {
                terms: vec![term.to_string()],
                claim_provenance: vec![],
            },
        );
        generated
    }

    fn candidate_target_counts(database: &Database) -> (i64, i64, i64, i64) {
        database
            .connection()
            .expect("database connection")
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM research_suggestions
                     WHERE decision_kind = 'incident_pair'
                       AND record_a_id = 'incident-id-a'
                       AND record_b_id = 'incident-id-b'),
                    (SELECT COUNT(*) FROM suggestion_events events
                     JOIN research_suggestions suggestions
                       ON suggestions.suggestion_id = events.suggestion_id
                     WHERE suggestions.decision_kind = 'incident_pair'
                       AND suggestions.record_a_id = 'incident-id-a'
                       AND suggestions.record_b_id = 'incident-id-b'),
                    (SELECT COUNT(*) FROM suggestion_references reference_rows
                     JOIN research_suggestions suggestions
                       ON suggestions.suggestion_id = reference_rows.suggestion_id
                     WHERE suggestions.decision_kind = 'incident_pair'
                       AND suggestions.record_a_id = 'incident-id-a'
                       AND suggestions.record_b_id = 'incident-id-b'),
                    (SELECT COUNT(*) FROM candidate_evidence_snapshots evidence
                     JOIN research_suggestions suggestions
                       ON suggestions.suggestion_id = evidence.suggestion_id
                     WHERE suggestions.decision_kind = 'incident_pair'
                       AND suggestions.record_a_id = 'incident-id-a'
                       AND suggestions.record_b_id = 'incident-id-b')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("count candidate persistence rows")
    }

    #[test]
    fn snapshot_json_shape_round_trips_with_database_metadata_paths() {
        let (corpus, candidate) = evidence_fixture();
        let snapshot =
            CandidateEvidenceSnapshot::new(candidate, &corpus, CANDIDATE_GENERATOR_VERSION)
                .expect("candidate snapshot");
        let value = serde_json::to_value(&snapshot).expect("serialize snapshot");
        assert_eq!(value["candidate"]["left_incident_id"], "incident-id-a");
        assert_eq!(value["candidate"]["right_incident_id"], "incident-id-b");
        assert_eq!(value["generatorVersion"], snapshot.generator_version);
        assert_eq!(
            value["evidenceFormatVersion"],
            snapshot.evidence_format_version
        );
        assert_eq!(value["fingerprint"], snapshot.fingerprint);
        let decoded: CandidateEvidenceSnapshot =
            serde_json::from_value(value).expect("deserialize snapshot");
        assert_eq!(decoded, snapshot);
        decoded.validate_integrity().expect("validate snapshot");
    }

    #[test]
    fn snapshot_persists_and_reads_with_suggestion_metadata_validation() {
        let database = db();
        seed_evidence_fixture(&database);
        let generated =
            generate_latest_incident_candidates(&database).expect("generate candidate corpus");
        let candidate = generated.candidates[0].clone();
        let snapshot =
            CandidateEvidenceSnapshot::new(candidate, &generated, CANDIDATE_GENERATOR_VERSION)
                .expect("candidate snapshot");
        let suggestion_id = create_snapshot_suggestion(&database, &snapshot);

        database
            .insert_candidate_evidence_snapshot(&suggestion_id, &snapshot)
            .expect("insert candidate evidence");
        assert_eq!(
            database
                .get_candidate_evidence_snapshot(&suggestion_id)
                .expect("read candidate evidence"),
            Some(snapshot)
        );
        assert_eq!(
            database
                .get_candidate_evidence_snapshot("missing-suggestion")
                .expect("read absent candidate evidence"),
            None
        );
    }

    #[test]
    fn snapshot_storage_enforces_json_foreign_key_and_append_only_rules() {
        let database = db();
        seed_evidence_fixture(&database);
        let generated =
            generate_latest_incident_candidates(&database).expect("generate candidate corpus");
        let snapshot = CandidateEvidenceSnapshot::new(
            generated.candidates[0].clone(),
            &generated,
            CANDIDATE_GENERATOR_VERSION,
        )
        .expect("candidate snapshot");
        let suggestion_id = create_snapshot_suggestion(&database, &snapshot);
        database
            .insert_candidate_evidence_snapshot(&suggestion_id, &snapshot)
            .expect("insert candidate evidence");

        let trigger_check_suggestion = create_snapshot_suggestion(&database, &snapshot);
        let invalid_json_suggestion = create_snapshot_suggestion(&database, &snapshot);
        let connection = database.connection().expect("database connection");
        assert!(
            connection
                .execute(
                    "UPDATE candidate_evidence_snapshots
                     SET snapshot_json = '{}'
                     WHERE suggestion_id = ?1",
                    [&suggestion_id],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "DELETE FROM candidate_evidence_snapshots WHERE suggestion_id = ?1",
                    [&suggestion_id],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "INSERT INTO candidate_evidence_snapshots (
                        suggestion_id, snapshot_json, fingerprint, generator_version,
                        evidence_format_version, created_at
                     ) VALUES (?1, '{}', 'sha256:wrong', 'wrong-generator', 99, 'now')",
                    [&trigger_check_suggestion],
                )
                .is_err()
        );
        connection
            .execute_batch("DROP TRIGGER candidate_evidence_snapshot_validate_insert;")
            .expect("drop validation trigger for isolated foreign-key check");
        assert!(
            connection
                .execute(
                    "INSERT INTO candidate_evidence_snapshots (
                        suggestion_id, snapshot_json, fingerprint, generator_version,
                        evidence_format_version, created_at
                     ) VALUES (?1, '{', 'sha256:invalid', 'generator', 1, 'now')",
                    [&invalid_json_suggestion],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "INSERT INTO candidate_evidence_snapshots (
                        suggestion_id, snapshot_json, fingerprint, generator_version,
                        evidence_format_version, created_at
                     ) VALUES ('missing-suggestion', '{}', 'sha256:missing',
                               'generator', 1, 'now')",
                    [],
                )
                .is_err()
        );
    }

    #[test]
    fn snapshot_read_reports_malformed_or_inconsistent_stored_json() {
        let database = db();
        seed_evidence_fixture(&database);
        let generated =
            generate_latest_incident_candidates(&database).expect("generate candidate corpus");
        let snapshot = CandidateEvidenceSnapshot::new(
            generated.candidates[0].clone(),
            &generated,
            CANDIDATE_GENERATOR_VERSION,
        )
        .expect("candidate snapshot");
        let suggestion_id = create_snapshot_suggestion(&database, &snapshot);
        database
            .insert_candidate_evidence_snapshot(&suggestion_id, &snapshot)
            .expect("insert candidate evidence");

        {
            let connection = database.connection().expect("database connection");
            connection
                .execute_batch(
                    "DROP TRIGGER candidate_evidence_snapshot_no_update;
                     UPDATE candidate_evidence_snapshots
                     SET generator_version = 'inconsistent-generator';",
                )
                .expect("alter indexed metadata");
        }
        let metadata_error = database
            .get_candidate_evidence_snapshot(&suggestion_id)
            .expect_err("inconsistent indexed metadata must fail");
        assert!(metadata_error.contains(&suggestion_id));
        assert!(metadata_error.contains("metadata inconsistent"));
        {
            let connection = database.connection().expect("database connection");
            connection
                .execute(
                    "UPDATE candidate_evidence_snapshots SET snapshot_json = '{}'",
                    [],
                )
                .expect("replace snapshot with valid but incomplete JSON");
        }
        let error = database
            .get_candidate_evidence_snapshot(&suggestion_id)
            .expect_err("incomplete stored snapshot must fail");
        assert!(error.contains(&suggestion_id));
        assert!(error.contains("malformed"));
    }

    #[test]
    fn reconciliation_creates_once_then_reuses_without_duplicate_history() {
        let database = db();
        seed_evidence_fixture(&database);
        let generated =
            generate_latest_incident_candidates(&database).expect("generate candidate corpus");
        let first = database
            .reconcile_generated_incident_candidates(&generated)
            .expect("first reconciliation");
        assert_eq!(first.created_suggestion_ids.len(), 1);
        assert!(first.reused_suggestion_ids.is_empty());
        assert_eq!(first.skipped_candidates, 0);
        let created_id = first.created_suggestion_ids[0].clone();
        let snapshot = database
            .get_candidate_evidence_snapshot(&created_id)
            .expect("read generated evidence")
            .expect("snapshot exists");
        let suggestion = database
            .get_research_suggestion(&created_id)
            .expect("read generated suggestion")
            .expect("suggestion exists");
        assert_eq!(suggestion.proposed_outcome, None);
        assert_eq!(suggestion.origin.kind, SuggestionOriginKind::System);
        assert_eq!(
            suggestion.status,
            crate::research_decision::SuggestionStatus::Proposed
        );
        let reference_keys = suggestion
            .references
            .iter()
            .map(|reference| {
                (
                    reference.record.record_id.as_str(),
                    serde_json::to_string(&reference.record.record_kind)
                        .expect("serialize reference kind"),
                    reference.role.as_str(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(reference_keys.len(), suggestion.references.len());
        assert!(reference_keys.contains(&(
            "incident-id-a",
            "\"incident\"".to_string(),
            "candidate_incident"
        )));
        assert!(reference_keys.contains(&(
            "claim-id-a",
            "\"claim\"".to_string(),
            "candidate_claim"
        )));
        assert!(reference_keys.contains(&(
            "source-id-a",
            "\"source\"".to_string(),
            "candidate_source"
        )));
        assert!(reference_keys.contains(&(
            "relationship-id",
            "\"relationship\"".to_string(),
            "candidate_relationship"
        )));
        assert_eq!(snapshot.fingerprint.len(), 71);

        let counts_before = candidate_target_counts(&database);
        let second = database
            .reconcile_generated_incident_candidates(&generated)
            .expect("repeat reconciliation");
        assert_eq!(second.created_suggestion_ids.len(), 0);
        assert_eq!(second.reused_suggestion_ids, vec![created_id]);
        assert_eq!(second.skipped_candidates, 0);
        assert_eq!(candidate_target_counts(&database), counts_before);
    }

    #[test]
    fn changed_evidence_supersedes_only_the_current_system_proposal() {
        let database = db();
        seed_evidence_fixture(&database);
        let generated =
            generate_latest_incident_candidates(&database).expect("generate candidate corpus");
        let first = database
            .reconcile_generated_incident_candidates(&generated)
            .expect("initial reconciliation")
            .created_suggestion_ids[0]
            .clone();
        let changed = changed_evidence(generated.clone(), "changed-evidence-one");
        let second_result = database
            .reconcile_generated_incident_candidates(&changed)
            .expect("changed evidence reconciliation");
        let second = second_result.created_suggestion_ids[0].clone();
        assert_eq!(
            database
                .get_research_suggestion(&first)
                .expect("read superseded suggestion")
                .expect("suggestion exists")
                .status,
            crate::research_decision::SuggestionStatus::Superseded
        );
        let first_head = database
            .get_research_suggestion(&first)
            .expect("read old suggestion")
            .expect("old suggestion exists");
        assert_eq!(
            first_head
                .events
                .last()
                .unwrap()
                .replacement_suggestion_id
                .as_deref(),
            Some(second.as_str())
        );

        let changed_again = changed_evidence(generated, "changed-evidence-two");
        let third_result = database
            .reconcile_generated_incident_candidates(&changed_again)
            .expect("second changed evidence reconciliation");
        let third = third_result.created_suggestion_ids[0].clone();
        let second_head = database
            .get_research_suggestion(&second)
            .expect("read superseded current proposal")
            .expect("proposal exists");
        assert_eq!(
            second_head
                .events
                .last()
                .unwrap()
                .replacement_suggestion_id
                .as_deref(),
            Some(third.as_str())
        );
        assert_eq!(
            candidate_target_counts(&database),
            (3, 5, 21, 3),
            "historical proposals, evidence and events remain append-only"
        );
        assert!(
            database
                .get_candidate_evidence_snapshot(&first)
                .expect("read first evidence")
                .is_some()
        );
    }

    #[test]
    fn researcher_proposals_are_not_competed_with_or_superseded() {
        let database = db();
        seed_evidence_fixture(&database);
        let generated =
            generate_latest_incident_candidates(&database).expect("generate candidate corpus");
        let researcher_id = create_researcher_suggestion(&database, &generated.candidates[0]);
        let before = candidate_target_counts(&database);
        let result = database
            .reconcile_generated_incident_candidates(&generated)
            .expect("reconcile alongside researcher proposal");
        assert_eq!(result.skipped_candidates, 1);
        assert!(result.created_suggestion_ids.is_empty());
        assert_eq!(candidate_target_counts(&database), before);
        assert_eq!(
            database
                .get_research_suggestion(&researcher_id)
                .expect("read manual proposal")
                .expect("manual proposal exists")
                .status,
            crate::research_decision::SuggestionStatus::Proposed
        );
    }

    #[test]
    fn system_proposal_without_candidate_snapshot_is_not_assumed_to_have_changed_evidence() {
        let database = db();
        seed_evidence_fixture(&database);
        let generated =
            generate_latest_incident_candidates(&database).expect("generate candidate corpus");
        let legacy_system_id = create_snapshot_suggestion(
            &database,
            &CandidateEvidenceSnapshot::new(
                generated.candidates[0].clone(),
                &generated,
                CANDIDATE_GENERATOR_VERSION,
            )
            .expect("candidate snapshot"),
        );
        let before = candidate_target_counts(&database);

        let result = database
            .reconcile_generated_incident_candidates(&generated)
            .expect("reconcile existing system suggestion without evidence");
        assert_eq!(result.skipped_candidates, 1);
        assert_eq!(candidate_target_counts(&database), before);
        assert_eq!(
            database
                .get_research_suggestion(&legacy_system_id)
                .expect("read legacy system proposal")
                .expect("proposal exists")
                .status,
            crate::research_decision::SuggestionStatus::Proposed
        );
    }

    #[test]
    fn existing_candidate_snapshot_with_inconsistent_references_fails_safely() {
        let database = db();
        seed_evidence_fixture(&database);
        let generated =
            generate_latest_incident_candidates(&database).expect("generate candidate corpus");
        let snapshot = CandidateEvidenceSnapshot::new(
            generated.candidates[0].clone(),
            &generated,
            CANDIDATE_GENERATOR_VERSION,
        )
        .expect("candidate snapshot");
        let suggestion_id = create_snapshot_suggestion(&database, &snapshot);
        database
            .insert_candidate_evidence_snapshot(&suggestion_id, &snapshot)
            .expect("insert candidate evidence");
        let before = candidate_target_counts(&database);

        let error = database
            .reconcile_generated_incident_candidates(&generated)
            .expect_err("missing canonical references must be an integrity error");
        assert!(error.contains("inconsistent with its candidate evidence snapshot"));
        assert_eq!(candidate_target_counts(&database), before);
    }

    #[test]
    fn ambiguous_unlinked_suggestion_roots_abort_without_writes() {
        let database = db();
        seed_evidence_fixture(&database);
        let generated =
            generate_latest_incident_candidates(&database).expect("generate candidate corpus");
        create_researcher_suggestion(&database, &generated.candidates[0]);
        let snapshot = CandidateEvidenceSnapshot::new(
            generated.candidates[0].clone(),
            &generated,
            CANDIDATE_GENERATOR_VERSION,
        )
        .expect("candidate snapshot");
        create_snapshot_suggestion(&database, &snapshot);
        let before = candidate_target_counts(&database);

        let error = database
            .reconcile_generated_incident_candidates(&generated)
            .expect_err("ambiguous roots must fail safely");
        assert!(error.contains("expected exactly one"));
        assert_eq!(candidate_target_counts(&database), before);
    }

    #[test]
    fn dismissed_resolved_and_reversed_decision_targets_are_not_recreated() {
        let dismissed_database = db();
        seed_evidence_fixture(&dismissed_database);
        let dismissed_corpus = generate_latest_incident_candidates(&dismissed_database)
            .expect("generate dismissed fixture");
        let dismissed_id =
            create_researcher_suggestion(&dismissed_database, &dismissed_corpus.candidates[0]);
        let dismissed = dismissed_database
            .get_research_suggestion(&dismissed_id)
            .expect("read suggestion")
            .expect("suggestion exists");
        dismissed_database
            .dismiss_research_suggestion(
                &dismissed_id,
                &dismissed.events.last().unwrap().event_id,
                "Not relevant at this time.",
            )
            .expect("dismiss suggestion");
        let before = candidate_target_counts(&dismissed_database);
        let result = dismissed_database
            .reconcile_generated_incident_candidates(&dismissed_corpus)
            .expect("reconcile dismissed target");
        assert_eq!(result.skipped_candidates, 1);
        assert_eq!(candidate_target_counts(&dismissed_database), before);

        let resolved_database = db();
        seed_evidence_fixture(&resolved_database);
        let resolved_corpus = generate_latest_incident_candidates(&resolved_database)
            .expect("generate resolved fixture");
        let resolved_id =
            create_researcher_suggestion(&resolved_database, &resolved_corpus.candidates[0]);
        let resolved = resolved_database
            .get_research_suggestion(&resolved_id)
            .expect("read suggestion")
            .expect("suggestion exists");
        resolved_database
            .resolve_research_suggestion(
                &resolved_id,
                &resolved.events.last().unwrap().event_id,
                ResolveSuggestionInput {
                    outcome: DecisionOutcome::SameEvent,
                    rationale: "Researcher reviewed the evidence.".to_string(),
                    references: vec![],
                },
            )
            .expect("resolve suggestion");
        let before = candidate_target_counts(&resolved_database);
        let result = resolved_database
            .reconcile_generated_incident_candidates(&resolved_corpus)
            .expect("reconcile resolved target");
        assert_eq!(result.skipped_candidates, 1);
        assert_eq!(candidate_target_counts(&resolved_database), before);

        let decision_database = db();
        seed_evidence_fixture(&decision_database);
        let decision_corpus = generate_latest_incident_candidates(&decision_database)
            .expect("generate decision fixture");
        let target = DecisionTargetPair::new(
            DecisionKind::IncidentPair,
            RecordTarget {
                record_id: "incident-id-a".to_string(),
                record_kind: DecisionRecordKind::Incident,
            },
            RecordTarget {
                record_id: "incident-id-b".to_string(),
                record_kind: DecisionRecordKind::Incident,
            },
            None,
        )
        .expect("decision target");
        let decision = decision_database
            .create_research_decision(CreateDecisionInput {
                target,
                outcome: DecisionOutcome::SameEvent,
                rationale: "Recorded before candidate generation.".to_string(),
                references: vec![],
                origin_suggestion_id: None,
            })
            .expect("create decision");
        let head = decision.current_head.expect("decision event head");
        decision_database
            .reverse_research_decision(
                &decision.decision_id,
                &head.event_id,
                "Reversed by researcher.",
            )
            .expect("reverse decision");
        let before = candidate_target_counts(&decision_database);
        let result = decision_database
            .reconcile_generated_incident_candidates(&decision_corpus)
            .expect("reconcile target with reversed decision");
        assert_eq!(result.skipped_candidates, 1);
        assert_eq!(candidate_target_counts(&decision_database), before);
    }

    #[test]
    fn reconciliation_rejects_stale_latest_import_set_without_writes() {
        let database = db();
        seed_evidence_fixture(&database);
        let generated =
            generate_latest_incident_candidates(&database).expect("generate candidate corpus");
        seed_import(
            &database,
            "evidence-dataset",
            "v2",
            2,
            2,
            &[
                incident("incident-a", "incident-id-a", &["source-a"]),
                incident("incident-b", "incident-id-b", &["source-b"]),
                source("source-a", "source-id-a"),
                source("source-b", "source-id-b"),
                claim("claim-a", "claim-id-a", "source-a", Some("incident-a")),
                claim("claim-b", "claim-id-b", "source-b", Some("incident-b")),
                relationship(
                    "dependency",
                    "relationship-id",
                    "quotes",
                    "source",
                    "source-a",
                    "source",
                    "source-b",
                ),
            ],
        );
        let error = database
            .reconcile_generated_incident_candidates(&generated)
            .expect_err("stale candidate corpus must fail");
        assert!(error.contains("Candidate corpus is stale"));
        assert_eq!(candidate_target_counts(&database), (0, 0, 0, 0));
    }

    #[test]
    fn reconciliation_rejects_a_new_dataset_added_after_generation() {
        let database = db();
        seed_evidence_fixture(&database);
        let generated =
            generate_latest_incident_candidates(&database).expect("generate candidate corpus");
        seed_import(&database, "added-dataset", "v1", 1, 2, &[]);

        let error = database
            .reconcile_generated_incident_candidates(&generated)
            .expect_err("new dataset must stale the generated corpus");
        assert!(error.contains("Candidate corpus is stale"));
        assert_eq!(candidate_target_counts(&database), (0, 0, 0, 0));
    }

    #[test]
    fn reconciliation_rolls_back_each_creation_and_supersession_write_stage() {
        let failing_triggers = [
            (
                "fail_suggestion",
                "CREATE TRIGGER test_fail BEFORE INSERT ON research_suggestions BEGIN SELECT RAISE(ABORT, 'injected'); END;",
            ),
            (
                "fail_reference",
                "CREATE TRIGGER test_fail BEFORE INSERT ON suggestion_references BEGIN SELECT RAISE(ABORT, 'injected'); END;",
            ),
            (
                "fail_initial_event",
                "CREATE TRIGGER test_fail BEFORE INSERT ON suggestion_events WHEN NEW.status = 'proposed' BEGIN SELECT RAISE(ABORT, 'injected'); END;",
            ),
            (
                "fail_evidence",
                "CREATE TRIGGER test_fail BEFORE INSERT ON candidate_evidence_snapshots BEGIN SELECT RAISE(ABORT, 'injected'); END;",
            ),
        ];
        for (case, trigger_sql) in failing_triggers {
            let database = db();
            seed_evidence_fixture(&database);
            let generated =
                generate_latest_incident_candidates(&database).expect("generate candidate corpus");
            database
                .connection()
                .expect("database connection")
                .execute_batch(trigger_sql)
                .expect("install failure trigger");
            assert!(
                database
                    .reconcile_generated_incident_candidates(&generated)
                    .is_err(),
                "injected {case} failure must abort reconciliation"
            );
            assert_eq!(candidate_target_counts(&database), (0, 0, 0, 0));
        }

        let database = db();
        seed_evidence_fixture(&database);
        let generated =
            generate_latest_incident_candidates(&database).expect("generate candidate corpus");
        let original = database
            .reconcile_generated_incident_candidates(&generated)
            .expect("create initial proposal")
            .created_suggestion_ids[0]
            .clone();
        let before = candidate_target_counts(&database);
        database
            .connection()
            .expect("database connection")
            .execute_batch(
                "CREATE TRIGGER test_fail BEFORE INSERT ON suggestion_events
                 WHEN NEW.status = 'superseded'
                 BEGIN SELECT RAISE(ABORT, 'injected supersession failure'); END;",
            )
            .expect("install supersession failure trigger");
        assert!(
            database
                .reconcile_generated_incident_candidates(&changed_evidence(
                    generated,
                    "new-evidence"
                ))
                .is_err()
        );
        assert_eq!(candidate_target_counts(&database), before);
        assert_eq!(
            database
                .get_research_suggestion(&original)
                .expect("read original proposal")
                .expect("proposal exists")
                .status,
            crate::research_decision::SuggestionStatus::Proposed
        );
    }

    #[test]
    fn concurrent_reconciliation_across_connections_creates_one_active_proposal() {
        let path = std::env::temp_dir().join(format!(
            "xenoscope-reconcile-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let first_database = Arc::new(Database::open(&path).expect("open first database"));
        seed_evidence_fixture(&first_database);
        let generated = generate_latest_incident_candidates(&first_database)
            .expect("generate candidate corpus");
        let second_database = Arc::new(Database::open(&path).expect("open second database"));

        let first_db = Arc::clone(&first_database);
        let first_generated = generated.clone();
        let first = std::thread::spawn(move || {
            first_db.reconcile_generated_incident_candidates(&first_generated)
        });
        let second_db = Arc::clone(&second_database);
        let second = std::thread::spawn(move || {
            second_db.reconcile_generated_incident_candidates(&generated)
        });
        let first_result = first
            .join()
            .expect("first worker joins")
            .expect("first run");
        let second_result = second
            .join()
            .expect("second worker joins")
            .expect("second run");
        assert_eq!(
            first_result.created_suggestion_ids.len() + second_result.created_suggestion_ids.len(),
            1
        );
        assert_eq!(
            first_result.reused_suggestion_ids.len() + second_result.reused_suggestion_ids.len(),
            1
        );
        assert_eq!(candidate_target_counts(&first_database), (1, 1, 7, 1));

        drop(second_database);
        drop(first_database);
        std::fs::remove_file(&path).expect("remove temporary database");
    }

    #[test]
    fn reads_latest_import_per_dataset_and_excludes_removed_historical_keys() {
        let database = db();
        let first = vec![
            incident("old", "incident-old", &["source"]),
            incident("current", "incident-current", &["source"]),
            source("source", "source-a"),
        ];
        let current = vec![
            incident("current", "incident-current", &["source"]),
            source("source", "source-a"),
        ];
        seed_import(&database, "dataset-a", "v1", 1, 2, &first);
        seed_import(&database, "dataset-a", "v2", 2, 2, &current);
        seed_import(
            &database,
            "dataset-b",
            "v1",
            1,
            2,
            &[
                incident("b-incident", "incident-b", &["b-source"]),
                source("b-source", "source-b"),
            ],
        );

        let corpus = read_candidate_corpus(&database).expect("read candidate corpus");
        assert_eq!(corpus.selected_imports.len(), 2);
        assert_eq!(
            corpus
                .selected_imports
                .iter()
                .map(|import| (import.dataset_id.as_str(), import.import_sequence))
                .collect::<Vec<_>>(),
            [("dataset-a", 2), ("dataset-b", 1)]
        );
        assert_eq!(
            corpus
                .input
                .incidents
                .iter()
                .map(|incident| incident.identity.as_str())
                .collect::<Vec<_>>(),
            ["incident-b", "incident-current"]
        );
        assert_eq!(corpus.incident_provenance.len(), 2);
        assert!(
            corpus.incident_provenance.iter().all(
                |provenance| provenance.import_sequence == 1 || provenance.import_sequence == 2
            )
        );
    }

    #[test]
    fn preserves_legacy_unknown_fields_database_identity_and_dataset_provenance() {
        let database = db();
        seed_import(
            &database,
            "legacy-dataset",
            "legacy-v1",
            1,
            1,
            &[legacy_incident("legacy-case", "canonical-incident-id")],
        );
        let corpus = read_candidate_corpus(&database).expect("read legacy corpus");
        let incident = &corpus.input.incidents[0];
        assert_eq!(incident.identity, "canonical-incident-id");
        assert_eq!(incident.description.as_deref(), None);
        assert_eq!(incident.location, None);
        assert_eq!(incident.time_period, None);
        assert!(incident.source_ids.is_empty());
        assert_eq!(
            corpus.incident_provenance,
            [CandidateIncidentProvenance {
                database_id: "canonical-incident-id".to_string(),
                dataset_id: "legacy-dataset".to_string(),
                dataset_key: "legacy-case".to_string(),
                dataset_version: "legacy-v1".to_string(),
                import_sequence: 1,
                imported_at: "2026-01-01T00:00:00Z".to_string(),
            }]
        );
    }

    #[test]
    fn preserves_typed_date_and_location_precision_without_inference() {
        let database = db();
        let mut incident_record = incident("case", "incident-id", &["source"]);
        incident_record.snapshot["timePeriod"] = json!({
            "startDate": "2017-10-01",
            "endDate": "2017-10-31",
            "precision": "month",
            "originalWording": "October 2017",
        });
        incident_record.snapshot["location"] = json!({
            "description": "Al Taqaddum",
            "precision": "named_place",
            "originalWording": "near Al Taqaddum",
        });
        seed_import(
            &database,
            "precision",
            "v1",
            1,
            2,
            &[incident_record, source("source", "source-id")],
        );
        let corpus = read_candidate_corpus(&database).expect("read typed data");
        let record = &corpus.input.incidents[0];
        assert_eq!(
            record.time_period,
            Some(DatasetTimePeriod {
                start_date: "2017-10-01".to_string(),
                end_date: "2017-10-31".to_string(),
                precision: crate::research_dataset::DatasetTimePrecision::Month,
                original_wording: Some("October 2017".to_string()),
            })
        );
        assert_eq!(
            record.location,
            Some(DatasetLocation {
                description: "Al Taqaddum".to_string(),
                precision: crate::research_dataset::DatasetLocationPrecision::NamedPlace,
                original_wording: Some("near Al Taqaddum".to_string()),
            })
        );
    }

    #[test]
    fn attaches_only_directly_linked_claims_and_resolves_real_sources() {
        let database = db();
        seed_import(
            &database,
            "claims-dataset",
            "v1",
            1,
            2,
            &[
                incident("incident-a", "incident-id-a", &["source-a"]),
                incident("incident-b", "incident-id-b", &["source-b"]),
                source("source-a", "source-id-a"),
                source("source-b", "source-id-b"),
                claim("claim-a", "claim-id-a", "source-a", Some("incident-a")),
                claim("claim-b", "claim-id-b", "source-b", Some("incident-b")),
                claim("unlinked", "claim-id-unlinked", "source-a", None),
            ],
        );

        let corpus = read_candidate_corpus(&database).expect("read claims");
        let left = corpus
            .input
            .incidents
            .iter()
            .find(|incident| incident.identity == "incident-id-a")
            .unwrap();
        let right = corpus
            .input
            .incidents
            .iter()
            .find(|incident| incident.identity == "incident-id-b")
            .unwrap();
        assert_eq!(left.source_ids, ["source-id-a"]);
        assert_eq!(right.source_ids, ["source-id-b"]);
        assert_eq!(left.claims.len(), 1);
        assert_eq!(left.claims[0].text, "Claim claim-a");
        assert_eq!(left.claims[0].record_id, "claim-id-a");
        assert_eq!(left.claims[0].dataset_id, "claims-dataset");
        assert_eq!(left.claims[0].dataset_key, "claim-a");
        assert_eq!(left.claims[0].category, ClaimCategory::Observation);
        assert_eq!(left.claims[0].source_id, "source-id-a");
        assert_eq!(right.claims[0].text, "Claim claim-b");
        assert_eq!(
            corpus.claim_provenance,
            [
                CandidateClaimProvenance {
                    database_id: "claim-id-a".to_string(),
                    dataset_id: "claims-dataset".to_string(),
                    dataset_key: "claim-a".to_string(),
                    incident_id: "incident-id-a".to_string(),
                    source_id: "source-id-a".to_string(),
                },
                CandidateClaimProvenance {
                    database_id: "claim-id-b".to_string(),
                    dataset_id: "claims-dataset".to_string(),
                    dataset_key: "claim-b".to_string(),
                    incident_id: "incident-id-b".to_string(),
                    source_id: "source-id-b".to_string(),
                },
            ]
        );
        assert!(
            !corpus
                .claim_provenance
                .iter()
                .any(|claim| claim.database_id == "claim-id-unlinked")
        );
        assert_eq!(
            corpus.source_origins["source-id-a"],
            [CandidateSourceOrigin {
                dataset_id: "claims-dataset".to_string(),
                dataset_key: "source-a".to_string(),
            }]
        );
    }

    #[test]
    fn preserves_direct_dependency_ids_direction_and_kind_and_filters_other_edges() {
        let database = db();
        let records = vec![
            incident(
                "incident",
                "incident-id",
                &["source-a", "source-b", "source-c"],
            ),
            source("source-a", "source-id-a"),
            source("source-b", "source-id-b"),
            source("source-c", "source-id-c"),
            relationship(
                "rel-quotes",
                "relationship-quotes-id",
                "quotes",
                "source",
                "source-a",
                "source",
                "source-b",
            ),
            relationship(
                "rel-repeats",
                "relationship-repeats-id",
                "repeats",
                "source",
                "source-b",
                "source",
                "source-c",
            ),
            relationship(
                "rel-derived",
                "relationship-derived-id",
                "derived_from",
                "source",
                "source-c",
                "source",
                "source-a",
            ),
            relationship(
                "rel-supports",
                "relationship-supports-id",
                "supports",
                "source",
                "source-a",
                "source",
                "source-c",
            ),
            relationship(
                "rel-wrong-endpoints",
                "relationship-wrong-id",
                "quotes",
                "incident",
                "incident",
                "source",
                "source-a",
            ),
        ];
        seed_import(&database, "relations", "v1", 1, 2, &records);

        let corpus = read_candidate_corpus(&database).expect("read dependencies");
        assert_eq!(corpus.input.source_dependencies.len(), 3);
        assert_eq!(corpus.source_dependency_records.len(), 3);
        assert_eq!(
            corpus
                .source_dependency_records
                .iter()
                .map(|record| record.relationship_record_id.as_str())
                .collect::<Vec<_>>(),
            [
                "relationship-derived-id",
                "relationship-quotes-id",
                "relationship-repeats-id"
            ]
        );
        assert!(corpus.source_dependency_records.iter().any(|record| {
            record.relationship_record_id == "relationship-quotes-id"
                && record.edge.relationship_record_id == "relationship-quotes-id"
                && record.edge.from_source_id == "source-id-a"
                && record.edge.to_source_id == "source-id-b"
                && record.edge.kind == SourceDependencyKind::Quotes
        }));
        assert!(corpus.source_dependency_records.iter().any(|record| {
            record.relationship_record_id == "relationship-repeats-id"
                && record.edge.relationship_record_id == "relationship-repeats-id"
                && record.edge.from_source_id == "source-id-b"
                && record.edge.to_source_id == "source-id-c"
                && record.edge.kind == SourceDependencyKind::Repeats
        }));
        assert!(corpus.source_dependency_records.iter().any(|record| {
            record.relationship_record_id == "relationship-derived-id"
                && record.edge.relationship_record_id == "relationship-derived-id"
                && record.edge.from_source_id == "source-id-c"
                && record.edge.to_source_id == "source-id-a"
                && record.edge.kind == SourceDependencyKind::DerivedFrom
        }));
    }

    #[test]
    fn dependency_edges_can_connect_sources_in_different_selected_datasets() {
        let database = db();
        seed_import(
            &database,
            "dataset-a",
            "v1",
            1,
            2,
            &[
                incident(
                    "incident-a",
                    "incident-id-a",
                    &["source-a", "shared-source"],
                ),
                source("source-a", "source-id-a"),
                source("shared-source", "source-id-shared"),
                relationship(
                    "cross-dataset-link",
                    "cross-dataset-relationship",
                    "quotes",
                    "source",
                    "source-a",
                    "source",
                    "shared-source",
                ),
            ],
        );
        seed_import(
            &database,
            "dataset-b",
            "v1",
            1,
            2,
            &[
                incident("incident-b", "incident-id-b", &["source"]),
                source("source", "source-id-shared"),
            ],
        );

        let corpus = read_candidate_corpus(&database).expect("read cross-dataset dependency");
        assert_eq!(corpus.source_dependency_records.len(), 1);
        assert_eq!(
            corpus.source_dependency_records[0].relationship_record_id,
            "cross-dataset-relationship"
        );
        assert_eq!(
            corpus.source_dependency_records[0].edge.to_source_id,
            "source-id-shared"
        );
        assert_eq!(
            corpus.source_origins["source-id-shared"],
            [
                CandidateSourceOrigin {
                    dataset_id: "dataset-a".to_string(),
                    dataset_key: "shared-source".to_string(),
                },
                CandidateSourceOrigin {
                    dataset_id: "dataset-b".to_string(),
                    dataset_key: "source".to_string(),
                },
            ]
        );
    }

    #[test]
    fn generated_output_retains_suppressions_and_selected_import_metadata() {
        let database = db();
        let mut records = vec![source("shared-source", "source-id")];
        for index in 0..4 {
            records.push(incident(
                &format!("incident-{index}"),
                &format!("incident-id-{index}"),
                &["shared-source"],
            ));
        }
        records.push(claim(
            "claim-0",
            "claim-id-0",
            "shared-source",
            Some("incident-0"),
        ));
        records.push(claim(
            "claim-1",
            "claim-id-1",
            "shared-source",
            Some("incident-1"),
        ));
        records[5].snapshot["assertionText"] = json!("diamond pattern observed");
        records[6].snapshot["assertionText"] = json!("diamond pattern observed");
        seed_import(&database, "generated", "v1", 7, 2, &records);

        let generated =
            generate_latest_incident_candidates(&database).expect("generate latest candidates");
        assert_eq!(
            generated.selected_imports,
            [SelectedDatasetImport {
                dataset_id: "generated".to_string(),
                dataset_version: "v1".to_string(),
                import_sequence: 7,
                imported_at: "2026-01-07T00:00:00Z".to_string(),
            }]
        );
        assert!(
            generated
                .suppressed_buckets
                .iter()
                .any(|bucket| bucket.pass == crate::research_candidate::RetrievalPass::Text)
        );
        assert!(!generated.candidates.is_empty());
        assert!(generated.source_origins.contains_key("source-id"));
        assert_eq!(
            generated
                .claim_provenance
                .iter()
                .map(|claim| claim.database_id.as_str())
                .collect::<Vec<_>>(),
            ["claim-id-0", "claim-id-1"]
        );
        assert!(generated.candidates.iter().any(|candidate| {
            candidate.supporting_signals.iter().any(|signal| {
                matches!(
                    signal,
                    crate::research_candidate::SupportingSignal::SharedTextTerms {
                        claim_provenance,
                        ..
                    } if claim_provenance.iter().any(|term| {
                        term.term == "diamond"
                            && term.left_claims.iter().any(|claim| claim.claim_id == "claim-id-0")
                            && term.right_claims.iter().any(|claim| claim.claim_id == "claim-id-1")
                    })
                )
            })
        }));
    }

    #[test]
    fn candidate_evidence_is_stable_across_reordered_provenance() {
        let (mut corpus, candidate) = evidence_fixture();
        corpus
            .source_origins
            .get_mut("source-id-a")
            .unwrap()
            .push(CandidateSourceOrigin {
                dataset_id: "second-origin".to_string(),
                dataset_key: "source-a-alias".to_string(),
            });
        corpus.selected_imports.push(SelectedDatasetImport {
            dataset_id: "second-origin".to_string(),
            dataset_version: "v2".to_string(),
            import_sequence: 2,
            imported_at: "second timestamp".to_string(),
        });
        let first =
            CandidateEvidenceSnapshot::new(candidate.clone(), &corpus, CANDIDATE_GENERATOR_VERSION)
                .expect("first evidence snapshot");
        let mut reordered = corpus.clone();
        reordered.claim_provenance.reverse();
        reordered.incident_provenance.reverse();
        reordered.source_dependency_records.reverse();
        reordered.selected_imports.reverse();
        for origins in reordered.source_origins.values_mut() {
            origins.reverse();
        }
        let second =
            CandidateEvidenceSnapshot::new(candidate, &reordered, CANDIDATE_GENERATOR_VERSION)
                .expect("reordered evidence snapshot");
        assert_eq!(first, second);
        assert_eq!(
            serde_json::to_string(&first).expect("serialize first"),
            serde_json::to_string(&second).expect("serialize second")
        );
    }

    #[test]
    fn import_timestamps_are_serialized_but_excluded_from_fingerprint() {
        let (corpus, candidate) = evidence_fixture();
        let first =
            CandidateEvidenceSnapshot::new(candidate.clone(), &corpus, CANDIDATE_GENERATOR_VERSION)
                .expect("first evidence snapshot");
        let mut changed_timestamps = corpus;
        changed_timestamps.selected_imports[0].imported_at = "later timestamp".to_string();
        for provenance in &mut changed_timestamps.incident_provenance {
            provenance.imported_at = "later timestamp".to_string();
        }
        let second = CandidateEvidenceSnapshot::new(
            candidate,
            &changed_timestamps,
            CANDIDATE_GENERATOR_VERSION,
        )
        .expect("second evidence snapshot");
        assert_eq!(first.fingerprint, second.fingerprint);
        let serialized = serde_json::to_string(&second).expect("serialize evidence snapshot");
        assert!(serialized.contains("later timestamp"));
    }

    #[test]
    fn generator_and_format_versions_change_the_fingerprint() {
        let (corpus, candidate) = evidence_fixture();
        let snapshot =
            CandidateEvidenceSnapshot::new(candidate.clone(), &corpus, CANDIDATE_GENERATOR_VERSION)
                .expect("evidence snapshot");
        let changed_generator =
            CandidateEvidenceSnapshot::new(candidate.clone(), &corpus, "xenoscope-stage2d-next")
                .expect("changed generator snapshot");
        assert_ne!(snapshot.fingerprint, changed_generator.fingerprint);

        let changed_format_fingerprint = candidate_evidence_fingerprint(
            CANDIDATE_EVIDENCE_FORMAT_VERSION + 1,
            &snapshot.generator_version,
            &snapshot.candidate,
            &snapshot.incident_provenance,
            &snapshot.claim_provenance,
            &snapshot.relationship_provenance,
            &snapshot.source_origins,
            &snapshot.selected_imports,
            &snapshot.corpus_suppressions,
        )
        .expect("changed format fingerprint");
        assert_ne!(snapshot.fingerprint, changed_format_fingerprint);
        assert!(snapshot.fingerprint.starts_with("sha256:"));
    }

    #[test]
    fn material_candidate_and_relevant_provenance_changes_change_fingerprint() {
        let (corpus, candidate) = evidence_fixture();
        let original =
            CandidateEvidenceSnapshot::new(candidate.clone(), &corpus, CANDIDATE_GENERATOR_VERSION)
                .expect("original snapshot");

        let mut changed_candidate = candidate.clone();
        changed_candidate.supporting_signals.pop();
        let changed_signal =
            CandidateEvidenceSnapshot::new(changed_candidate, &corpus, CANDIDATE_GENERATOR_VERSION)
                .expect("changed signal snapshot");
        assert_ne!(original.fingerprint, changed_signal.fingerprint);

        let mut changed_provenance = corpus.clone();
        changed_provenance
            .source_origins
            .get_mut("source-id-a")
            .unwrap()
            .push(CandidateSourceOrigin {
                dataset_id: "second-origin".to_string(),
                dataset_key: "source-a-alias".to_string(),
            });
        changed_provenance
            .selected_imports
            .push(SelectedDatasetImport {
                dataset_id: "second-origin".to_string(),
                dataset_version: "v3".to_string(),
                import_sequence: 3,
                imported_at: "ignored timestamp".to_string(),
            });
        let changed_origin = CandidateEvidenceSnapshot::new(
            candidate,
            &changed_provenance,
            CANDIDATE_GENERATOR_VERSION,
        )
        .expect("changed source origin snapshot");
        assert_ne!(original.fingerprint, changed_origin.fingerprint);
    }

    #[test]
    fn unrelated_corpus_changes_do_not_change_candidate_fingerprint() {
        let (corpus, candidate) = evidence_fixture();
        let original =
            CandidateEvidenceSnapshot::new(candidate.clone(), &corpus, CANDIDATE_GENERATOR_VERSION)
                .expect("original snapshot");
        let mut changed_corpus = corpus;
        changed_corpus
            .claim_provenance
            .push(CandidateClaimProvenance {
                database_id: "unrelated-claim".to_string(),
                dataset_id: "unrelated-dataset".to_string(),
                dataset_key: "unrelated-claim-key".to_string(),
                incident_id: "unrelated-incident".to_string(),
                source_id: "unrelated-source".to_string(),
            });
        changed_corpus
            .source_origins
            .insert("unrelated-source".to_string(), vec![]);
        changed_corpus.selected_imports.push(SelectedDatasetImport {
            dataset_id: "unrelated-dataset".to_string(),
            dataset_version: "v99".to_string(),
            import_sequence: 99,
            imported_at: "unrelated timestamp".to_string(),
        });
        changed_corpus
            .suppressed_buckets
            .push(SuppressedIndexBucket {
                pass: crate::research_candidate::RetrievalPass::Text,
                key: "unrelated".to_string(),
                reason: crate::research_candidate::SuppressionReason::CommonTerm {
                    document_frequency: 100,
                    max_document_frequency: 10,
                },
                affected_incident_ids: vec!["unrelated-incident".to_string()],
            });
        let changed =
            CandidateEvidenceSnapshot::new(candidate, &changed_corpus, CANDIDATE_GENERATOR_VERSION)
                .expect("snapshot with unrelated corpus changes");
        assert_eq!(original.fingerprint, changed.fingerprint);
    }

    #[test]
    fn snapshot_limits_provenance_and_suppressions_to_relevant_records() {
        let (mut corpus, candidate) = evidence_fixture();
        let without_suppression =
            CandidateEvidenceSnapshot::new(candidate.clone(), &corpus, CANDIDATE_GENERATOR_VERSION)
                .expect("snapshot without suppression context");
        corpus.claim_provenance.push(CandidateClaimProvenance {
            database_id: "other-claim".to_string(),
            dataset_id: "other-dataset".to_string(),
            dataset_key: "other-claim-key".to_string(),
            incident_id: "other-incident".to_string(),
            source_id: "other-source".to_string(),
        });
        corpus.source_origins.insert(
            "source-id-a".to_string(),
            vec![
                CandidateSourceOrigin {
                    dataset_id: "evidence-dataset".to_string(),
                    dataset_key: "source-a".to_string(),
                },
                CandidateSourceOrigin {
                    dataset_id: "second-source-dataset".to_string(),
                    dataset_key: "alias-a".to_string(),
                },
            ],
        );
        corpus.source_origins.insert(
            "other-source".to_string(),
            vec![CandidateSourceOrigin {
                dataset_id: "other-dataset".to_string(),
                dataset_key: "other-source".to_string(),
            }],
        );
        corpus
            .source_dependency_records
            .push(CandidateSourceDependencyRecord {
                relationship_record_id: "unrelated-relationship".to_string(),
                dataset_id: "other-dataset".to_string(),
                dataset_key: "unrelated-link".to_string(),
                edge: SourceDependencyInput {
                    relationship_record_id: "unrelated-relationship".to_string(),
                    from_source_id: "other-source".to_string(),
                    to_source_id: "another-source".to_string(),
                    kind: SourceDependencyKind::Repeats,
                },
            });
        corpus.suppressed_buckets.extend([
            SuppressedIndexBucket {
                pass: crate::research_candidate::RetrievalPass::TimeOverlap,
                key: "incident-id-a".to_string(),
                reason: crate::research_candidate::SuppressionReason::LongPeriod {
                    month_span: 121,
                    max_month_span: 120,
                },
                affected_incident_ids: vec!["incident-id-a".to_string()],
            },
            SuppressedIndexBucket {
                pass: crate::research_candidate::RetrievalPass::TimeOverlap,
                key: "other-incident".to_string(),
                reason: crate::research_candidate::SuppressionReason::LongPeriod {
                    month_span: 200,
                    max_month_span: 120,
                },
                affected_incident_ids: vec!["other-incident".to_string()],
            },
            SuppressedIndexBucket {
                pass: crate::research_candidate::RetrievalPass::Text,
                key: "hidden-common-term".to_string(),
                reason: crate::research_candidate::SuppressionReason::CommonTerm {
                    document_frequency: 4,
                    max_document_frequency: 3,
                },
                affected_incident_ids: vec![
                    "incident-id-b".to_string(),
                    "incident-id-a".to_string(),
                    "incident-id-b".to_string(),
                ],
            },
            SuppressedIndexBucket {
                pass: crate::research_candidate::RetrievalPass::Text,
                key: "one-sided-term".to_string(),
                reason: crate::research_candidate::SuppressionReason::CommonTerm {
                    document_frequency: 4,
                    max_document_frequency: 3,
                },
                affected_incident_ids: vec!["incident-id-a".to_string()],
            },
        ]);
        let expected_candidate = candidate.clone();
        let snapshot =
            CandidateEvidenceSnapshot::new(candidate, &corpus, CANDIDATE_GENERATOR_VERSION)
                .expect("candidate snapshot");
        assert_eq!(snapshot.claim_provenance.len(), 2);
        assert!(
            snapshot
                .claim_provenance
                .iter()
                .all(|claim| claim.incident_id == "incident-id-a"
                    || claim.incident_id == "incident-id-b")
        );
        assert_eq!(snapshot.relationship_provenance.len(), 1);
        assert_eq!(
            snapshot.relationship_provenance[0].relationship_record_id,
            "relationship-id"
        );
        assert_eq!(
            snapshot.source_origins["source-id-a"].len(),
            2,
            "origins remain grouped under their canonical source ID"
        );
        assert_eq!(snapshot.corpus_suppressions.len(), 2);
        assert!(
            snapshot
                .corpus_suppressions
                .iter()
                .any(|suppression| suppression.key == "incident-id-a")
        );
        let attributed_text_suppression = snapshot
            .corpus_suppressions
            .iter()
            .find(|suppression| suppression.key == "hidden-common-term")
            .expect("shared text suppression applies to both candidate endpoints");
        assert_eq!(
            attributed_text_suppression.affected_incident_ids,
            vec!["incident-id-a".to_string(), "incident-id-b".to_string()]
        );
        assert!(
            snapshot
                .corpus_suppressions
                .iter()
                .all(|suppression| suppression.key != "one-sided-term"
                    && suppression.key != "other-incident")
        );
        assert_ne!(without_suppression.fingerprint, snapshot.fingerprint);
        assert_eq!(snapshot.candidate, expected_candidate);
        assert!(
            snapshot
                .selected_imports
                .iter()
                .all(|import| import.dataset_id != "other-dataset")
        );
    }

    #[test]
    fn snapshot_requires_canonical_incident_pair_order() {
        let (corpus, mut candidate) = evidence_fixture();
        std::mem::swap(
            &mut candidate.left_incident_id,
            &mut candidate.right_incident_id,
        );
        assert!(
            CandidateEvidenceSnapshot::new(candidate, &corpus, CANDIDATE_GENERATOR_VERSION)
                .expect_err("reversed pair must be rejected")
                .contains("canonically ordered")
        );
    }

    #[test]
    fn selected_relationship_kind_mismatch_errors_before_filtering() {
        let database = db();
        seed_import(
            &database,
            "relationship-kind-mismatch",
            "v1",
            1,
            2,
            &[
                incident("case", "incident-id", &["source-a", "source-b"]),
                source("source-a", "source-id-a"),
                source("source-b", "source-id-b"),
                relationship(
                    "relation",
                    "relationship-id",
                    "quotes",
                    "source",
                    "source-a",
                    "source",
                    "source-b",
                ),
            ],
        );
        database
            .connection()
            .unwrap()
            .execute(
                "UPDATE relationships SET relationship_kind = 'supports'
                 WHERE record_id = 'relationship-id'",
                [],
            )
            .expect("mutate persisted relation kind");

        let error = read_candidate_corpus(&database)
            .expect_err("kind mismatch must not be silently filtered");
        assert!(error.contains("relationship-kind-mismatch"));
        assert!(error.contains("relation"));
        assert!(error.contains("kind differs"));
    }

    #[test]
    fn malformed_latest_snapshot_errors_without_falling_back() {
        let database = db();
        seed_import(
            &database,
            "corrupt-latest",
            "v1",
            1,
            2,
            &[
                incident("old-case", "old-incident-id", &["source"]),
                source("source", "source-id"),
            ],
        );
        seed_import(
            &database,
            "corrupt-latest",
            "v2",
            2,
            2,
            &[
                incident("old-case", "old-incident-id", &["source"]),
                incident("latest-case", "latest-incident-id", &["source"]),
                source("source", "source-id"),
            ],
        );
        database
            .connection()
            .unwrap()
            .execute(
                "UPDATE dataset_record_mappings
                 SET content_snapshot_json = '{\"label\":42}'
                 WHERE dataset_id = 'corrupt-latest' AND record_key = 'latest-case'",
                [],
            )
            .expect("corrupt latest mapping");

        let error = read_candidate_corpus(&database).expect_err("corrupt latest must fail");
        assert!(error.contains("corrupt-latest"));
        assert!(error.contains("record \"latest-case\""));
        assert!(error.contains("malformed"));
    }

    #[test]
    fn broken_source_reference_errors_instead_of_being_dropped() {
        let database = db();
        seed_import(
            &database,
            "broken-reference",
            "v1",
            1,
            2,
            &[
                incident("case", "incident-id", &["missing-source"]),
                source("real-source", "source-id"),
            ],
        );
        let error = read_candidate_corpus(&database).expect_err("broken reference must fail");
        assert!(error.contains("broken-reference"));
        assert!(error.contains("missing-source"));
    }

    #[test]
    fn broken_claim_source_reference_errors_instead_of_being_dropped() {
        let database = db();
        seed_import(
            &database,
            "broken-claim-source",
            "v1",
            1,
            2,
            &[
                incident("case", "incident-id", &["source"]),
                source("source", "source-id"),
                claim("claim", "claim-id", "source", Some("case")),
            ],
        );
        database
            .connection()
            .unwrap()
            .execute(
                "UPDATE dataset_record_mappings
                 SET content_snapshot_json = ?1
                 WHERE dataset_id = 'broken-claim-source' AND record_key = 'claim'",
                [json!({
                    "assertionText": "Claim claim",
                    "category": "observation",
                    "sourceKey": "missing-source",
                    "sourceLocator": { "kind": "page", "page": 1 },
                    "incidentKey": "case",
                    "authorStatedConfidence": null,
                    "review": { "status": "unreviewed" }
                })
                .to_string()],
            )
            .expect("corrupt claim source reference");
        let error = read_candidate_corpus(&database).expect_err("broken claim source must fail");
        assert!(error.contains("broken-claim-source"));
        assert!(error.contains("missing-source"));
    }

    #[test]
    fn malformed_manifest_errors_with_dataset_context() {
        let database = db();
        seed_import(
            &database,
            "bad-manifest",
            "v1",
            1,
            1,
            &[legacy_incident("case", "incident-id")],
        );
        database
            .connection()
            .unwrap()
            .execute(
                "UPDATE dataset_imports
                 SET record_keys_json = '[\"missing-record\"]'
                 WHERE dataset_id = 'bad-manifest'",
                [],
            )
            .expect("corrupt manifest");
        let error = read_candidate_corpus(&database).expect_err("bad manifest must fail");
        assert!(error.contains("bad-manifest"));
        assert!(error.contains("manifest"));
    }

    #[test]
    fn repeated_reads_are_deterministic_and_bulk_assemble_many_incidents() {
        let database = db();
        let mut records = vec![source("source", "shared-source")];
        for index in 0..300 {
            records.push(incident(
                &format!("incident-{index:03}"),
                &format!("database-incident-{index:03}"),
                &["source"],
            ));
        }
        seed_import(&database, "large", "v1", 1, 2, &records);

        let first = read_candidate_corpus(&database).expect("first bulk read");
        let second = read_candidate_corpus(&database).expect("second bulk read");
        assert_eq!(first, second);
        assert_eq!(first.input.incidents.len(), 300);
        assert!(
            first
                .input
                .incidents
                .windows(2)
                .all(|pair| { pair[0].identity.as_str() < pair[1].identity.as_str() })
        );
    }

    #[test]
    fn empty_manifest_is_a_valid_latest_snapshot_with_no_incidents() {
        let database = db();
        seed_import(&database, "empty-dataset", "v1", 1, 1, &[]);
        let corpus = read_candidate_corpus(&database).expect("empty import is valid");
        assert!(corpus.input.incidents.is_empty());
        assert_eq!(corpus.selected_imports.len(), 1);
    }
}
