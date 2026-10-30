CREATE TABLE records (
    record_id TEXT PRIMARY KEY NOT NULL,
    record_kind TEXT NOT NULL CHECK (
        record_kind IN (
            'incident',
            'source',
            'evidence_item',
            'claim',
            'relationship',
            'discrepancy',
            'finding',
            'history'
        )
    ),
    created_at TEXT NOT NULL,
    created_by_kind TEXT NOT NULL CHECK (created_by_kind IN ('researcher', 'system', 'ai')),
    created_by_name TEXT,
    UNIQUE (record_id, record_kind)
);

-- Incidents group reports alleged to concern one event; the grouping is not evidence that it occurred.
CREATE TABLE incidents (
    record_id TEXT PRIMARY KEY NOT NULL,
    record_kind TEXT NOT NULL DEFAULT 'incident' CHECK (record_kind = 'incident'),
    label TEXT NOT NULL,
    FOREIGN KEY (record_id, record_kind)
        REFERENCES records (record_id, record_kind)
        ON DELETE RESTRICT
);

CREATE TABLE sources (
    record_id TEXT PRIMARY KEY NOT NULL,
    record_kind TEXT NOT NULL DEFAULT 'source' CHECK (record_kind = 'source'),
    source_kind TEXT NOT NULL CHECK (
        source_kind IN (
            'web_page',
            'publication',
            'official_record',
            'dataset',
            'firsthand_account',
            'archive',
            'other'
        )
    ),
    title TEXT,
    publisher TEXT,
    author TEXT,
    publication_date TEXT,
    url TEXT,
    other_locator TEXT,
    -- Verifying a source establishes source provenance, not the truth of claims it contains.
    verification_status TEXT NOT NULL DEFAULT 'not_assessed' CHECK (
        verification_status IN ('not_assessed', 'unverified', 'verified', 'needs_review')
    ),
    FOREIGN KEY (record_id, record_kind)
        REFERENCES records (record_id, record_kind)
        ON DELETE RESTRICT
);

CREATE TABLE evidence_items (
    record_id TEXT PRIMARY KEY NOT NULL,
    record_kind TEXT NOT NULL DEFAULT 'evidence_item' CHECK (record_kind = 'evidence_item'),
    source_id TEXT NOT NULL REFERENCES sources (record_id) ON DELETE RESTRICT,
    locator_kind TEXT NOT NULL CHECK (
        locator_kind IN ('page', 'section', 'timecode', 'text_fragment', 'other')
    ),
    locator_json TEXT NOT NULL CHECK (json_valid(locator_json)),
    evidence_kind TEXT NOT NULL CHECK (
        evidence_kind IN ('document', 'image', 'video', 'audio', 'dataset', 'physical_item', 'other')
    ),
    description TEXT NOT NULL,
    FOREIGN KEY (record_id, record_kind)
        REFERENCES records (record_id, record_kind)
        ON DELETE RESTRICT
);

CREATE TABLE claims (
    record_id TEXT PRIMARY KEY NOT NULL,
    record_kind TEXT NOT NULL DEFAULT 'claim' CHECK (record_kind = 'claim'),
    assertion_text TEXT NOT NULL,
    category TEXT NOT NULL CHECK (
        category IN ('observation', 'identification', 'timing', 'location', 'context', 'other')
    ),
    source_id TEXT NOT NULL REFERENCES sources (record_id) ON DELETE RESTRICT,
    source_locator_kind TEXT NOT NULL CHECK (
        source_locator_kind IN ('page', 'section', 'timecode', 'text_fragment', 'other')
    ),
    source_locator_json TEXT NOT NULL CHECK (json_valid(source_locator_json)),
    incident_id TEXT REFERENCES incidents (record_id) ON DELETE RESTRICT,
    review_status TEXT NOT NULL DEFAULT 'unreviewed' CHECK (
        review_status IN ('unreviewed', 'in_review', 'reviewed', 'needs_revision')
    ),
    reviewed_at TEXT,
    reviewed_by_kind TEXT CHECK (reviewed_by_kind IN ('researcher', 'system', 'ai')),
    reviewed_by_name TEXT,
    CHECK (
        (reviewed_at IS NULL AND reviewed_by_kind IS NULL AND reviewed_by_name IS NULL)
        OR (reviewed_at IS NOT NULL AND reviewed_by_kind IS NOT NULL)
    ),
    FOREIGN KEY (record_id, record_kind)
        REFERENCES records (record_id, record_kind)
        ON DELETE RESTRICT
);

CREATE TABLE claim_author_confidence (
    claim_id TEXT PRIMARY KEY NOT NULL REFERENCES claims (record_id) ON DELETE RESTRICT,
    stated_by TEXT NOT NULL,
    value_as_stated TEXT NOT NULL,
    source_id TEXT NOT NULL REFERENCES sources (record_id) ON DELETE RESTRICT,
    locator_kind TEXT CHECK (
        locator_kind IS NULL OR locator_kind IN ('page', 'section', 'timecode', 'text_fragment', 'other')
    ),
    locator_json TEXT CHECK (locator_json IS NULL OR json_valid(locator_json)),
    CHECK ((locator_kind IS NULL) = (locator_json IS NULL))
);

CREATE TABLE relationships (
    record_id TEXT PRIMARY KEY NOT NULL,
    record_kind TEXT NOT NULL DEFAULT 'relationship' CHECK (record_kind = 'relationship'),
    relationship_kind TEXT NOT NULL CHECK (
        relationship_kind IN (
            'quotes',
            'repeats',
            'derived_from',
            'analyses',
            'supports',
            'contradicts',
            'corrects'
        )
    ),
    -- Direction: from is the record making the reference/action; to is the referenced record.
    -- For "corrects", the newer claim points to the earlier claim it corrects.
    from_record_id TEXT NOT NULL,
    from_record_kind TEXT NOT NULL CHECK (
        from_record_kind IN (
            'incident',
            'source',
            'evidence_item',
            'claim',
            'relationship',
            'discrepancy',
            'finding',
            'history'
        )
    ),
    to_record_id TEXT NOT NULL,
    to_record_kind TEXT NOT NULL CHECK (
        to_record_kind IN (
            'incident',
            'source',
            'evidence_item',
            'claim',
            'relationship',
            'discrepancy',
            'finding',
            'history'
        )
    ),
    CHECK (from_record_id <> to_record_id),
    FOREIGN KEY (record_id, record_kind)
        REFERENCES records (record_id, record_kind)
        ON DELETE RESTRICT,
    FOREIGN KEY (from_record_id, from_record_kind)
        REFERENCES records (record_id, record_kind)
        ON DELETE RESTRICT,
    FOREIGN KEY (to_record_id, to_record_kind)
        REFERENCES records (record_id, record_kind)
        ON DELETE RESTRICT
);

CREATE TABLE discrepancies (
    record_id TEXT PRIMARY KEY NOT NULL,
    record_kind TEXT NOT NULL DEFAULT 'discrepancy' CHECK (record_kind = 'discrepancy'),
    description TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'investigating', 'documented')),
    FOREIGN KEY (record_id, record_kind)
        REFERENCES records (record_id, record_kind)
        ON DELETE RESTRICT
);

CREATE TABLE discrepancy_claims (
    discrepancy_id TEXT NOT NULL REFERENCES discrepancies (record_id) ON DELETE RESTRICT,
    claim_id TEXT NOT NULL REFERENCES claims (record_id) ON DELETE RESTRICT,
    PRIMARY KEY (discrepancy_id, claim_id)
);

CREATE TABLE findings (
    record_id TEXT PRIMARY KEY NOT NULL,
    record_kind TEXT NOT NULL DEFAULT 'finding' CHECK (record_kind = 'finding'),
    assessment TEXT NOT NULL,
    attribution_kind TEXT NOT NULL CHECK (attribution_kind IN ('researcher', 'system', 'ai')),
    attribution_name TEXT,
    review_status TEXT NOT NULL DEFAULT 'unreviewed' CHECK (
        review_status IN ('unreviewed', 'in_review', 'reviewed', 'needs_revision')
    ),
    reviewed_at TEXT,
    reviewed_by_kind TEXT CHECK (reviewed_by_kind IN ('researcher', 'system', 'ai')),
    reviewed_by_name TEXT,
    CHECK (
        (reviewed_at IS NULL AND reviewed_by_kind IS NULL AND reviewed_by_name IS NULL)
        OR (reviewed_at IS NOT NULL AND reviewed_by_kind IS NOT NULL)
    ),
    FOREIGN KEY (record_id, record_kind)
        REFERENCES records (record_id, record_kind)
        ON DELETE RESTRICT
);

CREATE TABLE finding_claim_references (
    finding_id TEXT NOT NULL REFERENCES findings (record_id) ON DELETE RESTRICT,
    claim_id TEXT NOT NULL REFERENCES claims (record_id) ON DELETE RESTRICT,
    reference_role TEXT NOT NULL CHECK (reference_role IN ('supporting', 'conflicting')),
    PRIMARY KEY (finding_id, claim_id, reference_role)
);

CREATE TABLE finding_evidence_references (
    finding_id TEXT NOT NULL REFERENCES findings (record_id) ON DELETE RESTRICT,
    evidence_item_id TEXT NOT NULL REFERENCES evidence_items (record_id) ON DELETE RESTRICT,
    reference_role TEXT NOT NULL CHECK (reference_role IN ('supporting', 'conflicting')),
    PRIMARY KEY (finding_id, evidence_item_id, reference_role)
);

CREATE TABLE record_history (
    record_id TEXT PRIMARY KEY NOT NULL,
    record_kind TEXT NOT NULL DEFAULT 'history' CHECK (record_kind = 'history'),
    subject_record_id TEXT NOT NULL,
    subject_record_kind TEXT NOT NULL CHECK (
        subject_record_kind IN (
            'incident',
            'source',
            'evidence_item',
            'claim',
            'relationship',
            'discrepancy',
            'finding'
        )
    ),
    event_kind TEXT NOT NULL CHECK (
        event_kind IN (
            'record_created',
            'field_changed',
            'correction_linked',
            'review_status_changed',
            'source_verification_changed',
            'relationship_added'
        )
    ),
    summary TEXT NOT NULL,
    changes_json TEXT CHECK (changes_json IS NULL OR json_valid(changes_json)),
    FOREIGN KEY (record_id, record_kind)
        REFERENCES records (record_id, record_kind)
        ON DELETE RESTRICT,
    FOREIGN KEY (subject_record_id, subject_record_kind)
        REFERENCES records (record_id, record_kind)
        ON DELETE RESTRICT
);

CREATE TABLE history_related_records (
    history_id TEXT NOT NULL REFERENCES record_history (record_id) ON DELETE RESTRICT,
    related_record_id TEXT NOT NULL,
    related_record_kind TEXT NOT NULL CHECK (
        related_record_kind IN (
            'incident',
            'source',
            'evidence_item',
            'claim',
            'relationship',
            'discrepancy',
            'finding'
        )
    ),
    PRIMARY KEY (history_id, related_record_id, related_record_kind),
    FOREIGN KEY (related_record_id, related_record_kind)
        REFERENCES records (record_id, record_kind)
        ON DELETE RESTRICT
);

CREATE TRIGGER record_history_no_update
BEFORE UPDATE ON record_history
BEGIN
    SELECT RAISE(ABORT, 'record history is append-only');
END;

CREATE TRIGGER record_history_no_delete
BEFORE DELETE ON record_history
BEGIN
    SELECT RAISE(ABORT, 'record history is append-only');
END;

CREATE TRIGGER history_record_identity_no_update
BEFORE UPDATE ON records
WHEN OLD.record_kind = 'history'
BEGIN
    SELECT RAISE(ABORT, 'record history is append-only');
END;

CREATE TRIGGER history_record_identity_no_delete
BEFORE DELETE ON records
WHEN OLD.record_kind = 'history'
BEGIN
    SELECT RAISE(ABORT, 'record history is append-only');
END;

CREATE TRIGGER history_related_records_no_update
BEFORE UPDATE ON history_related_records
BEGIN
    SELECT RAISE(ABORT, 'record history is append-only');
END;

CREATE TRIGGER history_related_records_no_delete
BEFORE DELETE ON history_related_records
BEGIN
    SELECT RAISE(ABORT, 'record history is append-only');
END;
