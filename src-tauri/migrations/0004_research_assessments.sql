CREATE UNIQUE INDEX dataset_imports_dataset_version_sequence_idx
    ON dataset_imports (dataset_id, dataset_version, import_sequence);

CREATE UNIQUE INDEX dataset_record_mappings_assessment_reference_idx
    ON dataset_record_mappings (dataset_id, record_key, record_id);

CREATE TABLE research_assessments (
    assessment_id TEXT PRIMARY KEY NOT NULL,
    dataset_id TEXT NOT NULL,
    dataset_version TEXT NOT NULL,
    import_sequence INTEGER NOT NULL CHECK (import_sequence > 0),
    created_at TEXT NOT NULL,
    created_by_kind TEXT NOT NULL CHECK (created_by_kind IN ('researcher', 'system', 'ai')),
    created_by_name TEXT,
    status TEXT NOT NULL CHECK (
        status IN ('preliminary', 'limited_by_missing_information', 'reviewed_for_scope')
    ),
    score INTEGER CHECK (score IS NULL OR score BETWEEN 1 AND 100),
    scoring_profile_version TEXT NOT NULL,
    scoring_profile_json TEXT NOT NULL CHECK (
        json_valid(scoring_profile_json) AND json_type(scoring_profile_json) = 'object'
    ),
    algorithm_version TEXT NOT NULL,
    input_snapshot_json TEXT NOT NULL CHECK (
        json_valid(input_snapshot_json) AND json_type(input_snapshot_json) = 'object'
    ),
    is_sealed INTEGER NOT NULL DEFAULT 0 CHECK (is_sealed IN (0, 1)),
    supersedes_assessment_id TEXT,
    revision_reason TEXT,
    UNIQUE (assessment_id, dataset_id),
    CHECK (
        supersedes_assessment_id IS NULL
        OR supersedes_assessment_id <> assessment_id
    ),
    CHECK (
        supersedes_assessment_id IS NULL
        OR (revision_reason IS NOT NULL AND length(trim(revision_reason)) > 0)
    ),
    FOREIGN KEY (dataset_id, dataset_version, import_sequence)
        REFERENCES dataset_imports (dataset_id, dataset_version, import_sequence)
        ON DELETE RESTRICT,
    FOREIGN KEY (supersedes_assessment_id, dataset_id)
        REFERENCES research_assessments (assessment_id, dataset_id)
        ON DELETE RESTRICT
);

CREATE TRIGGER research_assessments_require_sealed_predecessor
BEFORE INSERT ON research_assessments
WHEN NEW.supersedes_assessment_id IS NOT NULL
 AND NOT EXISTS (
    SELECT 1 FROM research_assessments predecessor
    WHERE predecessor.assessment_id = NEW.supersedes_assessment_id
      AND predecessor.dataset_id = NEW.dataset_id
      AND predecessor.is_sealed = 1
)
BEGIN
    SELECT RAISE(ABORT, 'assessment predecessor must be sealed');
END;

CREATE TRIGGER research_assessments_only_allow_sealing
BEFORE UPDATE ON research_assessments
WHEN OLD.is_sealed <> 0
  OR NEW.is_sealed <> 1
  OR NEW.assessment_id IS NOT OLD.assessment_id
  OR NEW.dataset_id IS NOT OLD.dataset_id
  OR NEW.dataset_version IS NOT OLD.dataset_version
  OR NEW.import_sequence IS NOT OLD.import_sequence
  OR NEW.created_at IS NOT OLD.created_at
  OR NEW.created_by_kind IS NOT OLD.created_by_kind
  OR NEW.created_by_name IS NOT OLD.created_by_name
  OR NEW.status IS NOT OLD.status
  OR NEW.score IS NOT OLD.score
  OR NEW.scoring_profile_version IS NOT OLD.scoring_profile_version
  OR NEW.scoring_profile_json IS NOT OLD.scoring_profile_json
  OR NEW.algorithm_version IS NOT OLD.algorithm_version
  OR NEW.input_snapshot_json IS NOT OLD.input_snapshot_json
  OR NEW.supersedes_assessment_id IS NOT OLD.supersedes_assessment_id
  OR NEW.revision_reason IS NOT OLD.revision_reason
BEGIN
    SELECT RAISE(ABORT, 'assessments are immutable; create a successor');
END;

CREATE TABLE assessment_claims (
    assessment_id TEXT NOT NULL,
    dataset_id TEXT NOT NULL,
    claim_key TEXT NOT NULL,
    claim_record_id TEXT NOT NULL REFERENCES claims (record_id) ON DELETE RESTRICT,
    scope_role TEXT NOT NULL CHECK (scope_role IN ('core', 'context')),
    outcome TEXT NOT NULL CHECK (
        outcome IN ('supported', 'contradicted', 'mixed', 'unexamined', 'indeterminate')
    ),
    rationale TEXT,
    PRIMARY KEY (assessment_id, claim_record_id),
    UNIQUE (assessment_id, dataset_id, claim_record_id),
    FOREIGN KEY (assessment_id, dataset_id)
        REFERENCES research_assessments (assessment_id, dataset_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (dataset_id, claim_key, claim_record_id)
        REFERENCES dataset_record_mappings (dataset_id, record_key, record_id)
        ON DELETE RESTRICT
);

CREATE TRIGGER assessment_claims_require_manifest_key
BEFORE INSERT ON assessment_claims
WHEN NOT EXISTS (
    SELECT 1
    FROM research_assessments a
    JOIN dataset_imports i
      ON i.dataset_id = a.dataset_id
     AND i.dataset_version = a.dataset_version
     AND i.import_sequence = a.import_sequence
    JOIN json_each(i.record_keys_json) manifest_key
      ON manifest_key.value = NEW.claim_key
    WHERE a.assessment_id = NEW.assessment_id
      AND a.dataset_id = NEW.dataset_id
)
BEGIN
    SELECT RAISE(ABORT, 'assessment claim is not in the selected import manifest');
END;

CREATE TRIGGER assessment_claims_require_unsealed_assessment
BEFORE INSERT ON assessment_claims
WHEN NOT EXISTS (
    SELECT 1 FROM research_assessments
    WHERE assessment_id = NEW.assessment_id
      AND is_sealed = 0
)
BEGIN
    SELECT RAISE(ABORT, 'cannot add claim inputs to a sealed assessment');
END;

CREATE TABLE assessment_claim_evidence (
    assessment_evidence_id TEXT PRIMARY KEY NOT NULL,
    assessment_id TEXT NOT NULL,
    dataset_id TEXT NOT NULL,
    claim_record_id TEXT NOT NULL,
    evidence_key TEXT,
    evidence_record_id TEXT REFERENCES evidence_items (record_id) ON DELETE RESTRICT,
    state TEXT NOT NULL CHECK (
        state IN (
            'supports',
            'conflicts',
            'not_examined',
            'unavailable',
            'irrelevant_to_scope',
            'indeterminate'
        )
    ),
    note TEXT,
    CHECK (evidence_record_id IS NULL OR evidence_key IS NOT NULL),
    CHECK (
        evidence_record_id IS NOT NULL
        OR (
            state = 'unavailable'
            AND (
                length(trim(coalesce(note, ''))) > 0
                OR length(trim(coalesce(evidence_key, ''))) > 0
            )
        )
    ),
    FOREIGN KEY (assessment_id, dataset_id, claim_record_id)
        REFERENCES assessment_claims (assessment_id, dataset_id, claim_record_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (dataset_id, evidence_key, evidence_record_id)
        REFERENCES dataset_record_mappings (dataset_id, record_key, record_id)
        ON DELETE RESTRICT
);

CREATE UNIQUE INDEX assessment_claim_evidence_record_idx
    ON assessment_claim_evidence (assessment_id, claim_record_id, evidence_record_id)
    WHERE evidence_record_id IS NOT NULL;

CREATE TRIGGER assessment_evidence_requires_manifest_key
BEFORE INSERT ON assessment_claim_evidence
WHEN NEW.evidence_record_id IS NOT NULL
 AND NOT EXISTS (
    SELECT 1
    FROM research_assessments a
    JOIN dataset_imports i
      ON i.dataset_id = a.dataset_id
     AND i.dataset_version = a.dataset_version
     AND i.import_sequence = a.import_sequence
    JOIN json_each(i.record_keys_json) manifest_key
      ON manifest_key.value = NEW.evidence_key
    WHERE a.assessment_id = NEW.assessment_id
      AND a.dataset_id = NEW.dataset_id
)
BEGIN
    SELECT RAISE(ABORT, 'assessment evidence is not in the selected import manifest');
END;

CREATE TRIGGER assessment_evidence_requires_unsealed_assessment
BEFORE INSERT ON assessment_claim_evidence
WHEN NOT EXISTS (
    SELECT 1 FROM research_assessments
    WHERE assessment_id = NEW.assessment_id
      AND is_sealed = 0
)
BEGIN
    SELECT RAISE(ABORT, 'cannot add evidence inputs to a sealed assessment');
END;

CREATE TABLE assessment_source_dependencies (
    dependency_id TEXT PRIMARY KEY NOT NULL,
    assessment_id TEXT NOT NULL,
    dataset_id TEXT NOT NULL,
    source_a_key TEXT NOT NULL,
    source_a_id TEXT NOT NULL REFERENCES sources (record_id) ON DELETE RESTRICT,
    source_b_key TEXT NOT NULL,
    source_b_id TEXT NOT NULL REFERENCES sources (record_id) ON DELETE RESTRICT,
    state TEXT NOT NULL CHECK (
        state IN ('dependent', 'assessed_independent', 'unknown')
    ),
    rationale TEXT,
    basis_references_json TEXT NOT NULL DEFAULT '[]' CHECK (
        json_valid(basis_references_json) AND json_type(basis_references_json) = 'array'
    ),
    CHECK (source_a_id < source_b_id),
    UNIQUE (assessment_id, source_a_id, source_b_id),
    FOREIGN KEY (assessment_id, dataset_id)
        REFERENCES research_assessments (assessment_id, dataset_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (dataset_id, source_a_key, source_a_id)
        REFERENCES dataset_record_mappings (dataset_id, record_key, record_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (dataset_id, source_b_key, source_b_id)
        REFERENCES dataset_record_mappings (dataset_id, record_key, record_id)
        ON DELETE RESTRICT
);

CREATE TRIGGER assessment_source_dependencies_require_manifest_keys
BEFORE INSERT ON assessment_source_dependencies
WHEN NOT EXISTS (
    SELECT 1
    FROM research_assessments a
    JOIN dataset_imports i
      ON i.dataset_id = a.dataset_id
     AND i.dataset_version = a.dataset_version
     AND i.import_sequence = a.import_sequence
    WHERE a.assessment_id = NEW.assessment_id
      AND a.dataset_id = NEW.dataset_id
      AND EXISTS (
          SELECT 1 FROM json_each(i.record_keys_json)
          WHERE value = NEW.source_a_key
      )
      AND EXISTS (
          SELECT 1 FROM json_each(i.record_keys_json)
          WHERE value = NEW.source_b_key
      )
)
BEGIN
    SELECT RAISE(ABORT, 'assessment sources are not in the selected import manifest');
END;

CREATE TRIGGER assessment_source_dependencies_require_unsealed_assessment
BEFORE INSERT ON assessment_source_dependencies
WHEN NOT EXISTS (
    SELECT 1 FROM research_assessments
    WHERE assessment_id = NEW.assessment_id
      AND is_sealed = 0
)
BEGIN
    SELECT RAISE(ABORT, 'cannot add source dependencies to a sealed assessment');
END;

CREATE TABLE assessment_factor_ratings (
    assessment_id TEXT NOT NULL REFERENCES research_assessments (assessment_id) ON DELETE RESTRICT,
    factor_key TEXT NOT NULL CHECK (
        factor_key IN (
            'evidence_quality',
            'provenance',
            'independent_corroboration',
            'consistency'
        )
    ),
    rating_json TEXT CHECK (rating_json IS NULL OR json_valid(rating_json)),
    rationale TEXT,
    references_json TEXT NOT NULL DEFAULT '[]' CHECK (
        json_valid(references_json) AND json_type(references_json) = 'array'
    ),
    PRIMARY KEY (assessment_id, factor_key)
);

CREATE TRIGGER assessment_factor_ratings_require_unsealed_assessment
BEFORE INSERT ON assessment_factor_ratings
WHEN NOT EXISTS (
    SELECT 1 FROM research_assessments
    WHERE assessment_id = NEW.assessment_id
      AND is_sealed = 0
)
BEGIN
    SELECT RAISE(ABORT, 'cannot add factor ratings to a sealed assessment');
END;

CREATE TRIGGER research_assessments_no_delete
BEFORE DELETE ON research_assessments
BEGIN
    SELECT RAISE(ABORT, 'assessments are immutable');
END;

CREATE TRIGGER assessment_claims_no_update
BEFORE UPDATE ON assessment_claims
BEGIN
    SELECT RAISE(ABORT, 'assessment claim inputs are immutable');
END;

CREATE TRIGGER assessment_claims_no_delete
BEFORE DELETE ON assessment_claims
BEGIN
    SELECT RAISE(ABORT, 'assessment claim inputs are immutable');
END;

CREATE TRIGGER assessment_claim_evidence_no_update
BEFORE UPDATE ON assessment_claim_evidence
BEGIN
    SELECT RAISE(ABORT, 'assessment evidence inputs are immutable');
END;

CREATE TRIGGER assessment_claim_evidence_no_delete
BEFORE DELETE ON assessment_claim_evidence
BEGIN
    SELECT RAISE(ABORT, 'assessment evidence inputs are immutable');
END;

CREATE TRIGGER assessment_source_dependencies_no_update
BEFORE UPDATE ON assessment_source_dependencies
BEGIN
    SELECT RAISE(ABORT, 'assessment source dependencies are immutable');
END;

CREATE TRIGGER assessment_source_dependencies_no_delete
BEFORE DELETE ON assessment_source_dependencies
BEGIN
    SELECT RAISE(ABORT, 'assessment source dependencies are immutable');
END;

CREATE TRIGGER assessment_factor_ratings_no_update
BEFORE UPDATE ON assessment_factor_ratings
BEGIN
    SELECT RAISE(ABORT, 'assessment factor ratings are immutable');
END;

CREATE TRIGGER assessment_factor_ratings_no_delete
BEFORE DELETE ON assessment_factor_ratings
BEGIN
    SELECT RAISE(ABORT, 'assessment factor ratings are immutable');
END;
