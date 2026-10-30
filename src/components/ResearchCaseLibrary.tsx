import {
  BUNDLED_RESEARCH_DATASET_ID,
  type ResearchCaseListEntry,
} from "../domain/researchDataset";

type Props = {
  entries: ResearchCaseListEntry[];
  isLoading: boolean;
  error: string;
  importMessage: string;
  importError: string;
  isImporting: boolean;
  onSelect: (datasetId: string) => void;
  onRetry: () => void;
  onImportBundled: () => void;
};

function countSummary(entry: ResearchCaseListEntry): string {
  const { evidenceItems, sources, claims } = entry.recordCounts;
  return `${sources} ${sources === 1 ? "source" : "sources"} · ${evidenceItems} ${
    evidenceItems === 1 ? "evidence item" : "evidence items"
  } · ${claims} ${claims === 1 ? "claim" : "claims"}`;
}

export default function ResearchCaseLibrary({
  entries,
  isLoading,
  error,
  importMessage,
  importError,
  isImporting,
  onSelect,
  onRetry,
  onImportBundled,
}: Props) {
  const bundledCaseAvailable = entries.some(
    (entry) => entry.datasetId === BUNDLED_RESEARCH_DATASET_ID,
  );

  return (
    <div className="mx-auto max-w-[1500px] space-y-5 px-5 py-6 sm:px-8 sm:py-8 lg:px-10">
      <section className="rounded-2xl border border-white/[0.07] bg-[#0b121c] p-5 sm:p-7">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div>
            <p className="text-[10px] font-semibold uppercase tracking-[0.16em] text-cyan-300/70">
              Persisted research data
            </p>
            <h2 className="mt-2 text-lg font-semibold text-slate-100">Case file library</h2>
            <p className="mt-2 max-w-2xl text-xs leading-5 text-slate-400">
              Each entry represents a persisted dataset view and may contain multiple incident
              records. Counts describe stored records, not evidence strength or confidence.
            </p>
          </div>
          {!isLoading && !error && (
            <button
              className="rounded-lg border border-cyan-200/20 bg-cyan-300 px-4 py-2.5 text-xs font-semibold text-[#071019] transition hover:bg-cyan-200 disabled:cursor-wait disabled:opacity-60"
              disabled={isImporting}
              onClick={onImportBundled}
              type="button"
            >
              {isImporting
                ? "Checking bundled case…"
                : bundledCaseAvailable
                  ? "Check bundled case import"
                  : "Import bundled case"}
            </button>
          )}
        </div>
        {isLoading && (
          <p aria-live="polite" className="mt-4 text-xs text-slate-500">
            Loading persisted case files…
          </p>
        )}
        {error && (
          <div role="alert" className="mt-4 rounded-lg border border-rose-200/15 bg-rose-200/[0.04] p-3">
            <p className="text-xs text-rose-100">{error}</p>
            <button
              className="mt-2 text-xs text-slate-300 underline underline-offset-2"
              onClick={onRetry}
              type="button"
            >
              Retry loading library
            </button>
          </div>
        )}
        {importMessage && (
          <p aria-live="polite" className="mt-4 rounded-lg border border-emerald-200/10 bg-emerald-200/[0.04] p-3 text-xs text-emerald-100/80">
            {importMessage}
          </p>
        )}
        {importError && (
          <div role="alert" className="mt-4 rounded-lg border border-rose-200/15 bg-rose-200/[0.04] p-3">
            <p className="text-xs text-rose-100">Import failed: {importError}</p>
            <button
              className="mt-2 text-xs text-slate-300 underline underline-offset-2"
              disabled={isImporting}
              onClick={onImportBundled}
              type="button"
            >
              Try import again
            </button>
          </div>
        )}
      </section>

      {entries.length > 0 ? (
        <ul className="grid gap-4 lg:grid-cols-2">
          {entries.map((entry) => (
            <li key={entry.datasetId}>
              <button
                className="h-full w-full rounded-xl border border-white/[0.07] bg-[#0b121c] p-5 text-left transition hover:border-cyan-200/20 hover:bg-[#0e1722] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan-200"
                onClick={() => onSelect(entry.datasetId)}
                type="button"
              >
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0">
                    {entry.incidentLabels.length > 0 ? (
                      <>
                        <h3 className="text-sm font-semibold text-slate-100">
                          {entry.incidentLabels[0]}
                        </h3>
                        {entry.incidentLabels.length > 1 && (
                          <p className="mt-1 text-[11px] text-slate-400">
                            Includes {entry.incidentLabels.length} incidents
                          </p>
                        )}
                      </>
                    ) : (
                      <h3 className="break-all text-sm font-semibold text-slate-100">
                        {entry.datasetId}
                      </h3>
                    )}
                  </div>
                  <span className="shrink-0 rounded border border-cyan-200/10 bg-cyan-200/[0.04] px-2 py-1 text-[9px] uppercase tracking-wide text-cyan-100/70">
                    v{entry.datasetVersion}
                  </span>
                </div>
                {entry.incidentLabels.length > 1 && (
                  <ul className="mt-3 space-y-1">
                    {entry.incidentLabels.slice(1).map((label) => (
                      <li className="text-[11px] text-slate-400" key={label}>
                        {label}
                      </li>
                    ))}
                  </ul>
                )}
                <p className="mt-3 break-all text-[10px] text-slate-600">{entry.datasetId}</p>
                <div className="mt-4 flex flex-wrap gap-x-3 gap-y-1 text-[10px] text-slate-500">
                  <span>{countSummary(entry)}</span>
                  <span>{entry.recordCounts.relationships} relationships</span>
                  <span>{entry.recordCounts.discrepancies} discrepancies</span>
                  <span>{entry.recordCounts.findings} findings</span>
                </div>
                {entry.recordCounts.discrepancies > 0 && (
                  <p className="mt-2 text-[10px] text-amber-100/70">
                    Discrepancy status: {entry.discrepancyStatusCounts.open} open ·{" "}
                    {entry.discrepancyStatusCounts.investigating} investigating ·{" "}
                    {entry.discrepancyStatusCounts.documented} documented
                  </p>
                )}
                <p className="mt-3 text-[10px] text-slate-600">
                  Import sequence {entry.importSequence} · {entry.importedAt}
                </p>
                <p className="mt-4 text-xs font-medium text-cyan-200">Open investigation →</p>
              </button>
            </li>
          ))}
        </ul>
      ) : (
        !isLoading &&
        !error && (
          <section className="rounded-xl border border-white/[0.07] bg-[#0b121c] p-5 text-xs text-slate-400">
            No persisted case files are available yet. Importing the bundled case is always an
            explicit action.
          </section>
        )
      )}
    </div>
  );
}
