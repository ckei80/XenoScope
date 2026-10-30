export type UUID = string;
export type ISODateTime = string;

export type RecordKind =
  | "incident"
  | "source"
  | "evidence_item"
  | "claim"
  | "relationship"
  | "discrepancy"
  | "finding"
  | "history";

export interface RecordActor {
  kind: "researcher" | "system" | "ai";
  displayName?: string;
}

export interface RecordMetadata {
  id: UUID;
  createdAt: ISODateTime;
  createdBy: RecordActor;
}

export interface RecordReference {
  recordKind: RecordKind;
  recordId: UUID;
}

export interface Incident extends RecordMetadata {
  label: string;
}

export type SourceKind =
  | "web_page"
  | "publication"
  | "official_record"
  | "dataset"
  | "firsthand_account"
  | "archive"
  | "other";

export type SourceVerificationStatus =
  | "not_assessed"
  | "unverified"
  | "verified"
  | "needs_review";

export interface Source extends RecordMetadata {
  sourceKind: SourceKind;
  title?: string;
  publisher?: string;
  author?: string;
  publicationDate?: string;
  url?: string;
  otherLocator?: string;
  verificationStatus: SourceVerificationStatus;
}

export type SourceLocator =
  | { kind: "page"; page: number }
  | { kind: "section"; section: string; paragraph?: number }
  | { kind: "timecode"; start: string; end?: string }
  | { kind: "text_fragment"; text: string }
  | { kind: "other"; description: string };

export type EvidenceKind =
  | "document"
  | "image"
  | "video"
  | "audio"
  | "dataset"
  | "physical_item"
  | "other";

export interface EvidenceItem extends RecordMetadata {
  sourceId: UUID;
  locator: SourceLocator;
  evidenceKind: EvidenceKind;
  description: string;
}

export type ClaimCategory =
  | "observation"
  | "identification"
  | "timing"
  | "location"
  | "context"
  | "other";

export type ClaimReviewStatus =
  | "unreviewed"
  | "in_review"
  | "reviewed"
  | "needs_revision";

export interface ReviewMetadata {
  status: ClaimReviewStatus;
  reviewedAt?: ISODateTime;
  reviewedBy?: RecordActor;
}

export interface AuthorStatedConfidence {
  statedBy: string;
  valueAsStated: string;
  sourceId: UUID;
  locator?: SourceLocator;
}

export interface Claim extends RecordMetadata {
  assertionText: string;
  category: ClaimCategory;
  sourceId: UUID;
  sourceLocator: SourceLocator;
  incidentId?: UUID;
  authorStatedConfidence?: AuthorStatedConfidence;
  review: ReviewMetadata;
}

export type RelationshipKind =
  | "quotes"
  | "repeats"
  | "derived_from"
  | "analyses"
  | "supports"
  | "contradicts"
  | "corrects";

export interface Relationship extends RecordMetadata {
  kind: RelationshipKind;
  from: RecordReference;
  to: RecordReference;
}

export type DiscrepancyStatus = "open" | "investigating" | "documented";

export interface Discrepancy extends RecordMetadata {
  claimIds: [UUID, UUID, ...UUID[]];
  description: string;
  status: DiscrepancyStatus;
}

export interface Finding extends RecordMetadata {
  assessment: string;
  attribution: RecordActor;
  supportingClaims: UUID[];
  supportingEvidence: UUID[];
  conflictingClaims: UUID[];
  conflictingEvidence: UUID[];
  review: ReviewMetadata;
}

export type RecordHistoryEventKind =
  | "record_created"
  | "field_changed"
  | "correction_linked"
  | "review_status_changed"
  | "source_verification_changed"
  | "relationship_added";

export interface FieldChange {
  field: string;
  previousValue?: string;
  newValue?: string;
}

export interface RecordHistory extends RecordMetadata {
  subject: RecordReference;
  eventKind: RecordHistoryEventKind;
  summary: string;
  changes?: FieldChange[];
  relatedRecords?: RecordReference[];
}
