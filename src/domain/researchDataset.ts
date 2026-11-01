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
