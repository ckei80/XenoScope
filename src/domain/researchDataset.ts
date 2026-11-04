import type {
  ClaimCategory,
  ClaimReviewStatus,
  RecordActor,
  RecordKind,
  SourceKind,
  SourceLocator,
  SourceVerificationStatus,
} from "./research";

export type ResearchDatasetImportStatus = "imported" | "already_imported";
export const BUNDLED_RESEARCH_DATASET_ID = "xenoscope.case.al-taqaddum-2017";

export interface ResearchDatasetImportResult {
  datasetId: string;
  datasetVersion: string;
  status: ResearchDatasetImportStatus;
  /** Existing sequence for an identical no-op; no sequence is consumed. */
  importSequence: number;
  removedKeys: string[];
}

export interface ResearchDatasetReferenceConvention {
  recordIdentity: string;
  references: string;
  omittedRecordMetadata: string;
}

export interface ResearchDatasetSourceAccess {
  sourceKey: string;
  canonicalUrl: string;
  inspectedCopyUrl: string | null;
  accessNote: string | null;
  equivalenceLimitation: string | null;
  inspectionNote: string | null;
}

export interface ResearchDatasetSourceMetadata {
  sourceKey: string;
  fieldsAsPrinted: Record<string, string> | null;
  fieldsAsDisplayed: Record<string, string> | null;
  dateInterpretation: string;
}

export interface ResearchDatasetRelationshipNote {
  relationshipKey: string;
  basis: string;
}

export interface ResearchDatasetMetadata {
  referenceConvention: ResearchDatasetReferenceConvention;
  sourceAccess: ResearchDatasetSourceAccess[];
  sourceMetadata: ResearchDatasetSourceMetadata[];
  relationshipNotes: ResearchDatasetRelationshipNote[];
}

export interface ResearchDatasetImportHistoryEntry {
  datasetId: string;
  datasetVersion: string;
  importSequence: number;
  importedAt: string;
  metadata: ResearchDatasetMetadata;
}

export interface ResearchCaseRecordReference {
  datasetKey: string;
  recordId: string;
  recordKind: RecordKind;
}

export interface ResearchCaseRecordMetadata {
  id: string;
  datasetKey: string;
  recordKind: RecordKind;
  createdAt: string;
  createdBy: RecordActor;
}

export interface ResearchCaseIncident {
  record: ResearchCaseRecordMetadata;
  label: string;
  description: string | null;
  location: ResearchIncidentLocation | null;
  timePeriod: ResearchIncidentTimePeriod | null;
  sources: ResearchCaseSource[] | null;
}

export type ResearchIncidentLocationPrecision =
  | "named_place"
  | "approximate_area"
  | "bounded_region";

export interface ResearchIncidentLocation {
  description: string;
  precision: ResearchIncidentLocationPrecision;
  originalWording?: string;
}

export type ResearchIncidentTimePrecision =
  | "exact_day"
  | "approximate_day"
  | "month"
  | "year"
  | "bounded_period";

export interface ResearchIncidentTimePeriod {
  startDate: string;
  endDate: string;
  precision: ResearchIncidentTimePrecision;
  originalWording?: string;
}

export interface ResearchCaseSource {
  record: ResearchCaseRecordMetadata;
  sourceKind: SourceKind;
  title: string | null;
  publisher: string | null;
  author: string | null;
  publicationDate: string | null;
  url: string | null;
  otherLocator: string | null;
  verificationStatus: SourceVerificationStatus;
}

export type ResearchEvidenceKind =
  | "document"
  | "image"
  | "video"
  | "audio"
  | "dataset"
  | "physical_item"
  | "other";

export interface ResearchCaseEvidenceItem {
  record: ResearchCaseRecordMetadata;
  source: ResearchCaseRecordReference;
  locator: SourceLocator;
  evidenceKind: ResearchEvidenceKind;
  description: string;
}

export interface ResearchCaseAuthorStatedConfidence {
  statedBy: string;
  valueAsStated: string;
  source: ResearchCaseRecordReference;
  locator: SourceLocator | null;
}

export interface ResearchCaseClaim {
  record: ResearchCaseRecordMetadata;
  assertionText: string;
  category: ClaimCategory;
  source: ResearchCaseRecordReference;
  sourceLocator: SourceLocator;
  incident: ResearchCaseRecordReference | null;
  authorStatedConfidence: ResearchCaseAuthorStatedConfidence | null;
  review: {
    status: ClaimReviewStatus;
    reviewedAt?: string;
    reviewedBy?: RecordActor;
  };
}

export type ResearchRelationshipKind =
  | "quotes"
  | "repeats"
  | "derived_from"
  | "analyses"
  | "supports"
  | "contradicts"
  | "corrects";

export interface ResearchCaseRelationship {
  record: ResearchCaseRecordMetadata;
  relationshipKind: ResearchRelationshipKind;
  from: ResearchCaseRecordReference;
  to: ResearchCaseRecordReference;
}

export type ResearchDiscrepancyStatus = "open" | "investigating" | "documented";

export interface ResearchCaseDiscrepancy {
  record: ResearchCaseRecordMetadata;
  claimReferences: ResearchCaseRecordReference[];
  description: string;
  status: ResearchDiscrepancyStatus;
}

export type ResearchFindingReferenceRole = "supporting" | "conflicting";

export interface ResearchCaseFindingReference {
  record: ResearchCaseRecordReference;
  role: ResearchFindingReferenceRole;
}

export interface ResearchCaseFinding {
  record: ResearchCaseRecordMetadata;
  assessment: string;
  attribution: RecordActor;
  claimReferences: ResearchCaseFindingReference[];
  evidenceReferences: ResearchCaseFindingReference[];
  review: {
    status: ClaimReviewStatus;
    reviewedAt?: string;
    reviewedBy?: RecordActor;
  };
}

export interface ImportedResearchCase {
  datasetId: string;
  datasetVersion: string;
  importSequence: number;
  importedAt: string;
  metadata: ResearchDatasetMetadata;
  incidents: ResearchCaseIncident[];
  sources: ResearchCaseSource[];
  evidenceItems: ResearchCaseEvidenceItem[];
  claims: ResearchCaseClaim[];
  relationships: ResearchCaseRelationship[];
  discrepancies: ResearchCaseDiscrepancy[];
  findings: ResearchCaseFinding[];
}

export interface ResearchCaseRecordCounts {
  incidents: number;
  sources: number;
  evidenceItems: number;
  claims: number;
  relationships: number;
  discrepancies: number;
  findings: number;
}

export interface ResearchCaseDiscrepancyCounts {
  open: number;
  investigating: number;
  documented: number;
}

export interface ResearchCaseListEntry {
  datasetId: string;
  datasetVersion: string;
  importSequence: number;
  importedAt: string;
  incidentLabels: string[];
  recordCounts: ResearchCaseRecordCounts;
  discrepancyStatusCounts: ResearchCaseDiscrepancyCounts;
}

export interface ResearchCandidate {
  incidentId: string;
  incidentLabel: string;
  candidateCount: number;
  evidenceItemCount: number;
  claimCount: number;
  relationshipCount: number;
  discrepancyCount: number;
  findingCount: number;
  latestImportedAt: string;
  datasetVersion: string;
}

// ── Candidate retrieval (candidate-corpus) ──────────────────────────────

/** One candidate pair with its supporting / tension signals. */
export interface CandidatePair {
  leftIncidentId: string;
  rightIncidentId: string;
  generatedBy: RetrievalPass[];
  tensionSignals: TensionSignal[];
  supportingSignals: SupportingSignal[];
}

/** Which retrieval pass contributed this candidate pair. */
export type RetrievalPass =
  | "timeOverlap"
  | "namedPlace"
  | "sharedSource"
  | "sharedDependentSource"
  | "text";

/** Tension signals (evidence of incompatibility between the pair). */
export type TensionSignal =
  | { kind: "nonOverlappingPrecisePeriods"; left: string; right: string }
  | { kind: "nonOverlappingPrecisePeriodsBounded"; leftStart: string; leftEnd: string; rightStart: string; rightEnd: string }
  | { kind: "nonOverlappingPrecisePeriodsBoundedAsymmetric"; leftStart: string; leftEnd: string; rightStart: string; rightEnd: string; leftIsBounded: boolean; rightIsBounded: boolean }
  | { kind: "nonOverlappingPrecisePeriodsBoundedAsymmetricReversed"; leftStart: string; leftEnd: string; rightStart: string; rightEnd: string; leftIsBounded: boolean; rightIsBounded: boolean }
  | { kind: "differentNamedPlace"; left: string; right: string }
  | { kind: "differentBoundedRegions"; left: string; right: string }
  | { kind: "differentApproximateAreas"; left: string; right: string };

/** Supporting signals (evidence of linkage between the pair). */
export type SupportingSignal =
  | { kind: "sharedTextTerms"; terms: string[]; claimProvenance: SharedTextTermProvenance[] }
  | { kind: "sharedSource"; leftSourceId: string; rightSourceId: string }
  | { kind: "sharedDependentSource"; leftSourceId: string; rightSourceId: string; relation: string }
  | { kind: "sharedNamedPlace"; place: string }
  | { kind: "overlappingBoundedRegions"; left: string; right: string }
  | { kind: "overlappingApproximateAreas"; left: string; right: string };

/** A term and the claims on each side that contain it. */
export interface SharedTextTermProvenance {
  term: string;
  leftClaims: CandidateClaimProvenance[];
  rightClaims: CandidateClaimProvenance[];
}

/** Full candidate corpus returned by the candidate-corpus endpoint. */
export interface CandidateCorpus {
  candidates: CandidatePair[];
  suppressedBuckets: SuppressedIndexBucket[];
  selectedImports: SelectedDatasetImport[];
  incidentProvenance: CandidateIncidentProvenance[];
  claimProvenance: CandidateClaimProvenance[];
  sourceOrigins: Record<string, CandidateSourceOrigin[]>;
  sourceDependencyRecords: CandidateSourceDependencyRecord[];
}

/** A term bucket that was suppressed because it appeared in too many incidents. */
export interface SuppressedIndexBucket {
  term: string;
  reason: string;
}

/** Metadata about a dataset import that was selected for candidate generation. */
export interface SelectedDatasetImport {
  datasetId: string;
  datasetVersion: string;
  importSequence: number;
  importedAt: string;
}

/** Provenance linking a candidate pair back to its source incident record. */
export interface CandidateIncidentProvenance {
  databaseId: string;
  datasetId: string;
  datasetKey: string;
  datasetVersion: string;
  importSequence: number;
  importedAt: string;
}

/** Provenance linking a candidate pair back to a specific claim record. */
export interface CandidateClaimProvenance {
  databaseId: string;
  datasetId: string;
  datasetKey: string;
  incidentId: string;
  sourceId: string;
}

/** A source that appeared in the manifest of a selected import. */
export interface CandidateSourceOrigin {
  datasetId: string;
  datasetKey: string;
}

/** A source-dependency relationship record from the database. */
export interface CandidateSourceDependencyRecord {
  relationshipRecordId: string;
  datasetId: string;
  datasetKey: string;
  edge: CandidateSourceDependencyEdge;
}

/** The edge direction and kind for a source dependency. */
export interface CandidateSourceDependencyEdge {
  relationshipRecordId: string;
  fromSourceId: string;
  toSourceId: string;
  kind: "quotes" | "repeats" | "derivedFrom";
}
