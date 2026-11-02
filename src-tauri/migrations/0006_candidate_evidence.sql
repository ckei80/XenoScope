CREATE TABLE candidate_evidence_snapshots (
    suggestion_id TEXT PRIMARY KEY NOT NULL
        REFERENCES research_suggestions (suggestion_id) ON DELETE RESTRICT,
    snapshot_json TEXT NOT NULL CHECK (json_valid(snapshot_json)),
    fingerprint TEXT NOT NULL CHECK (length(trim(fingerprint)) > 0),
    generator_version TEXT NOT NULL CHECK (length(trim(generator_version)) > 0),
    evidence_format_version INTEGER NOT NULL CHECK (evidence_format_version > 0),
    created_at TEXT NOT NULL,
    CHECK (length(trim(created_at)) > 0)
);

CREATE INDEX candidate_evidence_fingerprint_idx
    ON candidate_evidence_snapshots (fingerprint);

CREATE INDEX research_suggestions_target_idx
    ON research_suggestions (decision_kind, record_a_id, record_b_id);

CREATE TRIGGER candidate_evidence_snapshot_validate_insert
BEFORE INSERT ON candidate_evidence_snapshots
WHEN NOT EXISTS (
    SELECT 1
    FROM research_suggestions suggestion
    WHERE suggestion.suggestion_id = NEW.suggestion_id
      AND suggestion.decision_kind = 'incident_pair'
      AND json_type(NEW.snapshot_json, '$.candidate.left_incident_id') = 'text'
      AND json_type(NEW.snapshot_json, '$.candidate.right_incident_id') = 'text'
      AND json_extract(NEW.snapshot_json, '$.candidate.left_incident_id') =
          suggestion.record_a_id
      AND json_extract(NEW.snapshot_json, '$.candidate.right_incident_id') =
          suggestion.record_b_id
      AND json_type(NEW.snapshot_json, '$.evidenceFormatVersion') = 'integer'
      AND json_extract(NEW.snapshot_json, '$.evidenceFormatVersion') =
          NEW.evidence_format_version
      AND json_type(NEW.snapshot_json, '$.generatorVersion') = 'text'
      AND json_extract(NEW.snapshot_json, '$.generatorVersion') = NEW.generator_version
      AND json_type(NEW.snapshot_json, '$.fingerprint') = 'text'
      AND json_extract(NEW.snapshot_json, '$.fingerprint') = NEW.fingerprint
)
BEGIN
    SELECT RAISE(ABORT, 'candidate evidence snapshot metadata or target mismatch');
END;

CREATE TRIGGER candidate_evidence_snapshot_no_update
BEFORE UPDATE ON candidate_evidence_snapshots
BEGIN
    SELECT RAISE(ABORT, 'candidate evidence snapshots are immutable');
END;

CREATE TRIGGER candidate_evidence_snapshot_no_delete
BEFORE DELETE ON candidate_evidence_snapshots
BEGIN
    SELECT RAISE(ABORT, 'candidate evidence snapshots are immutable');
END;
