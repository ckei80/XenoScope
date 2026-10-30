import { useEffect, useState } from "react";
import { CircleMarker, MapContainer, TileLayer, Tooltip } from "react-leaflet";
import {
  type ImportedResearchCase,
  type ResearchCaseListEntry,
} from "./domain/researchDataset";
import ResearchCaseDetail from "./components/ResearchCaseDetail";
import ResearchCaseLibrary from "./components/ResearchCaseLibrary";
import {
  getResearchCase,
  getResearchCaseLibrary,
  importBundledResearchDataset,
} from "./lib/researchDataset";
import "./App.css";

const navigation = [
  { label: "Overview", glyph: "◫", view: "overview" },
  { label: "Case Files", glyph: "▤", view: "caseFiles" },
  { label: "Evidence", glyph: "⌕", view: "evidence" },
  { label: "Scan History", glyph: "↗", view: "scanHistory" },
];

const metrics = [
  { label: "Reports tracked", value: "128", detail: "Across all sample records" },
  { label: "Open case files", value: "46", detail: "Illustrative placeholder" },
  { label: "Evidence items", value: "312", detail: "Illustrative placeholder" },
  { label: "Last scan", value: "—", detail: "Scanning not available yet" },
];

type DemoMarker = {
  id: string;
  position: [number, number];
  description: string;
};

const demoMarkers: DemoMarker[] = [
  {
    id: "Demo marker 01",
    position: [18, -38],
    description:
      "Synthetic UI marker for map interaction only. No real-world event or observation is represented.",
  },
  {
    id: "Demo marker 02",
    position: [8, 92],
    description:
      "Synthetic UI marker for map interaction only. No report, source, or evidence is represented.",
  },
  {
    id: "Demo marker 03",
    position: [-28, 148],
    description:
      "Synthetic UI marker for map interaction only. This is not a verified finding.",
  },
];

function App() {
  const [notice, setNotice] = useState("");
  const [selectedDemoId, setSelectedDemoId] = useState(demoMarkers[0].id);
  const [activeView, setActiveView] = useState("overview");
  const [caseLibrary, setCaseLibrary] = useState<ResearchCaseListEntry[]>([]);
  const [libraryState, setLibraryState] = useState<"loading" | "ready">("loading");
  const [libraryError, setLibraryError] = useState("");
  const [selectedDatasetId, setSelectedDatasetId] = useState<string | null>(null);
  const [researchCase, setResearchCase] = useState<ImportedResearchCase | null>(null);
  const [detailState, setDetailState] = useState<"idle" | "loading" | "ready" | "error">("idle");
  const [detailError, setDetailError] = useState("");
  const [importError, setImportError] = useState("");
  const [importMessage, setImportMessage] = useState("");
  const [isImporting, setIsImporting] = useState(false);
  const selectedDemo = demoMarkers.find((marker) => marker.id === selectedDemoId);

  async function refreshCaseLibrary(): Promise<boolean> {
    setLibraryState("loading");
    setLibraryError("");
    try {
      setCaseLibrary(await getResearchCaseLibrary());
      setLibraryState("ready");
      return true;
    } catch (error) {
      setLibraryError(error instanceof Error ? error.message : String(error));
      setLibraryState("ready");
      return false;
    }
  }

  async function openResearchCase(datasetId: string) {
    setSelectedDatasetId(datasetId);
    setResearchCase(null);
    setDetailState("loading");
    setDetailError("");
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
              {activeView === "overview" && (
                <span className="rounded border border-amber-200/10 bg-amber-200/[0.04] px-2 py-1 text-[9px] font-semibold uppercase tracking-[0.14em] text-amber-100/70">
                  Demo data
                </span>
              )}
            </div>
            <p className="mt-1 text-[13px] text-slate-500">
              {activeView === "overview"
                ? "A workspace for organizing and examining UAP reports."
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

        {activeView === "overview" ? (
        <div className="mx-auto max-w-[1500px] space-y-6 px-5 py-6 sm:px-8 sm:py-8 lg:px-10">
          <section className="relative overflow-hidden rounded-2xl border border-cyan-200/[0.1] bg-gradient-to-br from-[#101e2b] via-[#0d1722] to-[#0b121c] p-6 sm:p-8">
            <div
              aria-hidden="true"
              className="pointer-events-none absolute -right-10 -top-24 size-72 rounded-full bg-cyan-300/[0.05] blur-3xl"
            />
            <div className="relative max-w-2xl">
              <p className="text-[10px] font-semibold uppercase tracking-[0.2em] text-cyan-300/70">
                Welcome to XenoScope
              </p>
              <h2 className="mt-3 text-2xl font-semibold tracking-tight text-slate-100 sm:text-3xl">
                Investigate with care.
              </h2>
              <p className="mt-2 max-w-xl text-[13px] leading-6 text-slate-400">
                Organize reports, review evidence, and keep a traceable research
                history. No live sources or analysis are connected yet.
              </p>
              <button
                className="mt-5 inline-flex items-center gap-2 rounded-lg border border-cyan-200/20 bg-cyan-300 px-4 py-2.5 text-[12px] font-semibold text-[#071019] transition hover:bg-cyan-200 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan-200"
                onClick={() =>
                  setNotice("Research scanning is not implemented yet. No scan was started.")
                }
                type="button"
              >
                Scan the Sky
                <span aria-hidden="true" className="text-sm">
                  →
                </span>
              </button>
              {notice && (
                <p aria-live="polite" className="mt-3 text-xs text-cyan-100/80">
                  {notice}
                </p>
              )}
            </div>
            <div className="absolute bottom-0 right-0 hidden h-full w-[34%] opacity-40 sm:block">
              <div className="absolute inset-0 bg-[radial-gradient(ellipse_at_75%_55%,rgba(34,211,238,0.12),transparent_60%)]" />
              <div className="absolute bottom-8 right-10 size-40 rounded-full border border-cyan-100/10" />
              <div className="absolute bottom-8 right-10 size-40 scale-y-100 rounded-full border border-cyan-100/[0.07]" />
              <div className="absolute bottom-8 right-[calc(2.5rem+2.5rem)] h-40 border-l border-cyan-100/[0.07]" />
              <div className="absolute bottom-[calc(2rem+5rem)] right-10 w-40 border-t border-cyan-100/[0.07]" />
            </div>
          </section>

          <section aria-label="Sample workspace metrics" className="grid grid-cols-2 gap-3 lg:grid-cols-4">
            {metrics.map((metric) => (
              <article
                className="rounded-xl border border-white/[0.07] bg-[#0c131d] p-4 sm:p-5"
                key={metric.label}
              >
                <div className="flex items-start justify-between gap-2">
                  <p className="text-[11px] font-medium text-slate-400 sm:text-xs">
                    {metric.label}
                  </p>
                  <span className="rounded border border-cyan-300/10 bg-cyan-300/[0.05] px-1.5 py-0.5 text-[8px] font-semibold uppercase tracking-[0.12em] text-cyan-200/70">
                    Sample
                  </span>
                </div>
                <p className="mt-4 text-2xl font-semibold tracking-tight text-slate-100">
                  {metric.value}
                </p>
                <p className="mt-1 text-[10px] leading-4 text-slate-600">
                  {metric.detail}
                </p>
              </article>
            ))}
          </section>

          <div className="grid gap-6 xl:grid-cols-[minmax(0,1.5fr)_minmax(280px,0.85fr)]">
            <section className="overflow-hidden rounded-xl border border-white/[0.07] bg-[#0b121c]">
              <div className="flex flex-wrap items-center justify-between gap-3 border-b border-white/[0.06] px-5 py-4">
                <div>
                  <h2 className="text-[13px] font-semibold text-slate-200">
                    Global report distribution
                  </h2>
                  <p className="mt-1 text-[11px] text-slate-600">
                    Interactive map · synthetic demonstration markers only
                  </p>
                </div>
                <span className="rounded-md border border-amber-200/10 bg-amber-200/[0.04] px-2 py-1 text-[9px] font-medium uppercase tracking-[0.12em] text-amber-100/70">
                  Demo data
                </span>
              </div>
              <MapContainer
                aria-label="Interactive world map with synthetic demonstration markers"
                center={[18, 12]}
                className="case-map"
                scrollWheelZoom
                zoom={2}
                minZoom={2}
                maxZoom={7}
              >
                <TileLayer
                  attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap contributors</a>'
                  url="https://tile.openstreetmap.org/{z}/{x}/{y}.png"
                />
                {demoMarkers.map((marker) => (
                  <CircleMarker
                    center={marker.position}
                    eventHandlers={{
                      click: () => setSelectedDemoId(marker.id),
                    }}
                    key={marker.id}
                    pathOptions={{
                      color: selectedDemoId === marker.id ? "#a5f3fc" : "#22d3ee",
                      fillColor: selectedDemoId === marker.id ? "#22d3ee" : "#0e7490",
                      fillOpacity: 0.9,
                      weight: 2,
                    }}
                    radius={selectedDemoId === marker.id ? 9 : 7}
                  >
                    <Tooltip permanent direction="top" offset={[0, -8]}>
                      <span>DEMO · {marker.id.slice(-2)}</span>
                    </Tooltip>
                  </CircleMarker>
                ))}
              </MapContainer>
              <div
                aria-live="polite"
                className="border-t border-white/[0.06] bg-[#0c131d] px-5 py-4"
              >
                <p className="text-[9px] font-semibold uppercase tracking-[0.14em] text-amber-100/70">
                  Selected demonstration marker · not a sighting
                </p>
                {selectedDemo && (
                  <>
                    <h3 className="mt-1.5 text-xs font-medium text-slate-200">
                      {selectedDemo.id}
                    </h3>
                    <p className="mt-1 text-[11px] leading-5 text-slate-500">
                      {selectedDemo.description}
                    </p>
                  </>
                )}
              </div>
            </section>

            <section className="rounded-xl border border-white/[0.07] bg-[#0b121c]">
              <div className="flex items-center justify-between border-b border-white/[0.06] px-5 py-4">
                <div>
                  <h2 className="text-[13px] font-semibold text-slate-200">
                    Demo Cases
                  </h2>
                  <p className="mt-1 text-[11px] text-slate-600">
                    Synthetic examples · not real reports
                  </p>
                </div>
                <span className="text-[10px] font-medium text-slate-600">
                  03 demos
                </span>
              </div>
              <ul className="divide-y divide-white/[0.05]">
                {demoMarkers.map((marker) => (
                  <li className="px-5 py-4" key={marker.id}>
                    <div className="flex items-center justify-between gap-3">
                      <p className="text-xs font-medium text-slate-300">
                        {marker.id}
                      </p>
                      <span className="shrink-0 rounded border border-amber-200/10 bg-amber-200/[0.04] px-1.5 py-0.5 text-[8px] font-semibold tracking-[0.1em] text-amber-100/60">
                        DEMO
                      </span>
                    </div>
                    <p className="mt-1.5 text-[10px] leading-4 text-slate-600">
                      UI-only map example · no report details
                    </p>
                  </li>
                ))}
              </ul>
              <div className="border-t border-white/[0.05] px-5 py-3">
                <p className="text-[10px] text-slate-600">
                  Demonstration markers are synthetic and do not represent findings.
                </p>
              </div>
            </section>
          </div>
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
