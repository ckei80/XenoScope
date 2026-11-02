import { useEffect, useState } from "react";
import {
  type ImportedResearchCase,
  type ResearchCaseListEntry,
  type ResearchDatasetImportHistoryEntry,
} from "./domain/researchDataset";
import ResearchCaseDetail from "./components/ResearchCaseDetail";
import ResearchCaseLibrary from "./components/ResearchCaseLibrary";
import {
  getResearchCase,
  getResearchCaseLibrary,
  getResearchDatasetImportHistory,
  importBundledResearchDataset,
} from "./lib/researchDataset";
import "./App.css";

const navigation = [
  { label: "Overview", glyph: "◫", view: "overview" },
  { label: "Case Files", glyph: "▤", view: "caseFiles" },
  { label: "Evidence", glyph: "⌕", view: "evidence" },
  { label: "Scan History", glyph: "↗", view: "scanHistory" },
];

function formatImportDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? value
    : new Intl.DateTimeFormat(undefined, {
        dateStyle: "medium",
        timeStyle: "short",
      }).format(date);
}

function App() {
  const [notice, setNotice] = useState("");
  const [activeView, setActiveView] = useState("overview");
  const [caseLibrary, setCaseLibrary] = useState<ResearchCaseListEntry[]>([]);
  const [libraryState, setLibraryState] = useState<"loading" | "ready">("loading");
  const [libraryError, setLibraryError] = useState("");
  const [dashboardCases, setDashboardCases] = useState<ImportedResearchCase[]>([]);
  const [recentCasesState, setRecentCasesState] = useState<"loading" | "ready">("loading");
  const [recentCasesError, setRecentCasesError] = useState("");
  const [importActivity, setImportActivity] = useState<ResearchDatasetImportHistoryEntry[]>([]);
  const [activityState, setActivityState] = useState<"loading" | "ready">("loading");
  const [activityError, setActivityError] = useState("");
  const [selectedDatasetId, setSelectedDatasetId] = useState<string | null>(null);
  const [researchCase, setResearchCase] = useState<ImportedResearchCase | null>(null);
  const [detailState, setDetailState] = useState<"idle" | "loading" | "ready" | "error">("idle");
  const [detailError, setDetailError] = useState("");
  const [importError, setImportError] = useState("");
  const [importMessage, setImportMessage] = useState("");
  const [isImporting, setIsImporting] = useState(false);

  async function refreshDashboardData(entries: ResearchCaseListEntry[]) {
    setRecentCasesState("loading");
    setActivityState("loading");
    setRecentCasesError("");
    setActivityError("");

    if (entries.length === 0) {
      setDashboardCases([]);
      setImportActivity([]);
      setRecentCasesState("ready");
      setActivityState("ready");
      return;
    }

    const [caseResults, historyResults] = await Promise.all([
      Promise.allSettled(entries.map((entry) => getResearchCase(entry.datasetId))),
      Promise.allSettled(
        entries.map((entry) => getResearchDatasetImportHistory(entry.datasetId)),
      ),
    ]);

    const cases = caseResults.flatMap((result) =>
      result.status === "fulfilled" ? [result.value] : [],
    );
    const caseErrors = caseResults.flatMap((result, index) =>
      result.status === "rejected"
        ? [`${entries[index].datasetId}: ${String(result.reason)}`]
        : [],
    );
    const histories = historyResults.flatMap((result) =>
      result.status === "fulfilled" ? result.value : [],
    );
    const historyErrors = historyResults.flatMap((result, index) =>
      result.status === "rejected"
        ? [`${entries[index].datasetId}: ${String(result.reason)}`]
        : [],
    );

    setDashboardCases(cases);
    setImportActivity(histories);
    setRecentCasesError(caseErrors.join("; "));
    setActivityError(historyErrors.join("; "));
    setRecentCasesState("ready");
    setActivityState("ready");
  }

  async function refreshCaseLibrary(): Promise<boolean> {
    setLibraryState("loading");
    setLibraryError("");
    try {
      const entries = await getResearchCaseLibrary();
      setCaseLibrary(entries);
      setLibraryState("ready");
      await refreshDashboardData(entries);
      return true;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setLibraryError(message);
      setDashboardCases([]);
      setImportActivity([]);
      setRecentCasesError(message);
      setActivityError(message);
      setLibraryState("ready");
      setRecentCasesState("ready");
      setActivityState("ready");
      return false;
    }
  }

  async function openResearchCase(
    datasetId: string,
    cachedCase?: ImportedResearchCase,
  ) {
    setSelectedDatasetId(datasetId);
    setActiveView("caseFiles");
    setResearchCase(cachedCase ?? null);
    setDetailState(cachedCase ? "ready" : "loading");
    setDetailError("");
    if (cachedCase) return;
    try {
      setResearchCase(await getResearchCase(datasetId));
      setDetailState("ready");
    } catch (error) {
      setDetailError(error instanceof Error ? error.message : String(error));
      setDetailState("error");
    }
  }

  function returnToCaseLibrary() {
    setSelectedDatasetId(null);
    setResearchCase(null);
    setDetailState("idle");
    setDetailError("");
  }

  useEffect(() => {
    void refreshCaseLibrary();
  }, []);

  async function handleImportBundledCase() {
    setIsImporting(true);
    setImportError("");
    setImportMessage("");
    try {
      const result = await importBundledResearchDataset();
      const removedKeysMessage =
        result.removedKeys.length > 0
          ? ` ${result.removedKeys.length} stable key(s) are absent from this version: ${result.removedKeys.join(", ")}. Earlier records and mappings are retained.`
          : "";
      const statusMessage =
        (result.status === "imported"
          ? `Imported dataset ${result.datasetId} version ${result.datasetVersion} (sequence ${result.importSequence}).`
          : `Already imported: dataset ${result.datasetId} version ${result.datasetVersion} (sequence ${result.importSequence}).`) +
        removedKeysMessage;
      setImportMessage(statusMessage);
      await refreshCaseLibrary();
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setImportError(message);
    } finally {
      setIsImporting(false);
    }
  }

  return (
    <div className="min-h-screen bg-[#080d15] text-slate-100 md:flex">
      <aside className="border-b border-white/[0.07] bg-[#0b111b] px-5 py-5 md:sticky md:top-0 md:flex md:h-screen md:w-60 md:shrink-0 md:flex-col md:border-b-0 md:border-r md:border-white/[0.07] md:px-4 md:py-6">
        <a
          aria-label="XenoScope overview"
          className="flex w-fit items-center gap-3"
          href="#overview"
          onClick={() => setActiveView("overview")}
        >
          <span className="flex size-10 items-center justify-center rounded-xl border border-cyan-300/20 bg-cyan-300/[0.08] text-sm font-bold tracking-tight text-cyan-200">
            XS
          </span>
          <span>
            <span className="block text-[15px] font-semibold tracking-wide text-slate-100">
              XenoScope
            </span>
            <span className="mt-0.5 block text-[10px] font-medium uppercase tracking-[0.2em] text-slate-500">
              Research workstation
            </span>
          </span>
        </a>

        <p className="mb-2 mt-8 hidden px-3 text-[10px] font-semibold uppercase tracking-[0.18em] text-slate-600 md:block">
          Workspace
        </p>
        <nav
          aria-label="Primary navigation"
          className="mt-5 flex gap-2 overflow-x-auto md:mt-0 md:flex-col"
        >
          {navigation.map((item) => (
            <button
              aria-current={activeView === item.view ? "page" : undefined}
              className={`flex shrink-0 items-center gap-3 rounded-lg px-3 py-2.5 text-left text-[13px] transition ${
                activeView === item.view
                  ? "border border-cyan-300/[0.12] bg-cyan-300/[0.08] font-medium text-cyan-100"
                  : "border border-transparent text-slate-500 hover:bg-white/[0.04] hover:text-slate-300"
              }`}
              key={item.label}
              onClick={() => {
                if (item.view === "overview" || item.view === "caseFiles") {
                  setActiveView(item.view);
                  setNotice("");
                  if (item.view === "caseFiles") returnToCaseLibrary();
                } else {
                  setNotice(`${item.label} will be available in a future milestone.`);
                }
              }}
              type="button"
            >
              <span
                aria-hidden="true"
                className={`flex size-5 items-center justify-center text-base ${
                  activeView === item.view ? "text-cyan-300" : "text-slate-600"
                }`}
              >
                {item.glyph}
              </span>
              <span>{item.label}</span>
              {item.view !== "overview" && item.view !== "caseFiles" && (
                <span className="ml-auto hidden text-[9px] uppercase tracking-wider text-slate-600 md:inline">
                  Soon
                </span>
              )}
            </button>
          ))}
        </nav>

        <div className="mt-auto hidden rounded-xl border border-white/[0.06] bg-white/[0.02] p-3 md:block">
          <p className="text-[10px] font-semibold uppercase tracking-[0.15em] text-slate-500">
            Research mode
          </p>
          <p className="mt-1.5 text-xs text-slate-400">Local workspace · Demo and imported case data</p>
        </div>
      </aside>

      <main className="min-w-0 flex-1" id="overview">
        <header className="flex flex-col gap-5 border-b border-white/[0.07] px-5 py-6 sm:flex-row sm:items-center sm:justify-between sm:px-8 lg:px-10">
          <div>
            <p className="text-[10px] font-semibold uppercase tracking-[0.2em] text-cyan-300/70">
              {activeView === "overview" ? "Research overview" : "Research records"}
            </p>
            <div className="mt-2 flex flex-wrap items-center gap-3">
              <h1 className="text-2xl font-semibold tracking-tight text-slate-100">
                {activeView === "overview"
                  ? "Overview"
                  : selectedDatasetId && researchCase
                    ? "Case investigation"
                    : "Case Files"}
              </h1>
            </div>
            <p className="mt-1 text-[13px] text-slate-500">
              {activeView === "overview"
                ? "Explore the collection, follow its sources, and see what the data contains."
                : selectedDatasetId
                  ? "Persisted case detail from this local workspace."
                  : "Persisted research datasets available in this local workspace."}
            </p>
          </div>
          <div className="flex w-fit items-center gap-3 rounded-xl border border-white/[0.07] bg-[#0c131e] px-3.5 py-3">
            <span className="relative flex size-2.5">
              <span className="absolute inline-flex size-full rounded-full bg-slate-500/30" />
              <span className="relative inline-flex size-2.5 rounded-full bg-slate-500" />
            </span>
            <span>
              <span className="block text-[10px] font-semibold uppercase tracking-[0.13em] text-slate-400">
                Local AI · Not connected
              </span>
              <span className="mt-1 block text-[11px] text-slate-600">
                Status placeholder
              </span>
            </span>
          </div>
        </header>

        {notice && (
          <p aria-live="polite" className="border-b border-white/[0.07] px-5 py-3 text-xs text-slate-400 sm:px-8 lg:px-10">
            {notice}
          </p>
        )}

        {activeView === "overview" ? (
        <div className="mx-auto max-w-[1500px] space-y-6 px-5 py-6 sm:px-8 sm:py-8 lg:px-10">
          <section className="relative overflow-hidden rounded-2xl border border-cyan-200/[0.1] bg-gradient-to-br from-[#101e2b] via-[#0d1722] to-[#0b121c] p-6 sm:p-8">
            <div
              aria-hidden="true"
              className="pointer-events-none absolute -right-10 -top-24 size-72 rounded-full bg-cyan-300/[0.05] blur-3xl"
            />
            <div className="relative max-w-2xl">
              <p className="text-[10px] font-semibold uppercase tracking-[0.2em] text-cyan-300/70">
                Explore the unknown
              </p>
              <h2 className="mt-3 text-2xl font-semibold tracking-tight text-slate-100 sm:text-3xl">
                Follow the evidence.
              </h2>
              <p className="mt-2 max-w-xl text-[13px] leading-6 text-slate-400">
                Discover what the data reveals. Explore imported reports and their
                sources while keeping uncertainty and missing information visible.
              </p>
              <button
                className="mt-5 inline-flex items-center gap-2 rounded-lg border border-cyan-200/20 bg-cyan-300 px-4 py-2.5 text-[12px] font-semibold text-[#071019] transition hover:bg-cyan-200 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan-200"
                onClick={() => {
                  setActiveView("caseFiles");
                  returnToCaseLibrary();
                }}
                type="button"
              >
                Explore case files
                <span aria-hidden="true" className="text-sm">
                  →
                </span>
              </button>
            </div>
            <div className="absolute bottom-0 right-0 hidden h-full w-[34%] opacity-40 sm:block">
              <div className="absolute inset-0 bg-[radial-gradient(ellipse_at_75%_55%,rgba(34,211,238,0.12),transparent_60%)]" />
              <div className="absolute bottom-8 right-10 size-40 rounded-full border border-cyan-100/10" />
              <div className="absolute bottom-8 right-10 size-40 scale-y-100 rounded-full border border-cyan-100/[0.07]" />
              <div className="absolute bottom-8 right-[calc(2.5rem+2.5rem)] h-40 border-l border-cyan-100/[0.07]" />
              <div className="absolute bottom-[calc(2rem+5rem)] right-10 w-40 border-t border-cyan-100/[0.07]" />
            </div>
          </section>

          {libraryError && (
            <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-rose-200/15 bg-rose-200/[0.04] p-4" role="alert">
              <p className="break-words text-xs text-rose-100">
                Dashboard collection data could not be loaded: {libraryError}
              </p>
              <button
                className="rounded-lg border border-white/10 px-3 py-2 text-xs text-slate-200 hover:bg-white/[0.05]"
                onClick={() => void refreshCaseLibrary()}
                type="button"
              >
                Retry
              </button>
            </div>
          )}

          <section
            aria-label="Persisted research totals"
            className="grid grid-cols-2 gap-3 lg:grid-cols-4"
          >
            {[
              { label: "Imported datasets", value: caseLibrary.length },
              {
                label: "Incident records",
                value: caseLibrary.reduce(
                  (total, entry) => total + entry.recordCounts.incidents,
                  0,
                ),
              },
              {
                label: "Source records",
                value: caseLibrary.reduce(
                  (total, entry) => total + entry.recordCounts.sources,
                  0,
                ),
              },
              {
                label: "Evidence items",
                value: caseLibrary.reduce(
                  (total, entry) => total + entry.recordCounts.evidenceItems,
                  0,
                ),
              },
            ].map((metric) => (
              <article
                className="rounded-xl border border-white/[0.07] bg-[#0c131d] p-4 sm:p-5"
                key={metric.label}
              >
                <div className="flex items-start justify-between gap-2">
                  <p className="text-[11px] font-medium text-slate-400 sm:text-xs">
                    {metric.label}
                  </p>
                </div>
                <p className="mt-4 text-2xl font-semibold tracking-tight text-slate-100">
                  {libraryState === "loading" || libraryError ? "—" : metric.value}
                </p>
                <p className="mt-1 text-[10px] leading-4 text-slate-600">
                  {libraryError
                    ? "Collection totals unavailable"
                    : "Persisted records in the latest imports"}
                </p>
              </article>
            ))}
          </section>

          <div className="grid gap-6 xl:grid-cols-[minmax(0,1.5fr)_minmax(280px,0.85fr)]">
            <section className="rounded-xl border border-white/[0.07] bg-[#0b121c] p-5 sm:p-6">
              <p className="text-[10px] font-semibold uppercase tracking-[0.15em] text-cyan-300/70">
                Global exploration
              </p>
              <h2 className="mt-2 text-sm font-semibold text-slate-200">Map view unavailable</h2>
              <p className="mt-2 max-w-xl text-xs leading-5 text-slate-400">
                The current collection stores location descriptions, but no geographic
                coordinates. No locations are plotted; prose locations are not converted
                into map points.
              </p>
            </section>

            <section className="rounded-xl border border-white/[0.07] bg-[#0b121c]">
              <div className="border-b border-white/[0.06] px-5 py-4">
                <h2 className="text-[13px] font-semibold text-slate-200">Recently added</h2>
                <p className="mt-1 text-[11px] text-slate-600">
                  Incident records in the latest imported dataset snapshots
                </p>
              </div>
              {recentCasesState === "loading" ? (
                <p aria-live="polite" className="px-5 py-4 text-xs text-slate-500">
                  Loading imported incident records…
                </p>
              ) : (
                <>
                  {recentCasesError && (
                    <p className="px-5 pt-4 text-xs text-rose-200" role="alert">
                      Some incident records could not be loaded: {recentCasesError}
                    </p>
                  )}
                  {dashboardCases.length > 0 ? (
                    <ul className="divide-y divide-white/[0.05]">
                      {dashboardCases
                        .flatMap((researchCase) =>
                          researchCase.incidents.map((incident) => ({
                            datasetId: researchCase.datasetId,
                            importSequence: researchCase.importSequence,
                            importedAt: researchCase.importedAt,
                            incident,
                            researchCase,
                          })),
                        )
                        .sort(
                          (a, b) =>
                            Date.parse(b.importedAt) - Date.parse(a.importedAt) ||
                            a.incident.label.localeCompare(b.incident.label),
                        )
                        .slice(0, 6)
                        .map((item) => (
                          <li
                            className="px-5 py-4"
                            key={`${item.datasetId}:${item.incident.record.datasetKey}`}
                          >
                            <button
                              className="w-full text-left focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan-200"
                              onClick={() => void openResearchCase(item.datasetId, item.researchCase)}
                              type="button"
                            >
                              <span className="block text-xs font-medium text-slate-200">
                                {item.incident.label}
                              </span>
                              <span className="mt-1 block text-[10px] text-slate-500">
                                Dataset imported {formatImportDate(item.importedAt)} · sequence{" "}
                                {item.importSequence}
                              </span>
                              <span className="mt-2 block text-[11px] leading-5 text-slate-400">
                                {item.incident.description ?? "No incident description is recorded."}
                              </span>
                              <span className="mt-2 block text-[10px] font-medium text-cyan-200">
                                Open case details →
                              </span>
                            </button>
                          </li>
                        ))}
                    </ul>
                  ) : (
                    !recentCasesError && (
                      <p className="px-5 py-4 text-xs text-slate-500">
                        No incident records are available in the imported collection yet.
                      </p>
                    )
                  )}
                </>
              )}
            </section>
          </div>

          <section className="rounded-xl border border-white/[0.07] bg-[#0b121c]">
            <div className="border-b border-white/[0.06] px-5 py-4">
              <h2 className="text-[13px] font-semibold text-slate-200">Research activity</h2>
              <p className="mt-1 text-[11px] text-slate-600">
                Recorded dataset imports. An import is not necessarily a new incident discovery.
              </p>
            </div>
            {activityState === "loading" ? (
              <p aria-live="polite" className="px-5 py-4 text-xs text-slate-500">
                Loading import history…
              </p>
            ) : (
              <>
                {activityError && (
                  <p className="px-5 pt-4 text-xs text-rose-200" role="alert">
                    Some import history could not be loaded: {activityError}
                  </p>
                )}
                {importActivity.length > 0 ? (
                  <ul className="divide-y divide-white/[0.05]">
                    {[...importActivity]
                      .sort(
                        (a, b) =>
                          Date.parse(b.importedAt) - Date.parse(a.importedAt) ||
                          b.importSequence - a.importSequence,
                      )
                      .slice(0, 8)
                      .map((entry) => (
                        <li
                          className="flex flex-wrap items-start justify-between gap-2 px-5 py-3"
                          key={`${entry.datasetId}:${entry.importSequence}`}
                        >
                          <div>
                            <p className="text-xs font-medium text-slate-300">Dataset imported</p>
                            <p className="mt-1 break-all text-[10px] text-slate-500">
                              {entry.datasetId} · v{entry.datasetVersion} · sequence{" "}
                              {entry.importSequence}
                            </p>
                          </div>
                          <time className="text-[10px] text-slate-500" dateTime={entry.importedAt}>
                            {formatImportDate(entry.importedAt)}
                          </time>
                        </li>
                      ))}
                  </ul>
                ) : (
                  !activityError && (
                    <p className="px-5 py-4 text-xs text-slate-500">
                      No dataset import activity is recorded yet.
                    </p>
                  )
                )}
              </>
            )}
          </section>
        </div>
        ) : selectedDatasetId ? (
          <div className="mx-auto max-w-[1500px] space-y-5 px-5 py-6 sm:px-8 sm:py-8 lg:px-10">
            {detailState === "loading" && (
              <p aria-live="polite" className="rounded-xl border border-white/[0.07] bg-[#0b121c] p-5 text-sm text-slate-400">
                Loading persisted case investigation…
              </p>
            )}
            {detailState === "error" && (
              <div role="alert" className="rounded-xl border border-rose-200/15 bg-rose-200/[0.04] p-4">
                <p className="text-sm font-medium text-rose-100">Could not load this case.</p>
                <p className="mt-2 break-words text-xs text-rose-100/70">{detailError}</p>
                <div className="mt-3 flex gap-2">
                  <button
                    className="rounded-lg border border-white/10 px-3 py-2 text-xs text-slate-200 hover:bg-white/[0.05]"
                    onClick={() => void openResearchCase(selectedDatasetId)}
                    type="button"
                  >
                    Retry loading case
                  </button>
                  <button
                    className="rounded-lg border border-white/10 px-3 py-2 text-xs text-slate-300 hover:bg-white/[0.05]"
                    onClick={returnToCaseLibrary}
                    type="button"
                  >
                    Back to case files
                  </button>
                </div>
              </div>
            )}
            {researchCase && (
              <ResearchCaseDetail
                onBack={returnToCaseLibrary}
                researchCase={researchCase}
              />
            )}
          </div>
        ) : (
          <ResearchCaseLibrary
            entries={caseLibrary}
            error={libraryError}
            importError={importError}
            importMessage={importMessage}
            isImporting={isImporting}
            isLoading={libraryState === "loading"}
            onImportBundled={() => void handleImportBundledCase()}
            onRetry={() => void refreshCaseLibrary()}
            onSelect={(datasetId) => void openResearchCase(datasetId)}
          />
        )}
      </main>
    </div>
  );
}

export default App;
