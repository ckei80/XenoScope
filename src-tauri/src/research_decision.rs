use crate::research_records::{ActorKind, RecordActor};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{error::Error, fmt};

const LOCAL_RESEARCHER: &str = "Local researcher";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DecisionKind {
    IncidentPair,
    SourceDependency,
    SourceLineage,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DecisionOutcome {
    SameEvent,
    RelatedDistinct,
    NotSameEvent,
    Dependent,
    AssessedIndependent,
    Unknown,
    CopiedFrom,
    DerivedFrom,
    Quotes,
    Repeats,
    References,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LineageType {
    CopiedFrom,
    DerivedFrom,
    Quotes,
    Repeats,
    References,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SuggestionStatus {
    Proposed,
    Dismissed,
    Superseded,
    Resolved,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DecisionEventKind {
    Recorded,
    Superseded,
    Reversed,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SuggestionOriginKind {
    Researcher,
    System,
    Ai,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DecisionRecordKind {
    Incident,
    Source,
    EvidenceItem,
    Claim,
    Relationship,
    Discrepancy,
    Finding,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecordTarget {
    pub record_id: String,
    pub record_kind: DecisionRecordKind,
}

impl RecordTarget {
    fn validate(&self) -> Result<(), ResearchDecisionError> {
        require_nonblank("Record ID", &self.record_id)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DecisionTargetPair {
    pub decision_kind: DecisionKind,
    pub record_a: RecordTarget,
    pub record_b: RecordTarget,
    pub lineage_type: Option<LineageType>,
}

impl DecisionTargetPair {
    pub(crate) fn new(
        decision_kind: DecisionKind,
        first: RecordTarget,
        second: RecordTarget,
        lineage_type: Option<LineageType>,
    ) -> Result<Self, ResearchDecisionError> {
        first.validate()?;
        second.validate()?;
        if first.record_id == second.record_id {
            return Err(ResearchDecisionError::IdenticalTargets);
        }

        match decision_kind {
            DecisionKind::IncidentPair => {
                if first.record_kind != DecisionRecordKind::Incident
                    || second.record_kind != DecisionRecordKind::Incident
                {
                    return Err(ResearchDecisionError::InvalidTargetKinds { decision_kind });
                }
                if lineage_type.is_some() {
                    return Err(ResearchDecisionError::LineageTypeNotAllowed);
                }
            }
            DecisionKind::SourceDependency => {
                if first.record_kind != DecisionRecordKind::Source
                    || second.record_kind != DecisionRecordKind::Source
                {
                    return Err(ResearchDecisionError::InvalidTargetKinds { decision_kind });
                }
                if lineage_type.is_some() {
                    return Err(ResearchDecisionError::LineageTypeNotAllowed);
                }
            }
            DecisionKind::SourceLineage => {
                if first.record_kind != DecisionRecordKind::Source
                    || second.record_kind != DecisionRecordKind::Source
                {
                    return Err(ResearchDecisionError::InvalidTargetKinds { decision_kind });
                }
                if lineage_type.is_none() {
                    return Err(ResearchDecisionError::LineageTypeRequired);
                }
            }
        }

        let (record_a, record_b) = match decision_kind {
            DecisionKind::SourceLineage => (first, second),
            DecisionKind::IncidentPair | DecisionKind::SourceDependency => {
                if first.record_id < second.record_id {
                    (first, second)
                } else {
                    (second, first)
                }
            }
        };
        Ok(Self {
            decision_kind,
            record_a,
            record_b,
            lineage_type,
        })
    }

    pub(crate) fn validate(&self) -> Result<(), ResearchDecisionError> {
        let normalized = Self::new(
            self.decision_kind,
            self.record_a.clone(),
            self.record_b.clone(),
            self.lineage_type,
        )?;
        if normalized != *self {
            return Err(ResearchDecisionError::PairNotCanonical);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SuggestionOrigin {
    pub kind: SuggestionOriginKind,
    pub display_name: Option<String>,
    pub generator_version: Option<String>,
}

impl SuggestionOrigin {
    pub(crate) fn validate(&self) -> Result<(), ResearchDecisionError> {
        for (field, value) in [
            ("displayName", self.display_name.as_deref()),
            ("generatorVersion", self.generator_version.as_deref()),
        ] {
            if value.is_some_and(|value| value.trim().is_empty()) {
                return Err(ResearchDecisionError::InvalidSuggestionOrigin(field));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ResearchReferenceInput {
    pub record: RecordTarget,
    pub role: String,
    #[serde(default)]
    pub locator: Option<Value>,
    #[serde(default)]
    pub note: Option<String>,
}

impl ResearchReferenceInput {
    pub(crate) fn validate(&self) -> Result<(), ResearchDecisionError> {
        if self.record.record_id.trim().is_empty() {
            return Err(ResearchDecisionError::InvalidReference("recordId"));
        }
        if self.role.trim().is_empty() {
            return Err(ResearchDecisionError::InvalidReference("role"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResearchReferenceDto {
    pub reference_id: String,
    pub record: RecordTarget,
    pub role: String,
    pub locator: Option<Value>,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CreateSuggestionInput {
    pub target: DecisionTargetPair,
    pub proposed_outcome: Option<DecisionOutcome>,
    pub origin: SuggestionOrigin,
    pub actor: RecordActor,
    pub summary: String,
    #[serde(default)]
    pub references: Vec<ResearchReferenceInput>,
}

impl CreateSuggestionInput {
    pub(crate) fn validate(&self) -> Result<(), ResearchDecisionError> {
        self.target.validate()?;
        if let Some(outcome) = self.proposed_outcome {
            validate_outcome(&self.target, outcome)?;
        }
        self.origin.validate()?;
        if self
            .actor
            .display_name
            .as_deref()
            .is_some_and(|name| name.trim().is_empty())
        {
            return Err(ResearchDecisionError::InvalidSuggestionActor);
        }
        require_nonblank("Suggestion summary", &self.summary)?;
        validate_references(&self.references)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SuggestionEventDto {
    pub event_id: String,
    pub status: SuggestionStatus,
    pub predecessor_event_id: Option<String>,
    pub occurred_at: String,
    pub actor: RecordActor,
    pub rationale: Option<String>,
    pub replacement_suggestion_id: Option<String>,
    pub decision_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SuggestionDto {
    pub suggestion_id: String,
    pub target: DecisionTargetPair,
    pub proposed_outcome: Option<DecisionOutcome>,
    pub origin: SuggestionOrigin,
    pub created_at: String,
    pub summary: String,
    pub references: Vec<ResearchReferenceDto>,
    pub status: SuggestionStatus,
    pub events: Vec<SuggestionEventDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CreateDecisionInput {
    pub target: DecisionTargetPair,
    pub outcome: DecisionOutcome,
    pub rationale: String,
    #[serde(default)]
    pub origin_suggestion_id: Option<String>,
    #[serde(default)]
    pub references: Vec<ResearchReferenceInput>,
}

impl CreateDecisionInput {
    pub(crate) fn validate(&self) -> Result<(), ResearchDecisionError> {
        self.target.validate()?;
        validate_outcome(&self.target, self.outcome)?;
        require_rationale(&self.rationale)?;
        validate_optional_nonblank(
            "Originating suggestion ID",
            self.origin_suggestion_id.as_deref(),
        )?;
        validate_references(&self.references)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ResolveSuggestionInput {
    pub outcome: DecisionOutcome,
    pub rationale: String,
    #[serde(default)]
    pub references: Vec<ResearchReferenceInput>,
}

impl ResolveSuggestionInput {
    pub(crate) fn validate(
        &self,
        target: &DecisionTargetPair,
    ) -> Result<(), ResearchDecisionError> {
        target.validate()?;
        validate_outcome(target, self.outcome)?;
        require_rationale(&self.rationale)?;
        validate_references(&self.references)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DecisionConclusionInput {
    pub outcome: DecisionOutcome,
    pub rationale: String,
    #[serde(default)]
    pub references: Vec<ResearchReferenceInput>,
}

impl DecisionConclusionInput {
    pub(crate) fn validate(
        &self,
        target: &DecisionTargetPair,
    ) -> Result<(), ResearchDecisionError> {
        target.validate()?;
        validate_outcome(target, self.outcome)?;
        require_rationale(&self.rationale)?;
        validate_references(&self.references)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DecisionEventDto {
    pub event_id: String,
    pub event_kind: DecisionEventKind,
    pub outcome: Option<DecisionOutcome>,
    pub predecessor_event_id: Option<String>,
    pub occurred_at: String,
    pub actor: RecordActor,
    pub rationale: String,
    pub references: Vec<ResearchReferenceDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResearchDecisionDto {
    pub decision_id: String,
    pub target: DecisionTargetPair,
    pub created_at: String,
    pub origin_suggestion_id: Option<String>,
    pub current_head: Option<DecisionEventDto>,
    pub unresolved: bool,
    pub history: Vec<DecisionEventDto>,
}

pub(crate) fn researcher_decision_actor() -> RecordActor {
    RecordActor {
        kind: ActorKind::Researcher,
        display_name: Some(LOCAL_RESEARCHER.to_string()),
    }
}

pub(crate) fn validate_outcome(
    target: &DecisionTargetPair,
    outcome: DecisionOutcome,
) -> Result<(), ResearchDecisionError> {
    target.validate()?;
    let compatible = match target.decision_kind {
        DecisionKind::IncidentPair => matches!(
            outcome,
            DecisionOutcome::SameEvent
                | DecisionOutcome::RelatedDistinct
                | DecisionOutcome::NotSameEvent
        ),
        DecisionKind::SourceDependency => matches!(
            outcome,
            DecisionOutcome::Dependent
                | DecisionOutcome::AssessedIndependent
                | DecisionOutcome::Unknown
        ),
        DecisionKind::SourceLineage => {
            validate_lineage_outcome(target.lineage_type, outcome)?;
            true
        }
    };
    if compatible {
        Ok(())
    } else {
        Err(ResearchDecisionError::IncompatibleOutcome {
            decision_kind: target.decision_kind,
            outcome,
        })
    }
}

pub(crate) fn validate_lineage_outcome(
    lineage_type: Option<LineageType>,
    outcome: DecisionOutcome,
) -> Result<(), ResearchDecisionError> {
    let Some(lineage_type) = lineage_type else {
        return Err(ResearchDecisionError::LineageTypeRequired);
    };
    let matches = matches!(
        (lineage_type, outcome),
        (LineageType::CopiedFrom, DecisionOutcome::CopiedFrom)
            | (LineageType::DerivedFrom, DecisionOutcome::DerivedFrom)
            | (LineageType::Quotes, DecisionOutcome::Quotes)
            | (LineageType::Repeats, DecisionOutcome::Repeats)
            | (LineageType::References, DecisionOutcome::References)
    );
    if matches {
        Ok(())
    } else {
        Err(ResearchDecisionError::LineageOutcomeMismatch {
            lineage_type,
            outcome,
        })
    }
}

pub(crate) fn validate_decision_actor(actor: &RecordActor) -> Result<(), ResearchDecisionError> {
    if actor.kind != ActorKind::Researcher {
        return Err(ResearchDecisionError::ResearcherRequired);
    }
    Ok(())
}

pub(crate) fn validate_decision_event_shape(
    event_kind: DecisionEventKind,
    outcome: Option<DecisionOutcome>,
    target: &DecisionTargetPair,
    actor: &RecordActor,
    rationale: &str,
) -> Result<(), ResearchDecisionError> {
    target.validate()?;
    validate_decision_actor(actor)?;
    require_rationale(rationale)?;
    match (event_kind, outcome) {
        (DecisionEventKind::Reversed, None) => Ok(()),
        (DecisionEventKind::Recorded | DecisionEventKind::Superseded, Some(outcome)) => {
            validate_outcome(target, outcome)
        }
        _ => Err(ResearchDecisionError::InvalidEventShape),
    }
}

pub(crate) fn validate_suggestion_conversion(
    suggestion: &SuggestionDto,
    decision: &CreateDecisionInput,
) -> Result<(), ResearchDecisionError> {
    if suggestion.status != SuggestionStatus::Proposed
        || decision.origin_suggestion_id.as_deref() != Some(&suggestion.suggestion_id)
        || suggestion.target != decision.target
    {
        return Err(ResearchDecisionError::SuggestionConversionMismatch);
    }
    suggestion.target.validate()?;
    if let Some(outcome) = suggestion.proposed_outcome {
        validate_outcome(&suggestion.target, outcome)?;
    }
    decision.validate()
}

fn validate_references(references: &[ResearchReferenceInput]) -> Result<(), ResearchDecisionError> {
    for reference in references {
        reference.validate()?;
    }
    Ok(())
}

fn require_nonblank(field: &'static str, value: &str) -> Result<(), ResearchDecisionError> {
    if value.trim().is_empty() {
        Err(ResearchDecisionError::BlankField(field))
    } else {
        Ok(())
    }
}

fn require_rationale(value: &str) -> Result<(), ResearchDecisionError> {
    if value.trim().is_empty() {
        Err(ResearchDecisionError::BlankRationale)
    } else {
        Ok(())
    }
}

fn validate_optional_nonblank(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), ResearchDecisionError> {
    if let Some(value) = value {
        require_nonblank(field, value)?;
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ResearchDecisionError {
    BlankField(&'static str),
    BlankRationale,
    InvalidSuggestionOrigin(&'static str),
    InvalidSuggestionActor,
    InvalidReference(&'static str),
    InvalidTargetKinds {
        decision_kind: DecisionKind,
    },
    IdenticalTargets,
    LineageTypeRequired,
    LineageTypeNotAllowed,
    PairNotCanonical,
    IncompatibleOutcome {
        decision_kind: DecisionKind,
        outcome: DecisionOutcome,
    },
    LineageOutcomeMismatch {
        lineage_type: LineageType,
        outcome: DecisionOutcome,
    },
    ResearcherRequired,
    InvalidEventShape,
    SuggestionConversionMismatch,
}

impl fmt::Display for ResearchDecisionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BlankField(field) => write!(formatter, "{field} must not be blank"),
            Self::BlankRationale => write!(formatter, "Decision rationale must not be blank"),
            Self::InvalidSuggestionOrigin(field) => {
                write!(formatter, "Suggestion origin {field} must not be blank")
            }
            Self::InvalidSuggestionActor => {
                write!(formatter, "Suggestion event actor name must not be blank")
            }
            Self::InvalidReference(field) => write!(formatter, "Reference {field} is invalid"),
            Self::InvalidTargetKinds { decision_kind } => {
                write!(
                    formatter,
                    "Target record kinds are invalid for {decision_kind:?}"
                )
            }
            Self::IdenticalTargets => write!(formatter, "A relationship cannot target itself"),
            Self::LineageTypeRequired => {
                write!(formatter, "Source lineage requires a lineage type")
            }
            Self::LineageTypeNotAllowed => {
                write!(formatter, "Lineage type is only valid for source lineage")
            }
            Self::PairNotCanonical => write!(formatter, "Symmetric target pair is not normalized"),
            Self::IncompatibleOutcome {
                decision_kind,
                outcome,
            } => write!(
                formatter,
                "Outcome {outcome:?} is incompatible with {decision_kind:?}"
            ),
            Self::LineageOutcomeMismatch {
                lineage_type,
                outcome,
            } => write!(
                formatter,
                "Outcome {outcome:?} does not match lineage type {lineage_type:?}"
            ),
            Self::ResearcherRequired => {
                write!(formatter, "Reviewed decisions must be researcher-authored")
            }
            Self::InvalidEventShape => write!(
                formatter,
                "Decision event kind and outcome have an invalid combination"
            ),
            Self::SuggestionConversionMismatch => write!(
                formatter,
                "Decision does not match a proposed originating suggestion"
            ),
        }
    }
}

impl Error for ResearchDecisionError {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, to_value};

    fn target(
        decision_kind: DecisionKind,
        first_id: &str,
        first_kind: DecisionRecordKind,
        second_id: &str,
        second_kind: DecisionRecordKind,
        lineage_type: Option<LineageType>,
    ) -> DecisionTargetPair {
        DecisionTargetPair::new(
            decision_kind,
            RecordTarget {
                record_id: first_id.to_string(),
                record_kind: first_kind,
            },
            RecordTarget {
                record_id: second_id.to_string(),
                record_kind: second_kind,
            },
            lineage_type,
        )
        .expect("valid pair")
    }

    #[test]
    fn accepts_each_outcome_for_its_decision_kind() {
        let incident_pair = target(
            DecisionKind::IncidentPair,
            "incident-a",
            DecisionRecordKind::Incident,
            "incident-b",
            DecisionRecordKind::Incident,
            None,
        );
        for outcome in [
            DecisionOutcome::SameEvent,
            DecisionOutcome::RelatedDistinct,
            DecisionOutcome::NotSameEvent,
        ] {
            assert_eq!(validate_outcome(&incident_pair, outcome), Ok(()));
        }

        let dependency = target(
            DecisionKind::SourceDependency,
            "source-a",
            DecisionRecordKind::Source,
            "source-b",
            DecisionRecordKind::Source,
            None,
        );
        for outcome in [
            DecisionOutcome::Dependent,
            DecisionOutcome::AssessedIndependent,
            DecisionOutcome::Unknown,
        ] {
            assert_eq!(validate_outcome(&dependency, outcome), Ok(()));
        }

        for (lineage_type, outcome) in [
            (LineageType::CopiedFrom, DecisionOutcome::CopiedFrom),
            (LineageType::DerivedFrom, DecisionOutcome::DerivedFrom),
            (LineageType::Quotes, DecisionOutcome::Quotes),
            (LineageType::Repeats, DecisionOutcome::Repeats),
            (LineageType::References, DecisionOutcome::References),
        ] {
            let lineage = target(
                DecisionKind::SourceLineage,
                "source-a",
                DecisionRecordKind::Source,
                "source-b",
                DecisionRecordKind::Source,
                Some(lineage_type),
            );
            assert_eq!(validate_outcome(&lineage, outcome), Ok(()));
        }
    }

    #[test]
    fn rejects_outcomes_incompatible_with_decision_kind_or_lineage_type() {
        let incident_pair = target(
            DecisionKind::IncidentPair,
            "incident-a",
            DecisionRecordKind::Incident,
            "incident-b",
            DecisionRecordKind::Incident,
            None,
        );
        assert!(validate_outcome(&incident_pair, DecisionOutcome::Dependent).is_err());

        let dependency = target(
            DecisionKind::SourceDependency,
            "source-a",
            DecisionRecordKind::Source,
            "source-b",
            DecisionRecordKind::Source,
            None,
        );
        assert!(validate_outcome(&dependency, DecisionOutcome::SameEvent).is_err());

        let lineage = target(
            DecisionKind::SourceLineage,
            "source-a",
            DecisionRecordKind::Source,
            "source-b",
            DecisionRecordKind::Source,
            Some(LineageType::Quotes),
        );
        assert!(validate_outcome(&lineage, DecisionOutcome::Repeats).is_err());
    }

    #[test]
    fn normalizes_symmetric_pairs_and_preserves_lineage_direction() {
        let forward = target(
            DecisionKind::IncidentPair,
            "incident-z",
            DecisionRecordKind::Incident,
            "incident-a",
            DecisionRecordKind::Incident,
            None,
        );
        let reverse = target(
            DecisionKind::IncidentPair,
            "incident-a",
            DecisionRecordKind::Incident,
            "incident-z",
            DecisionRecordKind::Incident,
            None,
        );
        assert_eq!(forward, reverse);
        assert_eq!(forward.record_a.record_id, "incident-a");

        let lineage = target(
            DecisionKind::SourceLineage,
            "source-z",
            DecisionRecordKind::Source,
            "source-a",
            DecisionRecordKind::Source,
            Some(LineageType::Quotes),
        );
        assert_eq!(lineage.record_a.record_id, "source-z");
        assert_eq!(lineage.record_b.record_id, "source-a");
    }

    #[test]
    fn rejects_invalid_target_kinds_identical_targets_and_missing_lineage_type() {
        assert_eq!(
            DecisionTargetPair::new(
                DecisionKind::IncidentPair,
                RecordTarget {
                    record_id: "incident-a".to_string(),
                    record_kind: DecisionRecordKind::Incident,
                },
                RecordTarget {
                    record_id: "source-b".to_string(),
                    record_kind: DecisionRecordKind::Source,
                },
                None,
            ),
            Err(ResearchDecisionError::InvalidTargetKinds {
                decision_kind: DecisionKind::IncidentPair,
            })
        );
        assert_eq!(
            DecisionTargetPair::new(
                DecisionKind::SourceLineage,
                RecordTarget {
                    record_id: "source-a".to_string(),
                    record_kind: DecisionRecordKind::Source,
                },
                RecordTarget {
                    record_id: "source-b".to_string(),
                    record_kind: DecisionRecordKind::Source,
                },
                None,
            ),
            Err(ResearchDecisionError::LineageTypeRequired)
        );
        assert_eq!(
            DecisionTargetPair::new(
                DecisionKind::SourceDependency,
                RecordTarget {
                    record_id: "same".to_string(),
                    record_kind: DecisionRecordKind::Source,
                },
                RecordTarget {
                    record_id: "same".to_string(),
                    record_kind: DecisionRecordKind::Source,
                },
                None,
            ),
            Err(ResearchDecisionError::IdenticalTargets)
        );
        assert_eq!(
            DecisionTargetPair::new(
                DecisionKind::IncidentPair,
                RecordTarget {
                    record_id: "incident-a".to_string(),
                    record_kind: DecisionRecordKind::Incident,
                },
                RecordTarget {
                    record_id: "incident-b".to_string(),
                    record_kind: DecisionRecordKind::Incident,
                },
                Some(LineageType::Quotes),
            ),
            Err(ResearchDecisionError::LineageTypeNotAllowed)
        );
    }

    #[test]
    fn validates_suggestion_origin_and_keeps_it_separate_from_event_actor() {
        for kind in [
            SuggestionOriginKind::Researcher,
            SuggestionOriginKind::System,
            SuggestionOriginKind::Ai,
        ] {
            SuggestionOrigin {
                kind,
                display_name: None,
                generator_version: None,
            }
            .validate()
            .expect("valid suggestion origin");
        }
        assert!(serde_json::from_value::<SuggestionOriginKind>(json!("unknown")).is_err());
        assert_eq!(
            SuggestionOrigin {
                kind: SuggestionOriginKind::Ai,
                display_name: Some("  ".to_string()),
                generator_version: None,
            }
            .validate(),
            Err(ResearchDecisionError::InvalidSuggestionOrigin(
                "displayName"
            ))
        );

        let input = CreateSuggestionInput {
            target: target(
                DecisionKind::IncidentPair,
                "incident-a",
                DecisionRecordKind::Incident,
                "incident-b",
                DecisionRecordKind::Incident,
                None,
            ),
            proposed_outcome: Some(DecisionOutcome::SameEvent),
            origin: SuggestionOrigin {
                kind: SuggestionOriginKind::Ai,
                display_name: Some("Local model".to_string()),
                generator_version: Some("v1".to_string()),
            },
            actor: RecordActor {
                kind: ActorKind::Researcher,
                display_name: None,
            },
            summary: "Possible same event".to_string(),
            references: Vec::new(),
        };
        input
            .validate()
            .expect("valid independent origin and actor");
    }

    #[test]
    fn supplies_default_researcher_name_and_rejects_nonresearcher_decision_actor() {
        assert_eq!(
            researcher_decision_actor(),
            RecordActor {
                kind: ActorKind::Researcher,
                display_name: Some("Local researcher".to_string()),
            }
        );
        assert_eq!(
            validate_decision_actor(&RecordActor {
                kind: ActorKind::Ai,
                display_name: None,
            }),
            Err(ResearchDecisionError::ResearcherRequired)
        );
        assert!(validate_decision_actor(&researcher_decision_actor()).is_ok());
    }

    #[test]
    fn rejects_blank_rationale_and_invalid_reference_fields() {
        let target = target(
            DecisionKind::IncidentPair,
            "incident-a",
            DecisionRecordKind::Incident,
            "incident-b",
            DecisionRecordKind::Incident,
            None,
        );
        for rationale in ["", "  \n"] {
            assert_eq!(
                validate_decision_event_shape(
                    DecisionEventKind::Recorded,
                    Some(DecisionOutcome::SameEvent),
                    &target,
                    &researcher_decision_actor(),
                    rationale,
                ),
                Err(ResearchDecisionError::BlankRationale)
            );
        }
        assert_eq!(
            ResearchReferenceInput {
                record: RecordTarget {
                    record_id: " ".to_string(),
                    record_kind: DecisionRecordKind::Source,
                },
                role: "basis".to_string(),
                locator: None,
                note: None,
            }
            .validate(),
            Err(ResearchDecisionError::InvalidReference("recordId"))
        );
        assert_eq!(
            ResearchReferenceInput {
                record: RecordTarget {
                    record_id: "source-a".to_string(),
                    record_kind: DecisionRecordKind::Source,
                },
                role: " ".to_string(),
                locator: None,
                note: None,
            }
            .validate(),
            Err(ResearchDecisionError::InvalidReference("role"))
        );
        ResearchReferenceInput {
            record: RecordTarget {
                record_id: "source-a".to_string(),
                record_kind: DecisionRecordKind::Source,
            },
            role: "basis".to_string(),
            locator: Some(json!({"page": 3, "quote": ["text"]})),
            note: Some(" ".to_string()),
        }
        .validate()
        .expect("optional locator and note follow schema nullability");
    }

    #[test]
    fn validates_suggestion_conversion_by_identity_not_proposed_outcome() {
        let target = target(
            DecisionKind::IncidentPair,
            "incident-a",
            DecisionRecordKind::Incident,
            "incident-b",
            DecisionRecordKind::Incident,
            None,
        );
        let suggestion = SuggestionDto {
            suggestion_id: "suggestion-1".to_string(),
            target: target.clone(),
            proposed_outcome: Some(DecisionOutcome::SameEvent),
            origin: SuggestionOrigin {
                kind: SuggestionOriginKind::Ai,
                display_name: None,
                generator_version: None,
            },
            created_at: "2026-11-01T00:00:00Z".to_string(),
            summary: "Possible match".to_string(),
            references: Vec::new(),
            status: SuggestionStatus::Proposed,
            events: Vec::new(),
        };
        let decision = CreateDecisionInput {
            target: target.clone(),
            outcome: DecisionOutcome::RelatedDistinct,
            rationale: "Reviewed as related but distinct".to_string(),
            origin_suggestion_id: Some("suggestion-1".to_string()),
            references: Vec::new(),
        };
        validate_suggestion_conversion(&suggestion, &decision)
            .expect("review may reject the proposed outcome");

        let mut mismatched = decision;
        mismatched.origin_suggestion_id = Some("different-suggestion".to_string());
        assert_eq!(
            validate_suggestion_conversion(&suggestion, &mismatched),
            Err(ResearchDecisionError::SuggestionConversionMismatch)
        );
    }

    #[test]
    fn validates_decision_event_shape_without_thread_state() {
        let target = target(
            DecisionKind::IncidentPair,
            "incident-a",
            DecisionRecordKind::Incident,
            "incident-b",
            DecisionRecordKind::Incident,
            None,
        );
        let actor = researcher_decision_actor();
        assert!(
            validate_decision_event_shape(
                DecisionEventKind::Reversed,
                None,
                &target,
                &actor,
                "Withdrawing the conclusion",
            )
            .is_ok()
        );
        assert!(
            validate_decision_event_shape(
                DecisionEventKind::Reversed,
                Some(DecisionOutcome::SameEvent),
                &target,
                &actor,
                "Withdrawing the conclusion",
            )
            .is_err()
        );
        assert!(
            validate_decision_event_shape(
                DecisionEventKind::Recorded,
                None,
                &target,
                &actor,
                "Recording a conclusion",
            )
            .is_err()
        );
        assert!(
            validate_decision_event_shape(
                DecisionEventKind::Superseded,
                Some(DecisionOutcome::Dependent),
                &target,
                &actor,
                "Replacing a conclusion",
            )
            .is_err()
        );
    }

    #[test]
    fn serializes_domain_enums_and_inputs_with_expected_names() {
        assert_eq!(
            to_value(DecisionKind::SourceLineage).expect("serialize decision kind"),
            json!("source_lineage")
        );
        assert_eq!(
            to_value(DecisionOutcome::AssessedIndependent).expect("serialize outcome"),
            json!("assessed_independent")
        );
        let input = CreateDecisionInput {
            target: target(
                DecisionKind::IncidentPair,
                "incident-a",
                DecisionRecordKind::Incident,
                "incident-b",
                DecisionRecordKind::Incident,
                None,
            ),
            outcome: DecisionOutcome::SameEvent,
            rationale: "Reviewed".to_string(),
            origin_suggestion_id: None,
            references: Vec::new(),
        };
        let serialized = to_value(&input).expect("serialize decision input");
        assert_eq!(serialized["originSuggestionId"], Value::Null);
        assert_eq!(serialized["target"]["decisionKind"], json!("incident_pair"));
        assert_eq!(serialized["outcome"], json!("same_event"));

        let event = DecisionEventDto {
            event_id: "event-1".to_string(),
            event_kind: DecisionEventKind::Recorded,
            outcome: Some(DecisionOutcome::SameEvent),
            predecessor_event_id: None,
            occurred_at: "2026-11-01T00:00:00Z".to_string(),
            actor: researcher_decision_actor(),
            rationale: "Reviewed".to_string(),
            references: Vec::new(),
        };
        let decision = ResearchDecisionDto {
            decision_id: "decision-1".to_string(),
            target: input.target,
            created_at: "2026-11-01T00:00:00Z".to_string(),
            origin_suggestion_id: None,
            current_head: Some(event),
            unresolved: false,
            history: Vec::new(),
        };
        let serialized = to_value(decision).expect("serialize decision DTO");
        assert_eq!(serialized["currentHead"]["eventKind"], json!("recorded"));
        assert_eq!(
            serialized["currentHead"]["actor"]["displayName"],
            json!(LOCAL_RESEARCHER)
        );
        assert_eq!(serialized["unresolved"], json!(false));

        let suggestion = SuggestionDto {
            suggestion_id: "suggestion-1".to_string(),
            target: target(
                DecisionKind::IncidentPair,
                "incident-a",
                DecisionRecordKind::Incident,
                "incident-b",
                DecisionRecordKind::Incident,
                None,
            ),
            proposed_outcome: Some(DecisionOutcome::SameEvent),
            origin: SuggestionOrigin {
                kind: SuggestionOriginKind::Ai,
                display_name: Some("Local model".to_string()),
                generator_version: Some("v1".to_string()),
            },
            created_at: "2026-11-01T00:00:00Z".to_string(),
            summary: "Possible match".to_string(),
            references: Vec::new(),
            status: SuggestionStatus::Proposed,
            events: vec![SuggestionEventDto {
                event_id: "suggestion-event-1".to_string(),
                status: SuggestionStatus::Proposed,
                predecessor_event_id: None,
                occurred_at: "2026-11-01T00:00:00Z".to_string(),
                actor: RecordActor {
                    kind: ActorKind::Ai,
                    display_name: Some("Local model".to_string()),
                },
                rationale: None,
                replacement_suggestion_id: None,
                decision_id: None,
            }],
        };
        let serialized = to_value(suggestion).expect("serialize suggestion DTO");
        assert_eq!(serialized["suggestionId"], json!("suggestion-1"));
        assert_eq!(serialized["events"][0]["predecessorEventId"], Value::Null);
        assert_eq!(serialized["origin"]["generatorVersion"], json!("v1"));
    }
}
