import { useEffect, useState, type ReactNode } from "react";
import type {
  ImportedResearchCase,
  ResearchCaseRecordReference,
  ResearchDatasetImportHistoryEntry,
} from "../domain/researchDataset";
import type { SourceLocator } from "../domain/research";
import {
  getResearchCase,
  getResearchCaseImport,
  getResearchDatasetImportHistory,
} from "../lib/researchDataset";

type Props = {
  researchCase: ImportedResearchCase;
  onBack: () => void;
};

function Locator({ locator }: { locator: SourceLocator }) {
  switch (locator.kind) {
    case "page":
      return <span>Page {locator.page}</span>;
    case "section":
      return (
        <span>
          Section: {locator.section}
          {locator.paragraph !== undefined && ` · Paragraph ${locator.paragraph}`}
        </span>
      );
    case "timecode":
      return (
        <span>
          Timecode: {locator.start}
          {locator.end !== undefined && `–${locator.end}`}
        </span>
      );
    case "text_fragment":
      return <span>Text fragment: “{locator.text}”</span>;
    case "other":
      return <span>Locator: {locator.description}</span>;
  }
}

function RecordReference({ reference }: { reference: ResearchCaseRecordReference }) {
  return (
    <span className="break-all">
      {reference.recordKind} · {reference.datasetKey}
    </span>
  );
}

function sourceTitle(researchCase: ImportedResearchCase, datasetKey: string): string {
  const source = researchCase.sources.find((item) => item.record.datasetKey === datasetKey);
  return source?.title ?? datasetKey;
}

function sourceAccess(researchCase: ImportedResearchCase, datasetKey: string) {
  return researchCase.metadata.sourceAccess.filter((item) => item.sourceKey === datasetKey);
}

function sourceMetadata(researchCase: ImportedResearchCase, datasetKey: string) {
  return researchCase.metadata.sourceMetadata.filter((item) => item.sourceKey === datasetKey);
}

function ExternalUrl({ label, url }: { label: string; url: string }) {
  return (
    <p className="break-all text-xs">
      <span className="text-slate-500">{label}: </span>
      <a
        className="text-cyan-200 underline decoration-cyan-200/30 underline-offset-2 hover:text-cyan-100"
        href={url}
        rel="noreferrer"
        target="_blank"
      >
        {url}
      </a>
    </p>
  );
}

function formatStatus(status: string): string {
  return status.replace(/_/g, " ");
}

function MetadataFields({ fields }: { fields: Record<string, string> | null }) {
  if (!fields || Object.keys(fields).length === 0) return null;

  return (
    <dl className="mt-3 grid gap-2 sm:grid-cols-2">
      {Object.entries(fields).map(([field, value]) => (
        <div className="rounded-lg bg-white/[0.03] p-3" key={field}>
          <dt className="text-[10px] font-medium uppercase tracking-wide text-slate-500">
            {field}
          </dt>
          <dd className="mt-1 break-words text-xs text-slate-300">{value}</dd>
        </div>
      ))}
    </dl>
  );
}

function Section({
  title,
  count,
  children,
}: {
  title: string;
  count?: number;
  children: ReactNode;
}) {
  return (
    <section className="rounded-xl border border-white/[0.07] bg-[#0b121c]">
      <div className="flex items-center justify-between gap-3 border-b border-white/[0.06] px-4 py-4 sm:px-5">
        <h2 className="text-sm font-semibold text-slate-200">{title}</h2>
        {count !== undefined && (
          <span className="text-[10px] text-slate-500">{count}</span>
        )}
      </div>
      <div className="divide-y divide-white/[0.05]">{children}</div>
    </section>
  );
}

function EmptySection({ children }: { children: string }) {
  return <p className="px-4 py-4 text-xs text-slate-500 sm:px-5">{children}</p>;
}

export default function ResearchCaseDetail({ researchCase, onBack }: Props) {
  const [displayedCase, setDisplayedCase] = useState(researchCase);
  const [history, setHistory] = useState<ResearchDatasetImportHistoryEntry[] | null>(null);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyError, setHistoryError] = useState("");
  const [selectionLoading, setSelectionLoading] = useState(false);
  const [selectionError, setSelectionError] = useState("");

  useEffect(() => {
    setDisplayedCase(researchCase);
    setHistory(null);
    setHistoryOpen(false);
    setHistoryError("");
    setSelectionError("");
  }, [researchCase]);

  async function toggleHistory() {
    if (historyOpen) {
      setHistoryOpen(false);
      return;
    }

    setHistoryOpen(true);
    if (history !== null) return;

    setHistoryLoading(true);
    setHistoryError("");
    try {
      setHistory(await getResearchDatasetImportHistory(researchCase.datasetId));
    } catch (error) {
      setHistoryError(error instanceof Error ? error.message : String(error));
    } finally {
      setHistoryLoading(false);
    }
  }

  async function selectImport(importSequence: number) {
    if (importSequence === displayedCase.importSequence) return;
    setSelectionLoading(true);
    setSelectionError("");
    try {
      setDisplayedCase(
        await getResearchCaseImport(researchCase.datasetId, importSequence),
      );
    } catch (error) {
      setSelectionError(error instanceof Error ? error.message : String(error));
    } finally {
      setSelectionLoading(false);
    }
  }

  async function returnToLatest() {
    const latestSequence =
      history && history.length > 0
        ? history[history.length - 1].importSequence
        : undefined;
    if (latestSequence === undefined || latestSequence === displayedCase.importSequence) return;
    setSelectionLoading(true);
    setSelectionError("");
    try {
      setDisplayedCase(await getResearchCase(researchCase.datasetId));
    } catch (error) {
      setSelectionError(error instanceof Error ? error.message : String(error));
    } finally {
      setSelectionLoading(false);
    }
  }

  const latestSequence =
    history && history.length > 0
      ? history[history.length - 1].importSequence
      : researchCase.importSequence;
  const isHistorical = displayedCase.importSequence !== latestSequence;
  const title = displayedCase.incidents[0]?.label ?? displayedCase.datasetId;

  return (
    <div className="space-y-5">
      <section className="rounded-2xl border border-cyan-200/[0.1] bg-gradient-to-br from-[#101e2b] via-[#0d1722] to-[#0b121c] p-5 sm:p-7">
        <button
          className="mb-5 rounded-lg border border-white/10 px-3 py-2 text-xs text-slate-300 transition hover:bg-white/[0.05] hover:text-white"
          onClick={onBack}
          type="button"
        >
          ← Back to case files
        </button>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <p className="text-[10px] font-semibold uppercase tracking-[0.18em] text-cyan-300/70">
              {isHistorical
                ? `Historical import — version ${displayedCase.datasetVersion}, sequence ${displayedCase.importSequence}`
                : `Imported research dataset · v${displayedCase.datasetVersion}`}
            </p>
            <h2 className="mt-2 text-xl font-semibold text-slate-100 sm:text-2xl">{title}</h2>
            {displayedCase.incidents.length > 1 && (
              <p className="mt-2 text-xs text-slate-400">
                Also recorded: {displayedCase.incidents.slice(1).map((incident) => incident.label).join("; ")}
              </p>
            )}
            <p className="mt-2 break-all text-[11px] text-slate-500">
              Dataset {displayedCase.datasetId} · import sequence {displayedCase.importSequence}
            </p>
            <p className="mt-1 text-[10px] text-slate-600">
              Imported at {displayedCase.importedAt}
            </p>
          </div>
          <span className="rounded border border-cyan-200/10 bg-cyan-200/[0.04] px-2 py-1 text-[9px] font-semibold uppercase tracking-wide text-cyan-100/70">
            Persisted case
          </span>
        </div>
        <div className="mt-5 flex flex-wrap items-center gap-2">
          <button
            aria-expanded={historyOpen}
            className="rounded-lg border border-white/10 px-3 py-2 text-xs text-slate-300 transition hover:bg-white/[0.05] hover:text-white"
            onClick={() => void toggleHistory()}
            type="button"
          >
            {historyOpen ? "Hide version history" : "Version history"}
          </button>
          {isHistorical && (
            <button
              className="rounded-lg border border-cyan-200/20 bg-cyan-200/[0.06] px-3 py-2 text-xs text-cyan-100 transition hover:bg-cyan-200/[0.1] disabled:opacity-50"
              disabled={selectionLoading}
              onClick={() => void returnToLatest()}
              type="button"
            >
              Return to latest import
            </button>
          )}
        </div>
        {historyOpen && (
          <div className="mt-3 rounded-xl border border-white/[0.07] bg-black/10 p-4">
            <p className="text-xs font-medium text-slate-200">Available imports</p>
            <p className="mt-1 text-[11px] leading-5 text-slate-500">
              Imported content follows the selected snapshot. Review details and discrepancy
              status may reflect current workflow state.
            </p>
            {historyLoading && (
              <p aria-live="polite" className="mt-3 text-xs text-slate-400">
                Loading version history…
              </p>
            )}
            {historyError && (
              <p className="mt-3 text-xs text-rose-200" role="alert">
                Could not load version history: {historyError}
              </p>
            )}
            {history && history.length === 0 && (
              <p className="mt-3 text-xs text-slate-400">No import history is available.</p>
            )}
            {history?.length === 1 && (
              <p className="mt-3 text-xs text-slate-400">
                This dataset has one recorded import; no older versions are available.
              </p>
            )}
            {history && history.length > 0 && (
              <ul className="mt-3 space-y-2">
                {[...history].reverse().map((entry) => {
                  const isSelected = entry.importSequence === displayedCase.importSequence;
                  const isLatest = entry.importSequence === latestSequence;
                  return (
                    <li key={entry.importSequence}>
                      <button
                        aria-current={isSelected ? "true" : undefined}
                        className="flex w-full flex-wrap items-center justify-between gap-2 rounded-lg border border-white/[0.06] px-3 py-2 text-left transition hover:bg-white/[0.04] disabled:cursor-wait disabled:opacity-60"
                        disabled={selectionLoading || isSelected}
                        onClick={() => void selectImport(entry.importSequence)}
                        type="button"
                      >
                        <span className="text-xs text-slate-200">
                          v{entry.datasetVersion} · sequence {entry.importSequence}
                        </span>
                        <span className="flex items-center gap-2 text-[10px] text-slate-500">
                          {isLatest && <span className="text-cyan-200">Latest</span>}
                          {isSelected && <span className="text-emerald-200">Selected</span>}
                          <time>{entry.importedAt}</time>
                        </span>
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
            {selectionLoading && (
              <p aria-live="polite" className="mt-3 text-xs text-slate-400">
                Loading selected import…
              </p>
            )}
            {selectionError && (
              <p className="mt-3 text-xs text-rose-200" role="alert">
                Could not load the selected import: {selectionError}
              </p>
            )}
          </div>
        )}
        <p className="mt-5 rounded-lg border border-amber-200/10 bg-amber-200/[0.04] p-3 text-xs leading-5 text-amber-100/80">
          AARO’s assessment is attributed to AARO; XenoScope has not independently verified it.
        </p>
        <p className="mt-3 text-[11px] leading-5 text-slate-500">
          The incident time period records its declared precision; more specific dates and
          durations remain source-reported claims, with unresolved differences shown below.
        </p>
      </section>

      <Section title="Dates and timing · source-reported" count={displayedCase.claims.filter((claim) => claim.category === "timing").length}>
        {displayedCase.claims.filter((claim) => claim.category === "timing").map((claim) => (
          <article className="space-y-2 px-4 py-4 sm:px-5" key={claim.record.datasetKey}>
            <p className="text-xs leading-5 text-slate-300">{claim.assertionText}</p>
            <p className="text-[11px] text-slate-400">
              Attributed to {sourceTitle(displayedCase, claim.source.datasetKey)}
            </p>
            <p className="break-words text-[11px] text-slate-500">
              <Locator locator={claim.sourceLocator} />
            </p>
          </article>
        ))}
        {!displayedCase.claims.some((claim) => claim.category === "timing") && (
          <EmptySection>No source-reported timing claims are recorded.</EmptySection>
        )}
      </Section>

      <Section title="Incident records" count={displayedCase.incidents.length}>
        {displayedCase.incidents.map((incident) => (
          <article className="space-y-3 px-4 py-4 sm:px-5" key={incident.record.datasetKey}>
            <div>
              <p className="text-xs font-medium text-slate-300">{incident.label}</p>
              <p className="mt-1 break-all text-[10px] text-slate-500">{incident.record.datasetKey}</p>
            </div>
            {incident.description && (
              <p className="text-xs leading-5 text-slate-300">{incident.description}</p>
            )}
            {incident.location && (
              <div className="text-xs text-slate-400">
                <p>
                  Location: {incident.location.description} ·{" "}
                  {formatStatus(incident.location.precision)}
                </p>
                {incident.location.originalWording && (
                  <p className="mt-1 text-[11px] text-slate-500">
                    Original wording: “{incident.location.originalWording}”
                  </p>
                )}
              </div>
            )}
            {incident.timePeriod && (
              <div className="text-xs text-slate-400">
                <p>
                  Time period: {incident.timePeriod.startDate} – {incident.timePeriod.endDate} ·{" "}
                  {formatStatus(incident.timePeriod.precision)}
                </p>
                {incident.timePeriod.originalWording && (
                  <p className="mt-1 text-[11px] text-slate-500">
                    Original wording: “{incident.timePeriod.originalWording}”
                  </p>
                )}
              </div>
            )}
            {incident.sources && incident.sources.length > 0 && (
              <div className="space-y-1">
                <p className="text-[11px] font-medium text-slate-400">Incident sources</p>
                {incident.sources.map((source) => (
                  <p className="text-[11px] text-slate-500" key={source.record.datasetKey}>
                    {source.title ?? source.publisher ?? source.record.datasetKey}
                    {" · "}
                    <span className="break-all">{source.record.datasetKey}</span>
                  </p>
                ))}
              </div>
            )}
          </article>
        ))}
        {displayedCase.incidents.length === 0 && <EmptySection>No incident records are recorded.</EmptySection>}
      </Section>

      <Section title="Dataset reference conventions">
        <div className="space-y-2 px-4 py-4 text-xs leading-5 text-slate-400 sm:px-5">
          <p>{displayedCase.metadata.referenceConvention.recordIdentity}</p>
          <p>{displayedCase.metadata.referenceConvention.references}</p>
          <p>{displayedCase.metadata.referenceConvention.omittedRecordMetadata}</p>
        </div>
      </Section>

      <Section title="Sources" count={displayedCase.sources.length}>
        {displayedCase.sources.map((source) => {
          const access = sourceAccess(displayedCase, source.record.datasetKey);
          const metadata = sourceMetadata(displayedCase, source.record.datasetKey);
          return (
            <article className="space-y-3 px-4 py-4 sm:px-5" key={source.record.datasetKey}>
              <div className="flex flex-wrap items-start justify-between gap-2">
                <div>
                  <h3 className="text-xs font-semibold text-slate-200">
                    {source.title ?? source.record.datasetKey}
                  </h3>
                  <p className="mt-1 text-[11px] text-slate-500">
                    {source.publisher ?? "Publisher not recorded"}
                    {source.author ? ` · ${source.author}` : ""}
                    {source.publicationDate ? ` · ${source.publicationDate}` : ""}
                  </p>
                </div>
                <span className="rounded border border-white/[0.08] px-2 py-1 text-[9px] uppercase tracking-wide text-slate-400">
                  Verification: {formatStatus(source.verificationStatus)}
                </span>
              </div>
              <p className="text-[10px] text-slate-500">
                {formatStatus(source.sourceKind)} · key {source.record.datasetKey}
              </p>
              {source.url ? (
                <ExternalUrl label="Stored source URL" url={source.url} />
              ) : (
                <p className="text-xs text-slate-500">No source URL recorded.</p>
              )}
              {source.otherLocator && (
                <p className="text-xs leading-5 text-slate-400">{source.otherLocator}</p>
              )}
              {access.map((item, index) => (
                <div
                  className="space-y-2 rounded-lg border border-white/[0.05] bg-white/[0.02] p-3"
                  key={`${item.sourceKey}-access-${index}`}
                >
                  <p className="text-[9px] font-semibold uppercase tracking-wide text-slate-500">
                    Dataset access notes · do not imply current reachability
                  </p>
                  <ExternalUrl label="Canonical URL" url={item.canonicalUrl} />
                  {item.inspectedCopyUrl && (
                    <ExternalUrl label="Inspected copy URL" url={item.inspectedCopyUrl} />
                  )}
                  {item.accessNote && (
                    <p className="text-[11px] leading-5 text-slate-400">{item.accessNote}</p>
                  )}
                  {item.equivalenceLimitation && (
                    <p className="text-[11px] leading-5 text-amber-100/70">
                      Limitation: {item.equivalenceLimitation}
                    </p>
                  )}
                  {item.inspectionNote && (
                    <p className="text-[11px] leading-5 text-slate-400">
                      {item.inspectionNote}
                    </p>
                  )}
                </div>
              ))}
              {metadata.map((item, index) => (
                <div
                  className="rounded-lg border border-white/[0.05] bg-white/[0.02] p-3"
                  key={`${item.sourceKey}-metadata-${index}`}
                >
                  <p className="text-[9px] font-semibold uppercase tracking-wide text-slate-500">
                    Source metadata
                  </p>
                  {item.fieldsAsPrinted && (
                    <>
                      <p className="mt-3 text-[10px] font-medium text-slate-400">Fields as printed</p>
                      <MetadataFields fields={item.fieldsAsPrinted} />
                    </>
                  )}
                  {item.fieldsAsDisplayed && (
                    <>
                      <p className="mt-3 text-[10px] font-medium text-slate-400">Fields as displayed</p>
                      <MetadataFields fields={item.fieldsAsDisplayed} />
                    </>
                  )}
                  <p className="mt-3 text-[11px] leading-5 text-slate-400">
                    {item.dateInterpretation}
                  </p>
                </div>
              ))}
            </article>
          );
        })}
        {displayedCase.sources.length === 0 && <EmptySection>No sources are recorded.</EmptySection>}
      </Section>

      <Section title="Evidence items" count={displayedCase.evidenceItems.length}>
        {displayedCase.evidenceItems.map((evidence) => (
          <article className="space-y-2 px-4 py-4 sm:px-5" key={evidence.record.datasetKey}>
            <div className="flex flex-wrap justify-between gap-2">
              <h3 className="text-xs font-semibold text-slate-200">
                {formatStatus(evidence.evidenceKind)}
              </h3>
              <span className="text-[10px] text-slate-500">{evidence.record.datasetKey}</span>
            </div>
            <p className="text-xs leading-5 text-slate-400">{evidence.description}</p>
            <p className="text-[11px] text-slate-400">
              Source: {sourceTitle(displayedCase, evidence.source.datasetKey)}
            </p>
            <p className="break-words text-[11px] leading-5 text-slate-500">
              <Locator locator={evidence.locator} />
            </p>
          </article>
        ))}
        {displayedCase.evidenceItems.length === 0 && (
          <EmptySection>No evidence items are recorded.</EmptySection>
        )}
      </Section>

      <Section title="Claims" count={displayedCase.claims.length}>
        {displayedCase.claims.map((claim) => (
          <article className="space-y-2 px-4 py-4 sm:px-5" key={claim.record.datasetKey}>
            <div className="flex flex-wrap items-center justify-between gap-2">
              <span className="text-[9px] uppercase tracking-wide text-slate-500">
                {formatStatus(claim.category)}
              </span>
              <span className="rounded border border-white/[0.08] px-2 py-1 text-[9px] uppercase tracking-wide text-slate-400">
                Review: {formatStatus(claim.review.status)}
              </span>
            </div>
            <p className="text-xs leading-5 text-slate-300">{claim.assertionText}</p>
            <p className="text-[11px] text-slate-400">
              Source: {sourceTitle(displayedCase, claim.source.datasetKey)}
            </p>
            <p className="break-words text-[11px] leading-5 text-slate-500">
              <Locator locator={claim.sourceLocator} />
            </p>
            {claim.review.reviewedAt && (
              <p className="text-[10px] text-slate-500">
                Review recorded at {claim.review.reviewedAt}
                {claim.review.reviewedBy &&
                  ` by ${claim.review.reviewedBy.displayName ?? claim.review.reviewedBy.kind}`}
              </p>
            )}
            {claim.authorStatedConfidence && (
              <div className="rounded-lg border border-cyan-200/[0.08] bg-cyan-200/[0.025] p-3">
                <p className="text-[9px] font-semibold uppercase tracking-wide text-cyan-100/60">
                  Author-stated confidence · not a XenoScope assessment
                </p>
                <p className="mt-1 text-[11px] text-slate-300">
                  {claim.authorStatedConfidence.valueAsStated} — stated by{" "}
                  {claim.authorStatedConfidence.statedBy}; source:{" "}
                  {sourceTitle(displayedCase, claim.authorStatedConfidence.source.datasetKey)}
                </p>
                {claim.authorStatedConfidence.locator && (
                  <p className="mt-1 break-words text-[10px] text-slate-500">
                    <Locator locator={claim.authorStatedConfidence.locator} />
                  </p>
                )}
              </div>
            )}
          </article>
        ))}
        {displayedCase.claims.length === 0 && <EmptySection>No claims are recorded.</EmptySection>}
      </Section>

      <Section title="Relationships" count={displayedCase.relationships.length}>
        {displayedCase.relationships.map((relationship) => (
          <article className="space-y-2 px-4 py-4 sm:px-5" key={relationship.record.datasetKey}>
            <p className="text-xs font-semibold text-slate-200">
              {formatStatus(relationship.relationshipKind)}
            </p>
            <p className="break-all text-[11px] text-slate-400">
              <RecordReference reference={relationship.from} />
            </p>
            <p aria-hidden="true" className="text-[10px] text-slate-600">↓ points to</p>
            <p className="break-all text-[11px] text-slate-400">
              <RecordReference reference={relationship.to} />
            </p>
            <p className="break-all text-[10px] text-slate-600">
              Relationship key: {relationship.record.datasetKey}
            </p>
          </article>
        ))}
        {displayedCase.relationships.length === 0 && (
          <EmptySection>No relationships are recorded.</EmptySection>
        )}
      </Section>

      <Section title="Unresolved discrepancies" count={displayedCase.discrepancies.length}>
        {displayedCase.discrepancies.map((discrepancy) => (
          <article className="space-y-3 px-4 py-4 sm:px-5" key={discrepancy.record.datasetKey}>
            <div className="flex flex-wrap items-center justify-between gap-2">
              <span className="rounded border border-amber-200/10 bg-amber-200/[0.04] px-2 py-1 text-[9px] font-semibold uppercase tracking-wide text-amber-100/80">
                {formatStatus(discrepancy.status)} · unresolved
              </span>
              <span className="break-all text-[10px] text-slate-600">
                {discrepancy.record.datasetKey}
              </span>
            </div>
            <p className="text-xs leading-5 text-slate-300">{discrepancy.description}</p>
            <div className="space-y-1">
              <p className="text-[9px] font-semibold uppercase tracking-wide text-slate-500">
                Linked claims
              </p>
              {discrepancy.claimReferences.map((reference) => {
                const claim = displayedCase.claims.find(
                  (item) => item.record.id === reference.recordId,
                );
                return (
                  <p className="break-words text-[11px] text-slate-400" key={reference.recordId}>
                    {claim?.assertionText ?? reference.datasetKey}
                  </p>
                );
              })}
            </div>
          </article>
        ))}
        {displayedCase.discrepancies.length === 0 && (
          <EmptySection>No discrepancies are recorded.</EmptySection>
        )}
      </Section>

      <Section title="Dataset relationship notes" count={displayedCase.metadata.relationshipNotes.length}>
        {displayedCase.metadata.relationshipNotes.map((note) => (
          <article className="space-y-2 px-4 py-4 sm:px-5" key={note.relationshipKey}>
            <p className="break-all text-[10px] text-slate-500">{note.relationshipKey}</p>
            <p className="text-xs leading-5 text-slate-400">{note.basis}</p>
            <p className="text-[10px] text-amber-100/60">
              Curated dataset note; not independent corroboration or additional evidence.
            </p>
          </article>
        ))}
        {displayedCase.metadata.relationshipNotes.length === 0 && (
          <EmptySection>No relationship notes are recorded.</EmptySection>
        )}
      </Section>

      <Section title="Findings" count={displayedCase.findings.length}>
        {displayedCase.findings.map((finding) => (
          <article className="space-y-2 px-4 py-4 sm:px-5" key={finding.record.datasetKey}>
            <p className="text-xs leading-5 text-slate-300">{finding.assessment}</p>
            <p className="text-[11px] text-slate-500">
              Attributed to {finding.attribution.kind}
              {finding.attribution.displayName ? ` · ${finding.attribution.displayName}` : ""}
              {" · "}Review: {formatStatus(finding.review.status)}
            </p>
            {[...finding.claimReferences, ...finding.evidenceReferences].map((reference) => (
              <p className="break-all text-[10px] text-slate-500" key={`${reference.record.recordId}-${reference.role}`}>
                {reference.role}: <RecordReference reference={reference.record} />
              </p>
            ))}
          </article>
        ))}
        {displayedCase.findings.length === 0 && (
          <EmptySection>No findings are recorded in this dataset version.</EmptySection>
        )}
      </Section>
    </div>
  );
}
