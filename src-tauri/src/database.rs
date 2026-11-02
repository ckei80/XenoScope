use crate::research_candidate::SupportingSignal;
use crate::research_candidate_read::{
    CANDIDATE_GENERATOR_VERSION, CandidateEvidenceSnapshot, GeneratedCandidateCorpus,
    generate_latest_incident_candidates,
};
use crate::research_decision::{
    CreateDecisionInput, CreateSuggestionInput, DecisionConclusionInput, DecisionEventDto,
    DecisionEventKind, DecisionKind, DecisionOutcome, DecisionRecordKind, DecisionTargetPair,
    LineageType, RecordTarget, ResearchDecisionDto, ResearchReferenceDto, ResearchReferenceInput,
    ResolveSuggestionInput, SuggestionDto, SuggestionEventDto, SuggestionOrigin,
    SuggestionOriginKind, SuggestionStatus, researcher_decision_actor,
    validate_decision_event_shape,
};
use crate::research_records::{
    ActorKind, AuthorStatedConfidence, Claim, ClaimReviewStatus, CreateClaimInput,
    CreateSourceInput, RecordActor, ReviewMetadata, Source, SourceLocator,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::Path,
    sync::{Mutex, MutexGuard},
};
use uuid::Uuid;

const LATEST_SCHEMA_VERSION: i64 = 6;
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../migrations/0001_initial.sql")),
    (2, include_str!("../migrations/0002_dataset_identity.sql")),
    (
        3,
        include_str!("../migrations/0003_dataset_import_sequence.sql"),
    ),
    (
        4,
        include_str!("../migrations/0004_research_assessments.sql"),
    ),
    (5, include_str!("../migrations/0005_research_decisions.sql")),
    (6, include_str!("../migrations/0006_candidate_evidence.sql")),
];

pub struct Database {
    connection: Mutex<Connection>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CandidateReconciliationResult {
    pub created_suggestion_ids: Vec<String>,
    pub reused_suggestion_ids: Vec<String>,
    pub skipped_candidates: usize,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Could not create database directory {}: {error}",
                    parent.display()
                )
            })?;
        }

        let mut connection = Connection::open(path)
            .map_err(|error| format!("Could not open database {}: {error}", path.display()))?;
        initialize(&mut connection)?;

        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn connection(&self) -> Result<MutexGuard<'_, Connection>, String> {
        self.connection
            .lock()
            .map_err(|_| "Database connection lock was poisoned".to_string())
    }

    #[cfg(test)]
    pub(crate) fn in_memory() -> Result<Self, String> {
        let mut connection = Connection::open_in_memory()
            .map_err(|error| format!("Could not open test database: {error}"))?;
        initialize(&mut connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn create_source(&self, input: CreateSourceInput) -> Result<Source, String> {
        input.validate()?;

        let id = Uuid::new_v4().to_string();
        let created_by = researcher_actor();
        let verification_status = input.verification_status.clone();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| format!("Could not begin source creation: {error}"))?;
        let created_at = current_timestamp(&transaction)?;

        insert_record(&transaction, &id, "source", &created_at, &created_by)?;
        transaction
            .execute(
                "INSERT INTO sources (
                    record_id, record_kind, source_kind, title, publisher, author,
                    publication_date, url, other_locator, verification_status
                 ) VALUES (?1, 'source', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    id,
                    enum_value(&input.source_kind)?,
                    input.title,
                    input.publisher,
                    input.author,
                    input.publication_date,
                    input.url,
                    input.other_locator,
                    enum_value(&verification_status)?,
                ],
            )
            .map_err(|error| format!("Could not save source record: {error}"))?;
        insert_creation_history(&transaction, &id, "source", "Source record created")?;
        transaction
            .commit()
            .map_err(|error| format!("Could not commit source record: {error}"))?;

        Ok(Source {
            id,
            created_at,
            created_by,
            source_kind: input.source_kind,
            title: input.title,
            publisher: input.publisher,
            author: input.author,
            publication_date: input.publication_date,
            url: input.url,
            other_locator: input.other_locator,
            verification_status,
        })
    }

    pub fn get_source(&self, id: &str) -> Result<Option<Source>, String> {
        let id = canonical_uuid("Source ID", id)?;
        let connection = self.connection()?;
        let raw_source = connection
            .query_row(
                "SELECT
                    r.record_id, r.created_at, r.created_by_kind, r.created_by_name,
                    s.source_kind, s.title, s.publisher, s.author, s.publication_date,
                    s.url, s.other_locator, s.verification_status
                 FROM records r
                 JOIN sources s ON s.record_id = r.record_id
                 WHERE r.record_id = ?1 AND r.record_kind = 'source'",
                [&id],
                |row| {
                    Ok(RawSource {
                        id: row.get(0)?,
                        created_at: row.get(1)?,
                        created_by_kind: row.get(2)?,
                        created_by_name: row.get(3)?,
                        source_kind: row.get(4)?,
                        title: row.get(5)?,
                        publisher: row.get(6)?,
                        author: row.get(7)?,
                        publication_date: row.get(8)?,
                        url: row.get(9)?,
                        other_locator: row.get(10)?,
                        verification_status: row.get(11)?,
                    })
                },
            )
            .optional()
            .map_err(|error| format!("Could not retrieve source {id}: {error}"))?;

        raw_source.map(RawSource::into_source).transpose()
    }

    pub fn create_claim(&self, mut input: CreateClaimInput) -> Result<Claim, String> {
        input.validate()?;
        input.source_id = canonical_uuid("Source ID", &input.source_id)?;
        if let Some(incident_id) = &mut input.incident_id {
            *incident_id = canonical_uuid("Incident ID", incident_id)?;
        }
        if let Some(confidence) = &mut input.author_stated_confidence {
            confidence.source_id = canonical_uuid("Confidence source ID", &confidence.source_id)?;
        }

        let id = Uuid::new_v4().to_string();
        let created_by = researcher_actor();
        let review = ReviewMetadata {
            status: ClaimReviewStatus::Unreviewed,
            reviewed_at: None,
            reviewed_by: None,
        };
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| format!("Could not begin claim creation: {error}"))?;

        require_existing_source(&transaction, &input.source_id)?;
        if let Some(incident_id) = &input.incident_id {
            require_existing_incident(&transaction, incident_id)?;
        }
        if let Some(confidence) = &input.author_stated_confidence {
            require_existing_source(&transaction, &confidence.source_id)?;
        }

        let created_at = current_timestamp(&transaction)?;
        insert_record(&transaction, &id, "claim", &created_at, &created_by)?;
        transaction
            .execute(
                "INSERT INTO claims (
                    record_id, record_kind, assertion_text, category, source_id,
                    source_locator_kind, source_locator_json, incident_id, review_status
                 ) VALUES (?1, 'claim', ?2, ?3, ?4, ?5, ?6, ?7, 'unreviewed')",
                params![
                    id,
                    input.assertion_text,
                    enum_value(&input.category)?,
                    input.source_id,
                    input.source_locator.kind(),
                    serde_json::to_string(&input.source_locator)
                        .map_err(|error| format!("Could not encode claim locator: {error}"))?,
                    input.incident_id,
                ],
            )
            .map_err(|error| format!("Could not save claim record: {error}"))?;

        if let Some(confidence) = &input.author_stated_confidence {
            insert_author_confidence(&transaction, &id, confidence)?;
        }

        insert_creation_history(&transaction, &id, "claim", "Claim record created")?;
        transaction
            .commit()
            .map_err(|error| format!("Could not commit claim record: {error}"))?;

        Ok(Claim {
            id,
            created_at,
            created_by,
            assertion_text: input.assertion_text,
            category: input.category,
            source_id: input.source_id,
            source_locator: input.source_locator,
            incident_id: input.incident_id,
            author_stated_confidence: input.author_stated_confidence,
            review,
        })
    }

    pub fn get_claim(&self, id: &str) -> Result<Option<Claim>, String> {
        let id = canonical_uuid("Claim ID", id)?;
        let connection = self.connection()?;
        let raw_claim = connection
            .query_row(
                "SELECT
                    r.record_id, r.created_at, r.created_by_kind, r.created_by_name,
                    c.assertion_text, c.category, c.source_id, c.source_locator_kind,
                    c.source_locator_json, c.incident_id, c.review_status, c.reviewed_at,
                    c.reviewed_by_kind, c.reviewed_by_name, ac.stated_by,
                    ac.value_as_stated, ac.source_id, ac.locator_kind, ac.locator_json
                 FROM records r
                 JOIN claims c ON c.record_id = r.record_id
                 LEFT JOIN claim_author_confidence ac ON ac.claim_id = c.record_id
                 WHERE r.record_id = ?1 AND r.record_kind = 'claim'",
                [&id],
                |row| {
                    Ok(RawClaim {
                        id: row.get(0)?,
                        created_at: row.get(1)?,
                        created_by_kind: row.get(2)?,
                        created_by_name: row.get(3)?,
                        assertion_text: row.get(4)?,
                        category: row.get(5)?,
                        source_id: row.get(6)?,
                        source_locator_kind: row.get(7)?,
                        source_locator_json: row.get(8)?,
                        incident_id: row.get(9)?,
                        review_status: row.get(10)?,
                        reviewed_at: row.get(11)?,
                        reviewed_by_kind: row.get(12)?,
                        reviewed_by_name: row.get(13)?,
                        confidence_stated_by: row.get(14)?,
                        confidence_value: row.get(15)?,
                        confidence_source_id: row.get(16)?,
                        confidence_locator_kind: row.get(17)?,
                        confidence_locator_json: row.get(18)?,
                    })
                },
            )
            .optional()
            .map_err(|error| format!("Could not retrieve claim {id}: {error}"))?;

        raw_claim.map(RawClaim::into_claim).transpose()
    }

    #[allow(dead_code)]
    pub(crate) fn create_research_suggestion(
        &self,
        mut input: CreateSuggestionInput,
    ) -> Result<SuggestionDto, String> {
        input.target = DecisionTargetPair::new(
            input.target.decision_kind,
            input.target.record_a.clone(),
            input.target.record_b.clone(),
            input.target.lineage_type,
        )
        .map_err(|error| format!("Invalid research suggestion target: {error}"))?;
        input
            .validate()
            .map_err(|error| format!("Invalid research suggestion: {error}"))?;

        let suggestion_id = Uuid::new_v4().to_string();
        let initial_event_id = Uuid::new_v4().to_string();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| format!("Could not begin research suggestion creation: {error}"))?;
        let suggestion =
            insert_research_suggestion(&transaction, &input, &suggestion_id, &initial_event_id)?;
        transaction
            .commit()
            .map_err(|error| format!("Could not commit research suggestion: {error}"))?;
        Ok(suggestion)
    }

    #[allow(dead_code)]
    pub(crate) fn dismiss_research_suggestion(
        &self,
        suggestion_id: &str,
        expected_event_id: &str,
        rationale: &str,
    ) -> Result<SuggestionDto, String> {
        validate_suggestion_transition_input(expected_event_id, rationale)?;
        let actor = researcher_decision_actor();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| format!("Could not begin suggestion dismissal: {error}"))?;
        let current = read_research_suggestion(&transaction, suggestion_id)?
            .ok_or_else(|| format!("Research suggestion {suggestion_id} does not exist"))?;
        validate_suggestion_head(&current, expected_event_id)?;

        let event_id = Uuid::new_v4().to_string();
        let occurred_at = current_timestamp(&transaction)?;
        transaction
            .execute(
                "INSERT INTO suggestion_events (
                    event_id, suggestion_id, status, predecessor_event_id,
                    occurred_at, actor_kind, actor_name, rationale
                 ) VALUES (?1, ?2, 'dismissed', ?3, ?4, ?5, ?6, ?7)",
                params![
                    event_id,
                    suggestion_id,
                    expected_event_id,
                    occurred_at,
                    enum_value(&actor.kind)?,
                    actor.display_name,
                    rationale,
                ],
            )
            .map_err(|error| {
                format!(
                    "Could not dismiss research suggestion: {}",
                    suggestion_storage_error(error)
                )
            })?;

        let suggestion = read_research_suggestion(&transaction, suggestion_id)?
            .ok_or_else(|| format!("Research suggestion {suggestion_id} disappeared"))?;
        transaction
            .commit()
            .map_err(|error| format!("Could not commit suggestion dismissal: {error}"))?;
        Ok(suggestion)
    }

    #[allow(dead_code)]
    pub(crate) fn supersede_research_suggestion(
        &self,
        suggestion_id: &str,
        expected_event_id: &str,
        replacement_suggestion_id: &str,
        rationale: &str,
    ) -> Result<SuggestionDto, String> {
        validate_suggestion_transition_input(expected_event_id, rationale)?;
        require_nonblank("Replacement suggestion ID", replacement_suggestion_id)?;
        if suggestion_id == replacement_suggestion_id {
            return Err("A suggestion cannot replace itself".to_string());
        }
        let actor = researcher_decision_actor();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| format!("Could not begin suggestion supersession: {error}"))?;
        let suggestion = supersede_research_suggestion_in_transaction(
            &transaction,
            suggestion_id,
            expected_event_id,
            replacement_suggestion_id,
            rationale,
            &actor,
        )?;
        transaction
            .commit()
            .map_err(|error| format!("Could not commit suggestion supersession: {error}"))?;
        Ok(suggestion)
    }

    #[allow(dead_code)]
    pub(crate) fn get_research_suggestion(
        &self,
        suggestion_id: &str,
    ) -> Result<Option<SuggestionDto>, String> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| format!("Could not begin research suggestion read: {error}"))?;
        let suggestion = read_research_suggestion(&transaction, suggestion_id)?;
        transaction
            .commit()
            .map_err(|error| format!("Could not finish research suggestion read: {error}"))?;
        Ok(suggestion)
    }

    #[allow(dead_code)]
    pub(crate) fn insert_candidate_evidence_snapshot(
        &self,
        suggestion_id: &str,
        snapshot: &CandidateEvidenceSnapshot,
    ) -> Result<(), String> {
        snapshot
            .validate_integrity()
            .map_err(|error| format!("Invalid candidate evidence snapshot: {error}"))?;
        let snapshot_json = serde_json::to_string(snapshot)
            .map_err(|error| format!("Could not serialize candidate evidence snapshot: {error}"))?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| {
                format!("Could not begin candidate evidence snapshot insert: {error}")
            })?;
        insert_candidate_evidence_snapshot(&transaction, suggestion_id, snapshot, &snapshot_json)?;
        transaction.commit().map_err(|error| {
            format!("Could not commit candidate evidence snapshot insert: {error}")
        })
    }

    #[allow(dead_code)]
    pub(crate) fn get_candidate_evidence_snapshot(
        &self,
        suggestion_id: &str,
    ) -> Result<Option<CandidateEvidenceSnapshot>, String> {
        let connection = self.connection()?;
        read_candidate_evidence_snapshot(&connection, suggestion_id)
    }

    #[allow(dead_code)]
    pub(crate) fn reconcile_latest_incident_candidates(
        &self,
    ) -> Result<CandidateReconciliationResult, String> {
        let generated = generate_latest_incident_candidates(self)?;
        self.reconcile_generated_incident_candidates(&generated)
    }

    #[allow(dead_code)]
    pub(crate) fn reconcile_generated_incident_candidates(
        &self,
        generated: &GeneratedCandidateCorpus,
    ) -> Result<CandidateReconciliationResult, String> {
        let prepared = generated
            .candidates
            .iter()
            .cloned()
            .map(|candidate| {
                let snapshot = CandidateEvidenceSnapshot::new(
                    candidate,
                    generated,
                    CANDIDATE_GENERATOR_VERSION,
                )?;
                let references = candidate_evidence_references(&snapshot)?;
                let target = DecisionTargetPair::new(
                    DecisionKind::IncidentPair,
                    RecordTarget {
                        record_id: snapshot.candidate.left_incident_id.clone(),
                        record_kind: DecisionRecordKind::Incident,
                    },
                    RecordTarget {
                        record_id: snapshot.candidate.right_incident_id.clone(),
                        record_kind: DecisionRecordKind::Incident,
                    },
                    None,
                )
                .map_err(|error| format!("Invalid generated candidate target: {error}"))?;
                Ok(PreparedCandidateSuggestion {
                    snapshot,
                    references,
                    target,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| format!("Could not begin candidate reconciliation: {error}"))?;
        validate_candidate_corpus_current(&transaction, &generated.selected_imports)?;

        let mut result = CandidateReconciliationResult::default();
        for candidate in &prepared {
            match reconcile_one_candidate(&transaction, candidate)? {
                CandidateReconciliationAction::Created(suggestion_id) => {
                    result.created_suggestion_ids.push(suggestion_id);
                }
                CandidateReconciliationAction::Reused(suggestion_id) => {
                    result.reused_suggestion_ids.push(suggestion_id);
                }
                CandidateReconciliationAction::Skipped => result.skipped_candidates += 1,
            }
        }
        transaction
            .commit()
            .map_err(|error| format!("Could not commit candidate reconciliation: {error}"))?;
        Ok(result)
    }

    #[allow(dead_code)]
    pub(crate) fn create_research_decision(
        &self,
        mut input: CreateDecisionInput,
    ) -> Result<ResearchDecisionDto, String> {
        if input.origin_suggestion_id.is_some() {
            return Err(
                "Direct decision creation cannot specify an originating suggestion".to_string(),
            );
        }
        input.target = normalize_decision_target(input.target)?;
        input.validate().map_err(|error| error.to_string())?;
        let actor = researcher_decision_actor();
        let decision_id = Uuid::new_v4().to_string();
        let event_id = Uuid::new_v4().to_string();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| format!("Could not begin decision creation: {error}"))?;
        let created_at = current_timestamp(&transaction)?;
        insert_decision_identity(
            &transaction,
            &decision_id,
            &event_id,
            &input.target,
            None,
            &created_at,
        )?;
        insert_decision_event(
            &transaction,
            &decision_id,
            &event_id,
            DecisionEventKind::Recorded,
            Some(input.outcome),
            None,
            &created_at,
            &actor,
            &input.rationale,
            &input.references,
        )?;
        let decision = read_research_decision(&transaction, &decision_id)?
            .ok_or_else(|| "Created research decision could not be read back".to_string())?;
        transaction
            .commit()
            .map_err(|error| format!("Could not commit decision creation: {error}"))?;
        Ok(decision)
    }

    #[allow(dead_code)]
    pub(crate) fn resolve_research_suggestion(
        &self,
        suggestion_id: &str,
        expected_suggestion_event_id: &str,
        input: ResolveSuggestionInput,
    ) -> Result<ResearchDecisionDto, String> {
        require_nonblank("Expected suggestion event ID", expected_suggestion_event_id)?;
        require_nonblank("Suggestion ID", suggestion_id)?;
        let actor = researcher_decision_actor();
        let decision_id = Uuid::new_v4().to_string();
        let decision_event_id = Uuid::new_v4().to_string();
        let suggestion_event_id = Uuid::new_v4().to_string();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| format!("Could not begin suggestion resolution: {error}"))?;
        let suggestion = read_research_suggestion(&transaction, suggestion_id)?
            .ok_or_else(|| format!("Research suggestion {suggestion_id} does not exist"))?;
        validate_suggestion_head(&suggestion, expected_suggestion_event_id)?;
        input
            .validate(&suggestion.target)
            .map_err(|error| format!("Invalid suggestion resolution: {error}"))?;
        let created_at = current_timestamp(&transaction)?;
        insert_decision_identity(
            &transaction,
            &decision_id,
            &decision_event_id,
            &suggestion.target,
            Some((suggestion_id, &suggestion_event_id)),
            &created_at,
        )?;
        insert_decision_event(
            &transaction,
            &decision_id,
            &decision_event_id,
            DecisionEventKind::Recorded,
            Some(input.outcome),
            None,
            &created_at,
            &actor,
            &input.rationale,
            &input.references,
        )?;
        transaction
            .execute(
                "INSERT INTO suggestion_events (
                    event_id, suggestion_id, status, predecessor_event_id,
                    occurred_at, actor_kind, actor_name, decision_id
                 ) VALUES (?1, ?2, 'resolved', ?3, ?4, ?5, ?6, ?7)",
                params![
                    suggestion_event_id,
                    suggestion_id,
                    expected_suggestion_event_id,
                    created_at,
                    enum_value(&actor.kind)?,
                    actor.display_name,
                    decision_id,
                ],
            )
            .map_err(|error| {
                format!(
                    "Could not resolve research suggestion: {}",
                    decision_storage_error(error)
                )
            })?;
        let decision = read_research_decision(&transaction, &decision_id)?
            .ok_or_else(|| "Resolved research decision could not be read back".to_string())?;
        transaction
            .commit()
            .map_err(|error| format!("Could not commit suggestion resolution: {error}"))?;
        Ok(decision)
    }

    #[allow(dead_code)]
    pub(crate) fn supersede_research_decision(
        &self,
        decision_id: &str,
        expected_event_id: &str,
        input: DecisionConclusionInput,
    ) -> Result<ResearchDecisionDto, String> {
        self.append_decision_event(
            decision_id,
            expected_event_id,
            DecisionEventKind::Superseded,
            Some(input),
            None,
        )
    }

    #[allow(dead_code)]
    pub(crate) fn reverse_research_decision(
        &self,
        decision_id: &str,
        expected_event_id: &str,
        rationale: &str,
    ) -> Result<ResearchDecisionDto, String> {
        self.append_decision_event(
            decision_id,
            expected_event_id,
            DecisionEventKind::Reversed,
            None,
            Some(rationale),
        )
    }

    #[allow(dead_code)]
    pub(crate) fn record_research_decision_after_reversal(
        &self,
        decision_id: &str,
        expected_reversal_event_id: &str,
        input: DecisionConclusionInput,
    ) -> Result<ResearchDecisionDto, String> {
        self.append_decision_event(
            decision_id,
            expected_reversal_event_id,
            DecisionEventKind::Recorded,
            Some(input),
            None,
        )
    }

    fn append_decision_event(
        &self,
        decision_id: &str,
        expected_event_id: &str,
        event_kind: DecisionEventKind,
        conclusion: Option<DecisionConclusionInput>,
        reversal_rationale: Option<&str>,
    ) -> Result<ResearchDecisionDto, String> {
        require_nonblank("Decision ID", decision_id)?;
        require_nonblank("Expected decision event ID", expected_event_id)?;
        let actor = researcher_decision_actor();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| format!("Could not begin decision event append: {error}"))?;
        let current = read_research_decision(&transaction, decision_id)?
            .ok_or_else(|| format!("Research decision {decision_id} does not exist"))?;
        let head = current
            .current_head
            .as_ref()
            .ok_or_else(|| format!("Research decision {decision_id} has no event history"))?;
        if head.event_id != expected_event_id {
            return Err(format!(
                "Stale decision event ID: expected current event {}",
                head.event_id
            ));
        }
        let valid_transition = match event_kind {
            DecisionEventKind::Superseded | DecisionEventKind::Reversed => {
                matches!(
                    head.event_kind,
                    DecisionEventKind::Recorded | DecisionEventKind::Superseded
                )
            }
            DecisionEventKind::Recorded => head.event_kind == DecisionEventKind::Reversed,
        };
        if !valid_transition {
            return Err(format!(
                "Decision transition {event_kind:?} is invalid after {:?}",
                head.event_kind
            ));
        }
        let (outcome, rationale, references) = if let Some(input) = conclusion {
            input
                .validate(&current.target)
                .map_err(|error| format!("Invalid decision conclusion: {error}"))?;
            (Some(input.outcome), input.rationale, input.references)
        } else {
            let rationale = reversal_rationale
                .ok_or_else(|| "Decision reversal rationale is required".to_string())?;
            if rationale.trim().is_empty() {
                return Err("Decision rationale must not be blank".to_string());
            }
            (None, rationale.to_string(), Vec::new())
        };
        validate_decision_event_shape(event_kind, outcome, &current.target, &actor, &rationale)
            .map_err(|error| format!("Invalid decision event: {error}"))?;
        let event_id = Uuid::new_v4().to_string();
        let occurred_at = current_timestamp(&transaction)?;
        insert_decision_event(
            &transaction,
            decision_id,
            &event_id,
            event_kind,
            outcome,
            Some(expected_event_id),
            &occurred_at,
            &actor,
            &rationale,
            &references,
        )?;
        let decision = read_research_decision(&transaction, decision_id)?
            .ok_or_else(|| format!("Updated research decision {decision_id} disappeared"))?;
        transaction
            .commit()
            .map_err(|error| format!("Could not commit decision event append: {error}"))?;
        Ok(decision)
    }

    #[allow(dead_code)]
    pub(crate) fn get_research_decision(
        &self,
        decision_id: &str,
    ) -> Result<Option<ResearchDecisionDto>, String> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| format!("Could not begin research decision read: {error}"))?;
        let decision = read_research_decision(&transaction, decision_id)?;
        transaction
            .commit()
            .map_err(|error| format!("Could not finish research decision read: {error}"))?;
        Ok(decision)
    }
}

fn normalize_decision_target(target: DecisionTargetPair) -> Result<DecisionTargetPair, String> {
    DecisionTargetPair::new(
        target.decision_kind,
        target.record_a,
        target.record_b,
        target.lineage_type,
    )
    .map_err(|error| format!("Invalid decision target: {error}"))
}

fn insert_decision_identity(
    transaction: &Transaction<'_>,
    decision_id: &str,
    initial_event_id: &str,
    target: &DecisionTargetPair,
    origin: Option<(&str, &str)>,
    created_at: &str,
) -> Result<(), String> {
    let (origin_suggestion_id, origin_suggestion_event_id, origin_suggestion_status) = origin
        .map_or((None, None, None), |(suggestion_id, event_id)| {
            (Some(suggestion_id), Some(event_id), Some("resolved"))
        });
    transaction
        .execute(
            "INSERT INTO research_decisions (
                decision_id, decision_kind, record_a_id, record_a_kind,
                record_b_id, record_b_kind, lineage_type, origin_suggestion_id,
                origin_suggestion_event_id, origin_suggestion_status,
                created_at, initial_event_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                decision_id,
                enum_value(&target.decision_kind)?,
                target.record_a.record_id,
                enum_value(&target.record_a.record_kind)?,
                target.record_b.record_id,
                enum_value(&target.record_b.record_kind)?,
                target
                    .lineage_type
                    .map(|lineage_type| enum_value(&lineage_type))
                    .transpose()?,
                origin_suggestion_id,
                origin_suggestion_event_id,
                origin_suggestion_status,
                created_at,
                initial_event_id,
            ],
        )
        .map_err(|error| {
            format!(
                "Could not save research decision identity: {}",
                decision_storage_error(error)
            )
        })?;
    Ok(())
}

fn insert_decision_event(
    transaction: &Transaction<'_>,
    decision_id: &str,
    event_id: &str,
    event_kind: DecisionEventKind,
    outcome: Option<DecisionOutcome>,
    predecessor_event_id: Option<&str>,
    occurred_at: &str,
    actor: &RecordActor,
    rationale: &str,
    references: &[ResearchReferenceInput],
) -> Result<(), String> {
    validate_decision_event_shape(
        event_kind,
        outcome,
        &read_decision_target(transaction, decision_id)?,
        actor,
        rationale,
    )
    .map_err(|error| format!("Invalid decision event: {error}"))?;
    transaction
        .execute(
            "INSERT INTO decision_events (
                event_id, decision_id, event_kind, outcome, predecessor_event_id,
                occurred_at, actor_kind, actor_name, rationale
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                event_id,
                decision_id,
                enum_value(&event_kind)?,
                outcome.map(|outcome| enum_value(&outcome)).transpose()?,
                predecessor_event_id,
                occurred_at,
                enum_value(&actor.kind)?,
                actor.display_name,
                rationale,
            ],
        )
        .map_err(|error| {
            format!(
                "Could not append research decision event: {}",
                decision_storage_error(error)
            )
        })?;

    for reference in references {
        reference
            .validate()
            .map_err(|error| format!("Invalid decision reference: {error}"))?;
        let locator_json = reference
            .locator
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| format!("Could not encode decision reference locator: {error}"))?;
        transaction
            .execute(
                "INSERT INTO decision_event_references (
                    reference_id, event_id, record_id, record_kind,
                    reference_role, locator_json, note
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    Uuid::new_v4().to_string(),
                    event_id,
                    reference.record.record_id,
                    enum_value(&reference.record.record_kind)?,
                    reference.role,
                    locator_json,
                    reference.note,
                ],
            )
            .map_err(|error| {
                format!(
                    "Could not save decision event reference: {}",
                    decision_storage_error(error)
                )
            })?;
    }
    Ok(())
}

fn read_decision_target(
    connection: &Connection,
    decision_id: &str,
) -> Result<DecisionTargetPair, String> {
    let raw = connection
        .query_row(
            "SELECT decision_kind, record_a_id, record_a_kind,
                    record_b_id, record_b_kind, lineage_type
             FROM research_decisions WHERE decision_id = ?1",
            [decision_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .map_err(|error| format!("Could not read decision target {decision_id}: {error}"))?;
    let target = DecisionTargetPair {
        decision_kind: parse_enum(&raw.0, "decision kind")?,
        record_a: RecordTarget {
            record_id: raw.1,
            record_kind: parse_enum(&raw.2, "first decision target kind")?,
        },
        record_b: RecordTarget {
            record_id: raw.3,
            record_kind: parse_enum(&raw.4, "second decision target kind")?,
        },
        lineage_type: parse_optional_enum(raw.5, "decision lineage type")?,
    };
    target
        .validate()
        .map_err(|error| format!("Stored decision target is invalid: {error}"))?;
    Ok(target)
}

fn decision_storage_error(error: rusqlite::Error) -> String {
    let message = error.to_string();
    if message.contains("decision transition requires a valid current head") {
        "decision is no longer at the expected active head or the transition is invalid".into()
    } else if message.contains("decision outcome is incompatible with its thread") {
        "decision outcome is incompatible with its decision kind or lineage type".into()
    } else if message.contains("originating suggestion must be proposed and match the decision") {
        "originating suggestion is not proposed or does not match the decision".into()
    } else if message.contains("resolved suggestion must match its originating decision")
        || message.contains("resolved suggestion requires its originating decision event")
    {
        "suggestion and decision resolution links are inconsistent".into()
    } else if message.contains("UNIQUE constraint failed: research_decisions.") {
        "a decision thread already exists for these targets".into()
    } else if message.contains("FOREIGN KEY constraint failed") {
        "a referenced record, suggestion, or event does not exist with the expected identity".into()
    } else if message.contains("CHECK constraint failed") {
        format!("decision fields violate schema constraints: {message}")
    } else {
        message
    }
}

fn read_research_decision(
    connection: &Connection,
    decision_id: &str,
) -> Result<Option<ResearchDecisionDto>, String> {
    let raw = connection
        .query_row(
            "SELECT decision_kind, record_a_id, record_a_kind,
                    record_b_id, record_b_kind, lineage_type, created_at,
                    initial_event_id, origin_suggestion_id,
                    origin_suggestion_event_id, origin_suggestion_status
             FROM research_decisions WHERE decision_id = ?1",
            [decision_id],
            |row| {
                Ok(RawResearchDecision {
                    decision_kind: row.get(0)?,
                    record_a_id: row.get(1)?,
                    record_a_kind: row.get(2)?,
                    record_b_id: row.get(3)?,
                    record_b_kind: row.get(4)?,
                    lineage_type: row.get(5)?,
                    created_at: row.get(6)?,
                    initial_event_id: row.get(7)?,
                    origin_suggestion_id: row.get(8)?,
                    origin_suggestion_event_id: row.get(9)?,
                    origin_suggestion_status: row.get(10)?,
                })
            },
        )
        .optional()
        .map_err(|error| format!("Could not retrieve research decision {decision_id}: {error}"))?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let target = DecisionTargetPair {
        decision_kind: parse_enum(&raw.decision_kind, "decision kind")?,
        record_a: RecordTarget {
            record_id: raw.record_a_id,
            record_kind: parse_enum(&raw.record_a_kind, "first decision target kind")?,
        },
        record_b: RecordTarget {
            record_id: raw.record_b_id,
            record_kind: parse_enum(&raw.record_b_kind, "second decision target kind")?,
        },
        lineage_type: parse_optional_enum(raw.lineage_type, "decision lineage type")?,
    };
    target
        .validate()
        .map_err(|error| format!("Stored decision target is invalid: {error}"))?;

    let origin = match (
        raw.origin_suggestion_id,
        raw.origin_suggestion_event_id,
        raw.origin_suggestion_status,
    ) {
        (None, None, None) => None,
        (Some(suggestion_id), Some(event_id), Some(status)) if status == "resolved" => {
            let suggestion = read_research_suggestion(connection, &suggestion_id)?
                .ok_or_else(|| format!("Originating suggestion {suggestion_id} is missing"))?;
            let resolution = suggestion.events.last().ok_or_else(|| {
                format!("Originating suggestion {suggestion_id} has no event history")
            })?;
            if suggestion.status != SuggestionStatus::Resolved
                || resolution.status != SuggestionStatus::Resolved
                || resolution.event_id != event_id
                || resolution.decision_id.as_deref() != Some(decision_id)
                || suggestion.target != target
            {
                return Err(format!(
                    "Research decision {decision_id} has inconsistent suggestion provenance"
                ));
            }
            Some(suggestion_id)
        }
        _ => {
            return Err(format!(
                "Research decision {decision_id} has incomplete suggestion provenance"
            ));
        }
    };

    let mut statement = connection
        .prepare(
            "SELECT event_id, event_kind, outcome, predecessor_event_id,
                    occurred_at, actor_kind, actor_name, rationale
             FROM decision_events WHERE decision_id = ?1",
        )
        .map_err(|error| format!("Could not prepare decision event read: {error}"))?;
    let raw_events = statement
        .query_map([decision_id], |row| {
            Ok(RawDecisionEvent {
                event_id: row.get(0)?,
                event_kind: row.get(1)?,
                outcome: row.get(2)?,
                predecessor_event_id: row.get(3)?,
                occurred_at: row.get(4)?,
                actor_kind: row.get(5)?,
                actor_name: row.get(6)?,
                rationale: row.get(7)?,
            })
        })
        .map_err(|error| format!("Could not read decision events: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Could not decode decision events: {error}"))?;
    drop(statement);
    let history = order_and_validate_decision_events(
        connection,
        decision_id,
        &raw.initial_event_id,
        &target,
        raw_events,
    )?;
    let current_head = history.last().cloned();
    let unresolved = current_head
        .as_ref()
        .is_some_and(|event| event.event_kind == DecisionEventKind::Reversed);
    Ok(Some(ResearchDecisionDto {
        decision_id: decision_id.to_string(),
        target,
        created_at: raw.created_at,
        origin_suggestion_id: origin,
        current_head,
        unresolved,
        history,
    }))
}

fn order_and_validate_decision_events(
    connection: &Connection,
    decision_id: &str,
    initial_event_id: &str,
    target: &DecisionTargetPair,
    raw_events: Vec<RawDecisionEvent>,
) -> Result<Vec<DecisionEventDto>, String> {
    if raw_events.is_empty() {
        return Err(format!("Research decision {decision_id} has no events"));
    }
    let mut events_by_id = HashMap::new();
    let mut successor_by_predecessor = HashMap::new();
    let mut roots = Vec::new();
    for event in raw_events {
        if let Some(predecessor_id) = &event.predecessor_event_id {
            if successor_by_predecessor
                .insert(predecessor_id.clone(), event.event_id.clone())
                .is_some()
            {
                return Err(format!(
                    "Research decision {decision_id} has a branched event history"
                ));
            }
        } else {
            roots.push(event.event_id.clone());
        }
        let event_id = event.event_id.clone();
        if events_by_id.insert(event_id, event).is_some() {
            return Err(format!(
                "Research decision {decision_id} has duplicate event IDs"
            ));
        }
    }
    if roots.len() != 1 || roots[0] != initial_event_id {
        return Err(format!(
            "Research decision {decision_id} does not have its required initial event"
        ));
    }

    let mut history = Vec::with_capacity(events_by_id.len());
    let mut next_event_id = Some(initial_event_id.to_string());
    let mut previous_kind = None;
    while let Some(event_id) = next_event_id {
        let raw = events_by_id
            .remove(&event_id)
            .ok_or_else(|| format!("Research decision {decision_id} has a broken event chain"))?;
        let event_kind: DecisionEventKind = parse_enum(&raw.event_kind, "decision event kind")?;
        let outcome: Option<DecisionOutcome> =
            parse_optional_enum(raw.outcome, "decision event outcome")?;
        let actor = RecordActor {
            kind: parse_enum(&raw.actor_kind, "decision event actor kind")?,
            display_name: raw.actor_name,
        };
        validate_decision_event_shape(event_kind, outcome, target, &actor, &raw.rationale)
            .map_err(|error| {
                format!("Stored decision event {} is invalid: {error}", raw.event_id)
            })?;
        let valid_transition = match (previous_kind, event_kind) {
            (None, DecisionEventKind::Recorded) => raw.predecessor_event_id.is_none(),
            (
                Some(DecisionEventKind::Recorded | DecisionEventKind::Superseded),
                DecisionEventKind::Superseded | DecisionEventKind::Reversed,
            ) => true,
            (Some(DecisionEventKind::Reversed), DecisionEventKind::Recorded) => true,
            _ => false,
        };
        if !valid_transition {
            return Err(format!(
                "Research decision {decision_id} has an invalid event transition"
            ));
        }
        let references = read_decision_event_references(connection, &raw.event_id)?;
        history.push(DecisionEventDto {
            event_id: raw.event_id.clone(),
            event_kind,
            outcome,
            predecessor_event_id: raw.predecessor_event_id,
            occurred_at: raw.occurred_at,
            actor,
            rationale: raw.rationale,
            references,
        });
        previous_kind = Some(event_kind);
        next_event_id = successor_by_predecessor.remove(&event_id);
    }
    if !events_by_id.is_empty() || !successor_by_predecessor.is_empty() {
        return Err(format!(
            "Research decision {decision_id} has disconnected or cyclic event history"
        ));
    }
    Ok(history)
}

fn read_decision_event_references(
    connection: &Connection,
    event_id: &str,
) -> Result<Vec<ResearchReferenceDto>, String> {
    let mut statement = connection
        .prepare(
            "SELECT reference_id, record_id, record_kind, reference_role, locator_json, note
             FROM decision_event_references WHERE event_id = ?1 ORDER BY reference_id",
        )
        .map_err(|error| format!("Could not prepare decision reference read: {error}"))?;
    let raw = statement
        .query_map([event_id], |row| {
            Ok(RawSuggestionReference {
                reference_id: row.get(0)?,
                record_id: row.get(1)?,
                record_kind: row.get(2)?,
                role: row.get(3)?,
                locator_json: row.get(4)?,
                note: row.get(5)?,
            })
        })
        .map_err(|error| format!("Could not read decision references: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Could not decode decision references: {error}"))?;
    raw.into_iter()
        .map(|reference| {
            require_nonblank("Stored decision reference record ID", &reference.record_id)?;
            require_nonblank("Stored decision reference role", &reference.role)?;
            let record_kind = parse_enum(&reference.record_kind, "decision reference record kind")?;
            validate_reference_record_identity(
                connection,
                "decision event",
                &reference.reference_id,
                &reference.record_id,
                record_kind,
            )?;
            let locator = reference
                .locator_json
                .map(|json| {
                    serde_json::from_str::<Value>(&json).map_err(|error| {
                        format!(
                            "Stored locator for decision reference {} is invalid: {error}",
                            reference.reference_id
                        )
                    })
                })
                .transpose()?;
            Ok(ResearchReferenceDto {
                reference_id: reference.reference_id,
                record: RecordTarget {
                    record_id: reference.record_id,
                    record_kind,
                },
                role: reference.role,
                locator,
                note: reference.note,
            })
        })
        .collect()
}

fn validate_reference_record_identity(
    connection: &Connection,
    reference_type: &str,
    reference_id: &str,
    record_id: &str,
    expected_kind: DecisionRecordKind,
) -> Result<(), String> {
    let actual_kind = connection
        .query_row(
            "SELECT record_kind FROM records WHERE record_id = ?1",
            [record_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| {
            format!("Could not validate {reference_type} reference {reference_id}: {error}")
        })?;
    let Some(actual_kind) = actual_kind else {
        return Err(format!(
            "{reference_type} reference {reference_id} points to missing record {record_id}"
        ));
    };
    let expected_kind_value = enum_value(&expected_kind)?;
    if actual_kind != expected_kind_value {
        return Err(format!(
            "{reference_type} reference {reference_id} declares record kind {expected_kind_value}, \
             but record {record_id} has kind {actual_kind}"
        ));
    }
    Ok(())
}

struct RawResearchDecision {
    decision_kind: String,
    record_a_id: String,
    record_a_kind: String,
    record_b_id: String,
    record_b_kind: String,
    lineage_type: Option<String>,
    created_at: String,
    initial_event_id: String,
    origin_suggestion_id: Option<String>,
    origin_suggestion_event_id: Option<String>,
    origin_suggestion_status: Option<String>,
}

struct RawDecisionEvent {
    event_id: String,
    event_kind: String,
    outcome: Option<String>,
    predecessor_event_id: Option<String>,
    occurred_at: String,
    actor_kind: String,
    actor_name: Option<String>,
    rationale: String,
}

fn insert_research_suggestion(
    transaction: &Transaction<'_>,
    input: &CreateSuggestionInput,
    suggestion_id: &str,
    initial_event_id: &str,
) -> Result<SuggestionDto, String> {
    input
        .validate()
        .map_err(|error| format!("Invalid research suggestion: {error}"))?;
    let created_at = current_timestamp(transaction)?;
    transaction
        .execute(
            "INSERT INTO research_suggestions (
                suggestion_id, decision_kind, record_a_id, record_a_kind,
                record_b_id, record_b_kind, lineage_type, proposed_outcome,
                origin_kind, origin_name, generator_version, created_at,
                summary, initial_event_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                suggestion_id,
                enum_value(&input.target.decision_kind)?,
                input.target.record_a.record_id,
                enum_value(&input.target.record_a.record_kind)?,
                input.target.record_b.record_id,
                enum_value(&input.target.record_b.record_kind)?,
                input
                    .target
                    .lineage_type
                    .map(|lineage_type| enum_value(&lineage_type))
                    .transpose()?,
                input
                    .proposed_outcome
                    .map(|outcome| enum_value(&outcome))
                    .transpose()?,
                enum_value(&input.origin.kind)?,
                input.origin.display_name,
                input.origin.generator_version,
                created_at,
                input.summary,
                initial_event_id,
            ],
        )
        .map_err(|error| {
            format!(
                "Could not save research suggestion identity: {}",
                suggestion_storage_error(error)
            )
        })?;

    for reference in &input.references {
        let reference_id = Uuid::new_v4().to_string();
        let locator_json = reference
            .locator
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| format!("Could not encode suggestion reference locator: {error}"))?;
        transaction
            .execute(
                "INSERT INTO suggestion_references (
                    reference_id, suggestion_id, record_id, record_kind,
                    reference_role, locator_json, note
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    reference_id,
                    suggestion_id,
                    reference.record.record_id,
                    enum_value(&reference.record.record_kind)?,
                    reference.role,
                    locator_json,
                    reference.note,
                ],
            )
            .map_err(|error| {
                format!(
                    "Could not save research suggestion reference: {}",
                    suggestion_storage_error(error)
                )
            })?;
    }

    transaction
        .execute(
            "INSERT INTO suggestion_events (
                event_id, suggestion_id, status, occurred_at,
                actor_kind, actor_name
             ) VALUES (?1, ?2, 'proposed', ?3, ?4, ?5)",
            params![
                initial_event_id,
                suggestion_id,
                created_at,
                enum_value(&input.actor.kind)?,
                input.actor.display_name,
            ],
        )
        .map_err(|error| {
            format!(
                "Could not save initial research suggestion event: {}",
                suggestion_storage_error(error)
            )
        })?;
    read_research_suggestion(transaction, suggestion_id)?
        .ok_or_else(|| "Created research suggestion could not be read back".to_string())
}

fn insert_candidate_evidence_snapshot(
    transaction: &Transaction<'_>,
    suggestion_id: &str,
    snapshot: &CandidateEvidenceSnapshot,
    snapshot_json: &str,
) -> Result<(), String> {
    snapshot
        .validate_integrity()
        .map_err(|error| format!("Invalid candidate evidence snapshot: {error}"))?;
    let target = transaction
        .query_row(
            "SELECT decision_kind, record_a_id, record_b_id
             FROM research_suggestions WHERE suggestion_id = ?1",
            [suggestion_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| {
            format!("Could not read target for research suggestion {suggestion_id:?}: {error}")
        })?
        .ok_or_else(|| format!("Research suggestion {suggestion_id:?} does not exist"))?;
    if target.0 != "incident_pair"
        || target.1 != snapshot.candidate.left_incident_id
        || target.2 != snapshot.candidate.right_incident_id
    {
        return Err(format!(
            "Candidate evidence target {}/{} does not match incident-pair suggestion {suggestion_id:?}",
            snapshot.candidate.left_incident_id, snapshot.candidate.right_incident_id
        ));
    }
    let created_at = current_timestamp(transaction)?;
    transaction
        .execute(
            "INSERT INTO candidate_evidence_snapshots (
                suggestion_id, snapshot_json, fingerprint, generator_version,
                evidence_format_version, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                suggestion_id,
                snapshot_json,
                snapshot.fingerprint,
                snapshot.generator_version,
                snapshot.evidence_format_version,
                created_at,
            ],
        )
        .map_err(|error| {
            format!(
                "Could not insert candidate evidence snapshot for suggestion {suggestion_id:?}: {error}"
            )
        })?;
    Ok(())
}

fn read_candidate_evidence_snapshot(
    connection: &Connection,
    suggestion_id: &str,
) -> Result<Option<CandidateEvidenceSnapshot>, String> {
    let stored = connection
        .query_row(
            "SELECT evidence.snapshot_json, evidence.fingerprint,
                    evidence.generator_version, evidence.evidence_format_version,
                    suggestion.decision_kind, suggestion.record_a_id,
                    suggestion.record_b_id
             FROM candidate_evidence_snapshots evidence
             JOIN research_suggestions suggestion
               ON suggestion.suggestion_id = evidence.suggestion_id
             WHERE evidence.suggestion_id = ?1",
            [suggestion_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, u32>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .optional()
        .map_err(|error| {
            format!(
                "Could not read candidate evidence snapshot for suggestion {suggestion_id:?}: {error}"
            )
        })?;
    let Some((snapshot_json, fingerprint, generator_version, format_version, kind, left, right)) =
        stored
    else {
        return Ok(None);
    };
    let snapshot: CandidateEvidenceSnapshot = serde_json::from_str(&snapshot_json).map_err(
        |error| {
            format!(
                "Stored candidate evidence snapshot for suggestion {suggestion_id:?} is malformed: {error}"
            )
        },
    )?;
    snapshot.validate_integrity().map_err(|error| {
        format!(
            "Stored candidate evidence snapshot for suggestion {suggestion_id:?} failed integrity validation: {error}"
        )
    })?;
    if kind != "incident_pair"
        || snapshot.candidate.left_incident_id != left
        || snapshot.candidate.right_incident_id != right
    {
        return Err(format!(
            "Stored candidate evidence snapshot for suggestion {suggestion_id:?} has a target inconsistent with its suggestion"
        ));
    }
    if snapshot.fingerprint != fingerprint
        || snapshot.generator_version != generator_version
        || snapshot.evidence_format_version != format_version
    {
        return Err(format!(
            "Stored candidate evidence snapshot for suggestion {suggestion_id:?} has metadata inconsistent with its indexed columns"
        ));
    }
    Ok(Some(snapshot))
}

fn supersede_research_suggestion_in_transaction(
    transaction: &Transaction<'_>,
    suggestion_id: &str,
    expected_event_id: &str,
    replacement_suggestion_id: &str,
    rationale: &str,
    actor: &RecordActor,
) -> Result<SuggestionDto, String> {
    validate_suggestion_transition_input(expected_event_id, rationale)?;
    require_nonblank("Replacement suggestion ID", replacement_suggestion_id)?;
    if suggestion_id == replacement_suggestion_id {
        return Err("A suggestion cannot replace itself".to_string());
    }
    let current = read_research_suggestion(transaction, suggestion_id)?
        .ok_or_else(|| format!("Research suggestion {suggestion_id} does not exist"))?;
    validate_suggestion_head(&current, expected_event_id)?;
    let replacement = read_research_suggestion(transaction, replacement_suggestion_id)?
        .ok_or_else(|| {
            format!("Replacement suggestion {replacement_suggestion_id} does not exist")
        })?;
    if replacement.target != current.target {
        return Err("Replacement suggestion addresses a different question".to_string());
    }
    if replacement.status != SuggestionStatus::Proposed {
        return Err("Replacement suggestion must still be proposed".to_string());
    }

    let event_id = Uuid::new_v4().to_string();
    let occurred_at = current_timestamp(transaction)?;
    transaction
        .execute(
            "INSERT INTO suggestion_events (
                event_id, suggestion_id, status, predecessor_event_id,
                occurred_at, actor_kind, actor_name,
                replacement_suggestion_id, rationale
             ) VALUES (?1, ?2, 'superseded', ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                event_id,
                suggestion_id,
                expected_event_id,
                occurred_at,
                enum_value(&actor.kind)?,
                actor.display_name,
                replacement_suggestion_id,
                rationale,
            ],
        )
        .map_err(|error| {
            format!(
                "Could not supersede research suggestion: {}",
                suggestion_storage_error(error)
            )
        })?;
    read_research_suggestion(transaction, suggestion_id)?
        .ok_or_else(|| format!("Research suggestion {suggestion_id} disappeared"))
}

fn validate_suggestion_transition_input(
    expected_event_id: &str,
    rationale: &str,
) -> Result<(), String> {
    require_nonblank("Expected suggestion event ID", expected_event_id)?;
    require_nonblank("Suggestion transition rationale", rationale)
}

fn require_nonblank(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field} must not be blank"))
    } else {
        Ok(())
    }
}

fn validate_suggestion_head(
    suggestion: &SuggestionDto,
    expected_event_id: &str,
) -> Result<(), String> {
    if suggestion.status != SuggestionStatus::Proposed {
        return Err(format!(
            "Research suggestion {} is already terminal ({:?})",
            suggestion.suggestion_id, suggestion.status
        ));
    }
    let current_event_id = suggestion
        .events
        .last()
        .map(|event| event.event_id.as_str())
        .ok_or_else(|| {
            format!(
                "Research suggestion {} has no event history",
                suggestion.suggestion_id
            )
        })?;
    if current_event_id != expected_event_id {
        return Err(format!(
            "Stale suggestion event ID: expected current event {current_event_id}"
        ));
    }
    Ok(())
}

struct PreparedCandidateSuggestion {
    snapshot: CandidateEvidenceSnapshot,
    references: Vec<ResearchReferenceInput>,
    target: DecisionTargetPair,
}

enum CandidateReconciliationAction {
    Created(String),
    Reused(String),
    Skipped,
}

fn candidate_evidence_references(
    snapshot: &CandidateEvidenceSnapshot,
) -> Result<Vec<ResearchReferenceInput>, String> {
    let endpoints = [
        snapshot.candidate.left_incident_id.as_str(),
        snapshot.candidate.right_incident_id.as_str(),
    ];
    let mut references = BTreeMap::<(String, &'static str), &'static str>::new();
    for incident_id in endpoints {
        references.insert((incident_id.to_string(), "incident"), "candidate_incident");
    }

    for claim in &snapshot.claim_provenance {
        if !endpoints.contains(&claim.incident_id.as_str()) {
            return Err(format!(
                "Candidate claim {:?} is not associated with either target incident",
                claim.database_id
            ));
        }
        if !snapshot.source_origins.contains_key(&claim.source_id) {
            return Err(format!(
                "Candidate claim {:?} references source {:?} without snapshot origins",
                claim.database_id, claim.source_id
            ));
        }
        references.insert((claim.database_id.clone(), "claim"), "candidate_claim");
    }
    for (source_id, origins) in &snapshot.source_origins {
        if origins.is_empty() {
            return Err(format!(
                "Candidate source {source_id:?} has no dataset origin in its evidence snapshot"
            ));
        }
        references.insert((source_id.clone(), "source"), "candidate_source");
    }
    for relationship in &snapshot.relationship_provenance {
        if !snapshot.candidate.supporting_signals.iter().any(|signal| {
            matches!(
                signal,
                SupportingSignal::RelatedSourceReferences { dependencies, .. }
                    if dependencies.iter().any(|edge| {
                        edge.relationship_record_id == relationship.relationship_record_id
                            && edge == &relationship.edge
                    })
            )
        }) {
            return Err(format!(
                "Candidate relationship {:?} is not represented by a dependency signal",
                relationship.relationship_record_id
            ));
        }
        references.insert(
            (relationship.relationship_record_id.clone(), "relationship"),
            "candidate_relationship",
        );
    }

    references
        .into_iter()
        .map(|((record_id, record_kind), role)| {
            let record_kind = match record_kind {
                "incident" => DecisionRecordKind::Incident,
                "source" => DecisionRecordKind::Source,
                "claim" => DecisionRecordKind::Claim,
                "relationship" => DecisionRecordKind::Relationship,
                _ => {
                    return Err(format!(
                        "Unsupported candidate reference kind {record_kind}"
                    ));
                }
            };
            Ok(ResearchReferenceInput {
                record: RecordTarget {
                    record_id,
                    record_kind,
                },
                role: role.to_string(),
                locator: None,
                note: None,
            })
        })
        .collect()
}

fn suggestion_references_match_evidence(
    suggestion: &SuggestionDto,
    snapshot: &CandidateEvidenceSnapshot,
) -> Result<bool, String> {
    let expected = candidate_evidence_references(snapshot)?
        .into_iter()
        .map(|reference| {
            Ok((
                reference.record.record_id,
                enum_value(&reference.record.record_kind)?,
                reference.role,
                reference
                    .locator
                    .map(|locator| serde_json::to_string(&locator))
                    .transpose()
                    .map_err(|error| {
                        format!("Could not encode expected candidate reference locator: {error}")
                    })?,
                reference.note,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut actual = suggestion
        .references
        .iter()
        .map(|reference| {
            Ok((
                reference.record.record_id.clone(),
                enum_value(&reference.record.record_kind)?,
                reference.role.clone(),
                reference
                    .locator
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()
                    .map_err(|error| {
                        format!("Could not encode stored candidate reference locator: {error}")
                    })?,
                reference.note.clone(),
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut expected = expected;
    expected.sort();
    actual.sort();
    Ok(actual == expected)
}

fn validate_candidate_corpus_current(
    transaction: &Transaction<'_>,
    selected_imports: &[crate::research_candidate_read::SelectedDatasetImport],
) -> Result<(), String> {
    let mut statement = transaction
        .prepare(
            "SELECT current.dataset_id, current.dataset_version, current.import_sequence
             FROM dataset_imports current
             WHERE NOT EXISTS (
                 SELECT 1 FROM dataset_imports newer
                 WHERE newer.dataset_id = current.dataset_id
                   AND newer.import_sequence > current.import_sequence
             )
             ORDER BY current.dataset_id",
        )
        .map_err(|error| format!("Could not prepare candidate corpus freshness check: {error}"))?;
    let current = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(|error| format!("Could not query current dataset imports: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Could not read current dataset imports: {error}"))?;
    let expected = selected_imports
        .iter()
        .map(|import| {
            (
                import.dataset_id.clone(),
                import.dataset_version.clone(),
                import.import_sequence,
            )
        })
        .collect::<Vec<_>>();
    if current != expected {
        return Err(
            "Candidate corpus is stale: latest dataset identities, versions, or import sequences changed before reconciliation"
                .to_string(),
        );
    }
    Ok(())
}

fn reconcile_one_candidate(
    transaction: &Transaction<'_>,
    candidate: &PreparedCandidateSuggestion,
) -> Result<CandidateReconciliationAction, String> {
    let current = read_current_suggestion_head(transaction, &candidate.target)?;
    let decision_exists: bool = transaction
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM research_decisions
                 WHERE decision_kind = 'incident_pair'
                   AND record_a_id = ?1 AND record_b_id = ?2
             )",
            params![
                candidate.snapshot.candidate.left_incident_id,
                candidate.snapshot.candidate.right_incident_id
            ],
            |row| row.get(0),
        )
        .map_err(|error| {
            format!(
                "Could not check decision history for candidate {}/{}: {error}",
                candidate.snapshot.candidate.left_incident_id,
                candidate.snapshot.candidate.right_incident_id
            )
        })?;
    if decision_exists {
        return Ok(CandidateReconciliationAction::Skipped);
    }

    let Some(current) = current else {
        let suggestion_id = create_candidate_suggestion(transaction, candidate)?;
        return Ok(CandidateReconciliationAction::Created(suggestion_id));
    };

    match current.status {
        SuggestionStatus::Dismissed | SuggestionStatus::Resolved => {
            Ok(CandidateReconciliationAction::Skipped)
        }
        SuggestionStatus::Proposed if current.origin.kind != SuggestionOriginKind::System => {
            Ok(CandidateReconciliationAction::Skipped)
        }
        SuggestionStatus::Proposed => {
            let Some(existing) =
                read_candidate_evidence_snapshot(transaction, &current.suggestion_id)?
            else {
                return Ok(CandidateReconciliationAction::Skipped);
            };
            if current.proposed_outcome.is_some()
                || current.origin.generator_version.as_deref()
                    != Some(existing.generator_version.as_str())
                || !suggestion_references_match_evidence(&current, &existing)?
            {
                return Err(format!(
                    "System suggestion {} is inconsistent with its candidate evidence snapshot",
                    current.suggestion_id
                ));
            }
            if existing.fingerprint == candidate.snapshot.fingerprint {
                return Ok(CandidateReconciliationAction::Reused(current.suggestion_id));
            }

            let replacement_id = create_candidate_suggestion(transaction, candidate)?;
            let expected_event_id = current
                .events
                .last()
                .map(|event| event.event_id.as_str())
                .ok_or_else(|| {
                    format!(
                        "Current system suggestion {} has no event head",
                        current.suggestion_id
                    )
                })?;
            let system_actor = RecordActor {
                kind: ActorKind::System,
                display_name: Some("XenoScope".to_string()),
            };
            supersede_research_suggestion_in_transaction(
                transaction,
                &current.suggestion_id,
                expected_event_id,
                &replacement_id,
                "Candidate evidence changed; replaced by a newer generated evidence snapshot.",
                &system_actor,
            )?;
            Ok(CandidateReconciliationAction::Created(replacement_id))
        }
        SuggestionStatus::Superseded => Err(format!(
            "Suggestion history for candidate {}/{} has a superseded current head",
            candidate.snapshot.candidate.left_incident_id,
            candidate.snapshot.candidate.right_incident_id
        )),
    }
}

fn create_candidate_suggestion(
    transaction: &Transaction<'_>,
    candidate: &PreparedCandidateSuggestion,
) -> Result<String, String> {
    let suggestion_id = Uuid::new_v4().to_string();
    let initial_event_id = Uuid::new_v4().to_string();
    let input = CreateSuggestionInput {
        target: candidate.target.clone(),
        proposed_outcome: None,
        origin: SuggestionOrigin {
            kind: SuggestionOriginKind::System,
            display_name: Some("XenoScope candidate generator".to_string()),
            generator_version: Some(candidate.snapshot.generator_version.clone()),
        },
        actor: RecordActor {
            kind: ActorKind::System,
            display_name: Some("XenoScope".to_string()),
        },
        summary: "Incident pair surfaced for further investigation.".to_string(),
        references: candidate.references.clone(),
    };
    let suggestion =
        insert_research_suggestion(transaction, &input, &suggestion_id, &initial_event_id)?;
    let snapshot_json = serde_json::to_string(&candidate.snapshot)
        .map_err(|error| format!("Could not serialize candidate evidence snapshot: {error}"))?;
    insert_candidate_evidence_snapshot(
        transaction,
        &suggestion.suggestion_id,
        &candidate.snapshot,
        &snapshot_json,
    )?;
    Ok(suggestion.suggestion_id)
}

fn read_current_suggestion_head(
    transaction: &Transaction<'_>,
    target: &DecisionTargetPair,
) -> Result<Option<SuggestionDto>, String> {
    let mut statement = transaction
        .prepare(
            "SELECT suggestion_id FROM research_suggestions
             WHERE decision_kind = 'incident_pair'
               AND record_a_id = ?1 AND record_b_id = ?2
             ORDER BY suggestion_id",
        )
        .map_err(|error| format!("Could not prepare suggestion history lookup: {error}"))?;
    let suggestion_ids = statement
        .query_map(
            params![target.record_a.record_id, target.record_b.record_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| format!("Could not query suggestion history: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Could not read suggestion history: {error}"))?;
    drop(statement);
    if suggestion_ids.is_empty() {
        return Ok(None);
    }

    let suggestions = suggestion_ids
        .iter()
        .map(|id| {
            read_research_suggestion(transaction, id)?
                .ok_or_else(|| format!("Suggestion {id:?} disappeared while reading history"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if suggestions
        .iter()
        .any(|suggestion| suggestion.target != *target)
    {
        return Err("Suggestion history contains a mismatched canonical target".to_string());
    }

    let by_id = suggestions
        .iter()
        .map(|suggestion| (suggestion.suggestion_id.as_str(), suggestion))
        .collect::<HashMap<_, _>>();
    let mut replacements = HashMap::<&str, &str>::new();
    let mut replaced = HashSet::<&str>::new();
    for suggestion in &suggestions {
        if suggestion.status == SuggestionStatus::Superseded {
            let event = suggestion.events.last().ok_or_else(|| {
                format!(
                    "Superseded suggestion {} has no terminal event",
                    suggestion.suggestion_id
                )
            })?;
            if event.status != SuggestionStatus::Superseded {
                return Err(format!(
                    "Suggestion {} has inconsistent superseded status",
                    suggestion.suggestion_id
                ));
            }
            let replacement_id = event.replacement_suggestion_id.as_deref().ok_or_else(|| {
                format!(
                    "Superseded suggestion {} has no replacement link",
                    suggestion.suggestion_id
                )
            })?;
            let replacement = by_id.get(replacement_id).ok_or_else(|| {
                format!(
                    "Suggestion {} points to missing replacement {replacement_id}",
                    suggestion.suggestion_id
                )
            })?;
            if replacement.target != *target || replacement_id == suggestion.suggestion_id {
                return Err(format!(
                    "Suggestion {} has an invalid replacement target",
                    suggestion.suggestion_id
                ));
            }
            if replacements
                .insert(suggestion.suggestion_id.as_str(), replacement_id)
                .is_some()
            {
                return Err(format!(
                    "Suggestion {} has multiple replacement links",
                    suggestion.suggestion_id
                ));
            }
            if !replaced.insert(replacement_id) {
                return Err(format!(
                    "Suggestion history for {}/{} branches into a replacement",
                    target.record_a.record_id, target.record_b.record_id
                ));
            }
        } else if suggestion
            .events
            .iter()
            .any(|event| event.status == SuggestionStatus::Superseded)
        {
            return Err(format!(
                "Suggestion {} has superseded history but a non-superseded head",
                suggestion.suggestion_id
            ));
        }
    }

    let roots = suggestions
        .iter()
        .filter(|suggestion| !replaced.contains(suggestion.suggestion_id.as_str()))
        .collect::<Vec<_>>();
    if roots.len() != 1 {
        return Err(format!(
            "Suggestion history for {}/{} has {} current heads; expected exactly one",
            target.record_a.record_id,
            target.record_b.record_id,
            roots.len()
        ));
    }
    let mut visited = HashSet::new();
    let mut next = roots[0].suggestion_id.as_str();
    while let Some(replacement) = replacements.get(next) {
        if !visited.insert(next) {
            return Err(format!(
                "Suggestion replacement history for {}/{} contains a cycle",
                target.record_a.record_id, target.record_b.record_id
            ));
        }
        next = replacement;
    }
    visited.insert(next);
    if visited.len() != suggestions.len() {
        return Err(format!(
            "Suggestion history for {}/{} is disconnected or inconsistent",
            target.record_a.record_id, target.record_b.record_id
        ));
    }
    Ok(Some(
        (*by_id
            .get(next)
            .ok_or_else(|| "Suggestion history head disappeared".to_string())?)
        .clone(),
    ))
}

fn suggestion_storage_error(error: rusqlite::Error) -> String {
    let message = error.to_string();
    if message.contains("suggestion transition requires its proposed current head") {
        "suggestion is no longer proposed or the expected event is stale".to_string()
    } else if message.contains("replacement suggestion must address the same question") {
        "replacement suggestion addresses a different question".to_string()
    } else if message.contains("FOREIGN KEY constraint failed") {
        "a referenced record or suggestion does not exist with the expected kind".to_string()
    } else if message.contains("CHECK constraint failed") {
        format!("stored suggestion fields violate schema constraints: {message}")
    } else {
        message
    }
}

fn read_research_suggestion(
    connection: &Connection,
    suggestion_id: &str,
) -> Result<Option<SuggestionDto>, String> {
    let raw = connection
        .query_row(
            "SELECT
                suggestion_id, decision_kind, record_a_id, record_a_kind,
                record_b_id, record_b_kind, lineage_type, proposed_outcome,
                origin_kind, origin_name, generator_version, created_at,
                summary, initial_event_id
             FROM research_suggestions
             WHERE suggestion_id = ?1",
            [suggestion_id],
            |row| {
                Ok(RawResearchSuggestion {
                    suggestion_id: row.get(0)?,
                    decision_kind: row.get(1)?,
                    record_a_id: row.get(2)?,
                    record_a_kind: row.get(3)?,
                    record_b_id: row.get(4)?,
                    record_b_kind: row.get(5)?,
                    lineage_type: row.get(6)?,
                    proposed_outcome: row.get(7)?,
                    origin_kind: row.get(8)?,
                    origin_name: row.get(9)?,
                    generator_version: row.get(10)?,
                    created_at: row.get(11)?,
                    summary: row.get(12)?,
                    initial_event_id: row.get(13)?,
                })
            },
        )
        .optional()
        .map_err(|error| {
            format!("Could not retrieve research suggestion {suggestion_id}: {error}")
        })?;
    let Some(raw) = raw else {
        return Ok(None);
    };

    let decision_kind: DecisionKind =
        parse_enum(&raw.decision_kind, "research suggestion decision kind")?;
    let record_a_kind: DecisionRecordKind =
        parse_enum(&raw.record_a_kind, "first suggestion target kind")?;
    let record_b_kind: DecisionRecordKind =
        parse_enum(&raw.record_b_kind, "second suggestion target kind")?;
    let lineage_type: Option<LineageType> =
        parse_optional_enum(raw.lineage_type, "suggestion lineage type")?;
    let stored_target = DecisionTargetPair {
        decision_kind,
        record_a: crate::research_decision::RecordTarget {
            record_id: raw.record_a_id,
            record_kind: record_a_kind,
        },
        record_b: crate::research_decision::RecordTarget {
            record_id: raw.record_b_id,
            record_kind: record_b_kind,
        },
        lineage_type,
    };
    stored_target
        .validate()
        .map_err(|error| format!("Stored research suggestion target is invalid: {error}"))?;
    let proposed_outcome: Option<crate::research_decision::DecisionOutcome> =
        parse_optional_enum(raw.proposed_outcome, "suggestion proposed outcome")?;
    if let Some(outcome) = proposed_outcome {
        crate::research_decision::validate_outcome(&stored_target, outcome)
            .map_err(|error| format!("Stored research suggestion outcome is invalid: {error}"))?;
    }
    let origin_kind: SuggestionOriginKind = parse_enum(&raw.origin_kind, "suggestion origin kind")?;
    let origin = SuggestionOrigin {
        kind: origin_kind,
        display_name: raw.origin_name,
        generator_version: raw.generator_version,
    };
    origin
        .validate()
        .map_err(|error| format!("Stored research suggestion origin is invalid: {error}"))?;
    require_nonblank("Stored research suggestion summary", &raw.summary)?;

    let mut reference_statement = connection
        .prepare(
            "SELECT reference_id, record_id, record_kind, reference_role, locator_json, note
             FROM suggestion_references
             WHERE suggestion_id = ?1
             ORDER BY reference_id",
        )
        .map_err(|error| format!("Could not prepare suggestion reference read: {error}"))?;
    let raw_references = reference_statement
        .query_map([&raw.suggestion_id], |row| {
            Ok(RawSuggestionReference {
                reference_id: row.get(0)?,
                record_id: row.get(1)?,
                record_kind: row.get(2)?,
                role: row.get(3)?,
                locator_json: row.get(4)?,
                note: row.get(5)?,
            })
        })
        .map_err(|error| format!("Could not read suggestion references: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Could not decode suggestion references: {error}"))?;
    let references = raw_references
        .into_iter()
        .map(|reference| {
            require_nonblank(
                "Stored suggestion reference record ID",
                &reference.record_id,
            )?;
            require_nonblank("Stored suggestion reference role", &reference.role)?;
            let record_kind =
                parse_enum(&reference.record_kind, "suggestion reference record kind")?;
            validate_reference_record_identity(
                connection,
                "suggestion",
                &reference.reference_id,
                &reference.record_id,
                record_kind,
            )?;
            let locator = reference
                .locator_json
                .map(|json| {
                    serde_json::from_str::<Value>(&json).map_err(|error| {
                        format!(
                            "Stored locator for suggestion reference {} is invalid: {error}",
                            reference.reference_id
                        )
                    })
                })
                .transpose()?;
            Ok(ResearchReferenceDto {
                reference_id: reference.reference_id,
                record: crate::research_decision::RecordTarget {
                    record_id: reference.record_id,
                    record_kind,
                },
                role: reference.role,
                locator,
                note: reference.note,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let mut event_statement = connection
        .prepare(
            "SELECT event_id, status, predecessor_event_id, occurred_at,
                    actor_kind, actor_name, rationale,
                    replacement_suggestion_id, decision_id
             FROM suggestion_events
             WHERE suggestion_id = ?1",
        )
        .map_err(|error| format!("Could not prepare suggestion event read: {error}"))?;
    let raw_events = event_statement
        .query_map([&raw.suggestion_id], |row| {
            Ok(RawSuggestionEvent {
                event_id: row.get(0)?,
                status: row.get(1)?,
                predecessor_event_id: row.get(2)?,
                occurred_at: row.get(3)?,
                actor_kind: row.get(4)?,
                actor_name: row.get(5)?,
                rationale: row.get(6)?,
                replacement_suggestion_id: row.get(7)?,
                decision_id: row.get(8)?,
            })
        })
        .map_err(|error| format!("Could not read suggestion events: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Could not decode suggestion events: {error}"))?;

    let events = order_and_validate_suggestion_events(
        connection,
        &raw.suggestion_id,
        &raw.initial_event_id,
        &stored_target,
        raw_events,
    )?;
    let status = events.last().map(|event| event.status).ok_or_else(|| {
        format!(
            "Research suggestion {} has no event history",
            raw.suggestion_id
        )
    })?;

    Ok(Some(SuggestionDto {
        suggestion_id: raw.suggestion_id,
        target: stored_target,
        proposed_outcome,
        origin,
        created_at: raw.created_at,
        summary: raw.summary,
        references,
        status,
        events,
    }))
}

fn order_and_validate_suggestion_events(
    connection: &Connection,
    suggestion_id: &str,
    initial_event_id: &str,
    target: &DecisionTargetPair,
    raw_events: Vec<RawSuggestionEvent>,
) -> Result<Vec<SuggestionEventDto>, String> {
    if raw_events.is_empty() {
        return Err(format!("Research suggestion {suggestion_id} has no events"));
    }
    let mut events_by_id = HashMap::new();
    let mut successor_by_predecessor = HashMap::new();
    let mut roots = Vec::new();
    for event in raw_events {
        if let Some(predecessor_id) = &event.predecessor_event_id {
            if successor_by_predecessor
                .insert(predecessor_id.clone(), event.event_id.clone())
                .is_some()
            {
                return Err(format!(
                    "Research suggestion {suggestion_id} has a branched event history"
                ));
            }
        } else {
            roots.push(event.event_id.clone());
        }
        let event_id = event.event_id.clone();
        if events_by_id.insert(event_id, event).is_some() {
            return Err(format!(
                "Research suggestion {suggestion_id} has duplicate event IDs"
            ));
        }
    }
    if roots.len() != 1 || roots[0] != initial_event_id {
        return Err(format!(
            "Research suggestion {suggestion_id} does not have its required initial event"
        ));
    }

    let mut ordered = Vec::with_capacity(events_by_id.len());
    let mut next_event_id = Some(initial_event_id.to_string());
    let mut previous_status = None;
    while let Some(event_id) = next_event_id {
        let raw = events_by_id.remove(&event_id).ok_or_else(|| {
            format!("Research suggestion {suggestion_id} has a broken event chain")
        })?;
        let status: SuggestionStatus = parse_enum(&raw.status, "suggestion event status")?;
        if previous_status.is_some_and(|previous| previous != SuggestionStatus::Proposed) {
            return Err(format!(
                "Research suggestion {suggestion_id} has an event after a terminal event"
            ));
        }
        let actor_kind: ActorKind = parse_enum(&raw.actor_kind, "suggestion event actor kind")?;
        if raw
            .actor_name
            .as_deref()
            .is_some_and(|name| name.trim().is_empty())
        {
            return Err(format!(
                "Suggestion event {} has a blank actor name",
                raw.event_id
            ));
        }
        validate_suggestion_event_shape(connection, suggestion_id, &raw, status, target)?;
        let actor = RecordActor {
            kind: actor_kind,
            display_name: raw.actor_name,
        };
        ordered.push(SuggestionEventDto {
            event_id: raw.event_id.clone(),
            status,
            predecessor_event_id: raw.predecessor_event_id,
            occurred_at: raw.occurred_at,
            actor,
            rationale: raw.rationale,
            replacement_suggestion_id: raw.replacement_suggestion_id,
            decision_id: raw.decision_id,
        });
        previous_status = Some(status);
        next_event_id = successor_by_predecessor.remove(&event_id);
    }
    if !events_by_id.is_empty() || !successor_by_predecessor.is_empty() {
        return Err(format!(
            "Research suggestion {suggestion_id} has disconnected or cyclic event history"
        ));
    }
    Ok(ordered)
}

fn validate_suggestion_event_shape(
    connection: &Connection,
    suggestion_id: &str,
    raw: &RawSuggestionEvent,
    status: SuggestionStatus,
    target: &DecisionTargetPair,
) -> Result<(), String> {
    match status {
        SuggestionStatus::Proposed => {
            if raw.predecessor_event_id.is_some()
                || raw.rationale.is_some()
                || raw.replacement_suggestion_id.is_some()
                || raw.decision_id.is_some()
            {
                return Err(format!(
                    "Proposed suggestion event {} has invalid fields",
                    raw.event_id
                ));
            }
        }
        SuggestionStatus::Dismissed => {
            require_terminal_suggestion_event(raw, "dismissed")?;
        }
        SuggestionStatus::Superseded => {
            require_terminal_suggestion_event(raw, "superseded")?;
            let replacement_id = raw.replacement_suggestion_id.as_deref().ok_or_else(|| {
                format!(
                    "Superseded suggestion event {} has no replacement",
                    raw.event_id
                )
            })?;
            if replacement_id == suggestion_id {
                return Err(format!(
                    "Superseded suggestion event {} has an invalid replacement link",
                    raw.event_id
                ));
            }
            validate_replacement_target(connection, raw, target)?;
        }
        SuggestionStatus::Resolved => {
            if raw.predecessor_event_id.is_none()
                || raw.replacement_suggestion_id.is_some()
                || raw.decision_id.is_none()
            {
                return Err(format!(
                    "Resolved suggestion event {} has invalid fields",
                    raw.event_id
                ));
            }
            validate_resolved_suggestion_provenance(connection, suggestion_id, raw, target)?;
        }
    }
    Ok(())
}

fn validate_resolved_suggestion_provenance(
    connection: &Connection,
    suggestion_id: &str,
    resolved_event: &RawSuggestionEvent,
    suggestion_target: &DecisionTargetPair,
) -> Result<(), String> {
    let decision_id = resolved_event.decision_id.as_deref().ok_or_else(|| {
        format!(
            "Resolved suggestion event {} has no decision",
            resolved_event.event_id
        )
    })?;
    let decision = connection
        .query_row(
            "SELECT decision_kind, record_a_id, record_a_kind,
                    record_b_id, record_b_kind, lineage_type,
                    origin_suggestion_id, origin_suggestion_event_id,
                    origin_suggestion_status, initial_event_id
             FROM research_decisions WHERE decision_id = ?1",
            [decision_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, String>(9)?,
                ))
            },
        )
        .optional()
        .map_err(|error| {
            format!(
                "Could not validate resolved suggestion event {}: {error}",
                resolved_event.event_id
            )
        })?
        .ok_or_else(|| {
            format!(
                "Resolved suggestion event {} points to missing decision {decision_id}",
                resolved_event.event_id
            )
        })?;

    let decision_target = DecisionTargetPair {
        decision_kind: parse_enum(&decision.0, "resolved decision kind")?,
        record_a: RecordTarget {
            record_id: decision.1,
            record_kind: parse_enum(&decision.2, "first resolved decision target kind")?,
        },
        record_b: RecordTarget {
            record_id: decision.3,
            record_kind: parse_enum(&decision.4, "second resolved decision target kind")?,
        },
        lineage_type: parse_optional_enum(decision.5, "resolved decision lineage type")?,
    };
    decision_target.validate().map_err(|error| {
        format!(
            "Resolved suggestion event {} points to an invalid decision target: {error}",
            resolved_event.event_id
        )
    })?;
    if decision.6.as_deref() != Some(suggestion_id)
        || decision.7.as_deref() != Some(resolved_event.event_id.as_str())
        || decision.8.as_deref() != Some("resolved")
        || decision_target != *suggestion_target
    {
        return Err(format!(
            "Resolved suggestion event {} has inconsistent decision provenance",
            resolved_event.event_id
        ));
    }

    let root = connection
        .query_row(
            "SELECT event_kind, outcome, predecessor_event_id,
                    actor_kind, actor_name, rationale
             FROM decision_events
             WHERE event_id = ?1 AND decision_id = ?2",
            params![decision.9, decision_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()
        .map_err(|error| {
            format!("Could not validate initial event for decision {decision_id}: {error}")
        })?
        .ok_or_else(|| {
            format!(
                "Resolved suggestion event {} points to decision {decision_id} with a missing initial event",
                resolved_event.event_id
            )
        })?;
    let root_kind: DecisionEventKind = parse_enum(&root.0, "initial decision event kind")?;
    let root_outcome: Option<DecisionOutcome> =
        parse_optional_enum(root.1, "initial decision event outcome")?;
    let root_actor = RecordActor {
        kind: parse_enum(&root.3, "initial decision event actor kind")?,
        display_name: root.4,
    };
    if root_kind != DecisionEventKind::Recorded || root.2.is_some() {
        return Err(format!(
            "Resolved suggestion event {} points to a decision without a valid recorded root",
            resolved_event.event_id
        ));
    }
    validate_decision_event_shape(
        root_kind,
        root_outcome,
        &decision_target,
        &root_actor,
        &root.5,
    )
    .map_err(|error| {
        format!(
            "Resolved suggestion event {} points to an invalid decision root: {error}",
            resolved_event.event_id
        )
    })?;
    Ok(())
}

fn require_terminal_suggestion_event(raw: &RawSuggestionEvent, label: &str) -> Result<(), String> {
    if raw.predecessor_event_id.is_none()
        || raw
            .rationale
            .as_deref()
            .is_none_or(|rationale| rationale.trim().is_empty())
        || raw.decision_id.is_some()
    {
        return Err(format!(
            "{label} suggestion event {} has invalid fields",
            raw.event_id
        ));
    }
    if label == "dismissed" && raw.replacement_suggestion_id.is_some() {
        return Err(format!(
            "Dismissed suggestion event {} has a replacement link",
            raw.event_id
        ));
    }
    if label == "superseded" && raw.replacement_suggestion_id.is_none() {
        return Err(format!(
            "Superseded suggestion event {} has no replacement",
            raw.event_id
        ));
    }
    Ok(())
}

fn validate_replacement_target(
    connection: &Connection,
    event: &RawSuggestionEvent,
    target: &DecisionTargetPair,
) -> Result<(), String> {
    let replacement_id = event.replacement_suggestion_id.as_deref().ok_or_else(|| {
        format!(
            "Superseded suggestion event {} has no replacement",
            event.event_id
        )
    })?;
    let compatible: bool = connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM research_suggestions replacement
                WHERE replacement.suggestion_id = ?1
                  AND replacement.decision_kind = ?2
                  AND replacement.record_a_id = ?3
                  AND replacement.record_a_kind = ?4
                  AND replacement.record_b_id = ?5
                  AND replacement.record_b_kind = ?6
                  AND replacement.lineage_type IS ?7
            )",
            params![
                replacement_id,
                enum_value(&target.decision_kind)?,
                target.record_a.record_id,
                enum_value(&target.record_a.record_kind)?,
                target.record_b.record_id,
                enum_value(&target.record_b.record_kind)?,
                target
                    .lineage_type
                    .map(|lineage_type| enum_value(&lineage_type))
                    .transpose()?,
            ],
            |row| row.get(0),
        )
        .map_err(|error| format!("Could not validate suggestion replacement: {error}"))?;
    if compatible {
        Ok(())
    } else {
        Err(format!(
            "Superseded suggestion event {} points to an incompatible replacement",
            event.event_id
        ))
    }
}

fn parse_optional_enum<T: DeserializeOwned>(
    value: Option<String>,
    field: &str,
) -> Result<Option<T>, String> {
    value.map(|value| parse_enum(&value, field)).transpose()
}

struct RawResearchSuggestion {
    suggestion_id: String,
    decision_kind: String,
    record_a_id: String,
    record_a_kind: String,
    record_b_id: String,
    record_b_kind: String,
    lineage_type: Option<String>,
    proposed_outcome: Option<String>,
    origin_kind: String,
    origin_name: Option<String>,
    generator_version: Option<String>,
    created_at: String,
    summary: String,
    initial_event_id: String,
}

struct RawSuggestionReference {
    reference_id: String,
    record_id: String,
    record_kind: String,
    role: String,
    locator_json: Option<String>,
    note: Option<String>,
}

struct RawSuggestionEvent {
    event_id: String,
    status: String,
    predecessor_event_id: Option<String>,
    occurred_at: String,
    actor_kind: String,
    actor_name: Option<String>,
    rationale: Option<String>,
    replacement_suggestion_id: Option<String>,
    decision_id: Option<String>,
}

struct RawSource {
    id: String,
    created_at: String,
    created_by_kind: String,
    created_by_name: Option<String>,
    source_kind: String,
    title: Option<String>,
    publisher: Option<String>,
    author: Option<String>,
    publication_date: Option<String>,
    url: Option<String>,
    other_locator: Option<String>,
    verification_status: String,
}

impl RawSource {
    fn into_source(self) -> Result<Source, String> {
        Ok(Source {
            id: self.id,
            created_at: self.created_at,
            created_by: parse_actor(self.created_by_kind, self.created_by_name)?,
            source_kind: parse_enum(&self.source_kind, "source kind")?,
            title: self.title,
            publisher: self.publisher,
            author: self.author,
            publication_date: self.publication_date,
            url: self.url,
            other_locator: self.other_locator,
            verification_status: parse_enum(
                &self.verification_status,
                "source verification status",
            )?,
        })
    }
}

struct RawClaim {
    id: String,
    created_at: String,
    created_by_kind: String,
    created_by_name: Option<String>,
    assertion_text: String,
    category: String,
    source_id: String,
    source_locator_kind: String,
    source_locator_json: String,
    incident_id: Option<String>,
    review_status: String,
    reviewed_at: Option<String>,
    reviewed_by_kind: Option<String>,
    reviewed_by_name: Option<String>,
    confidence_stated_by: Option<String>,
    confidence_value: Option<String>,
    confidence_source_id: Option<String>,
    confidence_locator_kind: Option<String>,
    confidence_locator_json: Option<String>,
}

impl RawClaim {
    fn into_claim(self) -> Result<Claim, String> {
        let source_locator: SourceLocator = serde_json::from_str(&self.source_locator_json)
            .map_err(|error| format!("Stored claim locator is invalid: {error}"))?;
        source_locator.validate()?;
        if source_locator.kind() != self.source_locator_kind {
            return Err("Stored claim locator type does not match its saved kind".to_string());
        }

        let author_stated_confidence = match (
            self.confidence_stated_by,
            self.confidence_value,
            self.confidence_source_id,
            self.confidence_locator_kind,
            self.confidence_locator_json,
        ) {
            (None, None, None, None, None) => None,
            (Some(stated_by), Some(value_as_stated), Some(source_id), kind, json) => {
                let locator = match (kind, json) {
                    (None, None) => None,
                    (Some(kind), Some(json)) => {
                        let locator: SourceLocator =
                            serde_json::from_str(&json).map_err(|error| {
                                format!("Stored confidence locator is invalid: {error}")
                            })?;
                        locator.validate()?;
                        if locator.kind() != kind {
                            return Err(
                                "Stored confidence locator type does not match its saved kind"
                                    .to_string(),
                            );
                        }
                        Some(locator)
                    }
                    _ => return Err("Stored confidence locator is incomplete".to_string()),
                };
                Some(AuthorStatedConfidence {
                    stated_by,
                    value_as_stated,
                    source_id,
                    locator,
                })
            }
            _ => return Err("Stored author-stated confidence is incomplete".to_string()),
        };

        let reviewed_by = self
            .reviewed_by_kind
            .map(|kind| parse_actor(kind, self.reviewed_by_name))
            .transpose()?;

        Ok(Claim {
            id: self.id,
            created_at: self.created_at,
            created_by: parse_actor(self.created_by_kind, self.created_by_name)?,
            assertion_text: self.assertion_text,
            category: parse_enum(&self.category, "claim category")?,
            source_id: self.source_id,
            source_locator,
            incident_id: self.incident_id,
            author_stated_confidence,
            review: ReviewMetadata {
                status: parse_enum(&self.review_status, "claim review status")?,
                reviewed_at: self.reviewed_at,
                reviewed_by,
            },
        })
    }
}

pub(crate) fn researcher_actor() -> RecordActor {
    RecordActor {
        kind: ActorKind::Researcher,
        display_name: None,
    }
}

fn parse_actor(kind: String, display_name: Option<String>) -> Result<RecordActor, String> {
    Ok(RecordActor {
        kind: parse_enum(&kind, "record actor")?,
        display_name,
    })
}

pub(crate) fn enum_value(value: &impl Serialize) -> Result<String, String> {
    serde_json::to_value(value)
        .map_err(|error| format!("Could not encode domain value: {error}"))?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "Domain enum did not serialize as a string".to_string())
}

fn parse_enum<T: DeserializeOwned>(value: &str, field: &str) -> Result<T, String> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .map_err(|error| format!("Stored {field} is invalid: {error}"))
}

fn canonical_uuid(field: &str, value: &str) -> Result<String, String> {
    Uuid::parse_str(value)
        .map(|id| id.hyphenated().to_string())
        .map_err(|_| format!("{field} must be a valid UUID"))
}

pub(crate) fn current_timestamp(connection: &Connection) -> Result<String, String> {
    connection
        .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| {
            row.get::<_, Option<String>>(0)
        })
        .map_err(|error| format!("Could not generate record creation time: {error}"))?
        .ok_or_else(|| "SQLite could not generate a UTC creation time".to_string())
}

pub(crate) fn insert_record(
    transaction: &Transaction<'_>,
    id: &str,
    record_kind: &str,
    created_at: &str,
    created_by: &RecordActor,
) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO records (record_id, record_kind, created_at, created_by_kind, created_by_name)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id,
                record_kind,
                created_at,
                enum_value(&created_by.kind)?,
                created_by.display_name,
            ],
        )
        .map_err(|error| format!("Could not save {record_kind} record identity: {error}"))?;
    Ok(())
}

pub(crate) fn insert_creation_history(
    transaction: &Transaction<'_>,
    subject_id: &str,
    subject_kind: &str,
    summary: &str,
) -> Result<(), String> {
    insert_creation_history_with_actor(
        transaction,
        subject_id,
        subject_kind,
        summary,
        &researcher_actor(),
    )
}

pub(crate) fn insert_creation_history_with_actor(
    transaction: &Transaction<'_>,
    subject_id: &str,
    subject_kind: &str,
    summary: &str,
    actor: &RecordActor,
) -> Result<(), String> {
    let history_id = Uuid::new_v4().to_string();
    let created_at = current_timestamp(transaction)?;
    insert_record(transaction, &history_id, "history", &created_at, actor)?;

    transaction
        .execute(
            "INSERT INTO record_history (
                record_id, record_kind, subject_record_id, subject_record_kind,
                event_kind, summary
             ) VALUES (?1, 'history', ?2, ?3, 'record_created', ?4)",
            params![history_id, subject_id, subject_kind, summary],
        )
        .map_err(|error| format!("Could not save {subject_kind} creation history: {error}"))?;
    Ok(())
}

pub(crate) fn insert_author_confidence(
    transaction: &Transaction<'_>,
    claim_id: &str,
    confidence: &AuthorStatedConfidence,
) -> Result<(), String> {
    let (locator_kind, locator_json) = confidence
        .locator
        .as_ref()
        .map(|locator| {
            Ok::<_, String>((
                Some(locator.kind()),
                Some(
                    serde_json::to_string(locator)
                        .map_err(|error| format!("Could not encode confidence locator: {error}"))?,
                ),
            ))
        })
        .transpose()?
        .unwrap_or((None, None));

    transaction
        .execute(
            "INSERT INTO claim_author_confidence (
                claim_id, stated_by, value_as_stated, source_id, locator_kind, locator_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                claim_id,
                confidence.stated_by,
                confidence.value_as_stated,
                confidence.source_id,
                locator_kind,
                locator_json,
            ],
        )
        .map_err(|error| format!("Could not save author-stated confidence: {error}"))?;
    Ok(())
}

fn require_existing_source(transaction: &Transaction<'_>, source_id: &str) -> Result<(), String> {
    let exists: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sources WHERE record_id = ?1)",
            [source_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("Could not validate source reference {source_id}: {error}"))?;
    if exists {
        Ok(())
    } else {
        Err(format!("Source {source_id} does not exist"))
    }
}

fn require_existing_incident(
    transaction: &Transaction<'_>,
    incident_id: &str,
) -> Result<(), String> {
    let exists: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM incidents WHERE record_id = ?1)",
            [incident_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("Could not validate incident reference {incident_id}: {error}"))?;
    if exists {
        Ok(())
    } else {
        Err(format!("Incident {incident_id} does not exist"))
    }
}

fn initialize(connection: &mut Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA recursive_triggers = ON;
             PRAGMA busy_timeout = 5000;",
        )
        .map_err(|error| format!("Could not configure SQLite connection: {error}"))?;

    let current_version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| format!("Could not read database schema version: {error}"))?;

    if current_version > LATEST_SCHEMA_VERSION {
        return Err(format!(
            "Database schema version {current_version} is newer than the supported version \
             {LATEST_SCHEMA_VERSION}"
        ));
    }

    for &(version, sql) in MIGRATIONS {
        if version > current_version {
            apply_migration(connection, version, sql)?;
        }
    }

    Ok(())
}

fn apply_migration(connection: &mut Connection, version: i64, sql: &str) -> Result<(), String> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| format!("Could not begin migration {version}: {error}"))?;

    transaction
        .execute_batch(sql)
        .map_err(|error| format!("Migration {version} failed and was rolled back: {error}"))?;

    transaction
        .pragma_update(None, "user_version", version)
        .map_err(|error| format!("Could not record migration {version}: {error}"))?;

    transaction
        .commit()
        .map_err(|error| format!("Could not commit migration {version}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research_decision::{
        DecisionKind, DecisionOutcome, DecisionRecordKind, DecisionTargetPair, LineageType,
        RecordTarget, ResearchReferenceInput, SuggestionOrigin, SuggestionOriginKind,
    };
    use crate::research_records::{
        AuthorStatedConfidence, ClaimCategory, CreateClaimInput, CreateSourceInput, SourceKind,
        SourceLocator, SourceVerificationStatus,
    };
    use rusqlite::Connection;
    use std::{
        fs,
        path::PathBuf,
        sync::{Arc, Barrier},
    };

    fn test_database() -> Database {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        initialize(&mut connection).expect("initialize schema");
        Database {
            connection: Mutex::new(connection),
        }
    }

    fn sample_source_input() -> CreateSourceInput {
        CreateSourceInput {
            source_kind: SourceKind::WebPage,
            title: Some("Research document".to_string()),
            publisher: Some("Example publisher".to_string()),
            author: Some("Example author".to_string()),
            publication_date: Some("2025-03".to_string()),
            url: Some("https://example.invalid/report".to_string()),
            other_locator: None,
            verification_status: SourceVerificationStatus::Verified,
        }
    }

    fn sample_claim_input(source_id: String) -> CreateClaimInput {
        CreateClaimInput {
            assertion_text: "A statement attributed to the cited source.".to_string(),
            category: ClaimCategory::Observation,
            source_id: source_id.clone(),
            source_locator: SourceLocator::Section {
                section: "Methods".to_string(),
                paragraph: Some(2),
            },
            incident_id: None,
            author_stated_confidence: Some(AuthorStatedConfidence {
                stated_by: "Example author".to_string(),
                value_as_stated: "High confidence".to_string(),
                source_id,
                locator: Some(SourceLocator::Page { page: 4 }),
            }),
        }
    }

    fn record_count(database: &Database, record_kind: &str) -> i64 {
        database
            .connection()
            .expect("lock database")
            .query_row(
                "SELECT COUNT(*) FROM records WHERE record_kind = ?1",
                [record_kind],
                |row| row.get(0),
            )
            .expect("count records")
    }

    fn initialized_connection() -> Connection {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        initialize(&mut connection).expect("initialize schema");
        connection
    }

    fn version_three_connection_with_research_data() -> Connection {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");
        apply_migration(
            &mut connection,
            1,
            include_str!("../migrations/0001_initial.sql"),
        )
        .expect("apply migration one");
        apply_migration(
            &mut connection,
            2,
            include_str!("../migrations/0002_dataset_identity.sql"),
        )
        .expect("apply migration two");
        apply_migration(
            &mut connection,
            3,
            include_str!("../migrations/0003_dataset_import_sequence.sql"),
        )
        .expect("apply migration three");

        connection
            .execute(
                "INSERT INTO research_datasets (dataset_id) VALUES ('dataset.one'), ('dataset.two')",
                [],
            )
            .expect("insert dataset identities");
        connection
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES
                    ('dataset.one', 'v1', 1, 'xenoscope-research-dataset', 1,
                     '2026-01-01T00:00:00Z', '{}',
                     '[\"source:a\",\"source:b\",\"claim:a\",\"claim:b\",\"evidence:a\",\"evidence:b\",\"evidence:c\",\"evidence:d\",\"evidence:e\",\"evidence:f\"]'),
                    ('dataset.one', 'v2', 2, 'xenoscope-research-dataset', 1,
                     '2026-02-01T00:00:00Z', '{}',
                     '[\"source:a\",\"source:b\",\"claim:a\",\"evidence:a\",\"evidence:b\",\"evidence:c\",\"evidence:d\",\"evidence:e\"]'),
                    ('dataset.two', 'v1', 1, 'xenoscope-research-dataset', 1,
                     '2026-01-01T00:00:00Z', '{}', '[]')",
                [],
            )
            .expect("insert versioned dataset imports");

        for (id, kind) in [
            ("source-a", "source"),
            ("source-b", "source"),
            ("claim-a", "claim"),
            ("claim-b", "claim"),
            ("evidence-a", "evidence_item"),
            ("evidence-b", "evidence_item"),
            ("evidence-c", "evidence_item"),
            ("evidence-d", "evidence_item"),
            ("evidence-e", "evidence_item"),
            ("evidence-f", "evidence_item"),
        ] {
            insert_record(&connection, id, kind);
        }
        for id in ["source-a", "source-b"] {
            connection
                .execute(
                    "INSERT INTO sources (record_id, record_kind, source_kind, verification_status)
                     VALUES (?1, 'source', 'web_page', 'not_assessed')",
                    [id],
                )
                .expect("insert source domain data");
        }
        for (id, source_id) in [("claim-a", "source-a"), ("claim-b", "source-b")] {
            connection
                .execute(
                    "INSERT INTO claims (
                        record_id, record_kind, assertion_text, category, source_id,
                        source_locator_kind, source_locator_json
                     ) VALUES (?1, 'claim', 'Test claim', 'observation', ?2,
                               'page', '{\"kind\":\"page\",\"page\":1}')",
                    params![id, source_id],
                )
                .expect("insert claim domain data");
        }
        for id in [
            "evidence-a",
            "evidence-b",
            "evidence-c",
            "evidence-d",
            "evidence-e",
            "evidence-f",
        ] {
            connection
                .execute(
                    "INSERT INTO evidence_items (
                        record_id, record_kind, source_id, locator_kind, locator_json,
                        evidence_kind, description
                     ) VALUES (?1, 'evidence_item', 'source-a', 'page',
                               '{\"kind\":\"page\",\"page\":1}', 'document', 'Test evidence')",
                    [id],
                )
                .expect("insert evidence domain data");
        }

        for (key, id, kind) in [
            ("source:a", "source-a", "source"),
            ("source:b", "source-b", "source"),
            ("claim:a", "claim-a", "claim"),
            ("claim:b", "claim-b", "claim"),
            ("evidence:a", "evidence-a", "evidence_item"),
            ("evidence:b", "evidence-b", "evidence_item"),
            ("evidence:c", "evidence-c", "evidence_item"),
            ("evidence:d", "evidence-d", "evidence_item"),
            ("evidence:e", "evidence-e", "evidence_item"),
            ("evidence:f", "evidence-f", "evidence_item"),
        ] {
            connection
                .execute(
                    "INSERT INTO dataset_record_mappings (
                        dataset_id, record_key, record_kind, record_id, content_snapshot_json
                     ) VALUES ('dataset.one', ?1, ?2, ?3, '{}')",
                    params![key, kind, id],
                )
                .expect("insert stable record mapping");
        }
        connection
    }

    fn migrated_assessment_test_connection() -> Connection {
        let mut connection = version_three_connection_with_research_data();
        initialize(&mut connection).expect("apply assessment schema migration");
        connection
    }

    fn insert_test_assessment(
        connection: &Connection,
        assessment_id: &str,
        dataset_id: &str,
        dataset_version: &str,
        import_sequence: i64,
        status: &str,
        score: Option<i64>,
        supersedes: Option<&str>,
        revision_reason: Option<&str>,
    ) -> rusqlite::Result<usize> {
        connection.execute(
            "INSERT INTO research_assessments (
                assessment_id, dataset_id, dataset_version, import_sequence, created_at,
                created_by_kind, status, score, scoring_profile_version, scoring_profile_json,
                algorithm_version, input_snapshot_json, supersedes_assessment_id, revision_reason
             ) VALUES (?1, ?2, ?3, ?4, '2026-03-01T00:00:00Z', 'researcher', ?5, ?6,
                       'profile-1', '{\"version\":1}', 'algorithm-1', '{\"inputs\":[]}',
                       ?7, ?8)",
            params![
                assessment_id,
                dataset_id,
                dataset_version,
                import_sequence,
                status,
                score,
                supersedes,
                revision_reason
            ],
        )
    }

    fn seal_test_assessment(connection: &Connection, assessment_id: &str) {
        connection
            .execute(
                "UPDATE research_assessments SET is_sealed = 1 WHERE assessment_id = ?1",
                [assessment_id],
            )
            .expect("seal assessment");
    }

    fn insert_record(connection: &Connection, id: &str, kind: &str) {
        connection
            .execute(
                "INSERT INTO records (record_id, record_kind, created_at, created_by_kind)
                 VALUES (?1, ?2, '2026-01-01T00:00:00Z', 'researcher')",
                (id, kind),
            )
            .expect("insert base record");
    }

    fn version_four_connection() -> Connection {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch("PRAGMA foreign_keys = ON; PRAGMA recursive_triggers = ON;")
            .expect("enable database constraints");
        for &(version, sql) in MIGRATIONS.iter().take(4) {
            apply_migration(&mut connection, version, sql)
                .unwrap_or_else(|error| panic!("apply migration {version}: {error}"));
        }
        connection
    }

    fn seed_decision_records(connection: &Connection) {
        for (id, kind) in [
            ("incident-a", "incident"),
            ("incident-b", "incident"),
            ("incident-c", "incident"),
            ("source-a", "source"),
            ("source-b", "source"),
            ("source-c", "source"),
            ("claim-a", "claim"),
            ("evidence-a", "evidence_item"),
        ] {
            insert_record(connection, id, kind);
        }
    }

    fn insert_test_suggestion(
        connection: &Connection,
        suggestion_id: &str,
        initial_event_id: &str,
        decision_kind: &str,
        record_a_id: &str,
        record_a_kind: &str,
        record_b_id: &str,
        record_b_kind: &str,
        lineage_type: Option<&str>,
    ) {
        let transaction = connection
            .unchecked_transaction()
            .expect("begin suggestion insert transaction");
        transaction
            .execute(
                "INSERT INTO research_suggestions (
                    suggestion_id, decision_kind, record_a_id, record_a_kind,
                    record_b_id, record_b_kind, lineage_type, origin_kind,
                    origin_name, created_at, summary, initial_event_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'researcher',
                           'Local researcher', '2026-11-01T00:00:00Z',
                           'Possible relationship', ?8)",
                params![
                    suggestion_id,
                    decision_kind,
                    record_a_id,
                    record_a_kind,
                    record_b_id,
                    record_b_kind,
                    lineage_type,
                    initial_event_id,
                ],
            )
            .expect("insert suggestion identity");
        transaction
            .execute(
                "INSERT INTO suggestion_events (
                    event_id, suggestion_id, status, occurred_at, actor_kind, actor_name
                 ) VALUES (?1, ?2, 'proposed', '2026-11-01T00:00:00Z',
                           'researcher', 'Local researcher')",
                params![initial_event_id, suggestion_id],
            )
            .expect("insert proposed suggestion event");
        transaction.commit().expect("commit suggestion");
    }

    fn insert_test_decision(
        connection: &Connection,
        decision_id: &str,
        initial_event_id: &str,
        decision_kind: &str,
        record_a_id: &str,
        record_a_kind: &str,
        record_b_id: &str,
        record_b_kind: &str,
        lineage_type: Option<&str>,
        outcome: &str,
    ) {
        let transaction = connection
            .unchecked_transaction()
            .expect("begin decision insert transaction");
        transaction
            .execute(
                "INSERT INTO research_decisions (
                    decision_id, decision_kind, record_a_id, record_a_kind,
                    record_b_id, record_b_kind, lineage_type, created_at,
                    initial_event_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7,
                           '2026-11-01T00:00:00Z', ?8)",
                params![
                    decision_id,
                    decision_kind,
                    record_a_id,
                    record_a_kind,
                    record_b_id,
                    record_b_kind,
                    lineage_type,
                    initial_event_id,
                ],
            )
            .expect("insert decision thread");
        transaction
            .execute(
                "INSERT INTO decision_events (
                    event_id, decision_id, event_kind, outcome, occurred_at,
                    actor_kind, actor_name, rationale
                 ) VALUES (?1, ?2, 'recorded', ?3, '2026-11-01T00:00:00Z',
                           'researcher', 'Local researcher', 'Reviewed the available records')",
                params![initial_event_id, decision_id, outcome],
            )
            .expect("insert initial decision event");
        transaction.commit().expect("commit decision");
    }

    fn suggestion_test_database() -> Database {
        let database = test_database();
        {
            let connection = database.connection().expect("lock test database");
            seed_decision_records(&connection);
        }
        database
    }

    fn incident_pair(first_id: &str, second_id: &str) -> DecisionTargetPair {
        DecisionTargetPair::new(
            DecisionKind::IncidentPair,
            RecordTarget {
                record_id: first_id.to_string(),
                record_kind: DecisionRecordKind::Incident,
            },
            RecordTarget {
                record_id: second_id.to_string(),
                record_kind: DecisionRecordKind::Incident,
            },
            None,
        )
        .expect("valid incident pair")
    }

    fn suggestion_input(
        target: DecisionTargetPair,
        origin_kind: SuggestionOriginKind,
        references: Vec<ResearchReferenceInput>,
    ) -> CreateSuggestionInput {
        CreateSuggestionInput {
            target,
            proposed_outcome: Some(DecisionOutcome::SameEvent),
            origin: SuggestionOrigin {
                kind: origin_kind,
                display_name: None,
                generator_version: None,
            },
            actor: RecordActor {
                kind: ActorKind::Researcher,
                display_name: Some("Initial reviewer".to_string()),
            },
            summary: "Possible relationship between reports".to_string(),
            references,
        }
    }

    fn decision_input(
        target: DecisionTargetPair,
        outcome: DecisionOutcome,
        rationale: &str,
        references: Vec<ResearchReferenceInput>,
    ) -> CreateDecisionInput {
        CreateDecisionInput {
            target,
            outcome,
            rationale: rationale.to_string(),
            origin_suggestion_id: None,
            references,
        }
    }

    fn file_suggestion_test_database() -> (Database, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "xenoscope-read-integrity-{}.sqlite3",
            Uuid::new_v4()
        ));
        let database = Database::open(&path).expect("open file-backed test database");
        {
            let connection = database.connection().expect("lock test database");
            seed_decision_records(&connection);
        }
        (database, path)
    }

    fn insert_corrupt_reference(
        path: &PathBuf,
        table: &str,
        owner_column: &str,
        owner_id: &str,
        reference_id: &str,
        record_id: &str,
        record_kind: &str,
    ) {
        let connection = Connection::open(path).expect("open corruption test connection");
        connection
            .execute_batch("PRAGMA foreign_keys = OFF;")
            .expect("disable foreign keys on isolated corruption connection");
        connection
            .execute(
                &format!(
                    "INSERT INTO {table} (
                        reference_id, {owner_column}, record_id, record_kind, reference_role
                     ) VALUES (?1, ?2, ?3, ?4, 'supports')"
                ),
                params![reference_id, owner_id, record_id, record_kind],
            )
            .expect("insert malformed reference on separate test connection");
    }

    #[test]
    fn suggestion_read_rejects_missing_and_mismatched_reference_record_identity() {
        for (reference_record_id, reference_kind, expected_error) in [
            ("missing-source", "source", "missing record"),
            ("incident-a", "source", "has kind incident"),
        ] {
            let (database, path) = file_suggestion_test_database();
            let suggestion = database
                .create_research_suggestion(suggestion_input(
                    incident_pair("incident-a", "incident-b"),
                    SuggestionOriginKind::Researcher,
                    Vec::new(),
                ))
                .expect("create suggestion");
            insert_corrupt_reference(
                &path,
                "suggestion_references",
                "suggestion_id",
                &suggestion.suggestion_id,
                "malformed-suggestion-reference",
                reference_record_id,
                reference_kind,
            );
            assert!(
                database
                    .get_research_suggestion(&suggestion.suggestion_id)
                    .unwrap_err()
                    .contains(expected_error)
            );
            drop(database);
            fs::remove_file(path).expect("remove temporary database");
        }
    }

    #[test]
    fn decision_read_rejects_missing_and_mismatched_event_reference_identity() {
        for (reference_record_id, reference_kind, expected_error) in [
            ("missing-evidence", "evidence_item", "missing record"),
            ("incident-a", "source", "has kind incident"),
        ] {
            let (database, path) = file_suggestion_test_database();
            let decision = database
                .create_research_decision(decision_input(
                    incident_pair("incident-a", "incident-b"),
                    DecisionOutcome::SameEvent,
                    "Reviewed conclusion",
                    Vec::new(),
                ))
                .expect("create decision");
            insert_corrupt_reference(
                &path,
                "decision_event_references",
                "event_id",
                &decision.history[0].event_id,
                "malformed-decision-reference",
                reference_record_id,
                reference_kind,
            );
            assert!(
                database
                    .get_research_decision(&decision.decision_id)
                    .unwrap_err()
                    .contains(expected_error)
            );
            drop(database);
            fs::remove_file(path).expect("remove temporary database");
        }
    }

    #[test]
    fn resolved_suggestion_provenance_rejects_missing_or_inconsistent_decision_links() {
        let connection = Connection::open_in_memory().expect("open isolated provenance fixture");
        connection
            .execute_batch(
                "CREATE TABLE research_decisions (
                    decision_id TEXT PRIMARY KEY, decision_kind TEXT NOT NULL,
                    record_a_id TEXT NOT NULL, record_a_kind TEXT NOT NULL,
                    record_b_id TEXT NOT NULL, record_b_kind TEXT NOT NULL,
                    lineage_type TEXT, origin_suggestion_id TEXT,
                    origin_suggestion_event_id TEXT, origin_suggestion_status TEXT,
                    initial_event_id TEXT NOT NULL
                 );
                 CREATE TABLE decision_events (
                    event_id TEXT PRIMARY KEY, decision_id TEXT NOT NULL,
                    event_kind TEXT NOT NULL, outcome TEXT, predecessor_event_id TEXT,
                    actor_kind TEXT NOT NULL, actor_name TEXT, rationale TEXT NOT NULL
                 );",
            )
            .expect("create isolated read-validation fixture tables");
        let target = incident_pair("incident-a", "incident-b");
        let resolved_event = RawSuggestionEvent {
            event_id: "resolved-event".to_string(),
            status: "resolved".to_string(),
            predecessor_event_id: Some("suggestion-root".to_string()),
            occurred_at: "2026-11-01T00:00:00Z".to_string(),
            actor_kind: "researcher".to_string(),
            actor_name: Some("Local researcher".to_string()),
            rationale: None,
            replacement_suggestion_id: None,
            decision_id: Some("decision-1".to_string()),
        };
        assert!(
            validate_resolved_suggestion_provenance(
                &connection,
                "suggestion-1",
                &resolved_event,
                &target,
            )
            .unwrap_err()
            .contains("missing decision")
        );

        let insert_decision = |origin_suggestion: &str, origin_event: &str, initial_event: &str| {
            connection
                .execute(
                    "INSERT INTO research_decisions (
                        decision_id, decision_kind, record_a_id, record_a_kind,
                        record_b_id, record_b_kind, origin_suggestion_id,
                        origin_suggestion_event_id, origin_suggestion_status, initial_event_id
                     ) VALUES (
                        'decision-1', 'incident_pair', 'incident-a', 'incident',
                        'incident-b', 'incident', ?1, ?2, 'resolved', ?3
                     )",
                    params![origin_suggestion, origin_event, initial_event],
                )
                .expect("insert provenance decision");
        };
        insert_decision("different-suggestion", "resolved-event", "decision-root");
        assert!(
            validate_resolved_suggestion_provenance(
                &connection,
                "suggestion-1",
                &resolved_event,
                &target,
            )
            .unwrap_err()
            .contains("inconsistent decision provenance")
        );
        connection
            .execute("DELETE FROM research_decisions", [])
            .expect("reset provenance decision");

        insert_decision("suggestion-1", "different-event", "decision-root");
        assert!(
            validate_resolved_suggestion_provenance(
                &connection,
                "suggestion-1",
                &resolved_event,
                &target,
            )
            .unwrap_err()
            .contains("inconsistent decision provenance")
        );
        connection
            .execute("DELETE FROM research_decisions", [])
            .expect("reset provenance decision");

        insert_decision("suggestion-1", "resolved-event", "missing-root");
        assert!(
            validate_resolved_suggestion_provenance(
                &connection,
                "suggestion-1",
                &resolved_event,
                &target,
            )
            .unwrap_err()
            .contains("missing initial event")
        );
        connection
            .execute("DELETE FROM research_decisions", [])
            .expect("reset provenance decision");

        insert_decision("suggestion-1", "resolved-event", "decision-root");
        connection
            .execute(
                "INSERT INTO decision_events (
                    event_id, decision_id, event_kind, outcome, predecessor_event_id,
                    actor_kind, actor_name, rationale
                 ) VALUES (
                    'decision-root', 'decision-1', 'superseded', 'same_event', NULL,
                    'researcher', 'Local researcher', 'Invalid root event kind'
                 )",
                [],
            )
            .expect("insert invalid decision root");
        assert!(
            validate_resolved_suggestion_provenance(
                &connection,
                "suggestion-1",
                &resolved_event,
                &target,
            )
            .unwrap_err()
            .contains("valid recorded root")
        );
    }

    fn conclusion_input(
        outcome: DecisionOutcome,
        rationale: &str,
        references: Vec<ResearchReferenceInput>,
    ) -> DecisionConclusionInput {
        DecisionConclusionInput {
            outcome,
            rationale: rationale.to_string(),
            references,
        }
    }

    #[test]
    fn creates_decisions_for_all_kinds_and_reads_event_references() {
        let database = suggestion_test_database();
        let initial_history_count = record_count(&database, "history");
        let cases = [
            (
                incident_pair("incident-b", "incident-a"),
                DecisionOutcome::RelatedDistinct,
                "incident-a",
                "incident-b",
            ),
            (
                DecisionTargetPair::new(
                    DecisionKind::SourceDependency,
                    RecordTarget {
                        record_id: "source-b".to_string(),
                        record_kind: DecisionRecordKind::Source,
                    },
                    RecordTarget {
                        record_id: "source-a".to_string(),
                        record_kind: DecisionRecordKind::Source,
                    },
                    None,
                )
                .expect("valid dependency pair"),
                DecisionOutcome::Unknown,
                "source-a",
                "source-b",
            ),
            (
                DecisionTargetPair::new(
                    DecisionKind::SourceLineage,
                    RecordTarget {
                        record_id: "source-b".to_string(),
                        record_kind: DecisionRecordKind::Source,
                    },
                    RecordTarget {
                        record_id: "source-a".to_string(),
                        record_kind: DecisionRecordKind::Source,
                    },
                    Some(LineageType::Quotes),
                )
                .expect("valid lineage pair"),
                DecisionOutcome::Quotes,
                "source-b",
                "source-a",
            ),
        ];
        for (target, outcome, expected_a, expected_b) in cases {
            let created = database
                .create_research_decision(decision_input(
                    target,
                    outcome,
                    "Reviewed the available evidence",
                    vec![ResearchReferenceInput {
                        record: RecordTarget {
                            record_id: "evidence-a".to_string(),
                            record_kind: DecisionRecordKind::EvidenceItem,
                        },
                        role: "supports".to_string(),
                        locator: Some(serde_json::json!({"frame": 8})),
                        note: Some("Key observation".to_string()),
                    }],
                ))
                .expect("create reviewed decision");
            assert_eq!(created.target.record_a.record_id, expected_a);
            assert_eq!(created.target.record_b.record_id, expected_b);
            assert!(!created.unresolved);
            assert_eq!(created.history.len(), 1);
            assert_eq!(
                created.current_head.as_ref().unwrap().outcome,
                Some(outcome)
            );
            assert_eq!(
                created
                    .current_head
                    .as_ref()
                    .unwrap()
                    .actor
                    .display_name
                    .as_deref(),
                Some("Local researcher")
            );
            assert_eq!(
                created.current_head.as_ref().unwrap().references[0].locator,
                Some(serde_json::json!({"frame": 8}))
            );
            let read_back = database
                .get_research_decision(&created.decision_id)
                .expect("read decision")
                .expect("decision exists");
            assert_eq!(read_back, created);
        }
        assert_eq!(record_count(&database, "history"), initial_history_count);
    }

    #[test]
    fn rejects_invalid_decision_input_without_partial_rows() {
        let database = suggestion_test_database();
        let mut incompatible = decision_input(
            incident_pair("incident-a", "incident-b"),
            DecisionOutcome::Dependent,
            "Incompatible outcome",
            Vec::new(),
        );
        assert!(
            database
                .create_research_decision(incompatible.clone())
                .unwrap_err()
                .contains("incompatible")
        );
        incompatible.outcome = DecisionOutcome::SameEvent;
        incompatible.rationale = "  \n".to_string();
        assert!(
            database
                .create_research_decision(incompatible)
                .unwrap_err()
                .contains("blank")
        );

        let missing_reference = decision_input(
            incident_pair("incident-a", "incident-b"),
            DecisionOutcome::SameEvent,
            "A reviewed conclusion",
            vec![ResearchReferenceInput {
                record: RecordTarget {
                    record_id: "missing-evidence".to_string(),
                    record_kind: DecisionRecordKind::EvidenceItem,
                },
                role: "supports".to_string(),
                locator: None,
                note: None,
            }],
        );
        assert!(
            database
                .create_research_decision(missing_reference)
                .unwrap_err()
                .contains("referenced record")
        );
        let connection = database.connection().expect("lock database");
        for table in [
            "research_decisions",
            "decision_events",
            "decision_event_references",
        ] {
            let count: i64 = connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .expect("count failed decision writes");
            assert_eq!(count, 0, "table {table} should remain empty");
        }
        drop(connection);
        let valid = database
            .create_research_decision(decision_input(
                incident_pair("incident-a", "incident-b"),
                DecisionOutcome::SameEvent,
                "Reviewed conclusion",
                Vec::new(),
            ))
            .expect("create valid decision");
        assert!(
            database
                .create_research_decision(decision_input(
                    incident_pair("incident-b", "incident-a"),
                    DecisionOutcome::NotSameEvent,
                    "Conflicting independent root",
                    Vec::new(),
                ))
                .unwrap_err()
                .contains("already exists")
        );
        assert_eq!(
            database
                .get_research_decision(&valid.decision_id)
                .expect("read original decision")
                .expect("original decision exists")
                .history
                .len(),
            1
        );
    }

    #[test]
    fn resolves_suggestions_atomically_even_when_reviewed_outcome_differs() {
        let database = suggestion_test_database();
        let suggestion = database
            .create_research_suggestion(suggestion_input(
                incident_pair("incident-a", "incident-b"),
                SuggestionOriginKind::Ai,
                Vec::new(),
            ))
            .expect("create suggestion");
        let expected_suggestion_event = suggestion.events[0].event_id.clone();
        let decision = database
            .resolve_research_suggestion(
                &suggestion.suggestion_id,
                &expected_suggestion_event,
                ResolveSuggestionInput {
                    outcome: DecisionOutcome::NotSameEvent,
                    rationale: "Independent timing establishes distinct incidents".to_string(),
                    references: vec![ResearchReferenceInput {
                        record: RecordTarget {
                            record_id: "claim-a".to_string(),
                            record_kind: DecisionRecordKind::Claim,
                        },
                        role: "supports".to_string(),
                        locator: None,
                        note: None,
                    }],
                },
            )
            .expect("resolve suggestion");
        assert_eq!(
            decision.origin_suggestion_id.as_deref(),
            Some(suggestion.suggestion_id.as_str())
        );
        assert_eq!(
            decision.current_head.as_ref().unwrap().outcome,
            Some(DecisionOutcome::NotSameEvent)
        );
        let resolved = database
            .get_research_suggestion(&suggestion.suggestion_id)
            .expect("read resolved suggestion")
            .expect("suggestion exists");
        assert_eq!(resolved.status, SuggestionStatus::Resolved);
        assert_eq!(resolved.events.len(), 2);
        assert_eq!(
            resolved.events[1].decision_id.as_deref(),
            Some(decision.decision_id.as_str())
        );
        assert_eq!(
            resolved.events[1].predecessor_event_id.as_deref(),
            Some(expected_suggestion_event.as_str())
        );
        assert!(
            database
                .resolve_research_suggestion(
                    &suggestion.suggestion_id,
                    &expected_suggestion_event,
                    ResolveSuggestionInput {
                        outcome: DecisionOutcome::SameEvent,
                        rationale: "Duplicate conversion".to_string(),
                        references: Vec::new(),
                    },
                )
                .unwrap_err()
                .contains("terminal")
        );
    }

    #[test]
    fn rejects_stale_and_terminal_suggestion_resolution_without_partial_decisions() {
        let database = suggestion_test_database();
        let stale = database
            .create_research_suggestion(suggestion_input(
                incident_pair("incident-a", "incident-b"),
                SuggestionOriginKind::System,
                Vec::new(),
            ))
            .expect("create stale suggestion");
        let input = || ResolveSuggestionInput {
            outcome: DecisionOutcome::SameEvent,
            rationale: "Reviewed conclusion".to_string(),
            references: Vec::new(),
        };
        assert!(
            database
                .resolve_research_suggestion(&stale.suggestion_id, "stale-event", input())
                .unwrap_err()
                .contains("Stale")
        );
        database
            .dismiss_research_suggestion(
                &stale.suggestion_id,
                &stale.events[0].event_id,
                "Dismissed instead",
            )
            .expect("dismiss suggestion");
        assert!(
            database
                .resolve_research_suggestion(
                    &stale.suggestion_id,
                    &stale.events[0].event_id,
                    input(),
                )
                .unwrap_err()
                .contains("terminal")
        );
        let count: i64 = database
            .connection()
            .expect("lock database")
            .query_row("SELECT COUNT(*) FROM research_decisions", [], |row| {
                row.get(0)
            })
            .expect("count decisions");
        assert_eq!(count, 0);

        let superseded = database
            .create_research_suggestion(suggestion_input(
                incident_pair("incident-a", "incident-c"),
                SuggestionOriginKind::System,
                Vec::new(),
            ))
            .expect("create suggestion to supersede");
        let replacement = database
            .create_research_suggestion(suggestion_input(
                incident_pair("incident-a", "incident-c"),
                SuggestionOriginKind::Researcher,
                Vec::new(),
            ))
            .expect("create replacement suggestion");
        database
            .supersede_research_suggestion(
                &superseded.suggestion_id,
                &superseded.events[0].event_id,
                &replacement.suggestion_id,
                "A revised suggestion replaces this one",
            )
            .expect("supersede suggestion");
        assert!(
            database
                .resolve_research_suggestion(
                    &superseded.suggestion_id,
                    &superseded.events[0].event_id,
                    input(),
                )
                .unwrap_err()
                .contains("terminal")
        );
    }

    #[test]
    fn failed_suggestion_resolution_rolls_back_both_events_and_decision() {
        let database = suggestion_test_database();
        let suggestion = database
            .create_research_suggestion(suggestion_input(
                incident_pair("incident-a", "incident-b"),
                SuggestionOriginKind::Ai,
                Vec::new(),
            ))
            .expect("create suggestion");
        let result = database.resolve_research_suggestion(
            &suggestion.suggestion_id,
            &suggestion.events[0].event_id,
            ResolveSuggestionInput {
                outcome: DecisionOutcome::SameEvent,
                rationale: "Reviewed but reference does not exist".to_string(),
                references: vec![ResearchReferenceInput {
                    record: RecordTarget {
                        record_id: "missing-claim".to_string(),
                        record_kind: DecisionRecordKind::Claim,
                    },
                    role: "supports".to_string(),
                    locator: None,
                    note: None,
                }],
            },
        );
        assert!(result.unwrap_err().contains("referenced record"));
        let suggestion = database
            .get_research_suggestion(&suggestion.suggestion_id)
            .expect("read suggestion after failed conversion")
            .expect("suggestion remains");
        assert_eq!(suggestion.status, SuggestionStatus::Proposed);
        assert_eq!(suggestion.events.len(), 1);
        let decisions: i64 = database
            .connection()
            .expect("lock database")
            .query_row("SELECT COUNT(*) FROM research_decisions", [], |row| {
                row.get(0)
            })
            .expect("count rolled-back decisions");
        assert_eq!(decisions, 0);
    }

    #[test]
    fn competing_suggestion_resolutions_with_one_expected_head_allow_only_one_write() {
        let (database, path) = file_suggestion_test_database();
        let database = Arc::new(database);
        let suggestion = database
            .create_research_suggestion(suggestion_input(
                incident_pair("incident-a", "incident-b"),
                SuggestionOriginKind::Ai,
                Vec::new(),
            ))
            .expect("create suggestion");
        let second_database =
            Arc::new(Database::open(&path).expect("open second database connection"));
        let barrier = Arc::new(Barrier::new(3));
        let workers = [Arc::clone(&database), second_database].map(|database| {
            let barrier = Arc::clone(&barrier);
            let suggestion_id = suggestion.suggestion_id.clone();
            let expected_event_id = suggestion.events[0].event_id.clone();
            std::thread::spawn(move || {
                barrier.wait();
                database.resolve_research_suggestion(
                    &suggestion_id,
                    &expected_event_id,
                    ResolveSuggestionInput {
                        outcome: DecisionOutcome::SameEvent,
                        rationale: "Reviewed conclusion from competing writer".to_string(),
                        references: Vec::new(),
                    },
                )
            })
        });
        barrier.wait();
        let results = workers.map(|worker| worker.join().expect("join suggestion resolver"));
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
        let resolved = database
            .get_research_suggestion(&suggestion.suggestion_id)
            .expect("read competed suggestion")
            .expect("suggestion exists");
        assert_eq!(resolved.status, SuggestionStatus::Resolved);
        assert_eq!(resolved.events.len(), 2);
        let decision_count: i64 = database
            .connection()
            .expect("lock database")
            .query_row("SELECT COUNT(*) FROM research_decisions", [], |row| {
                row.get(0)
            })
            .expect("count created decisions");
        assert_eq!(decision_count, 1);
        drop(database);
        fs::remove_file(path).expect("remove temporary database");
    }

    #[test]
    fn supersedes_reverses_and_records_a_new_conclusion_after_reversal() {
        let database = suggestion_test_database();
        let created = database
            .create_research_decision(decision_input(
                incident_pair("incident-a", "incident-b"),
                DecisionOutcome::SameEvent,
                "Initial review",
                Vec::new(),
            ))
            .expect("create decision");
        let superseded = database
            .supersede_research_decision(
                &created.decision_id,
                &created.history[0].event_id,
                conclusion_input(
                    DecisionOutcome::RelatedDistinct,
                    "New evidence shows related but distinct incidents",
                    Vec::new(),
                ),
            )
            .expect("supersede decision");
        assert_eq!(superseded.history.len(), 2);
        assert_eq!(
            superseded.history[1].event_kind,
            DecisionEventKind::Superseded
        );
        let reversed = database
            .reverse_research_decision(
                &created.decision_id,
                &superseded.history[1].event_id,
                "The evidence is insufficient to maintain a conclusion",
            )
            .expect("reverse decision");
        assert!(reversed.unresolved);
        assert_eq!(reversed.current_head.as_ref().unwrap().outcome, None);
        let recorded = database
            .record_research_decision_after_reversal(
                &created.decision_id,
                &reversed.history[2].event_id,
                conclusion_input(
                    DecisionOutcome::NotSameEvent,
                    "A new review establishes distinct incidents",
                    Vec::new(),
                ),
            )
            .expect("record new conclusion");
        assert!(!recorded.unresolved);
        assert_eq!(recorded.history.len(), 4);
        assert_eq!(recorded.history[3].event_kind, DecisionEventKind::Recorded);
        assert_eq!(
            recorded.history[3].predecessor_event_id.as_deref(),
            Some(reversed.history[2].event_id.as_str())
        );
        assert_eq!(
            recorded.history[3].outcome,
            Some(DecisionOutcome::NotSameEvent)
        );
        assert!(
            database
                .reverse_research_decision(
                    &created.decision_id,
                    &reversed.history[2].event_id,
                    "Stale retry",
                )
                .unwrap_err()
                .contains("Stale")
        );
        assert!(
            database
                .record_research_decision_after_reversal(
                    &created.decision_id,
                    &recorded.history[3].event_id,
                    conclusion_input(
                        DecisionOutcome::SameEvent,
                        "Invalid second conclusion",
                        Vec::new(),
                    ),
                )
                .unwrap_err()
                .contains("invalid after")
        );
    }

    #[test]
    fn decision_reader_rejects_disconnected_persisted_events() {
        let database = suggestion_test_database();
        let created = database
            .create_research_decision(decision_input(
                incident_pair("incident-a", "incident-b"),
                DecisionOutcome::SameEvent,
                "Initial review",
                Vec::new(),
            ))
            .expect("create decision");
        {
            let connection = database.connection().expect("lock database");
            connection
                .execute_batch(
                    "PRAGMA foreign_keys = OFF;
                     DROP TRIGGER decision_events_validate_insert;",
                )
                .expect("allow malformed fixture insertion");
            connection
                .execute(
                    "INSERT INTO decision_events (
                        event_id, decision_id, event_kind, outcome, predecessor_event_id,
                        occurred_at, actor_kind, actor_name, rationale
                     ) VALUES (
                        'disconnected-event', ?1, 'superseded', 'not_same_event',
                        'missing-predecessor', '2026-11-01T00:00:00Z',
                        'researcher', 'Local researcher', 'Malformed disconnected event'
                     )",
                    [&created.decision_id],
                )
                .expect("insert disconnected persisted event");
        }
        assert!(
            database
                .get_research_decision(&created.decision_id)
                .unwrap_err()
                .contains("disconnected or cyclic")
        );
    }

    #[test]
    fn competing_decision_supersessions_with_one_expected_head_allow_only_one_write() {
        let database_path =
            std::env::temp_dir().join(format!("xenoscope-decision-{}.sqlite3", Uuid::new_v4()));
        let database = Arc::new(Database::open(&database_path).expect("open test database"));
        {
            let connection = database.connection().expect("lock first database");
            seed_decision_records(&connection);
        }
        let created = database
            .create_research_decision(decision_input(
                incident_pair("incident-a", "incident-b"),
                DecisionOutcome::SameEvent,
                "Initial review",
                Vec::new(),
            ))
            .expect("create decision");
        let second_database =
            Arc::new(Database::open(&database_path).expect("open second database connection"));
        let barrier = Arc::new(Barrier::new(3));
        let workers = [Arc::clone(&database), second_database].map(|database| {
            let barrier = Arc::clone(&barrier);
            let decision_id = created.decision_id.clone();
            let expected_event_id = created.history[0].event_id.clone();
            std::thread::spawn(move || {
                barrier.wait();
                database.supersede_research_decision(
                    &decision_id,
                    &expected_event_id,
                    conclusion_input(
                        DecisionOutcome::NotSameEvent,
                        "Competing reviewed conclusion",
                        Vec::new(),
                    ),
                )
            })
        });
        barrier.wait();
        let results = workers.map(|worker| worker.join().expect("join decision writer"));
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
        let decision = database
            .get_research_decision(&created.decision_id)
            .expect("read competed decision")
            .expect("decision exists");
        assert_eq!(decision.history.len(), 2);
        drop(decision);
        drop(database);
        fs::remove_file(database_path).expect("remove temporary database");
    }

    #[test]
    fn creates_suggestions_atomically_and_reads_origin_actor_and_references() {
        let database = suggestion_test_database();
        let initial_history_count = record_count(&database, "history");
        let references = vec![
            ResearchReferenceInput {
                record: RecordTarget {
                    record_id: "source-a".to_string(),
                    record_kind: DecisionRecordKind::Source,
                },
                role: "basis".to_string(),
                locator: Some(serde_json::json!({"page": 2})),
                note: Some("Relevant passage".to_string()),
            },
            ResearchReferenceInput {
                record: RecordTarget {
                    record_id: "evidence-a".to_string(),
                    record_kind: DecisionRecordKind::EvidenceItem,
                },
                role: "context".to_string(),
                locator: None,
                note: None,
            },
        ];

        for origin_kind in [
            SuggestionOriginKind::Researcher,
            SuggestionOriginKind::System,
            SuggestionOriginKind::Ai,
        ] {
            let mut input = suggestion_input(
                DecisionTargetPair {
                    decision_kind: DecisionKind::IncidentPair,
                    record_a: RecordTarget {
                        record_id: "incident-b".to_string(),
                        record_kind: DecisionRecordKind::Incident,
                    },
                    record_b: RecordTarget {
                        record_id: "incident-a".to_string(),
                        record_kind: DecisionRecordKind::Incident,
                    },
                    lineage_type: None,
                },
                origin_kind,
                references.clone(),
            );
            input.origin.display_name = Some("Origin agent".to_string());
            input.origin.generator_version = Some("engine-1".to_string());
            let suggestion = database
                .create_research_suggestion(input)
                .expect("create research suggestion");

            assert_eq!(suggestion.target.record_a.record_id, "incident-a");
            assert_eq!(suggestion.target.record_b.record_id, "incident-b");
            assert_eq!(suggestion.status, SuggestionStatus::Proposed);
            assert_eq!(suggestion.events.len(), 1);
            assert_eq!(suggestion.events[0].status, SuggestionStatus::Proposed);
            assert_eq!(suggestion.events[0].actor.kind, ActorKind::Researcher);
            assert_eq!(
                suggestion.events[0].actor.display_name.as_deref(),
                Some("Initial reviewer")
            );
            assert_eq!(suggestion.origin.kind, origin_kind);
            assert_eq!(
                suggestion.origin.display_name.as_deref(),
                Some("Origin agent")
            );
            assert_eq!(
                suggestion.origin.generator_version.as_deref(),
                Some("engine-1")
            );
            assert_eq!(suggestion.references.len(), 2);
            assert_eq!(
                suggestion
                    .references
                    .iter()
                    .find(|reference| reference.role == "basis")
                    .expect("basis reference")
                    .locator,
                Some(serde_json::json!({"page": 2}))
            );
            assert_eq!(
                suggestion
                    .references
                    .iter()
                    .find(|reference| reference.role == "context")
                    .expect("context reference")
                    .locator,
                None
            );
        }
        assert_eq!(record_count(&database, "history"), initial_history_count);
    }

    #[test]
    fn creates_lineage_suggestions_without_reordering_the_directed_pair() {
        let database = suggestion_test_database();
        let target = DecisionTargetPair::new(
            DecisionKind::SourceLineage,
            RecordTarget {
                record_id: "source-b".to_string(),
                record_kind: DecisionRecordKind::Source,
            },
            RecordTarget {
                record_id: "source-a".to_string(),
                record_kind: DecisionRecordKind::Source,
            },
            Some(LineageType::Quotes),
        )
        .expect("valid directed lineage pair");
        let mut input = suggestion_input(target, SuggestionOriginKind::Ai, Vec::new());
        input.proposed_outcome = Some(DecisionOutcome::Quotes);
        let suggestion = database
            .create_research_suggestion(input)
            .expect("create lineage suggestion");

        assert_eq!(suggestion.target.record_a.record_id, "source-b");
        assert_eq!(suggestion.target.record_b.record_id, "source-a");
        assert_eq!(suggestion.target.lineage_type, Some(LineageType::Quotes));
        assert_eq!(suggestion.proposed_outcome, Some(DecisionOutcome::Quotes));
    }

    #[test]
    fn failed_suggestion_creation_leaves_no_partial_identity_reference_or_event() {
        let database = suggestion_test_database();
        let mut missing_reference = suggestion_input(
            incident_pair("incident-a", "incident-b"),
            SuggestionOriginKind::Researcher,
            vec![ResearchReferenceInput {
                record: RecordTarget {
                    record_id: "missing-source".to_string(),
                    record_kind: DecisionRecordKind::Source,
                },
                role: "basis".to_string(),
                locator: None,
                note: None,
            }],
        );
        missing_reference.proposed_outcome = Some(DecisionOutcome::SameEvent);
        assert!(
            database
                .create_research_suggestion(missing_reference)
                .unwrap_err()
                .contains("referenced record")
        );

        let wrong_reference_kind = suggestion_input(
            incident_pair("incident-a", "incident-b"),
            SuggestionOriginKind::Researcher,
            vec![ResearchReferenceInput {
                record: RecordTarget {
                    record_id: "incident-a".to_string(),
                    record_kind: DecisionRecordKind::Source,
                },
                role: "basis".to_string(),
                locator: None,
                note: None,
            }],
        );
        assert!(
            database
                .create_research_suggestion(wrong_reference_kind)
                .unwrap_err()
                .contains("referenced record")
        );

        let mut incompatible_outcome = suggestion_input(
            incident_pair("incident-a", "incident-b"),
            SuggestionOriginKind::Researcher,
            Vec::new(),
        );
        incompatible_outcome.proposed_outcome = Some(DecisionOutcome::Dependent);
        assert!(
            database
                .create_research_suggestion(incompatible_outcome)
                .unwrap_err()
                .contains("incompatible")
        );
        let mut blank_actor_name = suggestion_input(
            incident_pair("incident-a", "incident-b"),
            SuggestionOriginKind::Researcher,
            Vec::new(),
        );
        blank_actor_name.actor.display_name = Some("  ".to_string());
        assert!(
            database
                .create_research_suggestion(blank_actor_name)
                .unwrap_err()
                .contains("actor name")
        );

        let connection = database.connection().expect("lock database");
        for table in [
            "research_suggestions",
            "suggestion_references",
            "suggestion_events",
        ] {
            let count: i64 = connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .expect("count failed creation rows");
            assert_eq!(count, 0, "table {table} should remain empty");
        }
    }

    #[test]
    fn dismisses_suggestion_with_expected_head_and_preserves_ordered_history() {
        let database = suggestion_test_database();
        let created = database
            .create_research_suggestion(suggestion_input(
                incident_pair("incident-a", "incident-b"),
                SuggestionOriginKind::Ai,
                Vec::new(),
            ))
            .expect("create suggestion");
        let root_id = created.events[0].event_id.clone();

        assert!(
            database
                .dismiss_research_suggestion(
                    &created.suggestion_id,
                    "stale-event",
                    "Stale expected head",
                )
                .unwrap_err()
                .contains("Stale")
        );
        for rationale in ["", "  \n"] {
            assert!(
                database
                    .dismiss_research_suggestion(&created.suggestion_id, &root_id, rationale,)
                    .unwrap_err()
                    .contains("blank")
            );
        }

        let dismissed = database
            .dismiss_research_suggestion(
                &created.suggestion_id,
                &root_id,
                "The reports differ in time and location",
            )
            .expect("dismiss suggestion");
        assert_eq!(dismissed.status, SuggestionStatus::Dismissed);
        assert_eq!(dismissed.events.len(), 2);
        assert_eq!(dismissed.events[0].event_id, root_id);
        assert_eq!(
            dismissed.events[1].predecessor_event_id.as_deref(),
            Some(root_id.as_str())
        );
        assert_eq!(dismissed.events[1].actor.kind, ActorKind::Researcher);
        assert_eq!(
            dismissed.events[1].actor.display_name.as_deref(),
            Some("Local researcher")
        );

        assert!(
            database
                .dismiss_research_suggestion(&created.suggestion_id, &root_id, "Stale retry",)
                .unwrap_err()
                .contains("terminal")
        );
    }

    #[test]
    fn supersedes_only_with_a_proposed_matching_replacement() {
        let database = suggestion_test_database();
        let original = database
            .create_research_suggestion(suggestion_input(
                incident_pair("incident-a", "incident-b"),
                SuggestionOriginKind::System,
                Vec::new(),
            ))
            .expect("create original suggestion");
        let replacement = database
            .create_research_suggestion(suggestion_input(
                incident_pair("incident-b", "incident-a"),
                SuggestionOriginKind::Researcher,
                Vec::new(),
            ))
            .expect("create replacement suggestion");
        let incompatible = database
            .create_research_suggestion(suggestion_input(
                incident_pair("incident-a", "incident-c"),
                SuggestionOriginKind::Researcher,
                Vec::new(),
            ))
            .expect("create incompatible suggestion");
        let root_id = original.events[0].event_id.clone();

        for rationale in ["", "  \n"] {
            assert!(
                database
                    .supersede_research_suggestion(
                        &original.suggestion_id,
                        &root_id,
                        &replacement.suggestion_id,
                        rationale,
                    )
                    .unwrap_err()
                    .contains("blank")
            );
        }
        assert!(
            database
                .supersede_research_suggestion(
                    &original.suggestion_id,
                    &root_id,
                    &original.suggestion_id,
                    "Self replacement",
                )
                .unwrap_err()
                .contains("cannot replace itself")
        );
        assert!(
            database
                .supersede_research_suggestion(
                    &original.suggestion_id,
                    &root_id,
                    "missing-replacement",
                    "Missing replacement",
                )
                .unwrap_err()
                .contains("does not exist")
        );
        assert!(
            database
                .supersede_research_suggestion(
                    &original.suggestion_id,
                    &root_id,
                    &incompatible.suggestion_id,
                    "Different question",
                )
                .unwrap_err()
                .contains("different question")
        );
        assert!(
            database
                .dismiss_research_suggestion(
                    &replacement.suggestion_id,
                    &replacement.events[0].event_id,
                    "No longer relevant",
                )
                .is_ok()
        );
        assert!(
            database
                .supersede_research_suggestion(
                    &original.suggestion_id,
                    &root_id,
                    &replacement.suggestion_id,
                    "Replacement is terminal",
                )
                .unwrap_err()
                .contains("must still be proposed")
        );

        let replacement = database
            .create_research_suggestion(suggestion_input(
                incident_pair("incident-a", "incident-b"),
                SuggestionOriginKind::Ai,
                Vec::new(),
            ))
            .expect("create live replacement");
        let superseded = database
            .supersede_research_suggestion(
                &original.suggestion_id,
                &root_id,
                &replacement.suggestion_id,
                "Refined hypothesis is more precise",
            )
            .expect("supersede suggestion");
        assert_eq!(superseded.status, SuggestionStatus::Superseded);
        assert_eq!(
            superseded.events[1].replacement_suggestion_id.as_deref(),
            Some(replacement.suggestion_id.as_str())
        );
        assert_eq!(superseded.events.len(), 2);
    }

    #[test]
    fn competing_suggestion_dismissals_with_one_expected_head_allow_only_one_write() {
        let database_path =
            std::env::temp_dir().join(format!("xenoscope-suggestion-{}.sqlite3", Uuid::new_v4()));
        let database = Arc::new(Database::open(&database_path).expect("open test database"));
        {
            let connection = database.connection().expect("lock first database");
            seed_decision_records(&connection);
        }
        let created = database
            .create_research_suggestion(suggestion_input(
                incident_pair("incident-a", "incident-b"),
                SuggestionOriginKind::Researcher,
                Vec::new(),
            ))
            .expect("create suggestion");
        let second_database =
            Arc::new(Database::open(&database_path).expect("open second database connection"));
        let barrier = Arc::new(Barrier::new(3));
        let workers = [
            (Arc::clone(&database), "First review"),
            (Arc::clone(&second_database), "Competing review"),
        ]
        .into_iter()
        .map(|(database, rationale)| {
            let barrier = Arc::clone(&barrier);
            let suggestion_id = created.suggestion_id.clone();
            let expected_event_id = created.events[0].event_id.clone();
            std::thread::spawn(move || {
                barrier.wait();
                database.dismiss_research_suggestion(&suggestion_id, &expected_event_id, rationale)
            })
        })
        .collect::<Vec<_>>();
        barrier.wait();
        let results = workers
            .into_iter()
            .map(|worker| worker.join().expect("join suggestion transition"))
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
        let persisted = database
            .get_research_suggestion(&created.suggestion_id)
            .expect("read result")
            .expect("suggestion exists");
        assert_eq!(persisted.events.len(), 2);
        assert_eq!(persisted.status, SuggestionStatus::Dismissed);
        drop(second_database);
        drop(database);
        fs::remove_file(database_path).expect("remove temporary suggestion database");
    }

    #[test]
    fn malformed_persisted_suggestion_data_returns_an_error() {
        let database = suggestion_test_database();
        let created = database
            .create_research_suggestion(suggestion_input(
                incident_pair("incident-a", "incident-b"),
                SuggestionOriginKind::Researcher,
                Vec::new(),
            ))
            .expect("create suggestion");
        {
            let connection = database.connection().expect("lock database");
            connection
                .execute_batch(
                    "DROP TRIGGER research_suggestions_no_update;
                     PRAGMA ignore_check_constraints = ON;",
                )
                .expect("allow malformed test data");
            connection
                .execute(
                    "UPDATE research_suggestions SET origin_kind = 'unexpected'
                     WHERE suggestion_id = ?1",
                    [&created.suggestion_id],
                )
                .expect("corrupt stored origin kind");
        }
        assert!(
            database
                .get_research_suggestion(&created.suggestion_id)
                .unwrap_err()
                .contains("Stored suggestion origin kind is invalid")
        );
    }

    #[test]
    fn migrations_are_repeatable_and_do_not_seed_records_or_dataset_imports() {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        initialize(&mut connection).expect("first initialization");
        initialize(&mut connection).expect("repeat initialization");

        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version");
        let record_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))
            .expect("count records");
        let dataset_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM research_datasets", [], |row| {
                row.get(0)
            })
            .expect("count datasets");
        let import_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM dataset_imports", [], |row| row.get(0))
            .expect("count imports");
        let mapping_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM dataset_record_mappings", [], |row| {
                row.get(0)
            })
            .expect("count mappings");

        assert_eq!(version, LATEST_SCHEMA_VERSION);
        assert_eq!(record_count, 0);
        assert_eq!(dataset_count, 0);
        assert_eq!(import_count, 0);
        assert_eq!(mapping_count, 0);
    }

    #[test]
    fn migration_two_preserves_version_one_records_and_history() {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");
        apply_migration(
            &mut connection,
            1,
            include_str!("../migrations/0001_initial.sql"),
        )
        .expect("apply version one migration");

        insert_record(&connection, "source-before-upgrade", "source");
        connection
            .execute(
                "INSERT INTO sources (record_id, record_kind, source_kind, verification_status)
                 VALUES ('source-before-upgrade', 'source', 'web_page', 'verified')",
                [],
            )
            .expect("insert version one source");
        insert_record(&connection, "history-before-upgrade", "history");
        connection
            .execute(
                "INSERT INTO record_history (
                    record_id, record_kind, subject_record_id, subject_record_kind,
                    event_kind, summary
                 ) VALUES (
                    'history-before-upgrade', 'history', 'source-before-upgrade',
                    'source', 'record_created', 'Created before upgrade'
                 )",
                [],
            )
            .expect("insert version one history");

        initialize(&mut connection).expect("upgrade to version two");

        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version");
        let source_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sources WHERE record_id = 'source-before-upgrade')",
                [],
                |row| row.get(0),
            )
            .expect("check preserved source");
        let history_exists: bool = connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM record_history
                    WHERE record_id = 'history-before-upgrade'
                )",
                [],
                |row| row.get(0),
            )
            .expect("check preserved history");

        assert_eq!(version, LATEST_SCHEMA_VERSION);
        assert!(source_exists);
        assert!(history_exists);
    }

    fn apply_migrations_through_two(connection: &mut Connection) {
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");
        apply_migration(
            connection,
            1,
            include_str!("../migrations/0001_initial.sql"),
        )
        .expect("apply migration one");
        apply_migration(
            connection,
            2,
            include_str!("../migrations/0002_dataset_identity.sql"),
        )
        .expect("apply migration two");
    }

    fn insert_dataset_import(
        connection: &Connection,
        dataset_id: &str,
        dataset_version: &str,
        imported_at: &str,
    ) {
        connection
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES (
                    ?1, ?2, 'xenoscope-research-dataset', 1, ?3,
                    '{\"preserved\":true}', '[\"stable:key\"]'
                 )",
                params![dataset_id, dataset_version, imported_at],
            )
            .expect("insert existing import ledger row");
    }

    #[test]
    fn migration_three_preserves_ledger_rows_and_assigns_deterministic_sequences() {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        apply_migrations_through_two(&mut connection);
        connection
            .execute(
                "INSERT INTO research_datasets (dataset_id) VALUES ('dataset.one'), ('dataset.two')",
                [],
            )
            .expect("insert dataset identities");
        // These tied versions are deliberately inserted in reverse lexical order. Migration
        // 0003's version tie-breaker is deterministic, but cannot recover insertion order.
        insert_dataset_import(&connection, "dataset.one", "v3", "2026-02-01T00:00:00Z");
        insert_dataset_import(&connection, "dataset.one", "v1", "2026-01-01T00:00:00Z");
        insert_dataset_import(&connection, "dataset.one", "v2", "2026-02-01T00:00:00Z");
        insert_dataset_import(&connection, "dataset.two", "v1", "2026-01-15T00:00:00Z");

        initialize(&mut connection).expect("apply migration three");

        let rows: Vec<(String, String, i64, String, String, String)> = {
            let mut statement = connection
                .prepare(
                    "SELECT dataset_id, dataset_version, import_sequence, imported_at,
                            metadata_json, record_keys_json
                     FROM dataset_imports
                     ORDER BY dataset_id, import_sequence",
                )
                .expect("prepare imported rows query");
            statement
                .query_map([], |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                })
                .expect("query preserved imports")
                .collect::<Result<_, _>>()
                .expect("collect preserved imports")
        };
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version");

        assert_eq!(
            rows,
            vec![
                (
                    "dataset.one".to_string(),
                    "v1".to_string(),
                    1,
                    "2026-01-01T00:00:00Z".to_string(),
                    "{\"preserved\":true}".to_string(),
                    "[\"stable:key\"]".to_string()
                ),
                (
                    "dataset.one".to_string(),
                    "v2".to_string(),
                    2,
                    "2026-02-01T00:00:00Z".to_string(),
                    "{\"preserved\":true}".to_string(),
                    "[\"stable:key\"]".to_string()
                ),
                (
                    "dataset.one".to_string(),
                    "v3".to_string(),
                    3,
                    "2026-02-01T00:00:00Z".to_string(),
                    "{\"preserved\":true}".to_string(),
                    "[\"stable:key\"]".to_string()
                ),
                (
                    "dataset.two".to_string(),
                    "v1".to_string(),
                    1,
                    "2026-01-15T00:00:00Z".to_string(),
                    "{\"preserved\":true}".to_string(),
                    "[\"stable:key\"]".to_string()
                ),
            ]
        );
        let latest_version: String = connection
            .query_row(
                "SELECT dataset_version FROM dataset_imports
                 WHERE dataset_id = 'dataset.one'
                 ORDER BY import_sequence DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .expect("read deterministic migration fallback latest version");
        assert_eq!(latest_version, "v3");
        assert_eq!(version, LATEST_SCHEMA_VERSION);
    }

    #[test]
    fn import_sequence_must_be_positive_and_unique_per_dataset() {
        let connection = initialized_connection();
        connection
            .execute(
                "INSERT INTO research_datasets (dataset_id) VALUES ('dataset.one'), ('dataset.two')",
                [],
            )
            .expect("insert dataset identities");
        connection
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES (
                    'dataset.one', 'v1', 1, 'xenoscope-research-dataset', 1,
                    '2026-01-01T00:00:00Z', '{}', '[]'
                 )",
                [],
            )
            .expect("insert first sequence");
        connection
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES (
                    'dataset.one', 'v2', 1, 'xenoscope-research-dataset', 1,
                    '2026-01-02T00:00:00Z', '{}', '[]'
                 )",
                [],
            )
            .expect_err("sequence must be unique within one dataset");
        connection
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES (
                    'dataset.one', 'v0', 0, 'xenoscope-research-dataset', 1,
                    '2025-12-31T00:00:00Z', '{}', '[]'
                 )",
                [],
            )
            .expect_err("sequence must be positive");
        connection
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES (
                    'dataset.two', 'v1', 1, 'xenoscope-research-dataset', 1,
                    '2026-01-01T00:00:00Z', '{}', '[]'
                 )",
                [],
            )
            .expect("different datasets may reuse a sequence");
    }

    #[test]
    fn failed_sequence_migration_rolls_back_schema_and_user_version() {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        apply_migrations_through_two(&mut connection);
        let failing_sql = format!(
            "{}\nINVALID SQL;",
            include_str!("../migrations/0003_dataset_import_sequence.sql")
        );

        let error = apply_migration(&mut connection, 3, &failing_sql)
            .expect_err("invalid migration suffix should fail");

        assert!(error.contains("rolled back"));
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version");
        let sequence_column_exists: bool = connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM pragma_table_info('dataset_imports')
                    WHERE name = 'import_sequence'
                )",
                [],
                |row| row.get(0),
            )
            .expect("check sequence column rollback");
        assert_eq!(version, 2);
        assert!(!sequence_column_exists);
    }

    #[test]
    fn failed_dataset_identity_migration_rolls_back_tables_and_user_version() {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");
        apply_migration(
            &mut connection,
            1,
            include_str!("../migrations/0001_initial.sql"),
        )
        .expect("apply version one migration");
        let failing_sql = format!(
            "{}\nINVALID SQL;",
            include_str!("../migrations/0002_dataset_identity.sql")
        );

        let error = apply_migration(&mut connection, 2, &failing_sql)
            .expect_err("invalid migration suffix should fail");

        assert!(error.contains("rolled back"));
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version");
        let table_exists: bool = connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM sqlite_master WHERE type = 'table'
                    AND name = 'research_datasets'
                )",
                [],
                |row| row.get(0),
            )
            .expect("check migration table rollback");

        assert_eq!(version, 1);
        assert!(!table_exists);
    }

    #[test]
    fn migration_four_preserves_v3_research_data_and_adds_empty_assessment_tables() {
        let mut connection = version_three_connection_with_research_data();
        let version_three: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read v3 schema version");
        let before: (i64, i64, i64, i64, i64) = connection
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM records),
                    (SELECT COUNT(*) FROM sources),
                    (SELECT COUNT(*) FROM claims),
                    (SELECT COUNT(*) FROM dataset_imports),
                    (SELECT COUNT(*) FROM dataset_record_mappings)",
                [],
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
            .expect("count v3 data");
        let existing_rows_before: (String, String, String, String) = connection
            .query_row(
                "SELECT
                    (SELECT source_kind || ':' || verification_status
                     FROM sources WHERE record_id = 'source-a'),
                    (SELECT assertion_text || ':' || category || ':' || source_locator_json
                     FROM claims WHERE record_id = 'claim-a'),
                    (SELECT dataset_version || ':' || imported_at || ':' || metadata_json ||
                            ':' || record_keys_json
                     FROM dataset_imports
                     WHERE dataset_id = 'dataset.one' AND dataset_version = 'v2'),
                    (SELECT record_kind || ':' || record_id || ':' || content_snapshot_json
                     FROM dataset_record_mappings
                     WHERE dataset_id = 'dataset.one' AND record_key = 'claim:a')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("read representative v3 row values");
        assert_eq!(version_three, 3);

        apply_migration(
            &mut connection,
            4,
            include_str!("../migrations/0004_research_assessments.sql"),
        )
        .expect("migrate v3 database to v4");

        let version_four: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read v4 schema version");
        let after: (i64, i64, i64, i64, i64) = connection
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM records),
                    (SELECT COUNT(*) FROM sources),
                    (SELECT COUNT(*) FROM claims),
                    (SELECT COUNT(*) FROM dataset_imports),
                    (SELECT COUNT(*) FROM dataset_record_mappings)",
                [],
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
            .expect("count preserved data");
        let existing_rows_after: (String, String, String, String) = connection
            .query_row(
                "SELECT
                    (SELECT source_kind || ':' || verification_status
                     FROM sources WHERE record_id = 'source-a'),
                    (SELECT assertion_text || ':' || category || ':' || source_locator_json
                     FROM claims WHERE record_id = 'claim-a'),
                    (SELECT dataset_version || ':' || imported_at || ':' || metadata_json ||
                            ':' || record_keys_json
                     FROM dataset_imports
                     WHERE dataset_id = 'dataset.one' AND dataset_version = 'v2'),
                    (SELECT record_kind || ':' || record_id || ':' || content_snapshot_json
                     FROM dataset_record_mappings
                     WHERE dataset_id = 'dataset.one' AND record_key = 'claim:a')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("read preserved v3 row values");
        let assessment_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM research_assessments", [], |row| {
                row.get(0)
            })
            .expect("count new assessments");
        let foreign_key_violations: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .expect("check migrated foreign keys");

        assert_eq!(version_four, 4);
        assert_eq!(after, before);
        assert_eq!(existing_rows_after, existing_rows_before);
        assert_eq!(assessment_count, 0);
        assert_eq!(foreign_key_violations, 0);
    }

    #[test]
    fn migrations_five_and_six_preserve_v4_data_and_are_repeatable() {
        let mut connection = version_four_connection();
        connection
            .execute(
                "INSERT INTO research_datasets (dataset_id) VALUES ('decision-test-dataset')",
                [],
            )
            .expect("insert dataset");
        connection
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES (
                    'decision-test-dataset', 'v1', 1, 'xenoscope-research-dataset',
                    1, '2026-01-01T00:00:00Z', '{\"preserved\":true}', '[\"incident:a\"]'
                 )",
                [],
            )
            .expect("insert dataset import");
        insert_record(&connection, "migration-incident", "incident");
        connection
            .execute(
                "INSERT INTO incidents (record_id, record_kind, label)
                 VALUES ('migration-incident', 'incident', 'Preserved incident')",
                [],
            )
            .expect("insert incident");
        connection
            .execute(
                "INSERT INTO dataset_record_mappings (
                    dataset_id, record_key, record_kind, record_id, content_snapshot_json
                 ) VALUES (
                    'decision-test-dataset', 'incident:a', 'incident',
                    'migration-incident', '{\"label\":\"Original snapshot\"}'
                 )",
                [],
            )
            .expect("insert snapshot mapping");
        let before: (i64, i64, i64, String) = connection
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM records),
                    (SELECT COUNT(*) FROM dataset_imports),
                    (SELECT COUNT(*) FROM dataset_record_mappings),
                    (SELECT content_snapshot_json FROM dataset_record_mappings
                     WHERE record_id = 'migration-incident')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("read v4 baseline");
        let version_four: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version");
        assert_eq!(version_four, 4);

        initialize(&mut connection).expect("apply migration five");
        initialize(&mut connection).expect("repeat initialization");

        let version_five: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version");
        let after: (i64, i64, i64, String) = connection
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM records),
                    (SELECT COUNT(*) FROM dataset_imports),
                    (SELECT COUNT(*) FROM dataset_record_mappings),
                    (SELECT content_snapshot_json FROM dataset_record_mappings
                     WHERE record_id = 'migration-incident')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("read preserved data");
        let new_table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name IN (
                     'research_suggestions', 'suggestion_references', 'suggestion_events',
                     'research_decisions', 'decision_events', 'decision_event_references',
                     'candidate_evidence_snapshots'
                 )",
                [],
                |row| row.get(0),
            )
            .expect("count new tables");
        let violations: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .expect("check foreign keys");

        assert_eq!(version_five, 6);
        assert_eq!(after, before);
        assert_eq!(new_table_count, 7);
        assert_eq!(violations, 0);
    }

    #[test]
    fn failed_research_decisions_migration_rolls_back_tables_and_schema_version() {
        let mut connection = version_four_connection();
        let failing_sql = format!(
            "{}\nINVALID SQL;",
            include_str!("../migrations/0005_research_decisions.sql")
        );

        let error = apply_migration(&mut connection, 5, &failing_sql)
            .expect_err("invalid migration suffix should fail");

        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version");
        let table_exists: bool = connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM sqlite_master WHERE type = 'table'
                    AND name = 'research_suggestions'
                )",
                [],
                |row| row.get(0),
            )
            .expect("check migration rollback");
        assert!(error.contains("rolled back"));
        assert_eq!(version, 4);
        assert!(!table_exists);
    }

    #[test]
    fn research_suggestion_targets_references_and_status_constraints_are_enforced() {
        let connection = initialized_connection();
        seed_decision_records(&connection);

        assert!(
            connection
                .execute(
                    "INSERT INTO research_suggestions (
                        suggestion_id, decision_kind, record_a_id, record_a_kind,
                        record_b_id, record_b_kind, origin_kind, created_at,
                        summary, initial_event_id
                     ) VALUES (
                        'missing-target', 'incident_pair', 'missing', 'incident',
                        'incident-b', 'incident', 'ai', '2026-11-01T00:00:00Z',
                        'Possible match', 'missing-target-event'
                     )",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "INSERT INTO research_suggestions (
                        suggestion_id, decision_kind, record_a_id, record_a_kind,
                        record_b_id, record_b_kind, origin_kind, created_at,
                        summary, initial_event_id
                     ) VALUES (
                        'wrong-kind', 'incident_pair', 'source-a', 'incident',
                        'incident-b', 'incident', 'ai', '2026-11-01T00:00:00Z',
                        'Possible match', 'wrong-kind-event'
                     )",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "INSERT INTO research_suggestions (
                        suggestion_id, decision_kind, record_a_id, record_a_kind,
                        record_b_id, record_b_kind, origin_kind, created_at,
                        summary, initial_event_id
                     ) VALUES (
                        'bad-kind', 'unknown_kind', 'incident-a', 'incident',
                        'incident-b', 'incident', 'ai', '2026-11-01T00:00:00Z',
                        'Possible match', 'bad-kind-event'
                     )",
                    [],
                )
                .is_err()
        );

        insert_test_suggestion(
            &connection,
            "suggestion-valid",
            "suggestion-valid-root",
            "incident_pair",
            "incident-a",
            "incident",
            "incident-b",
            "incident",
            None,
        );
        insert_test_suggestion(
            &connection,
            "suggestion-valid-duplicate-pair",
            "suggestion-valid-duplicate-root",
            "incident_pair",
            "incident-a",
            "incident",
            "incident-b",
            "incident",
            None,
        );
        let same_pair_suggestions: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM research_suggestions
                 WHERE decision_kind = 'incident_pair'
                   AND record_a_id = 'incident-a' AND record_b_id = 'incident-b'",
                [],
                |row| row.get(0),
            )
            .expect("count suggestions for a pair");
        assert_eq!(same_pair_suggestions, 2);
        assert!(
            connection
                .execute(
                    "INSERT INTO suggestion_references (
                        reference_id, suggestion_id, record_id, record_kind, reference_role
                     ) VALUES (
                        'bad-reference', 'suggestion-valid', 'incident-a', 'source', 'basis'
                     )",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "INSERT INTO suggestion_events (
                        event_id, suggestion_id, status, occurred_at, actor_kind
                     ) VALUES (
                        'invalid-status-event', 'suggestion-valid', 'accepted',
                        '2026-11-01T00:00:00Z', 'researcher'
                     )",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "INSERT INTO suggestion_events (
                        event_id, suggestion_id, status, predecessor_event_id,
                        occurred_at, actor_kind, rationale
                     ) VALUES (
                        'invalid-dismissal', 'suggestion-valid', 'dismissed',
                        'suggestion-valid-root', '2026-11-01T00:00:00Z',
                        'researcher', '   '
                     )",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "INSERT INTO suggestion_events (
                        event_id, suggestion_id, status, predecessor_event_id,
                        occurred_at, actor_kind, replacement_suggestion_id, rationale
                     ) VALUES (
                        'invalid-supersession', 'suggestion-valid', 'superseded',
                        'suggestion-valid-root', '2026-11-01T00:00:00Z',
                        'researcher', 'missing-replacement', 'Replaced'
                     )",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "INSERT INTO suggestion_events (
                        event_id, suggestion_id, status, predecessor_event_id,
                        occurred_at, actor_kind, decision_id
                     ) VALUES (
                        'invalid-resolution', 'suggestion-valid', 'resolved',
                        'suggestion-valid-root', '2026-11-01T00:00:00Z',
                        'researcher', 'missing-decision'
                     )",
                    [],
                )
                .is_err()
        );
    }

    #[test]
    fn dismissed_suggestion_event_requires_nonblank_rationale() {
        let connection = initialized_connection();
        seed_decision_records(&connection);
        insert_test_suggestion(
            &connection,
            "dismissal-rationale",
            "dismissal-rationale-root",
            "incident_pair",
            "incident-a",
            "incident",
            "incident-b",
            "incident",
            None,
        );

        assert!(
            connection
                .execute(
                    "INSERT INTO suggestion_events (
                        event_id, suggestion_id, status, predecessor_event_id,
                        occurred_at, actor_kind, rationale
                     ) VALUES (
                        'dismissal-null-rationale', 'dismissal-rationale', 'dismissed',
                        'dismissal-rationale-root', '2026-11-01T00:00:00Z',
                        'researcher', NULL
                     )",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "INSERT INTO suggestion_events (
                        event_id, suggestion_id, status, predecessor_event_id,
                        occurred_at, actor_kind, rationale
                     ) VALUES (
                        'dismissal-blank-rationale', 'dismissal-rationale', 'dismissed',
                        'dismissal-rationale-root', '2026-11-01T00:00:00Z',
                        'researcher', '   '
                     )",
                    [],
                )
                .is_err()
        );
        assert_eq!(
            connection
                .execute(
                    "INSERT INTO suggestion_events (
                        event_id, suggestion_id, status, predecessor_event_id,
                        occurred_at, actor_kind, rationale
                     ) VALUES (
                        'dismissal-valid-rationale', 'dismissal-rationale', 'dismissed',
                        'dismissal-rationale-root', '2026-11-01T00:00:00Z',
                        'researcher', 'Reviewed and dismissed'
                     )",
                    [],
                )
                .expect("accept nonblank dismissal rationale"),
            1
        );
    }

    #[test]
    fn superseded_suggestion_event_requires_nonblank_rationale() {
        let connection = initialized_connection();
        seed_decision_records(&connection);
        insert_test_suggestion(
            &connection,
            "supersession-rationale",
            "supersession-rationale-root",
            "incident_pair",
            "incident-a",
            "incident",
            "incident-b",
            "incident",
            None,
        );
        insert_test_suggestion(
            &connection,
            "supersession-replacement",
            "supersession-replacement-root",
            "incident_pair",
            "incident-a",
            "incident",
            "incident-b",
            "incident",
            None,
        );

        assert!(
            connection
                .execute(
                    "INSERT INTO suggestion_events (
                        event_id, suggestion_id, status, predecessor_event_id,
                        occurred_at, actor_kind, replacement_suggestion_id, rationale
                     ) VALUES (
                        'supersession-null-rationale', 'supersession-rationale',
                        'superseded', 'supersession-rationale-root',
                        '2026-11-01T00:00:00Z', 'researcher',
                        'supersession-replacement', NULL
                     )",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "INSERT INTO suggestion_events (
                        event_id, suggestion_id, status, predecessor_event_id,
                        occurred_at, actor_kind, replacement_suggestion_id, rationale
                     ) VALUES (
                        'supersession-blank-rationale', 'supersession-rationale',
                        'superseded', 'supersession-rationale-root',
                        '2026-11-01T00:00:00Z', 'researcher',
                        'supersession-replacement', '   '
                     )",
                    [],
                )
                .is_err()
        );
        assert_eq!(
            connection
                .execute(
                    "INSERT INTO suggestion_events (
                        event_id, suggestion_id, status, predecessor_event_id,
                        occurred_at, actor_kind, replacement_suggestion_id, rationale
                     ) VALUES (
                        'supersession-valid-rationale', 'supersession-rationale',
                        'superseded', 'supersession-rationale-root',
                        '2026-11-01T00:00:00Z', 'researcher',
                        'supersession-replacement', 'Replaced by a more specific hypothesis'
                     )",
                    [],
                )
                .expect("accept nonblank supersession rationale"),
            1
        );
    }

    #[test]
    fn decision_targets_outcomes_and_thread_identity_constraints_are_enforced() {
        let connection = initialized_connection();
        seed_decision_records(&connection);

        assert!(
            connection
                .execute(
                    "INSERT INTO research_decisions (
                        decision_id, decision_kind, record_a_id, record_a_kind,
                        record_b_id, record_b_kind, created_at, initial_event_id
                     ) VALUES (
                        'invalid-decision', 'incident_pair', 'source-a', 'source',
                        'incident-b', 'incident', '2026-11-01T00:00:00Z',
                        'invalid-decision-event'
                     )",
                    [],
                )
                .is_err()
        );
        insert_test_decision(
            &connection,
            "incident-decision",
            "incident-decision-root",
            "incident_pair",
            "incident-a",
            "incident",
            "incident-b",
            "incident",
            None,
            "same_event",
        );
        assert!(
            connection
                .execute(
                    "INSERT INTO research_decisions (
                        decision_id, decision_kind, record_a_id, record_a_kind,
                        record_b_id, record_b_kind, created_at, initial_event_id
                     ) VALUES (
                        'duplicate-decision', 'incident_pair', 'incident-a', 'incident',
                        'incident-b', 'incident', '2026-11-01T00:00:00Z',
                        'duplicate-decision-event'
                     )",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "INSERT INTO decision_events (
                        event_id, decision_id, event_kind, outcome, occurred_at,
                        actor_kind, actor_name, rationale
                     ) VALUES (
                        'bad-outcome', 'incident-decision', 'recorded', 'dependent',
                        '2026-11-01T00:00:00Z', 'researcher', 'Local researcher', 'Review'
                     )",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "INSERT INTO decision_events (
                        event_id, decision_id, event_kind, outcome, occurred_at,
                        actor_kind, actor_name, rationale
                     ) VALUES (
                        'bad-actor', 'incident-decision', 'superseded', 'not_same_event',
                        '2026-11-01T00:00:00Z', 'ai', 'Automated process', 'Review'
                     )",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "INSERT INTO decision_events (
                        event_id, decision_id, event_kind, outcome, occurred_at,
                        actor_kind, actor_name, rationale
                     ) VALUES (
                        'blank-rationale', 'incident-decision', 'superseded', 'not_same_event',
                        '2026-11-01T00:00:00Z', 'researcher', 'Local researcher', '   '
                     )",
                    [],
                )
                .is_err()
        );

        insert_test_decision(
            &connection,
            "lineage-copied",
            "lineage-copied-root",
            "source_lineage",
            "source-a",
            "source",
            "source-b",
            "source",
            Some("copied_from"),
            "copied_from",
        );
        insert_test_decision(
            &connection,
            "lineage-quotes",
            "lineage-quotes-root",
            "source_lineage",
            "source-a",
            "source",
            "source-b",
            "source",
            Some("quotes"),
            "quotes",
        );
        assert!(
            connection
                .execute(
                    "INSERT INTO research_decisions (
                        decision_id, decision_kind, record_a_id, record_a_kind,
                        record_b_id, record_b_kind, lineage_type, created_at, initial_event_id
                     ) VALUES (
                        'lineage-copied-duplicate', 'source_lineage', 'source-a', 'source',
                        'source-b', 'source', 'copied_from', '2026-11-01T00:00:00Z',
                        'lineage-copied-duplicate-root'
                     )",
                    [],
                )
                .is_err()
        );
    }

    #[test]
    fn decision_event_chain_rejects_roots_branches_stale_heads_and_invalid_transitions() {
        let connection = initialized_connection();
        seed_decision_records(&connection);
        insert_test_decision(
            &connection,
            "chain-one",
            "chain-one-root",
            "incident_pair",
            "incident-a",
            "incident",
            "incident-b",
            "incident",
            None,
            "same_event",
        );
        insert_test_decision(
            &connection,
            "chain-two",
            "chain-two-root",
            "incident_pair",
            "incident-a",
            "incident",
            "incident-c",
            "incident",
            None,
            "related_distinct",
        );
        assert!(
            connection
                .execute(
                    "INSERT INTO decision_events (
                        event_id, decision_id, event_kind, outcome, occurred_at,
                        actor_kind, actor_name, rationale
                     ) VALUES (
                        'second-root', 'chain-one', 'recorded', 'same_event',
                        '2026-11-01T00:00:00Z', 'researcher', 'Local researcher', 'Root'
                     )",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "INSERT INTO decision_events (
                        event_id, decision_id, event_kind, outcome, predecessor_event_id,
                        occurred_at, actor_kind, actor_name, rationale
                     ) VALUES (
                        'wrong-thread-parent', 'chain-one', 'superseded', 'not_same_event',
                        'chain-two-root', '2026-11-01T00:00:00Z',
                        'researcher', 'Local researcher', 'Wrong thread'
                     )",
                    [],
                )
                .is_err()
        );
        connection
            .execute(
                "INSERT INTO decision_events (
                    event_id, decision_id, event_kind, outcome, predecessor_event_id,
                    occurred_at, actor_kind, actor_name, rationale
                 ) VALUES (
                    'chain-one-reversal', 'chain-one', 'reversed', NULL,
                    'chain-one-root', '2026-11-01T00:00:00Z',
                    'researcher', 'Local researcher', 'Withdraw initial conclusion'
                 )",
                [],
            )
            .expect("reverse active conclusion");
        assert!(
            connection
                .execute(
                    "INSERT INTO decision_events (
                        event_id, decision_id, event_kind, outcome, predecessor_event_id,
                        occurred_at, actor_kind, actor_name, rationale
                     ) VALUES (
                        'supersede-after-reversal', 'chain-one', 'superseded', 'not_same_event',
                        'chain-one-reversal', '2026-11-01T00:00:00Z',
                        'researcher', 'Local researcher', 'Invalid transition'
                     )",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "INSERT INTO decision_events (
                        event_id, decision_id, event_kind, outcome, predecessor_event_id,
                        occurred_at, actor_kind, actor_name, rationale
                     ) VALUES (
                        'second-reversal', 'chain-one', 'reversed', NULL,
                        'chain-one-reversal', '2026-11-01T00:00:00Z',
                        'researcher', 'Local researcher', 'Invalid transition'
                     )",
                    [],
                )
                .is_err()
        );
        connection
            .execute(
                "INSERT INTO decision_events (
                    event_id, decision_id, event_kind, outcome, predecessor_event_id,
                    occurred_at, actor_kind, actor_name, rationale
                 ) VALUES (
                    'chain-one-final', 'chain-one', 'recorded', 'not_same_event',
                    'chain-one-reversal', '2026-11-01T00:00:00Z',
                    'researcher', 'Local researcher', 'New conclusion after reversal'
                 )",
                [],
            )
            .expect("record new conclusion after reversal");
        assert!(
            connection
                .execute(
                    "INSERT INTO decision_events (
                        event_id, decision_id, event_kind, outcome, predecessor_event_id,
                        occurred_at, actor_kind, actor_name, rationale
                     ) VALUES (
                        'stale-reversal', 'chain-one', 'reversed', NULL,
                        'chain-one-root', '2026-11-01T00:00:00Z',
                        'researcher', 'Local researcher', 'Stale predecessor'
                     )",
                    [],
                )
                .is_err()
        );
    }

    #[test]
    fn suggestion_resolution_and_required_initial_events_are_transactional() {
        let connection = initialized_connection();
        seed_decision_records(&connection);
        insert_test_suggestion(
            &connection,
            "suggestion-resolve",
            "suggestion-resolve-root",
            "incident_pair",
            "incident-a",
            "incident",
            "incident-b",
            "incident",
            None,
        );

        let transaction = connection
            .unchecked_transaction()
            .expect("begin suggestion conversion");
        transaction
            .execute(
                "INSERT INTO research_decisions (
                    decision_id, decision_kind, record_a_id, record_a_kind,
                    record_b_id, record_b_kind, origin_suggestion_id,
                    origin_suggestion_event_id, origin_suggestion_status,
                    created_at, initial_event_id
                 ) VALUES (
                    'converted-decision', 'incident_pair', 'incident-a', 'incident',
                    'incident-b', 'incident', 'suggestion-resolve',
                    'suggestion-resolve-event', 'resolved', '2026-11-01T00:00:00Z',
                    'converted-decision-root'
                 )",
                [],
            )
            .expect("insert decision identity");
        transaction
            .execute(
                "INSERT INTO decision_events (
                    event_id, decision_id, event_kind, outcome, occurred_at,
                    actor_kind, actor_name, rationale
                 ) VALUES (
                    'converted-decision-root', 'converted-decision', 'recorded',
                    'not_same_event', '2026-11-01T00:00:00Z',
                    'researcher', 'Local researcher', 'Reviewed and rejected the hypothesis'
                 )",
                [],
            )
            .expect("insert explicit reviewed conclusion");
        transaction
            .execute(
                "INSERT INTO suggestion_events (
                    event_id, suggestion_id, status, predecessor_event_id,
                    occurred_at, actor_kind, actor_name, decision_id
                 ) VALUES (
                    'suggestion-resolve-event', 'suggestion-resolve', 'resolved',
                    'suggestion-resolve-root', '2026-11-01T00:00:00Z',
                    'researcher', 'Local researcher', 'converted-decision'
                 )",
                [],
            )
            .expect("resolve suggestion with its decision");
        transaction.commit().expect("commit suggestion conversion");

        let resolved_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM suggestion_events
                 WHERE suggestion_id = 'suggestion-resolve' AND status = 'resolved'",
                [],
                |row| row.get(0),
            )
            .expect("count resolved suggestion events");
        assert_eq!(resolved_count, 1);

        let missing_root = connection
            .unchecked_transaction()
            .expect("begin missing-root transaction");
        missing_root
            .execute(
                "INSERT INTO research_decisions (
                    decision_id, decision_kind, record_a_id, record_a_kind,
                    record_b_id, record_b_kind, created_at, initial_event_id
                 ) VALUES (
                    'missing-decision-root', 'incident_pair', 'incident-a', 'incident',
                    'incident-c', 'incident', '2026-11-01T00:00:00Z',
                    'absent-decision-root'
                 )",
                [],
            )
            .expect("insert decision that must not commit without root");
        assert!(missing_root.commit().is_err());
        let missing_root_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM research_decisions
                 WHERE decision_id = 'missing-decision-root'",
                [],
                |row| row.get(0),
            )
            .expect("count rolled-back rootless decisions");
        assert_eq!(missing_root_count, 0);

        insert_test_suggestion(
            &connection,
            "suggestion-rollback",
            "suggestion-rollback-root",
            "incident_pair",
            "incident-a",
            "incident",
            "incident-c",
            "incident",
            None,
        );
        let failed_conversion = connection
            .unchecked_transaction()
            .expect("begin failed conversion");
        failed_conversion
            .execute(
                "INSERT INTO research_decisions (
                    decision_id, decision_kind, record_a_id, record_a_kind,
                    record_b_id, record_b_kind, origin_suggestion_id,
                    origin_suggestion_event_id, origin_suggestion_status,
                    created_at, initial_event_id
                 ) VALUES (
                    'rollback-decision', 'incident_pair', 'incident-a', 'incident',
                    'incident-c', 'incident', 'suggestion-rollback',
                    'missing-resolution-event', 'resolved', '2026-11-01T00:00:00Z',
                    'rollback-decision-root'
                 )",
                [],
            )
            .expect("insert pending conversion decision");
        failed_conversion
            .execute(
                "INSERT INTO decision_events (
                    event_id, decision_id, event_kind, outcome, occurred_at,
                    actor_kind, actor_name, rationale
                 ) VALUES (
                    'rollback-decision-root', 'rollback-decision', 'recorded',
                    'same_event', '2026-11-01T00:00:00Z',
                    'researcher', 'Local researcher', 'Provisional test decision'
                 )",
                [],
            )
            .expect("insert pending conversion root");
        assert!(failed_conversion.commit().is_err());
        let rolled_back_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM research_decisions
                 WHERE decision_id = 'rollback-decision'",
                [],
                |row| row.get(0),
            )
            .expect("count rolled-back decisions");
        assert_eq!(rolled_back_count, 0);

        let not_resolved = connection
            .unchecked_transaction()
            .expect("begin non-resolved origin transaction");
        not_resolved
            .execute(
                "INSERT INTO research_decisions (
                    decision_id, decision_kind, record_a_id, record_a_kind,
                    record_b_id, record_b_kind, origin_suggestion_id,
                    origin_suggestion_event_id, origin_suggestion_status,
                    created_at, initial_event_id
                 ) VALUES (
                    'non-resolved-origin', 'incident_pair', 'incident-a', 'incident',
                    'incident-c', 'incident', 'suggestion-rollback',
                    'suggestion-rollback-root', 'resolved', '2026-11-01T00:00:00Z',
                    'non-resolved-origin-root'
                 )",
                [],
            )
            .expect("insert origin that will fail its deferred status reference");
        not_resolved
            .execute(
                "INSERT INTO decision_events (
                    event_id, decision_id, event_kind, outcome, occurred_at,
                    actor_kind, actor_name, rationale
                 ) VALUES (
                    'non-resolved-origin-root', 'non-resolved-origin', 'recorded',
                    'same_event', '2026-11-01T00:00:00Z',
                    'researcher', 'Local researcher', 'Must not commit'
                 )",
                [],
            )
            .expect("insert non-resolved origin root");
        assert!(not_resolved.commit().is_err());
        let non_resolved_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM research_decisions
                 WHERE decision_id = 'non-resolved-origin'",
                [],
                |row| row.get(0),
            )
            .expect("count invalid origin decisions");
        assert_eq!(non_resolved_count, 0);

        assert!(
            connection
                .execute(
                    "INSERT INTO research_suggestions (
                        suggestion_id, decision_kind, record_a_id, record_a_kind,
                        record_b_id, record_b_kind, origin_kind, created_at,
                        summary, initial_event_id
                     ) VALUES (
                        'missing-initial-event', 'incident_pair', 'incident-a', 'incident',
                        'incident-c', 'incident', 'ai', '2026-11-01T00:00:00Z',
                        'Must roll back without root', 'absent-suggestion-root'
                     )",
                    [],
                )
                .is_err()
        );
    }

    #[test]
    fn suggestion_and_decision_persistence_tables_are_append_only() {
        let connection = initialized_connection();
        seed_decision_records(&connection);
        insert_test_suggestion(
            &connection,
            "append-only-suggestion",
            "append-only-suggestion-root",
            "incident_pair",
            "incident-a",
            "incident",
            "incident-b",
            "incident",
            None,
        );
        connection
            .execute(
                "INSERT INTO suggestion_references (
                    reference_id, suggestion_id, record_id, record_kind, reference_role
                 ) VALUES (
                    'append-only-suggestion-reference', 'append-only-suggestion',
                    'source-a', 'source', 'support'
                 )",
                [],
            )
            .expect("insert suggestion reference");
        insert_test_decision(
            &connection,
            "append-only-decision",
            "append-only-decision-root",
            "incident_pair",
            "incident-a",
            "incident",
            "incident-b",
            "incident",
            None,
            "same_event",
        );
        connection
            .execute(
                "INSERT INTO decision_event_references (
                    reference_id, event_id, record_id, record_kind, reference_role
                 ) VALUES (
                    'append-only-decision-reference', 'append-only-decision-root',
                    'evidence-a', 'evidence_item', 'support'
                 )",
                [],
            )
            .expect("insert decision event reference");

        for (table, id_column, id) in [
            (
                "research_suggestions",
                "suggestion_id",
                "append-only-suggestion",
            ),
            (
                "suggestion_references",
                "reference_id",
                "append-only-suggestion-reference",
            ),
            (
                "suggestion_events",
                "event_id",
                "append-only-suggestion-root",
            ),
            ("research_decisions", "decision_id", "append-only-decision"),
            ("decision_events", "event_id", "append-only-decision-root"),
            (
                "decision_event_references",
                "reference_id",
                "append-only-decision-reference",
            ),
        ] {
            assert!(
                connection
                    .execute(
                        &format!(
                            "UPDATE {table} SET {id_column} = {id_column} WHERE {id_column} = ?1"
                        ),
                        [id],
                    )
                    .is_err(),
                "{table} update should be rejected"
            );
            assert!(
                connection
                    .execute(&format!("DELETE FROM {table} WHERE {id_column} = ?1"), [id],)
                    .is_err(),
                "{table} delete should be rejected"
            );
        }
    }

    #[test]
    fn assessment_status_score_snapshot_and_predecessor_constraints_are_enforced() {
        let connection = migrated_assessment_test_connection();
        insert_test_assessment(
            &connection,
            "assessment-v1",
            "dataset.one",
            "v1",
            1,
            "preliminary",
            None,
            None,
            None,
        )
        .expect("insert preliminary assessment without score");
        assert!(
            insert_test_assessment(
                &connection,
                "assessment-v2-too-early",
                "dataset.one",
                "v2",
                2,
                "preliminary",
                None,
                Some("assessment-v1"),
                Some("Parent is not sealed"),
            )
            .is_err()
        );
        seal_test_assessment(&connection, "assessment-v1");
        insert_test_assessment(
            &connection,
            "assessment-v2",
            "dataset.one",
            "v2",
            2,
            "limited_by_missing_information",
            Some(1),
            Some("assessment-v1"),
            Some("Added newer evidence"),
        )
        .expect("insert successor assessment");
        insert_test_assessment(
            &connection,
            "assessment-v3",
            "dataset.one",
            "v2",
            2,
            "reviewed_for_scope",
            Some(100),
            None,
            None,
        )
        .expect("insert assessment with maximum score");

        assert!(
            insert_test_assessment(
                &connection,
                "score-zero",
                "dataset.one",
                "v1",
                1,
                "preliminary",
                Some(0),
                None,
                None,
            )
            .is_err()
        );
        assert!(
            insert_test_assessment(
                &connection,
                "score-over-max",
                "dataset.one",
                "v1",
                1,
                "preliminary",
                Some(101),
                None,
                None,
            )
            .is_err()
        );
        assert!(
            insert_test_assessment(
                &connection,
                "invalid-status",
                "dataset.one",
                "v1",
                1,
                "complete",
                None,
                None,
                None,
            )
            .is_err()
        );
        assert!(
            insert_test_assessment(
                &connection,
                "mismatched-import",
                "dataset.one",
                "v1",
                2,
                "preliminary",
                None,
                None,
                None,
            )
            .is_err()
        );
        assert!(
            insert_test_assessment(
                &connection,
                "self-reference",
                "dataset.one",
                "v1",
                1,
                "preliminary",
                None,
                Some("self-reference"),
                Some("Self-reference"),
            )
            .is_err()
        );
        assert!(
            insert_test_assessment(
                &connection,
                "missing-predecessor",
                "dataset.one",
                "v1",
                1,
                "preliminary",
                None,
                Some("does-not-exist"),
                Some("Missing parent"),
            )
            .is_err()
        );
        assert!(
            insert_test_assessment(
                &connection,
                "cross-case-predecessor",
                "dataset.two",
                "v1",
                1,
                "preliminary",
                None,
                Some("assessment-v1"),
                Some("Wrong case"),
            )
            .is_err()
        );
        assert!(
            insert_test_assessment(
                &connection,
                "missing-revision-reason",
                "dataset.one",
                "v1",
                1,
                "preliminary",
                None,
                Some("assessment-v1"),
                None,
            )
            .is_err()
        );
        connection
            .execute(
                "INSERT INTO research_assessments (
                    assessment_id, dataset_id, dataset_version, import_sequence, created_at,
                    created_by_kind, status, scoring_profile_version, scoring_profile_json,
                    algorithm_version, input_snapshot_json
                 ) VALUES (
                    'invalid-snapshot', 'dataset.one', 'v1', 1, '2026-03-01T00:00:00Z',
                    'researcher', 'preliminary', 'profile-1', '[]',
                    'algorithm-1', '{\"inputs\":[]}'
                 )",
                [],
            )
            .expect_err("profile JSON must be an object");
        connection
            .execute(
                "INSERT INTO research_assessments (
                    assessment_id, dataset_id, dataset_version, import_sequence, created_at,
                    created_by_kind, status, scoring_profile_version, scoring_profile_json,
                    algorithm_version, input_snapshot_json
                 ) VALUES (
                    'invalid-input-json', 'dataset.one', 'v1', 1, '2026-03-01T00:00:00Z',
                    'researcher', 'preliminary', 'profile-1', '{}',
                    'algorithm-1', '{invalid'
                 )",
                [],
            )
            .expect_err("input snapshot must contain valid JSON");
    }

    #[test]
    fn assessment_claims_and_evidence_enforce_scope_and_valid_states() {
        let connection = migrated_assessment_test_connection();
        insert_test_assessment(
            &connection,
            "assessment-evidence",
            "dataset.one",
            "v1",
            1,
            "preliminary",
            None,
            None,
            None,
        )
        .expect("insert assessment");
        connection
            .execute(
                "INSERT INTO assessment_claims (
                    assessment_id, dataset_id, claim_key, claim_record_id, scope_role, outcome
                 ) VALUES (
                    'assessment-evidence', 'dataset.one', 'claim:a', 'claim-a', 'core', 'supported'
                 )",
                [],
            )
            .expect("select a claim");
        connection
            .execute(
                "INSERT INTO assessment_claims (
                    assessment_id, dataset_id, claim_key, claim_record_id, scope_role, outcome
                 ) VALUES (
                    'assessment-evidence', 'dataset.one', 'claim:a', 'claim-a', 'primary', 'supported'
                 )",
                [],
            )
            .expect_err("invalid scope role must be rejected");
        connection
            .execute(
                "INSERT INTO assessment_claims (
                    assessment_id, dataset_id, claim_key, claim_record_id, scope_role, outcome
                 ) VALUES (
                    'assessment-evidence', 'dataset.one', 'claim:a', 'claim-a', 'context', 'unknown'
                 )",
                [],
            )
            .expect_err("invalid claim outcome must be rejected");

        let evidence_states = [
            ("evidence-a", "supports"),
            ("evidence-b", "conflicts"),
            ("evidence-c", "not_examined"),
            ("evidence-d", "irrelevant_to_scope"),
            ("evidence-e", "indeterminate"),
        ];
        for (index, (evidence_id, state)) in evidence_states.iter().enumerate() {
            let key = format!("evidence:{}", (b'a' + index as u8) as char);
            connection
                .execute(
                    "INSERT INTO assessment_claim_evidence (
                        assessment_evidence_id, assessment_id, dataset_id, claim_record_id,
                        evidence_key, evidence_record_id, state
                     ) VALUES (?1, 'assessment-evidence', 'dataset.one', 'claim-a', ?2, ?3, ?4)",
                    params![format!("evidence-state-{state}"), key, evidence_id, state],
                )
                .expect("insert valid evidence state");
        }
        connection
            .execute(
                "INSERT INTO assessment_claim_evidence (
                    assessment_evidence_id, assessment_id, dataset_id, claim_record_id,
                    state, note
                 ) VALUES (
                    'evidence-unavailable', 'assessment-evidence', 'dataset.one',
                    'claim-a', 'unavailable', 'Referenced evidence is not available for review'
                 )",
                [],
            )
            .expect("record unavailable evidence without a fabricated record ID");
        connection
            .execute(
                "INSERT INTO assessment_claim_evidence (
                    assessment_evidence_id, assessment_id, dataset_id,
                    claim_record_id, evidence_key, state
                 ) VALUES (
                    'evidence-unavailable-key', 'assessment-evidence', 'dataset.one',
                    'claim-a', 'expected:evidence-key', 'unavailable'
                 )",
                [],
            )
            .expect("a descriptive stable reference can identify unavailable evidence");
        connection
            .execute(
                "INSERT INTO assessment_claim_evidence (
                    assessment_evidence_id, assessment_id, dataset_id, claim_record_id, state
                 ) VALUES (
                    'evidence-unavailable-no-note', 'assessment-evidence', 'dataset.one',
                    'claim-a', 'unavailable'
                 )",
                [],
            )
            .expect_err("unavailable evidence needs a meaningful note or reference");
        connection
            .execute(
                "INSERT INTO assessment_claim_evidence (
                    assessment_evidence_id, assessment_id, dataset_id, claim_record_id,
                    evidence_key, evidence_record_id, state
                 ) VALUES (
                    'evidence-wrong-claim', 'assessment-evidence', 'dataset.one',
                    'claim-b', 'evidence:a', 'evidence-a', 'supports'
                 )",
                [],
            )
            .expect_err("evidence must reference a claim selected for this assessment");
        connection
            .execute(
                "INSERT INTO assessment_claim_evidence (
                    assessment_evidence_id, assessment_id, dataset_id, claim_record_id,
                    evidence_key, evidence_record_id, state
                 ) VALUES (
                    'evidence-missing-record', 'assessment-evidence', 'dataset.one',
                    'claim-a', 'evidence:missing', 'evidence-missing', 'supports'
                 )",
                [],
            )
            .expect_err("evidence record references must satisfy foreign keys");
        connection
            .execute(
                "INSERT INTO assessment_claim_evidence (
                    assessment_evidence_id, assessment_id, dataset_id, claim_record_id,
                    evidence_key, evidence_record_id, state
                 ) VALUES (
                    'evidence-invalid-state', 'assessment-evidence', 'dataset.one',
                    'claim-a', 'evidence:f', 'evidence-f', 'uncertain'
                 )",
                [],
            )
            .expect_err("invalid evidence state must be rejected");

        insert_test_assessment(
            &connection,
            "assessment-new-version",
            "dataset.one",
            "v2",
            2,
            "preliminary",
            None,
            None,
            None,
        )
        .expect("insert assessment for newer import");
        connection
            .execute(
                "INSERT INTO assessment_claims (
                    assessment_id, dataset_id, claim_key, claim_record_id, scope_role, outcome
                 ) VALUES (
                    'assessment-new-version', 'dataset.one', 'claim:a', 'claim-a',
                    'core', 'unexamined'
                 )",
                [],
            )
            .expect("select claim present in the newer manifest");
        connection
            .execute(
                "INSERT INTO assessment_claims (
                    assessment_id, dataset_id, claim_key, claim_record_id, scope_role, outcome
                 ) VALUES (
                    'assessment-new-version', 'dataset.one', 'claim:b', 'claim-b',
                    'core', 'supported'
                 )",
                [],
            )
            .expect_err("claim absent from the selected import manifest must be rejected");
        connection
            .execute(
                "INSERT INTO assessment_claim_evidence (
                    assessment_evidence_id, assessment_id, dataset_id, claim_record_id,
                    evidence_key, evidence_record_id, state
                 ) VALUES (
                    'evidence-removed-from-manifest', 'assessment-new-version',
                    'dataset.one', 'claim-a', 'evidence:f', 'evidence-f', 'not_examined'
                 )",
                [],
            )
            .expect_err("evidence absent from the selected version manifest must be rejected");
    }

    #[test]
    fn assessment_factor_and_source_dependency_constraints_are_enforced() {
        let connection = migrated_assessment_test_connection();
        insert_test_assessment(
            &connection,
            "assessment-factors",
            "dataset.one",
            "v1",
            1,
            "preliminary",
            None,
            None,
            None,
        )
        .expect("insert assessment");

        for factor in [
            "evidence_quality",
            "provenance",
            "independent_corroboration",
            "consistency",
        ] {
            connection
                .execute(
                    "INSERT INTO assessment_factor_ratings (
                        assessment_id, factor_key, rating_json, rationale, references_json
                     ) VALUES ('assessment-factors', ?1, NULL, NULL, '[]')",
                    [factor],
                )
                .expect("insert valid nullable factor rating");
        }
        connection
            .execute(
                "INSERT INTO assessment_factor_ratings (assessment_id, factor_key)
                 VALUES ('assessment-factors', 'evidence_quality')",
                [],
            )
            .expect_err("factor key must be unique per assessment");
        connection
            .execute(
                "INSERT INTO assessment_factor_ratings (assessment_id, factor_key)
                 VALUES ('assessment-factors', 'source_quality')",
                [],
            )
            .expect_err("invalid factor key must be rejected");
        connection
            .execute(
                "INSERT INTO assessment_factor_ratings (
                    assessment_id, factor_key, rating_json, references_json
                 ) VALUES ('assessment-factors', 'provenance', 'not-json', '[]')",
                [],
            )
            .expect_err("factor rating JSON must be valid when present");
        connection
            .execute(
                "INSERT INTO assessment_factor_ratings (
                    assessment_id, factor_key, references_json
                 ) VALUES ('assessment-factors', 'consistency', '{}')",
                [],
            )
            .expect_err("factor references must be a JSON array");

        connection
            .execute(
                "INSERT INTO assessment_source_dependencies (
                    dependency_id, assessment_id, dataset_id,
                    source_a_key, source_a_id, source_b_key, source_b_id, state
                 ) VALUES (
                    'dependency-1', 'assessment-factors', 'dataset.one',
                    'source:a', 'source-a', 'source:b', 'source-b', 'unknown'
                 )",
                [],
            )
            .expect("insert unknown source dependency");
        insert_test_assessment(
            &connection,
            "assessment-dependency-independent",
            "dataset.one",
            "v1",
            1,
            "preliminary",
            None,
            None,
            None,
        )
        .expect("insert second dependency assessment");
        connection
            .execute(
                "INSERT INTO assessment_source_dependencies (
                    dependency_id, assessment_id, dataset_id,
                    source_a_key, source_a_id, source_b_key, source_b_id, state
                 ) VALUES (
                    'dependency-independent', 'assessment-dependency-independent',
                    'dataset.one', 'source:a', 'source-a', 'source:b', 'source-b',
                    'assessed_independent'
                 )",
                [],
            )
            .expect("insert assessed-independent dependency state");
        insert_test_assessment(
            &connection,
            "assessment-dependency-dependent",
            "dataset.one",
            "v1",
            1,
            "preliminary",
            None,
            None,
            None,
        )
        .expect("insert third dependency assessment");
        connection
            .execute(
                "INSERT INTO assessment_source_dependencies (
                    dependency_id, assessment_id, dataset_id,
                    source_a_key, source_a_id, source_b_key, source_b_id, state
                 ) VALUES (
                    'dependency-dependent', 'assessment-dependency-dependent',
                    'dataset.one', 'source:a', 'source-a', 'source:b', 'source-b',
                    'dependent'
                 )",
                [],
            )
            .expect("insert dependent source state");
        connection
            .execute(
                "INSERT INTO assessment_source_dependencies (
                    dependency_id, assessment_id, dataset_id,
                    source_a_key, source_a_id, source_b_key, source_b_id, state
                 ) VALUES (
                    'dependency-duplicate', 'assessment-factors', 'dataset.one',
                    'source:a', 'source-a', 'source:b', 'source-b', 'dependent'
                 )",
                [],
            )
            .expect_err("source pair must be unique per assessment");
        connection
            .execute(
                "INSERT INTO assessment_source_dependencies (
                    dependency_id, assessment_id, dataset_id,
                    source_a_key, source_a_id, source_b_key, source_b_id, state
                 ) VALUES (
                    'dependency-reversed', 'assessment-factors', 'dataset.one',
                    'source:b', 'source-b', 'source:a', 'source-a', 'dependent'
                 )",
                [],
            )
            .expect_err("source pairs must use normalized ascending record IDs");
        connection
            .execute(
                "INSERT INTO assessment_source_dependencies (
                    dependency_id, assessment_id, dataset_id,
                    source_a_key, source_a_id, source_b_key, source_b_id, state
                 ) VALUES (
                    'dependency-self', 'assessment-factors', 'dataset.one',
                    'source:a', 'source-a', 'source:a', 'source-a', 'dependent'
                 )",
                [],
            )
            .expect_err("self-dependencies must be rejected");
        connection
            .execute(
                "INSERT INTO assessment_source_dependencies (
                    dependency_id, assessment_id, dataset_id,
                    source_a_key, source_a_id, source_b_key, source_b_id, state
                 ) VALUES (
                    'dependency-invalid-state', 'assessment-factors', 'dataset.one',
                    'source:a', 'source-a', 'source:b', 'source-b', 'probably_independent'
                 )",
                [],
            )
            .expect_err("invalid dependency state must be rejected");
    }

    #[test]
    fn assessment_rows_and_inputs_cannot_be_overwritten_or_deleted() {
        let connection = migrated_assessment_test_connection();
        let recursive_triggers: bool = connection
            .pragma_query_value(None, "recursive_triggers", |row| row.get(0))
            .expect("read recursive trigger setting");
        assert!(recursive_triggers);

        insert_test_assessment(
            &connection,
            "assessment-immutable",
            "dataset.one",
            "v1",
            1,
            "preliminary",
            None,
            None,
            None,
        )
        .expect("insert assessment");
        connection
            .execute(
                "INSERT INTO assessment_claims (
                    assessment_id, dataset_id, claim_key, claim_record_id, scope_role, outcome,
                    rationale
                 ) VALUES (
                    'assessment-immutable', 'dataset.one', 'claim:a', 'claim-a',
                    'core', 'supported', 'Original claim rationale'
                 )",
                [],
            )
            .expect("insert assessment claim");
        connection
            .execute(
                "INSERT INTO assessment_factor_ratings (
                    assessment_id, factor_key, rationale
                 ) VALUES (
                    'assessment-immutable', 'evidence_quality', 'Original factor rationale'
                 )",
                [],
            )
            .expect("insert assessment factor");
        seal_test_assessment(&connection, "assessment-immutable");

        insert_test_assessment(
            &connection,
            "sealed-assessment-replace",
            "dataset.one",
            "v1",
            1,
            "preliminary",
            None,
            None,
            None,
        )
        .expect("insert assessment to isolate the replacement conflict");
        seal_test_assessment(&connection, "sealed-assessment-replace");

        let assessment_replace = connection.execute(
            "INSERT OR REPLACE INTO research_assessments (
                assessment_id, dataset_id, dataset_version, import_sequence, created_at,
                created_by_kind, status, score, scoring_profile_version, scoring_profile_json,
                algorithm_version, input_snapshot_json, is_sealed
             ) VALUES (
                'sealed-assessment-replace', 'dataset.one', 'v1', 1,
                '2026-03-02T00:00:00Z', 'researcher', 'reviewed_for_scope', 99,
                'profile-2', '{\"version\":2}', 'algorithm-2', '{\"inputs\":[\"replaced\"]}', 1
             )",
            [],
        );
        assert!(
            assessment_replace.is_err(),
            "INSERT OR REPLACE must not replace a sealed assessment: {assessment_replace:?}"
        );

        let factor_replace = connection.execute(
            "INSERT OR REPLACE INTO assessment_factor_ratings (
                assessment_id, factor_key, rationale, references_json
             ) VALUES (
                'assessment-immutable', 'evidence_quality', 'Replaced rationale', '[]'
             )",
            [],
        );
        assert!(
            factor_replace.is_err(),
            "INSERT OR REPLACE must not replace a sealed factor row: {factor_replace:?}"
        );

        let claim_replace = connection.execute(
            "INSERT OR REPLACE INTO assessment_claims (
                assessment_id, dataset_id, claim_key, claim_record_id, scope_role, outcome,
                rationale
             ) VALUES (
                'assessment-immutable', 'dataset.one', 'claim:a', 'claim-a',
                'core', 'contradicted', 'Replaced claim rationale'
             )",
            [],
        );
        assert!(
            claim_replace.is_err(),
            "INSERT OR REPLACE must not replace a sealed claim input: {claim_replace:?}"
        );

        let stored_snapshot: String = connection
            .query_row(
                "SELECT input_snapshot_json FROM research_assessments
                 WHERE assessment_id = 'sealed-assessment-replace'",
                [],
                |row| row.get(0),
            )
            .expect("read original sealed assessment");
        let stored_factor_rationale: String = connection
            .query_row(
                "SELECT rationale FROM assessment_factor_ratings
                 WHERE assessment_id = 'assessment-immutable'
                   AND factor_key = 'evidence_quality'",
                [],
                |row| row.get(0),
            )
            .expect("read original sealed factor");
        let stored_claim: (String, String) = connection
            .query_row(
                "SELECT outcome, rationale FROM assessment_claims
                 WHERE assessment_id = 'assessment-immutable' AND claim_record_id = 'claim-a'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read original sealed claim input");
        assert_eq!(stored_snapshot, "{\"inputs\":[]}");
        assert_eq!(stored_factor_rationale, "Original factor rationale");
        assert_eq!(
            stored_claim,
            (
                "supported".to_string(),
                "Original claim rationale".to_string()
            )
        );

        assert!(
            connection
                .execute(
                    "UPDATE research_assessments SET score = 50
                     WHERE assessment_id = 'assessment-immutable'",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "INSERT INTO assessment_factor_ratings (assessment_id, factor_key)
                     VALUES ('assessment-immutable', 'provenance')",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "DELETE FROM research_assessments WHERE assessment_id = 'assessment-immutable'",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "UPDATE assessment_factor_ratings SET rationale = 'changed'
                     WHERE assessment_id = 'assessment-immutable'",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "DELETE FROM assessment_factor_ratings
                     WHERE assessment_id = 'assessment-immutable'",
                    [],
                )
                .is_err()
        );
    }

    #[test]
    fn dataset_identity_constraints_enforce_dataset_keys_kinds_and_record_identity() {
        let connection = initialized_connection();
        insert_record(&connection, "mapped-source", "source");
        connection
            .execute(
                "INSERT INTO sources (record_id, record_kind, source_kind, verification_status)
                 VALUES ('mapped-source', 'source', 'web_page', 'verified')",
                [],
            )
            .expect("insert mapped source");
        connection
            .execute(
                "INSERT INTO research_datasets (dataset_id) VALUES ('dataset.one')",
                [],
            )
            .expect("insert dataset identity");
        connection
            .execute(
                "INSERT INTO dataset_record_mappings (
                    dataset_id, record_key, record_kind, record_id, content_snapshot_json
                 ) VALUES (
                    'dataset.one', 'dataset.one:source:report', 'source',
                    'mapped-source', '{}'
                 )",
                [],
            )
            .expect("insert valid record mapping");

        connection
            .execute(
                "INSERT INTO dataset_record_mappings (
                    dataset_id, record_key, record_kind, record_id, content_snapshot_json
                 ) VALUES (
                    'dataset.one', 'dataset.one:source:wrong-kind', 'claim',
                    'mapped-source', '{}'
                 )",
                [],
            )
            .expect_err("mapping kind must match the existing record kind");
        connection
            .execute(
                "INSERT INTO dataset_record_mappings (
                    dataset_id, record_key, record_kind, record_id, content_snapshot_json
                 ) VALUES (
                    'missing-dataset', 'missing-dataset:source:report', 'source',
                    'mapped-source', '{}'
                 )",
                [],
            )
            .expect_err("mapping must reference an existing dataset");
        connection
            .execute(
                "INSERT INTO dataset_record_mappings (
                    dataset_id, record_key, record_kind, record_id, content_snapshot_json
                 ) VALUES (
                    'dataset.one', 'dataset.one:history:entry', 'history',
                    'mapped-source', '{}'
                 )",
                [],
            )
            .expect_err("history records cannot be dataset-mapped");
        connection
            .execute(
                "INSERT INTO dataset_record_mappings (
                    dataset_id, record_key, record_kind, record_id, content_snapshot_json
                 ) VALUES (
                    'dataset.one', 'dataset.one:source:second-key', 'source',
                    'mapped-source', '{}'
                 )",
                [],
            )
            .expect_err("one dataset record cannot be mapped to multiple stable keys");
        insert_record(&connection, "another-source", "source");
        connection
            .execute(
                "INSERT INTO sources (record_id, record_kind, source_kind, verification_status)
                 VALUES ('another-source', 'source', 'web_page', 'verified')",
                [],
            )
            .expect("insert second source");
        connection
            .execute(
                "INSERT INTO dataset_record_mappings (
                    dataset_id, record_key, record_kind, record_id, content_snapshot_json
                 ) VALUES (
                    'dataset.one', 'dataset.one:source:report', 'source',
                    'another-source', '{}'
                 )",
                [],
            )
            .expect_err("stable key must be unique within its dataset");
        connection
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES (
                    'dataset.one', '1.0.0', 1, 'xenoscope-research-dataset', 1,
                    '2026-01-01T00:00:00Z', '{}', '[]'
                 )",
                [],
            )
            .expect("insert successful import ledger row");
        connection
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES (
                    'dataset.one', '1.0.0', 2, 'xenoscope-research-dataset', 1,
                    '2026-01-02T00:00:00Z', '{}', '[]'
                 )",
                [],
            )
            .expect_err("dataset version must be unique");
    }

    #[test]
    fn dataset_identity_json_columns_reject_invalid_json() {
        let connection = initialized_connection();
        connection
            .execute(
                "INSERT INTO research_datasets (dataset_id) VALUES ('dataset.json')",
                [],
            )
            .expect("insert dataset identity");

        connection
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES (
                    'dataset.json', '1.0.0', 1, 'xenoscope-research-dataset', 1,
                    '2026-01-01T00:00:00Z', '{invalid', '[]'
                 )",
                [],
            )
            .expect_err("metadata must be valid JSON");
        connection
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES (
                    'dataset.json', '1.0.0', 2, 'xenoscope-research-dataset', 1,
                    '2026-01-01T00:00:00Z', '{}', '{invalid'
                 )",
                [],
            )
            .expect_err("record manifest must be valid JSON");
        insert_record(&connection, "json-source", "source");
        connection
            .execute(
                "INSERT INTO sources (record_id, record_kind, source_kind, verification_status)
                 VALUES ('json-source', 'source', 'web_page', 'verified')",
                [],
            )
            .expect("insert source for mapping");
        connection
            .execute(
                "INSERT INTO dataset_record_mappings (
                    dataset_id, record_key, record_kind, record_id, content_snapshot_json
                 ) VALUES (
                    'dataset.json', 'dataset.json:source:report', 'source',
                    'json-source', '{invalid'
                 )",
                [],
            )
            .expect_err("record snapshot must be valid JSON");
    }

    #[test]
    fn claims_require_existing_sources_and_typed_relationship_endpoints() {
        let connection = initialized_connection();
        insert_record(&connection, "claim-1", "claim");
        connection
            .execute(
                "INSERT INTO claims (
                    record_id, record_kind, assertion_text, category,
                    source_id, source_locator_kind, source_locator_json, review_status
                 ) VALUES (
                    'claim-1', 'claim', 'Example assertion', 'context',
                    'missing-source', 'page', '{\"page\": 1}', 'unreviewed'
                 )",
                [],
            )
            .expect_err("claim without a source must fail");

        insert_record(&connection, "source-1", "source");
        connection
            .execute(
                "INSERT INTO sources (record_id, record_kind, source_kind, verification_status)
                 VALUES ('source-1', 'source', 'web_page', 'unverified')",
                [],
            )
            .expect("insert source");
        connection
            .execute(
                "INSERT INTO claims (
                    record_id, record_kind, assertion_text, category,
                    source_id, source_locator_kind, source_locator_json, review_status
                 ) VALUES (
                    'claim-1', 'claim', 'Example assertion', 'context',
                    'source-1', 'page', '{\"page\": 1}', 'unreviewed'
                 )",
                [],
            )
            .expect("insert source-attributed claim");

        insert_record(&connection, "relationship-1", "relationship");
        connection
            .execute(
                "INSERT INTO relationships (
                    record_id, record_kind, relationship_kind,
                    from_record_id, from_record_kind, to_record_id, to_record_kind
                 ) VALUES (
                    'relationship-1', 'relationship', 'corrects',
                    'source-1', 'claim', 'claim-1', 'claim'
                 )",
                [],
            )
            .expect_err("relationship endpoint kind must match its record");
    }

    #[test]
    fn discrepancies_and_findings_reference_existing_typed_records() {
        let connection = initialized_connection();
        insert_record(&connection, "discrepancy-1", "discrepancy");
        connection
            .execute(
                "INSERT INTO discrepancies (record_id, record_kind, description, status)
                 VALUES ('discrepancy-1', 'discrepancy', 'A documented difference', 'open')",
                [],
            )
            .expect("insert discrepancy");
        connection
            .execute(
                "INSERT INTO discrepancy_claims (discrepancy_id, claim_id)
                 VALUES ('discrepancy-1', 'missing-claim')",
                [],
            )
            .expect_err("discrepancy claim reference must exist");

        insert_record(&connection, "finding-1", "finding");
        connection
            .execute(
                "INSERT INTO findings (
                    record_id, record_kind, assessment, attribution_kind, review_status
                 ) VALUES ('finding-1', 'finding', 'Assessment text', 'researcher', 'unreviewed')",
                [],
            )
            .expect("insert finding");
        connection
            .execute(
                "INSERT INTO finding_claim_references (finding_id, claim_id, reference_role)
                 VALUES ('finding-1', 'missing-claim', 'supporting')",
                [],
            )
            .expect_err("finding claim reference must exist");
        connection
            .execute(
                "INSERT INTO finding_evidence_references (finding_id, evidence_item_id, reference_role)
                 VALUES ('finding-1', 'missing-evidence', 'conflicting')",
                [],
            )
            .expect_err("finding evidence reference must exist");
    }

    #[test]
    fn failed_migration_rolls_back_schema_and_version() {
        let mut connection = initialized_connection();
        let error = apply_migration(
            &mut connection,
            LATEST_SCHEMA_VERSION + 1,
            "CREATE TABLE migration_probe (id TEXT); INVALID SQL;",
        )
        .expect_err("invalid migration must fail");

        assert!(error.contains("rolled back"));
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version");
        let probe_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'migration_probe')",
                [],
                |row| row.get(0),
            )
            .expect("check rolled-back table");

        assert_eq!(version, LATEST_SCHEMA_VERSION);
        assert!(!probe_exists);
    }

    #[test]
    fn source_verification_and_claim_review_are_separate_fields() {
        let connection = initialized_connection();
        let source_columns: Vec<String> = connection
            .prepare("PRAGMA table_info(sources)")
            .expect("prepare source schema query")
            .query_map([], |row| row.get(1))
            .expect("query source columns")
            .collect::<Result<_, _>>()
            .expect("collect source columns");
        let claim_columns: Vec<String> = connection
            .prepare("PRAGMA table_info(claims)")
            .expect("prepare claim schema query")
            .query_map([], |row| row.get(1))
            .expect("query claim columns")
            .collect::<Result<_, _>>()
            .expect("collect claim columns");

        assert!(source_columns.contains(&"verification_status".to_string()));
        assert!(!source_columns.contains(&"review_status".to_string()));
        assert!(claim_columns.contains(&"review_status".to_string()));
        assert!(!claim_columns.contains(&"verification_status".to_string()));
    }

    #[test]
    fn newer_database_schema_is_rejected() {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .pragma_update(None, "user_version", LATEST_SCHEMA_VERSION + 1)
            .expect("set future schema version");

        let error = initialize(&mut connection).expect_err("future schema must be rejected");

        assert!(error.contains("newer than the supported version"));
    }

    #[test]
    fn record_history_cannot_be_updated_or_deleted() {
        let connection = initialized_connection();
        insert_record(&connection, "source-1", "source");
        connection
            .execute(
                "INSERT INTO sources (record_id, record_kind, source_kind, verification_status)
                 VALUES ('source-1', 'source', 'web_page', 'unverified')",
                [],
            )
            .expect("insert source");
        insert_record(&connection, "history-1", "history");
        connection
            .execute(
                "INSERT INTO record_history (
                    record_id, record_kind, subject_record_id, subject_record_kind,
                    event_kind, summary
                 ) VALUES (
                    'history-1', 'history', 'source-1', 'source', 'record_created', 'Created'
                 )",
                [],
            )
            .expect("insert history event");

        assert!(
            connection
                .execute(
                    "UPDATE record_history SET summary = 'Changed' WHERE record_id = 'history-1'",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "UPDATE records SET created_by_name = 'Changed' WHERE record_id = 'history-1'",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "DELETE FROM record_history WHERE record_id = 'history-1'",
                    []
                )
                .is_err()
        );
    }

    #[test]
    fn source_creation_and_retrieval_preserve_fields_and_provenance() {
        let database = test_database();
        let created = database
            .create_source(sample_source_input())
            .expect("create source");
        let retrieved = database
            .get_source(&created.id)
            .expect("retrieve source")
            .expect("source exists");

        assert_eq!(retrieved, created);
        assert_eq!(retrieved.created_by.kind, ActorKind::Researcher);
        assert!(retrieved.created_at.ends_with('Z'));
        assert_eq!(record_count(&database, "source"), 1);
        assert_eq!(record_count(&database, "history"), 1);
    }

    #[test]
    fn claim_creation_and_retrieval_preserve_locator_confidence_and_attribution() {
        let database = test_database();
        let source = database
            .create_source(sample_source_input())
            .expect("create source");
        let created = database
            .create_claim(sample_claim_input(source.id.clone()))
            .expect("create claim");
        let retrieved = database
            .get_claim(&created.id)
            .expect("retrieve claim")
            .expect("claim exists");

        assert_eq!(retrieved, created);
        assert_eq!(retrieved.source_id, source.id);
        assert_eq!(
            retrieved.source_locator,
            SourceLocator::Section {
                section: "Methods".to_string(),
                paragraph: Some(2),
            }
        );
        assert_eq!(
            retrieved
                .author_stated_confidence
                .expect("confidence stored")
                .value_as_stated,
            "High confidence"
        );
        assert_eq!(retrieved.review.status, ClaimReviewStatus::Unreviewed);
        assert_eq!(record_count(&database, "claim"), 1);
        assert_eq!(record_count(&database, "history"), 2);
    }

    #[test]
    fn claim_rejects_missing_sources_and_invalid_locators_without_writing() {
        let database = test_database();
        let missing_source_id = Uuid::new_v4().to_string();
        let error = database
            .create_claim(sample_claim_input(missing_source_id))
            .expect_err("missing source must be rejected");

        assert!(error.contains("does not exist"));
        assert_eq!(record_count(&database, "claim"), 0);
        assert_eq!(record_count(&database, "history"), 0);

        let mut invalid_locator_input = sample_claim_input(Uuid::new_v4().to_string());
        invalid_locator_input.source_locator = SourceLocator::Page { page: 0 };
        let error = database
            .create_claim(invalid_locator_input)
            .expect_err("zero page locator must be rejected");

        assert!(error.contains("greater than zero"));
        assert_eq!(record_count(&database, "claim"), 0);

        let malformed_locator =
            serde_json::from_str::<SourceLocator>(r#"{"kind":"page","paragraph":3}"#);
        assert!(malformed_locator.is_err());
        assert!(
            SourceLocator::Section {
                section: "  ".to_string(),
                paragraph: None,
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn failed_creation_history_rolls_back_source_and_claim_records() {
        let database = test_database();
        let source = database
            .create_source(sample_source_input())
            .expect("create source");
        database
            .connection()
            .expect("lock database")
            .execute_batch(
                "CREATE TRIGGER reject_creation_history
                 BEFORE INSERT ON record_history
                 BEGIN
                     SELECT RAISE(ABORT, 'test history failure');
                 END;",
            )
            .expect("install failing test trigger");

        assert!(database.create_source(sample_source_input()).is_err());
        assert_eq!(record_count(&database, "source"), 1);

        let claims_before = record_count(&database, "claim");
        assert!(
            database
                .create_claim(sample_claim_input(source.id))
                .is_err()
        );
        assert_eq!(record_count(&database, "claim"), claims_before);
        assert_eq!(record_count(&database, "history"), 1);
    }

    #[test]
    fn created_sources_and_claims_persist_after_database_reopen() {
        let path: PathBuf = std::env::temp_dir().join(format!(
            "xenoscope-record-persistence-{}.sqlite3",
            Uuid::new_v4()
        ));
        let (source, claim) = {
            let database = Database::open(&path).expect("open temporary database");
            let source = database
                .create_source(sample_source_input())
                .expect("create persistent source");
            let claim = database
                .create_claim(sample_claim_input(source.id.clone()))
                .expect("create persistent claim");
            (source, claim)
        };

        let reopened = Database::open(&path).expect("reopen temporary database");
        assert_eq!(
            reopened
                .get_source(&source.id)
                .expect("retrieve persistent source")
                .expect("source exists after reopen"),
            source
        );
        assert_eq!(
            reopened
                .get_claim(&claim.id)
                .expect("retrieve persistent claim")
                .expect("claim exists after reopen"),
            claim
        );
        drop(reopened);
        fs::remove_file(path).expect("remove temporary database");
    }
}
