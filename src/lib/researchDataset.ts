import { invoke } from "@tauri-apps/api/core";
import type {
  ImportedResearchCase,
  ResearchCaseListEntry,
  ResearchDatasetImportHistoryEntry,
  ResearchDatasetImportResult,
} from "../domain/researchDataset";

export function importBundledResearchDataset(): Promise<ResearchDatasetImportResult> {
  return invoke<ResearchDatasetImportResult>("import_bundled_research_dataset");
}

export function getBundledResearchCase(): Promise<ImportedResearchCase | null> {
  return invoke<ImportedResearchCase | null>("get_bundled_research_case");
}

export function getResearchCaseLibrary(): Promise<ResearchCaseListEntry[]> {
  return invoke<ResearchCaseListEntry[]>("get_research_case_library");
}

export function getResearchCase(datasetId: string): Promise<ImportedResearchCase> {
  return invoke<ImportedResearchCase>("get_research_case", { datasetId });
}

export function getResearchCaseImport(
  datasetId: string,
  importSequence: number,
): Promise<ImportedResearchCase> {
  return invoke<ImportedResearchCase>("get_research_case_import", {
    datasetId,
    importSequence,
  });
}

export function getResearchDatasetImportHistory(
  datasetId: string,
): Promise<ResearchDatasetImportHistoryEntry[]> {
  return invoke<ResearchDatasetImportHistoryEntry[]>("get_research_dataset_import_history", {
    datasetId,
  });
}
