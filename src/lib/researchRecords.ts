import { invoke } from "@tauri-apps/api/core";
import type {
  AuthorStatedConfidence,
  Claim,
  ClaimCategory,
  Source,
  SourceKind,
  SourceLocator,
  SourceVerificationStatus,
  UUID,
} from "../domain/research";

export interface CreateSourceInput {
  sourceKind: SourceKind;
  title?: string;
  publisher?: string;
  author?: string;
  publicationDate?: string;
  url?: string;
  otherLocator?: string;
  verificationStatus?: SourceVerificationStatus;
}

export interface CreateClaimInput {
  assertionText: string;
  category: ClaimCategory;
  sourceId: UUID;
  sourceLocator: SourceLocator;
  incidentId?: UUID;
  authorStatedConfidence?: AuthorStatedConfidence;
}

export function createSource(input: CreateSourceInput): Promise<Source> {
  return invoke<Source>("create_source", { input });
}

export function getSource(id: UUID): Promise<Source | null> {
  return invoke<Source | null>("get_source", { id });
}

export function createClaim(input: CreateClaimInput): Promise<Claim> {
  return invoke<Claim>("create_claim", { input });
}

export function getClaim(id: UUID): Promise<Claim | null> {
  return invoke<Claim | null>("get_claim", { id });
}
