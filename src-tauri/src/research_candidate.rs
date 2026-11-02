//! Deterministic incident-pair retrieval over caller-provided typed summaries.
//!
//! The indexes are inverted and emit pairs only within shared keys. Text terms
//! appearing in more than max(3, ceil(incident_count / 4)) records are suppressed.
//! Postings over 128 incidents are skipped and reported. Periods spanning more
//! than 120 calendar months are not time-indexed and are reported.
//! These explicit suppressions bound dense buckets; they may reduce recall, so the
//! caller must surface `suppressed_buckets` rather than treating it as exhaustive.
//! Index construction is linear in input terms and indexed period months, plus
//! pair expansion for retained postings (at most 128 members per posting); output
//! growth follows the number of distinct pairs selected by those postings.

use crate::research_dataset::{DatasetLocation, DatasetTimePeriod, DatasetTimePrecision};
use crate::research_records::ClaimCategory;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

const MAX_INDEX_POSTING: usize = 128;
const MAX_PERIOD_MONTHS: i64 = 120;
const STOP_WORDS: &[&str] = &[
    "about", "after", "again", "also", "and", "are", "been", "before", "being", "but", "for",
    "from", "had", "has", "have", "into", "its", "near", "not", "of", "on", "one", "over", "said",
    "that", "the", "their", "there", "they", "this", "was", "were", "with",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IncidentCandidateInput {
    pub identity: String,
    pub label: String,
    pub description: Option<String>,
    pub time_period: Option<DatasetTimePeriod>,
    pub location: Option<DatasetLocation>,
    pub source_ids: Vec<String>,
    pub claims: Vec<ClaimCandidateInput>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ClaimCandidateInput {
    pub record_id: String,
    pub dataset_id: String,
    pub dataset_key: String,
    pub text: String,
    pub category: ClaimCategory,
    pub source_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct SourceDependencyInput {
    pub relationship_record_id: String,
    pub from_source_id: String,
    pub to_source_id: String,
    pub kind: SourceDependencyKind,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct ClaimTextProvenance {
    pub claim_id: String,
    pub dataset_id: String,
    pub dataset_key: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct SharedTextTermProvenance {
    pub term: String,
    pub left_claims: Vec<ClaimTextProvenance>,
    pub right_claims: Vec<ClaimTextProvenance>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) enum SourceDependencyKind {
    Quotes,
    Repeats,
    DerivedFrom,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CandidateGenerationInput {
    pub incidents: Vec<IncidentCandidateInput>,
    pub source_dependencies: Vec<SourceDependencyInput>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) enum RetrievalPass {
    TimeOverlap,
    NamedPlace,
    SharedSource,
    SharedDependentSource,
    Text,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) enum RecordedTimePrecision {
    ExactDay,
    ApproximateDay,
    Month,
    Year,
    BoundedPeriod,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) enum SupportingSignal {
    OverlappingPeriods {
        left_precision: RecordedTimePrecision,
        right_precision: RecordedTimePrecision,
    },
    MatchingNamedPlaceWording,
    SharedSourceReference {
        source_id: String,
    },
    RelatedSourceReferences {
        left_source_ids: Vec<String>,
        right_source_ids: Vec<String>,
        source_ids: Vec<String>,
        dependencies: Vec<SourceDependencyInput>,
        directly_linked: bool,
    },
    SharedTextTerms {
        terms: Vec<String>,
        claim_provenance: Vec<SharedTextTermProvenance>,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) enum TensionSignal {
    NonOverlappingPrecisePeriods {
        left_precision: RecordedTimePrecision,
        right_precision: RecordedTimePrecision,
    },
    DifferentNamedPlaceWording {
        left: String,
        right: String,
        caveat: String,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) enum MissingInformation {
    TimePeriod { incident_id: String },
    Location { incident_id: String },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct IncidentPairCandidate {
    pub left_incident_id: String,
    pub right_incident_id: String,
    pub supporting_signals: Vec<SupportingSignal>,
    pub tension_signals: Vec<TensionSignal>,
    pub missing_information: Vec<MissingInformation>,
    pub generated_by: Vec<RetrievalPass>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) enum SuppressionReason {
    PostingSize {
        incident_count: usize,
        max_incidents: usize,
    },
    CommonTerm {
        document_frequency: usize,
        max_document_frequency: usize,
    },
    LongPeriod {
        month_span: usize,
        max_month_span: usize,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) struct SuppressedIndexBucket {
    pub pass: RetrievalPass,
    pub key: String,
    pub reason: SuppressionReason,
    pub affected_incident_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CandidateGenerationOutput {
    pub candidates: Vec<IncidentPairCandidate>,
    pub suppressed_buckets: Vec<SuppressedIndexBucket>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CandidateInputError {
    message: String,
}

impl fmt::Display for CandidateInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CandidateInputError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Date {
    year: i64,
    month: u32,
    day: u32,
}

impl Date {
    fn parse(value: &str) -> Option<Self> {
        if value.len() != 10
            || !value.is_ascii()
            || value.as_bytes()[4] != b'-'
            || value.as_bytes()[7] != b'-'
            || !value
                .bytes()
                .enumerate()
                .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
        {
            return None;
        }
        let mut parts = value.split('-');
        let date = Self {
            year: parts.next()?.parse().ok()?,
            month: parts.next()?.parse().ok()?,
            day: parts.next()?.parse().ok()?,
        };
        if parts.next().is_some()
            || date.year == 0
            || date.year > 9999
            || date.month == 0
            || date.month > 12
        {
            return None;
        }
        let max_day = days_in_month(date.year, date.month);
        (date.day > 0 && date.day <= max_day).then_some(date)
    }

    fn month_key(self) -> i64 {
        self.year * 12 + i64::from(self.month) - 1
    }
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[derive(Clone, Copy)]
struct PeriodBounds {
    start: Date,
    end: Date,
}

impl PeriodBounds {
    fn from_period(period: &DatasetTimePeriod) -> Option<Self> {
        let start = Date::parse(&period.start_date)?;
        let end = Date::parse(&period.end_date)?;
        if start > end {
            return None;
        }
        match period.precision {
            DatasetTimePrecision::Year => Some(Self {
                start: Date {
                    year: start.year,
                    month: 1,
                    day: 1,
                },
                end: Date {
                    year: end.year,
                    month: 12,
                    day: 31,
                },
            }),
            DatasetTimePrecision::Month => Some(Self {
                start: Date {
                    year: start.year,
                    month: start.month,
                    day: 1,
                },
                end: Date {
                    year: end.year,
                    month: end.month,
                    day: days_in_month(end.year, end.month),
                },
            }),
            _ => Some(Self { start, end }),
        }
    }

    fn overlaps(self, other: Self) -> bool {
        self.start <= other.end && other.start <= self.end
    }

    fn month_keys(self) -> Option<Vec<i64>> {
        let first = self.start.month_key();
        let last = self.end.month_key();
        if last - first + 1 > MAX_PERIOD_MONTHS {
            return None;
        }
        let month_count = usize::try_from(last - first + 1).ok()?;
        Some((first..=last).take(month_count).collect())
    }
}

fn recorded_precision(precision: DatasetTimePrecision) -> RecordedTimePrecision {
    match precision {
        DatasetTimePrecision::ExactDay => RecordedTimePrecision::ExactDay,
        DatasetTimePrecision::ApproximateDay => RecordedTimePrecision::ApproximateDay,
        DatasetTimePrecision::Month => RecordedTimePrecision::Month,
        DatasetTimePrecision::Year => RecordedTimePrecision::Year,
        DatasetTimePrecision::BoundedPeriod => RecordedTimePrecision::BoundedPeriod,
    }
}

#[derive(Default)]
struct CandidateBuilder {
    supporting: BTreeSet<SupportingSignal>,
    tensions: BTreeSet<TensionSignal>,
    missing: BTreeSet<MissingInformation>,
    passes: BTreeSet<RetrievalPass>,
}

fn canonical_pair(left: &str, right: &str) -> Option<(String, String)> {
    if left == right {
        return None;
    }
    if left < right {
        Some((left.to_string(), right.to_string()))
    } else {
        Some((right.to_string(), left.to_string()))
    }
}

fn normalize_place(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn tokenize(value: &str) -> BTreeSet<String> {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .filter(|token| token.chars().count() > 2 && !STOP_WORDS.contains(token))
        .map(str::to_string)
        .collect()
}

fn add_index_pair(
    index: &mut BTreeMap<String, BTreeSet<usize>>,
    key: String,
    incident_index: usize,
) {
    index.entry(key).or_default().insert(incident_index);
}

fn add_bucket_pairs(
    index: &BTreeMap<String, BTreeSet<usize>>,
    pass: RetrievalPass,
    incidents: &[IncidentCandidateInput],
    periods: Option<&[Option<PeriodBounds>]>,
    candidates: &mut BTreeMap<(String, String), CandidateBuilder>,
    suppressed: &mut BTreeSet<SuppressedIndexBucket>,
) {
    for (key, members) in index {
        if members.len() > MAX_INDEX_POSTING {
            suppressed.insert(SuppressedIndexBucket {
                pass,
                key: key.clone(),
                reason: SuppressionReason::PostingSize {
                    incident_count: members.len(),
                    max_incidents: MAX_INDEX_POSTING,
                },
                affected_incident_ids: members
                    .iter()
                    .map(|&index| incidents[index].identity.clone())
                    .collect(),
            });
            continue;
        }
        let members: Vec<_> = members.iter().copied().collect();
        for left_position in 0..members.len() {
            for right_position in left_position + 1..members.len() {
                if let Some(periods) = periods {
                    if !periods[members[left_position]]
                        .zip(periods[members[right_position]])
                        .is_some_and(|(left, right)| left.overlaps(right))
                    {
                        continue;
                    }
                }
                let left = &incidents[members[left_position]].identity;
                let right = &incidents[members[right_position]].identity;
                if let Some(pair) = canonical_pair(left, right) {
                    candidates.entry(pair).or_default().passes.insert(pass);
                }
            }
        }
    }
}

#[derive(Default)]
struct SourceUnionFind {
    parent: HashMap<String, String>,
    size: HashMap<String, usize>,
}

impl SourceUnionFind {
    fn add(&mut self, source: &str) {
        self.parent
            .entry(source.to_string())
            .or_insert_with(|| source.to_string());
        self.size.entry(source.to_string()).or_insert(1);
    }

    fn find(&mut self, source: &str) -> String {
        let mut root = self
            .parent
            .get(source)
            .cloned()
            .unwrap_or_else(|| source.to_string());
        let mut path = Vec::new();
        while let Some(parent) = self.parent.get(&root) {
            if parent == &root {
                break;
            }
            path.push(root);
            root = parent.clone();
        }
        for node in path {
            self.parent.insert(node, root.clone());
        }
        root
    }

    fn union(&mut self, left: &str, right: &str) {
        self.add(left);
        self.add(right);
        let left_root = self.find(left);
        let right_root = self.find(right);
        if left_root == right_root {
            return;
        }

        let left_size = self.size.get(&left_root).copied().unwrap_or(1);
        let right_size = self.size.get(&right_root).copied().unwrap_or(1);
        let (root, child, root_size, child_size) =
            if left_size > right_size || (left_size == right_size && left_root < right_root) {
                (left_root, right_root, left_size, right_size)
            } else {
                (right_root, left_root, right_size, left_size)
            };
        self.parent.insert(child.clone(), root.clone());
        self.size.insert(root, root_size + child_size);
        self.size.remove(&child);
    }
}

#[derive(Default)]
struct SourceComponents {
    component_by_source: HashMap<String, String>,
    members_by_component: HashMap<String, Vec<String>>,
    dependencies_by_component: HashMap<String, Vec<SourceDependencyInput>>,
}

fn source_components(
    incidents: &[IncidentCandidateInput],
    dependencies: &[SourceDependencyInput],
) -> SourceComponents {
    let mut union_find = SourceUnionFind::default();
    for incident in incidents {
        for source_id in &incident.source_ids {
            union_find.add(source_id);
        }
        for claim in &incident.claims {
            union_find.add(&claim.source_id);
        }
    }
    for edge in dependencies {
        union_find.union(&edge.from_source_id, &edge.to_source_id);
    }

    let sources: Vec<_> = union_find.parent.keys().cloned().collect();
    let mut components = SourceComponents::default();
    for source in sources {
        let root = union_find.find(&source);
        components
            .component_by_source
            .insert(source.clone(), root.clone());
        components
            .members_by_component
            .entry(root)
            .or_default()
            .push(source);
    }
    for members in components.members_by_component.values_mut() {
        members.sort();
    }
    let canonical_by_root: HashMap<_, _> = components
        .members_by_component
        .iter()
        .map(|(root, members)| (root.clone(), members[0].clone()))
        .collect();
    components.component_by_source = components
        .component_by_source
        .into_iter()
        .map(|(source, root)| (source, canonical_by_root[&root].clone()))
        .collect();
    components.members_by_component = components
        .members_by_component
        .into_iter()
        .map(|(root, members)| (canonical_by_root[&root].clone(), members))
        .collect();

    let mut edges_by_component = HashMap::<String, BTreeSet<SourceDependencyInput>>::new();
    for edge in dependencies {
        if let Some(component) = components.component_by_source.get(&edge.from_source_id) {
            edges_by_component
                .entry(component.clone())
                .or_default()
                .insert(edge.clone());
        }
    }
    components.dependencies_by_component = edges_by_component
        .into_iter()
        .map(|(component, edges)| (component, edges.into_iter().collect()))
        .collect();
    components
}

impl SourceComponents {
    fn dependency_edges_between(
        &self,
        component: &str,
        left_sources: &BTreeSet<String>,
        right_sources: &BTreeSet<String>,
    ) -> (Vec<SourceDependencyInput>, bool) {
        let edges = self
            .dependencies_by_component
            .get(component)
            .cloned()
            .unwrap_or_default();
        let directly_linked = edges.iter().any(|edge| {
            (left_sources.contains(&edge.from_source_id)
                && right_sources.contains(&edge.to_source_id))
                || (left_sources.contains(&edge.to_source_id)
                    && right_sources.contains(&edge.from_source_id))
        });
        (edges, directly_linked)
    }
}

pub(crate) fn generate_incident_candidates(
    input: &CandidateGenerationInput,
) -> Result<CandidateGenerationOutput, CandidateInputError> {
    let mut incidents = input.incidents.clone();
    let mut identities = BTreeSet::new();
    for incident in &incidents {
        if incident.identity.trim().is_empty() {
            return Err(CandidateInputError {
                message: "Incident identity must not be blank".to_string(),
            });
        }
        if incident.identity.trim() != incident.identity {
            return Err(CandidateInputError {
                message: format!(
                    "Incident identity {:?} must not contain surrounding whitespace",
                    incident.identity
                ),
            });
        }
        if !identities.insert(incident.identity.clone()) {
            return Err(CandidateInputError {
                message: format!("Duplicate incident identity {:?}", incident.identity),
            });
        }
    }
    incidents.sort_by(|left, right| left.identity.cmp(&right.identity));

    let source_components = source_components(&incidents, &input.source_dependencies);
    let mut time_index = BTreeMap::new();
    let mut place_index = BTreeMap::new();
    let mut source_index = BTreeMap::new();
    let mut dependency_index = BTreeMap::new();
    let mut text_index = BTreeMap::new();
    let mut periods = Vec::with_capacity(incidents.len());
    let mut words = Vec::with_capacity(incidents.len());
    let mut claim_terms = Vec::with_capacity(incidents.len());
    let mut suppressed = BTreeSet::new();

    for (index, incident) in incidents.iter().enumerate() {
        let period = incident
            .time_period
            .as_ref()
            .and_then(PeriodBounds::from_period);
        if let Some(period) = period {
            if let Some(months) = period.month_keys() {
                for month in months {
                    add_index_pair(&mut time_index, month.to_string(), index);
                }
            } else {
                suppressed.insert(SuppressedIndexBucket {
                    pass: RetrievalPass::TimeOverlap,
                    key: incident.identity.clone(),
                    reason: SuppressionReason::LongPeriod {
                        month_span: usize::try_from(
                            period.end.month_key() - period.start.month_key() + 1,
                        )
                        .unwrap_or(usize::MAX),
                        max_month_span: MAX_PERIOD_MONTHS as usize,
                    },
                    affected_incident_ids: vec![incident.identity.clone()],
                });
            }
        }
        periods.push(period);

        if let Some(location) = &incident.location {
            if let crate::research_dataset::DatasetLocationPrecision::NamedPlace =
                location.precision
            {
                let place = normalize_place(&location.description);
                if !place.is_empty() {
                    add_index_pair(&mut place_index, place, index);
                }
            }
        }

        let mut source_ids: BTreeSet<_> = incident.source_ids.iter().cloned().collect();
        source_ids.extend(incident.claims.iter().map(|claim| claim.source_id.clone()));
        for source_id in source_ids {
            add_index_pair(&mut source_index, source_id.clone(), index);
            if let Some(component) = source_components
                .component_by_source
                .get(&source_id)
                .filter(|component| {
                    source_components
                        .members_by_component
                        .get(*component)
                        .is_some_and(|members| members.len() > 1)
                })
            {
                add_index_pair(&mut dependency_index, component.clone(), index);
            }
        }

        let mut incident_words = BTreeSet::new();
        let mut incident_claim_terms = BTreeMap::<String, BTreeSet<ClaimTextProvenance>>::new();
        if let Some(description) = &incident.description {
            incident_words.extend(tokenize(description));
        }
        for claim in &incident.claims {
            let terms = tokenize(&claim.text);
            incident_words.extend(terms.iter().cloned());
            let provenance = ClaimTextProvenance {
                claim_id: claim.record_id.clone(),
                dataset_id: claim.dataset_id.clone(),
                dataset_key: claim.dataset_key.clone(),
            };
            for term in terms {
                incident_claim_terms
                    .entry(term)
                    .or_default()
                    .insert(provenance.clone());
            }
        }
        for word in &incident_words {
            add_index_pair(&mut text_index, word.clone(), index);
        }
        words.push(incident_words);
        claim_terms.push(incident_claim_terms);
    }

    let mut candidates = BTreeMap::<(String, String), CandidateBuilder>::new();
    add_bucket_pairs(
        &time_index,
        RetrievalPass::TimeOverlap,
        &incidents,
        Some(&periods),
        &mut candidates,
        &mut suppressed,
    );
    add_bucket_pairs(
        &place_index,
        RetrievalPass::NamedPlace,
        &incidents,
        None,
        &mut candidates,
        &mut suppressed,
    );
    add_bucket_pairs(
        &source_index,
        RetrievalPass::SharedSource,
        &incidents,
        None,
        &mut candidates,
        &mut suppressed,
    );
    add_bucket_pairs(
        &dependency_index,
        RetrievalPass::SharedDependentSource,
        &incidents,
        None,
        &mut candidates,
        &mut suppressed,
    );

    let common_term_threshold = 3usize.max(incidents.len().div_ceil(4));
    for (term, members) in &text_index {
        let exceeds_common_term_limit = members.len() > common_term_threshold;
        let exceeds_posting_limit = members.len() > MAX_INDEX_POSTING;
        if common_term_threshold <= MAX_INDEX_POSTING && exceeds_common_term_limit {
            suppressed.insert(SuppressedIndexBucket {
                pass: RetrievalPass::Text,
                key: term.clone(),
                reason: SuppressionReason::CommonTerm {
                    document_frequency: members.len(),
                    max_document_frequency: common_term_threshold,
                },
                affected_incident_ids: members
                    .iter()
                    .map(|&index| incidents[index].identity.clone())
                    .collect(),
            });
        } else if exceeds_posting_limit {
            suppressed.insert(SuppressedIndexBucket {
                pass: RetrievalPass::Text,
                key: term.clone(),
                reason: SuppressionReason::PostingSize {
                    incident_count: members.len(),
                    max_incidents: MAX_INDEX_POSTING,
                },
                affected_incident_ids: members
                    .iter()
                    .map(|&index| incidents[index].identity.clone())
                    .collect(),
            });
        }
        if exceeds_common_term_limit || exceeds_posting_limit {
            continue;
        }
        let member_indices: Vec<_> = members.iter().copied().collect();
        for left_position in 0..member_indices.len() {
            for right_position in left_position + 1..member_indices.len() {
                let left = &incidents[member_indices[left_position]].identity;
                let right = &incidents[member_indices[right_position]].identity;
                if let Some(pair) = canonical_pair(left, right) {
                    candidates
                        .entry(pair)
                        .or_default()
                        .passes
                        .insert(RetrievalPass::Text);
                }
            }
        }
    }

    for ((left_id, right_id), builder) in &mut candidates {
        let left_index = incidents
            .binary_search_by(|incident| incident.identity.cmp(left_id))
            .expect("candidate left identity is indexed");
        let right_index = incidents
            .binary_search_by(|incident| incident.identity.cmp(right_id))
            .expect("candidate right identity is indexed");
        let left = &incidents[left_index];
        let right = &incidents[right_index];

        for (incident, period) in [(left, periods[left_index]), (right, periods[right_index])] {
            if incident.time_period.is_none() || period.is_none() {
                builder.missing.insert(MissingInformation::TimePeriod {
                    incident_id: incident.identity.clone(),
                });
            }
            if incident.location.is_none() {
                builder.missing.insert(MissingInformation::Location {
                    incident_id: incident.identity.clone(),
                });
            }
        }

        if let (Some(left_period), Some(right_period)) = (periods[left_index], periods[right_index])
        {
            let left_recorded_precision = left.time_period.as_ref().unwrap().precision;
            let right_recorded_precision = right.time_period.as_ref().unwrap().precision;
            let left_precision = recorded_precision(left_recorded_precision);
            let right_precision = recorded_precision(right_recorded_precision);
            if left_period.overlaps(right_period) {
                builder
                    .supporting
                    .insert(SupportingSignal::OverlappingPeriods {
                        left_precision,
                        right_precision,
                    });
            } else if matches!(left_recorded_precision, DatasetTimePrecision::ExactDay)
                && matches!(right_recorded_precision, DatasetTimePrecision::ExactDay)
            {
                builder
                    .tensions
                    .insert(TensionSignal::NonOverlappingPrecisePeriods {
                        left_precision,
                        right_precision,
                    });
            }
        }

        if let (Some(left_location), Some(right_location)) = (&left.location, &right.location) {
            use crate::research_dataset::DatasetLocationPrecision::NamedPlace;
            if left_location.precision == NamedPlace && right_location.precision == NamedPlace {
                let left_place = normalize_place(&left_location.description);
                let right_place = normalize_place(&right_location.description);
                if !left_place.is_empty() && left_place == right_place {
                    builder
                        .supporting
                        .insert(SupportingSignal::MatchingNamedPlaceWording);
                } else if !left_place.is_empty() && !right_place.is_empty() {
                    builder
                        .tensions
                        .insert(TensionSignal::DifferentNamedPlaceWording {
                            left: left_location.description.clone(),
                            right: right_location.description.clone(),
                            caveat: "Recorded named-place wording differs; aliases and geographic overlap are not resolved.".to_string(),
                        });
                }
            }
        }

        let left_sources: BTreeSet<String> = left
            .source_ids
            .iter()
            .chain(left.claims.iter().map(|claim| &claim.source_id))
            .cloned()
            .collect();
        let right_sources: BTreeSet<String> = right
            .source_ids
            .iter()
            .chain(right.claims.iter().map(|claim| &claim.source_id))
            .cloned()
            .collect();
        for source_id in left_sources.intersection(&right_sources) {
            builder
                .supporting
                .insert(SupportingSignal::SharedSourceReference {
                    source_id: source_id.clone(),
                });
        }

        let left_components: BTreeSet<_> = left_sources
            .iter()
            .filter_map(|source| source_components.component_by_source.get(source))
            .collect();
        let right_components: BTreeSet<_> = right_sources
            .iter()
            .filter_map(|source| source_components.component_by_source.get(source))
            .collect();
        for component in left_components.intersection(&right_components) {
            if source_components
                .members_by_component
                .get(*component)
                .is_some_and(|members| members.len() > 1)
            {
                let left_component_sources: BTreeSet<_> = left_sources
                    .iter()
                    .filter(|source| {
                        source_components.component_by_source.get(*source) == Some(*component)
                    })
                    .cloned()
                    .collect();
                let right_component_sources: BTreeSet<_> = right_sources
                    .iter()
                    .filter(|source| {
                        source_components.component_by_source.get(*source) == Some(*component)
                    })
                    .cloned()
                    .collect();
                let (dependencies, directly_linked) = source_components.dependency_edges_between(
                    component,
                    &left_component_sources,
                    &right_component_sources,
                );
                let source_ids = left_component_sources
                    .union(&right_component_sources)
                    .cloned()
                    .collect();
                builder
                    .supporting
                    .insert(SupportingSignal::RelatedSourceReferences {
                        left_source_ids: left_component_sources.into_iter().collect(),
                        right_source_ids: right_component_sources.into_iter().collect(),
                        source_ids,
                        dependencies,
                        directly_linked,
                    });
            }
        }

        let shared_terms: Vec<_> = words[left_index]
            .intersection(&words[right_index])
            .filter(|term| {
                text_index
                    .get(*term)
                    .is_some_and(|members| members.len() <= common_term_threshold)
            })
            .cloned()
            .collect();
        if !shared_terms.is_empty() {
            let claim_provenance = shared_terms
                .iter()
                .map(|term| SharedTextTermProvenance {
                    term: term.clone(),
                    left_claims: claim_terms[left_index]
                        .get(term)
                        .into_iter()
                        .flatten()
                        .cloned()
                        .collect(),
                    right_claims: claim_terms[right_index]
                        .get(term)
                        .into_iter()
                        .flatten()
                        .cloned()
                        .collect(),
                })
                .collect();
            builder
                .supporting
                .insert(SupportingSignal::SharedTextTerms {
                    terms: shared_terms,
                    claim_provenance,
                });
        }
    }

    let candidates = candidates
        .into_iter()
        .map(
            |((left_incident_id, right_incident_id), builder)| IncidentPairCandidate {
                left_incident_id,
                right_incident_id,
                supporting_signals: builder.supporting.into_iter().collect(),
                tension_signals: builder.tensions.into_iter().collect(),
                missing_information: builder.missing.into_iter().collect(),
                generated_by: builder.passes.into_iter().collect(),
            },
        )
        .collect();
    Ok(CandidateGenerationOutput {
        candidates,
        suppressed_buckets: suppressed.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research_dataset::{DatasetLocationPrecision, DatasetTimePrecision};

    fn incident(identity: &str) -> IncidentCandidateInput {
        IncidentCandidateInput {
            identity: identity.to_string(),
            label: identity.to_string(),
            description: None,
            time_period: None,
            location: None,
            source_ids: Vec::new(),
            claims: Vec::new(),
        }
    }

    fn day_period(date: &str, precision: DatasetTimePrecision) -> DatasetTimePeriod {
        DatasetTimePeriod {
            start_date: date.to_string(),
            end_date: date.to_string(),
            precision,
            original_wording: None,
        }
    }

    fn location(description: &str, precision: DatasetLocationPrecision) -> DatasetLocation {
        DatasetLocation {
            description: description.to_string(),
            precision,
            original_wording: None,
        }
    }

    fn generate(incidents: Vec<IncidentCandidateInput>) -> CandidateGenerationOutput {
        generate_incident_candidates(&CandidateGenerationInput {
            incidents,
            source_dependencies: Vec::new(),
        })
        .expect("candidate generation succeeds")
    }

    fn candidate(
        output: &CandidateGenerationOutput,
        left: &str,
        right: &str,
    ) -> IncidentPairCandidate {
        output
            .candidates
            .iter()
            .find(|candidate| {
                candidate.left_incident_id == left && candidate.right_incident_id == right
            })
            .cloned()
            .expect("candidate exists")
    }

    #[test]
    fn blank_and_duplicate_identities_are_rejected() {
        let error = generate_incident_candidates(&CandidateGenerationInput {
            incidents: vec![incident(" ")],
            source_dependencies: Vec::new(),
        })
        .expect_err("blank identity is invalid");
        assert!(error.to_string().contains("must not be blank"));

        let error = generate_incident_candidates(&CandidateGenerationInput {
            incidents: vec![incident("same"), incident("same")],
            source_dependencies: Vec::new(),
        })
        .expect_err("duplicate identity is invalid");
        assert!(error.to_string().contains("Duplicate incident identity"));
    }

    #[test]
    fn self_pairs_are_never_emitted_and_pairs_are_normalized() {
        let mut first = incident("z");
        first.source_ids.push("shared".to_string());
        let mut second = incident("a");
        second.source_ids.push("shared".to_string());
        let output = generate(vec![first, second]);
        assert_eq!(output.candidates.len(), 1);
        assert_eq!(output.candidates[0].left_incident_id, "a");
        assert_eq!(output.candidates[0].right_incident_id, "z");
        assert!(
            !output
                .candidates
                .iter()
                .any(|pair| pair.left_incident_id == pair.right_incident_id)
        );
    }

    #[test]
    fn similar_descriptions_retrieve_pairs_despite_separated_dates() {
        let mut left = incident("a");
        left.description = Some("silver diamond shaped object".to_string());
        left.time_period = Some(day_period("2001-01-01", DatasetTimePrecision::ExactDay));
        let mut right = incident("b");
        right.description = Some("diamond shaped silver lights".to_string());
        right.time_period = Some(day_period("2024-06-15", DatasetTimePrecision::ExactDay));

        let output = generate(vec![left, right]);
        let pair = candidate(&output, "a", "b");
        assert!(pair.generated_by.contains(&RetrievalPass::Text));
        assert!(
            pair.tension_signals
                .iter()
                .any(|signal| matches!(signal, TensionSignal::NonOverlappingPrecisePeriods { .. }))
        );
        assert!(pair.supporting_signals.iter().any(|signal| matches!(
            signal,
            SupportingSignal::SharedTextTerms { terms, .. } if terms.contains(&"diamond".to_string())
        )));
    }

    #[test]
    fn shared_text_terms_include_only_contributing_claim_provenance() {
        let mut left = incident("a");
        left.claims.push(ClaimCandidateInput {
            record_id: "claim-left".to_string(),
            dataset_id: "dataset-left".to_string(),
            dataset_key: "left-key".to_string(),
            text: "diamond over ridge".to_string(),
            category: ClaimCategory::Observation,
            source_id: "source-left".to_string(),
        });
        left.claims.push(ClaimCandidateInput {
            record_id: "claim-left-unrelated".to_string(),
            dataset_id: "dataset-left".to_string(),
            dataset_key: "left-unrelated-key".to_string(),
            text: "bright lights".to_string(),
            category: ClaimCategory::Observation,
            source_id: "source-left".to_string(),
        });
        let mut right = incident("b");
        right.claims.push(ClaimCandidateInput {
            record_id: "claim-right".to_string(),
            dataset_id: "dataset-right".to_string(),
            dataset_key: "right-key".to_string(),
            text: "diamond above ridge".to_string(),
            category: ClaimCategory::Observation,
            source_id: "source-right".to_string(),
        });

        let pair = candidate(&generate(vec![left, right]), "a", "b");
        let signal = pair
            .supporting_signals
            .iter()
            .find_map(|signal| match signal {
                SupportingSignal::SharedTextTerms {
                    claim_provenance, ..
                } => Some(claim_provenance),
                _ => None,
            })
            .expect("shared text signal exists");
        let diamond = signal
            .iter()
            .find(|provenance| provenance.term == "diamond")
            .expect("diamond provenance exists");
        assert_eq!(
            diamond.left_claims,
            [ClaimTextProvenance {
                claim_id: "claim-left".to_string(),
                dataset_id: "dataset-left".to_string(),
                dataset_key: "left-key".to_string(),
            }]
        );
        assert_eq!(
            diamond.right_claims,
            [ClaimTextProvenance {
                claim_id: "claim-right".to_string(),
                dataset_id: "dataset-right".to_string(),
                dataset_key: "right-key".to_string(),
            }]
        );
        assert!(
            !signal
                .iter()
                .find(|provenance| provenance.term == "diamond")
                .unwrap()
                .left_claims
                .iter()
                .any(|claim| claim.claim_id == "claim-left-unrelated")
        );
    }

    #[test]
    fn overlapping_period_and_location_tension_remain_separate_signals() {
        let mut left = incident("a");
        left.time_period = Some(day_period("2010-03-01", DatasetTimePrecision::Month));
        left.location = Some(location("north base", DatasetLocationPrecision::NamedPlace));
        let mut right = incident("b");
        right.time_period = Some(DatasetTimePeriod {
            start_date: "2010-03-15".to_string(),
            end_date: "2010-03-20".to_string(),
            precision: DatasetTimePrecision::BoundedPeriod,
            original_wording: None,
        });
        right.location = Some(location(
            "south station",
            DatasetLocationPrecision::NamedPlace,
        ));

        let output = generate(vec![left, right]);
        let pair = candidate(&output, "a", "b");
        assert!(pair.supporting_signals.iter().any(|signal| matches!(
            signal,
            SupportingSignal::OverlappingPeriods {
                left_precision: RecordedTimePrecision::Month,
                right_precision: RecordedTimePrecision::BoundedPeriod
            }
        )));
        assert!(
            pair.tension_signals
                .iter()
                .any(|signal| matches!(signal, TensionSignal::DifferentNamedPlaceWording { .. }))
        );
    }

    #[test]
    fn month_and_year_precision_are_expanded_as_periods_not_exact_days() {
        let mut year = incident("year");
        year.time_period = Some(DatasetTimePeriod {
            start_date: "2017-01-01".to_string(),
            end_date: "2017-12-31".to_string(),
            precision: DatasetTimePrecision::Year,
            original_wording: Some("2017".to_string()),
        });
        let mut month = incident("month");
        month.time_period = Some(DatasetTimePeriod {
            start_date: "2017-10-01".to_string(),
            end_date: "2017-10-31".to_string(),
            precision: DatasetTimePrecision::Month,
            original_wording: Some("October 2017".to_string()),
        });
        let pair = candidate(&generate(vec![year, month]), "month", "year");
        assert!(pair.supporting_signals.iter().any(|signal| matches!(
            signal,
            SupportingSignal::OverlappingPeriods {
                left_precision: RecordedTimePrecision::Month,
                right_precision: RecordedTimePrecision::Year
            }
        )));
    }

    #[test]
    fn named_place_matching_is_normalized_and_approximate_places_are_not_indexed() {
        let mut left = incident("a");
        left.location = Some(location(
            "Al-Taqaddum Air Base, Iraq",
            DatasetLocationPrecision::NamedPlace,
        ));
        let mut right = incident("b");
        right.location = Some(location(
            "al taqaddum air base iraq",
            DatasetLocationPrecision::NamedPlace,
        ));
        let pair = candidate(&generate(vec![left.clone(), right.clone()]), "a", "b");
        assert!(
            pair.supporting_signals
                .contains(&SupportingSignal::MatchingNamedPlaceWording)
        );

        left.location = Some(location(
            "Al-Taqaddum Air Base, Iraq",
            DatasetLocationPrecision::ApproximateArea,
        ));
        right.location = Some(location(
            "al taqaddum air base iraq",
            DatasetLocationPrecision::ApproximateArea,
        ));
        assert!(generate(vec![left, right]).candidates.is_empty());
    }

    #[test]
    fn missing_dates_and_locations_are_unknown_not_contradictions() {
        let mut left = incident("a");
        left.description = Some("orbital debris observation".to_string());
        let mut right = incident("b");
        right.description = Some("observation orbital debris".to_string());
        let pair = candidate(&generate(vec![left, right]), "a", "b");
        assert_eq!(pair.missing_information.len(), 4);
        assert!(pair.tension_signals.is_empty());
    }

    #[test]
    fn shared_sources_retrieve_pairs_without_independent_source_counting() {
        let mut left = incident("a");
        left.source_ids.push("report-1".to_string());
        left.claims.extend([
            ClaimCandidateInput {
                record_id: "claim-bright".to_string(),
                dataset_id: "dataset-a".to_string(),
                dataset_key: "claim-bright-key".to_string(),
                text: "bright object".to_string(),
                category: ClaimCategory::Observation,
                source_id: "report-1".to_string(),
            },
            ClaimCandidateInput {
                record_id: "claim-moving".to_string(),
                dataset_id: "dataset-a".to_string(),
                dataset_key: "claim-moving-key".to_string(),
                text: "moving object".to_string(),
                category: ClaimCategory::Observation,
                source_id: "report-1".to_string(),
            },
        ]);
        let mut right = incident("b");
        right.source_ids.push("report-1".to_string());
        let pair = candidate(&generate(vec![left, right]), "a", "b");
        assert!(pair.generated_by.contains(&RetrievalPass::SharedSource));
        assert_eq!(
            pair.supporting_signals
                .iter()
                .filter(|signal| matches!(signal, SupportingSignal::SharedSourceReference { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn known_source_dependencies_are_preserved_without_claiming_independence() {
        let mut left = incident("a");
        left.source_ids.push("source-a".to_string());
        let mut right = incident("b");
        right.source_ids.push("source-b".to_string());
        let output = generate_incident_candidates(&CandidateGenerationInput {
            incidents: vec![left, right],
            source_dependencies: vec![SourceDependencyInput {
                relationship_record_id: "relation-repeats".to_string(),
                from_source_id: "source-b".to_string(),
                to_source_id: "source-a".to_string(),
                kind: SourceDependencyKind::Repeats,
            }],
        })
        .expect("dependency processing succeeds");
        let pair = candidate(&output, "a", "b");
        assert!(
            pair.generated_by
                .contains(&RetrievalPass::SharedDependentSource)
        );
        assert!(pair.supporting_signals.iter().any(|signal| matches!(
            signal,
            SupportingSignal::RelatedSourceReferences {
                dependencies,
                source_ids,
                directly_linked,
                left_source_ids,
                right_source_ids,
            } if dependencies == &[SourceDependencyInput {
                relationship_record_id: "relation-repeats".to_string(),
                from_source_id: "source-b".to_string(),
                to_source_id: "source-a".to_string(),
                kind: SourceDependencyKind::Repeats,
            }] && source_ids == &["source-a".to_string(), "source-b".to_string()]
                && *directly_linked
                && left_source_ids == &["source-a".to_string()]
                && right_source_ids == &["source-b".to_string()]
        )));
        assert!(
            !pair
                .supporting_signals
                .iter()
                .any(|signal| matches!(signal, SupportingSignal::SharedSourceReference { .. }))
        );
    }

    #[test]
    fn different_location_wording_is_tension_not_a_contradiction() {
        let mut left = incident("a");
        left.location = Some(location("West Ridge", DatasetLocationPrecision::NamedPlace));
        let mut right = incident("b");
        right.location = Some(location(
            "East Valley",
            DatasetLocationPrecision::NamedPlace,
        ));
        left.source_ids.push("same-record".to_string());
        right.source_ids.push("same-record".to_string());
        let pair = candidate(&generate(vec![left, right]), "a", "b");
        assert!(pair.tension_signals.iter().any(|signal| matches!(
            signal,
            TensionSignal::DifferentNamedPlaceWording { caveat, .. }
                if caveat.contains("not resolved")
        )));
        assert!(
            !pair
                .supporting_signals
                .iter()
                .any(|signal| matches!(signal, SupportingSignal::MatchingNamedPlaceWording))
        );
    }

    #[test]
    fn approximate_days_can_overlap_but_nonoverlap_is_not_tension() {
        let mut left = incident("a");
        left.source_ids.push("shared".to_string());
        left.time_period = Some(DatasetTimePeriod {
            start_date: "2020-05-01".to_string(),
            end_date: "2020-05-03".to_string(),
            precision: DatasetTimePrecision::ApproximateDay,
            original_wording: None,
        });
        let mut right = incident("b");
        right.source_ids.push("shared".to_string());
        right.time_period = Some(day_period("2020-05-03", DatasetTimePrecision::ExactDay));
        let pair = candidate(&generate(vec![left.clone(), right.clone()]), "a", "b");
        assert!(pair.supporting_signals.iter().any(|signal| matches!(
            signal,
            SupportingSignal::OverlappingPeriods {
                left_precision: RecordedTimePrecision::ApproximateDay,
                right_precision: RecordedTimePrecision::ExactDay
            }
        )));

        right.time_period = Some(day_period("2020-05-10", DatasetTimePrecision::ExactDay));
        let pair = candidate(&generate(vec![left, right]), "a", "b");
        assert!(pair.tension_signals.is_empty());
    }

    #[test]
    fn narrow_and_wide_approximate_periods_do_not_create_tension() {
        for (start, end) in [("2020-05-01", "2020-05-02"), ("2020-01-01", "2020-12-31")] {
            let mut left = incident("a");
            left.source_ids.push("shared".to_string());
            left.time_period = Some(DatasetTimePeriod {
                start_date: start.to_string(),
                end_date: end.to_string(),
                precision: DatasetTimePrecision::ApproximateDay,
                original_wording: None,
            });
            let mut right = incident("b");
            right.source_ids.push("shared".to_string());
            right.time_period = Some(day_period(
                "2021-01-01",
                DatasetTimePrecision::ApproximateDay,
            ));
            let pair = candidate(&generate(vec![left, right]), "a", "b");
            assert!(pair.tension_signals.is_empty());
        }
    }

    #[test]
    fn dependency_signals_are_stable_directed_and_keep_transitivity_distinct() {
        let mut left = incident("a");
        left.source_ids
            .extend(["source-z".to_string(), "source-a".to_string()]);
        let mut right = incident("b");
        right.source_ids.push("source-c".to_string());
        let edges = vec![
            SourceDependencyInput {
                relationship_record_id: "relation-quotes".to_string(),
                from_source_id: "source-a".to_string(),
                to_source_id: "source-b".to_string(),
                kind: SourceDependencyKind::Quotes,
            },
            SourceDependencyInput {
                relationship_record_id: "relation-derived".to_string(),
                from_source_id: "source-b".to_string(),
                to_source_id: "source-c".to_string(),
                kind: SourceDependencyKind::DerivedFrom,
            },
            SourceDependencyInput {
                relationship_record_id: "relation-repeats".to_string(),
                from_source_id: "source-z".to_string(),
                to_source_id: "source-b".to_string(),
                kind: SourceDependencyKind::Repeats,
            },
        ];
        let input = CandidateGenerationInput {
            incidents: vec![left.clone(), right.clone()],
            source_dependencies: edges.clone(),
        };
        let output = generate_incident_candidates(&input).expect("first run");
        let reordered = generate_incident_candidates(&CandidateGenerationInput {
            incidents: vec![right, left],
            source_dependencies: edges.into_iter().rev().collect(),
        })
        .expect("reordered run");
        assert_eq!(output, reordered);
        assert_eq!(
            generate_incident_candidates(&input).expect("repeated run"),
            output
        );
        let pair = candidate(&output, "a", "b");
        assert!(pair.supporting_signals.iter().any(|signal| matches!(
            signal,
            SupportingSignal::RelatedSourceReferences {
                left_source_ids,
                right_source_ids,
                source_ids,
                dependencies,
                directly_linked: false,
                ..
            } if left_source_ids == &["source-a".to_string(), "source-z".to_string()]
                && right_source_ids == &["source-c".to_string()]
                && source_ids == &[
                    "source-a".to_string(),
                    "source-c".to_string(),
                    "source-z".to_string(),
                ]
                && dependencies == &[
                    SourceDependencyInput {
                        relationship_record_id: "relation-derived".to_string(),
                        from_source_id: "source-b".to_string(),
                        to_source_id: "source-c".to_string(),
                        kind: SourceDependencyKind::DerivedFrom,
                    },
                    SourceDependencyInput {
                        relationship_record_id: "relation-quotes".to_string(),
                        from_source_id: "source-a".to_string(),
                        to_source_id: "source-b".to_string(),
                        kind: SourceDependencyKind::Quotes,
                    },
                    SourceDependencyInput {
                        relationship_record_id: "relation-repeats".to_string(),
                        from_source_id: "source-z".to_string(),
                        to_source_id: "source-b".to_string(),
                        kind: SourceDependencyKind::Repeats,
                    },
                ]
        )));
    }

    #[test]
    fn dependency_cycles_and_reversed_edges_are_safe_and_deterministic() {
        let mut left = incident("a");
        left.source_ids.push("one".to_string());
        let mut right = incident("b");
        right.source_ids.push("two".to_string());
        let dependencies = vec![
            SourceDependencyInput {
                relationship_record_id: "relation-quotes".to_string(),
                from_source_id: "one".to_string(),
                to_source_id: "two".to_string(),
                kind: SourceDependencyKind::Quotes,
            },
            SourceDependencyInput {
                relationship_record_id: "relation-repeats".to_string(),
                from_source_id: "two".to_string(),
                to_source_id: "one".to_string(),
                kind: SourceDependencyKind::Repeats,
            },
            SourceDependencyInput {
                relationship_record_id: "relation-derived".to_string(),
                from_source_id: "one".to_string(),
                to_source_id: "one".to_string(),
                kind: SourceDependencyKind::DerivedFrom,
            },
        ];
        let forward = generate_incident_candidates(&CandidateGenerationInput {
            incidents: vec![left.clone(), right.clone()],
            source_dependencies: dependencies.clone(),
        })
        .expect("cycle is safe");
        let reverse = generate_incident_candidates(&CandidateGenerationInput {
            incidents: vec![right, left],
            source_dependencies: dependencies.into_iter().rev().collect(),
        })
        .expect("reversed cycle is safe");
        assert_eq!(forward, reverse);
        let pair = candidate(&forward, "a", "b");
        assert!(pair.supporting_signals.iter().any(|signal| matches!(
            signal,
            SupportingSignal::RelatedSourceReferences {
                dependencies,
                directly_linked: true,
                ..
            } if dependencies.len() == 3
        )));
    }

    #[test]
    fn long_dependency_chains_are_processed_iteratively() {
        let last = 2048;
        let mut left = incident("a");
        left.source_ids.push("source-0000".to_string());
        let mut right = incident("b");
        right.source_ids.push(format!("source-{last:04}"));
        let source_dependencies = (0..last)
            .map(|index| SourceDependencyInput {
                relationship_record_id: format!("relation-{index:04}"),
                from_source_id: format!("source-{index:04}"),
                to_source_id: format!("source-{:04}", index + 1),
                kind: SourceDependencyKind::DerivedFrom,
            })
            .collect();
        let output = generate_incident_candidates(&CandidateGenerationInput {
            incidents: vec![left, right],
            source_dependencies,
        })
        .expect("long dependency chain succeeds");
        let pair = candidate(&output, "a", "b");
        assert!(pair.supporting_signals.iter().any(|signal| matches!(
            signal,
            SupportingSignal::RelatedSourceReferences {
                directly_linked: false,
                dependencies,
                ..
            } if dependencies.len() == last
        )));
    }

    #[test]
    fn leap_year_and_calendar_boundaries_are_respected() {
        let mut february = incident("february");
        february.time_period = Some(DatasetTimePeriod {
            start_date: "2020-02-01".to_string(),
            end_date: "2020-02-29".to_string(),
            precision: DatasetTimePrecision::Month,
            original_wording: None,
        });
        let mut leap_day = incident("leap-day");
        leap_day.time_period = Some(day_period("2020-02-29", DatasetTimePrecision::ExactDay));
        let pair = candidate(&generate(vec![february, leap_day]), "february", "leap-day");
        assert!(pair.supporting_signals.iter().any(|signal| matches!(
            signal,
            SupportingSignal::OverlappingPeriods {
                left_precision: RecordedTimePrecision::ExactDay,
                right_precision: RecordedTimePrecision::Month
            } | SupportingSignal::OverlappingPeriods {
                left_precision: RecordedTimePrecision::Month,
                right_precision: RecordedTimePrecision::ExactDay
            }
        )));

        assert!(
            PeriodBounds::from_period(&day_period("2019-02-29", DatasetTimePrecision::ExactDay))
                .is_none()
        );
    }

    #[test]
    fn suppression_notices_report_their_actual_units_and_limits() {
        let four_common = (0..4)
            .map(|index| {
                let mut item = incident(&format!("common-{index}"));
                item.description = Some("unusual object common".to_string());
                item
            })
            .collect();
        let common_output = generate(four_common);
        let common_suppression = common_output
            .suppressed_buckets
            .iter()
            .find(|bucket| {
                bucket.pass == RetrievalPass::Text
                    && bucket.key == "common"
                    && bucket.reason
                        == SuppressionReason::CommonTerm {
                            document_frequency: 4,
                            max_document_frequency: 3,
                        }
            })
            .expect("common term suppression is reported");
        assert_eq!(
            common_suppression.affected_incident_ids,
            vec!["common-0", "common-1", "common-2", "common-3"]
        );

        let mut long_period = incident("long");
        long_period.time_period = Some(DatasetTimePeriod {
            start_date: "1900-01-01".to_string(),
            end_date: "2020-12-31".to_string(),
            precision: DatasetTimePrecision::BoundedPeriod,
            original_wording: None,
        });
        let long_output = generate(vec![long_period]);
        let long_suppression = long_output
            .suppressed_buckets
            .iter()
            .find(|bucket| {
                bucket.reason
                    == SuppressionReason::LongPeriod {
                        month_span: 1452,
                        max_month_span: 120,
                    }
            })
            .expect("long period suppression is reported");
        assert_eq!(long_suppression.affected_incident_ids, vec!["long"]);
    }

    #[test]
    fn large_postings_are_reported_and_suppressed_deterministically() {
        let incidents: Vec<_> = (0..600)
            .map(|index| {
                let mut item = incident(&format!("large-{index:03}"));
                item.description = Some(if index < 129 {
                    format!("sharedword distinctive{index}")
                } else {
                    format!("distinctive{index}")
                });
                item
            })
            .collect();
        let output = generate(incidents.clone());
        assert!(output.candidates.is_empty());
        assert!(output.suppressed_buckets.iter().any(|bucket| {
            bucket.key == "sharedword"
                && bucket.reason
                    == SuppressionReason::PostingSize {
                        incident_count: 129,
                        max_incidents: MAX_INDEX_POSTING,
                    }
        }));
        assert!(!output.suppressed_buckets.iter().any(|bucket| {
            bucket.key == "sharedword"
                && matches!(bucket.reason, SuppressionReason::CommonTerm { .. })
        }));
        assert_eq!(output, generate(incidents.into_iter().rev().collect()));
    }

    #[test]
    fn multiple_retrieval_passes_deduplicate_the_pair_and_keep_each_reason() {
        let mut left = incident("a");
        left.description = Some("silver object".to_string());
        left.time_period = Some(day_period("2020-06-01", DatasetTimePrecision::ExactDay));
        left.location = Some(location(
            "North Ridge",
            DatasetLocationPrecision::NamedPlace,
        ));
        left.source_ids.push("shared-source".to_string());
        let mut right = incident("b");
        right.description = Some("object silver".to_string());
        right.time_period = Some(day_period("2020-06-01", DatasetTimePrecision::ExactDay));
        right.location = Some(location(
            "north ridge",
            DatasetLocationPrecision::NamedPlace,
        ));
        right.source_ids.push("shared-source".to_string());
        let pair = candidate(&generate(vec![left, right]), "a", "b");
        assert_eq!(pair.generated_by.len(), 4);
        assert!(pair.generated_by.contains(&RetrievalPass::TimeOverlap));
        assert!(pair.generated_by.contains(&RetrievalPass::NamedPlace));
        assert!(pair.generated_by.contains(&RetrievalPass::SharedSource));
        assert!(pair.generated_by.contains(&RetrievalPass::Text));
    }

    #[test]
    fn legacy_style_records_with_incomplete_information_are_valid() {
        let mut left = incident("legacy-a");
        left.label = "Imported incident".to_string();
        left.description = Some("unusual light over horizon".to_string());
        let mut right = incident("legacy-b");
        right.description = Some("light observed near horizon".to_string());
        let pair = candidate(&generate(vec![left, right]), "legacy-a", "legacy-b");
        assert_eq!(pair.missing_information.len(), 4);
        assert!(
            pair.supporting_signals
                .iter()
                .any(|signal| matches!(signal, SupportingSignal::SharedTextTerms { .. }))
        );
    }

    #[test]
    fn common_terms_are_suppressed_and_reported() {
        let incidents = (0..20)
            .map(|index| {
                let mut item = incident(&format!("incident-{index:02}"));
                item.description = Some(format!("object report unique{index}"));
                item
            })
            .collect();
        let output = generate(incidents);
        assert!(output.candidates.is_empty());
        assert!(
            output
                .suppressed_buckets
                .iter()
                .any(|bucket| { bucket.pass == RetrievalPass::Text && bucket.key == "object" }),
            "suppressed common term is exposed to the caller"
        );
    }

    #[test]
    fn output_is_stable_across_input_order_and_repeated_runs() {
        let mut first = incident("b");
        first.description = Some("orbital object bright".to_string());
        let mut second = incident("a");
        second.description = Some("bright orbital light".to_string());
        second.source_ids.push("shared-source".to_string());
        first.source_ids.push("shared-source".to_string());
        let input = CandidateGenerationInput {
            incidents: vec![first.clone(), second.clone()],
            source_dependencies: Vec::new(),
        };
        let repeated = generate_incident_candidates(&input).expect("first run");
        let repeated_again = generate_incident_candidates(&input).expect("second run");
        let reversed = generate_incident_candidates(&CandidateGenerationInput {
            incidents: vec![second, first],
            source_dependencies: Vec::new(),
        })
        .expect("reordered run");
        assert_eq!(repeated, repeated_again);
        assert_eq!(repeated, reversed);
    }

    #[test]
    fn excessive_time_buckets_are_reported_instead_of_expanded() {
        let mut long_period = incident("long");
        long_period.time_period = Some(DatasetTimePeriod {
            start_date: "1900-01-01".to_string(),
            end_date: "2020-12-31".to_string(),
            precision: DatasetTimePrecision::BoundedPeriod,
            original_wording: None,
        });
        let output = generate(vec![long_period, incident("other")]);
        assert!(output.candidates.is_empty());
        assert!(
            output.suppressed_buckets.iter().any(|bucket| {
                bucket.pass == RetrievalPass::TimeOverlap && bucket.key == "long"
            })
        );
    }
}
