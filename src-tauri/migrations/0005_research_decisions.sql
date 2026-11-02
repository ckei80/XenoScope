CREATE TABLE research_suggestions (
    suggestion_id TEXT PRIMARY KEY NOT NULL,
    decision_kind TEXT NOT NULL CHECK (
        decision_kind IN ('incident_pair', 'source_dependency', 'source_lineage')
    ),
    record_a_id TEXT NOT NULL,
    record_a_kind TEXT NOT NULL,
    record_b_id TEXT NOT NULL,
    record_b_kind TEXT NOT NULL,
    lineage_type TEXT CHECK (
        lineage_type IS NULL OR lineage_type IN (
            'copied_from',
            'derived_from',
            'quotes',
            'repeats',
            'references'
        )
    ),
    proposed_outcome TEXT CHECK (
        proposed_outcome IS NULL OR proposed_outcome IN (
            'same_event',
            'related_distinct',
            'not_same_event',
            'dependent',
            'assessed_independent',
            'unknown',
            'copied_from',
            'derived_from',
            'quotes',
            'repeats',
            'references'
        )
    ),
    origin_kind TEXT NOT NULL CHECK (origin_kind IN ('researcher', 'system', 'ai')),
    origin_name TEXT,
    generator_version TEXT,
    created_at TEXT NOT NULL,
    summary TEXT NOT NULL CHECK (length(trim(summary)) > 0),
    initial_event_id TEXT NOT NULL UNIQUE,
    CHECK (record_a_id <> record_b_id),
    CHECK (
        (decision_kind = 'incident_pair'
         AND record_a_kind = 'incident' AND record_b_kind = 'incident'
         AND record_a_id < record_b_id AND lineage_type IS NULL
         AND (proposed_outcome IS NULL OR proposed_outcome IN (
             'same_event', 'related_distinct', 'not_same_event'
         )))
        OR
        (decision_kind = 'source_dependency'
         AND record_a_kind = 'source' AND record_b_kind = 'source'
         AND record_a_id < record_b_id AND lineage_type IS NULL
         AND (proposed_outcome IS NULL OR proposed_outcome IN (
             'dependent', 'assessed_independent', 'unknown'
         )))
        OR
        (decision_kind = 'source_lineage'
         AND record_a_kind = 'source' AND record_b_kind = 'source'
         AND lineage_type IS NOT NULL
         AND (proposed_outcome IS NULL OR proposed_outcome = lineage_type))
    ),
    FOREIGN KEY (record_a_id, record_a_kind)
        REFERENCES records (record_id, record_kind) ON DELETE RESTRICT,
    FOREIGN KEY (record_b_id, record_b_kind)
        REFERENCES records (record_id, record_kind) ON DELETE RESTRICT,
    FOREIGN KEY (initial_event_id, suggestion_id)
        REFERENCES suggestion_events (event_id, suggestion_id)
        DEFERRABLE INITIALLY DEFERRED
);

CREATE TABLE suggestion_references (
    reference_id TEXT PRIMARY KEY NOT NULL,
    suggestion_id TEXT NOT NULL REFERENCES research_suggestions (suggestion_id)
        ON DELETE RESTRICT,
    record_id TEXT NOT NULL,
    record_kind TEXT NOT NULL CHECK (
        record_kind IN ('incident', 'source', 'evidence_item', 'claim', 'relationship',
                        'discrepancy', 'finding')
    ),
    reference_role TEXT NOT NULL CHECK (length(trim(reference_role)) > 0),
    locator_json TEXT CHECK (locator_json IS NULL OR json_valid(locator_json)),
    note TEXT,
    FOREIGN KEY (record_id, record_kind)
        REFERENCES records (record_id, record_kind) ON DELETE RESTRICT
);

CREATE INDEX suggestion_references_suggestion_idx
    ON suggestion_references (suggestion_id);

CREATE INDEX suggestion_references_record_idx
    ON suggestion_references (record_id, record_kind);

CREATE TABLE research_decisions (
    decision_id TEXT PRIMARY KEY NOT NULL,
    decision_kind TEXT NOT NULL CHECK (
        decision_kind IN ('incident_pair', 'source_dependency', 'source_lineage')
    ),
    record_a_id TEXT NOT NULL,
    record_a_kind TEXT NOT NULL,
    record_b_id TEXT NOT NULL,
    record_b_kind TEXT NOT NULL,
    lineage_type TEXT CHECK (
        lineage_type IS NULL OR lineage_type IN (
            'copied_from',
            'derived_from',
            'quotes',
            'repeats',
            'references'
        )
    ),
    origin_suggestion_id TEXT UNIQUE,
    origin_suggestion_event_id TEXT UNIQUE,
    origin_suggestion_status TEXT CHECK (
        origin_suggestion_status IS NULL OR origin_suggestion_status = 'resolved'
    ),
    created_at TEXT NOT NULL,
    initial_event_id TEXT NOT NULL UNIQUE,
    CHECK (record_a_id <> record_b_id),
    CHECK (
        (origin_suggestion_id IS NULL
         AND origin_suggestion_event_id IS NULL
         AND origin_suggestion_status IS NULL)
        OR
        (origin_suggestion_id IS NOT NULL
         AND origin_suggestion_event_id IS NOT NULL
         AND origin_suggestion_status = 'resolved')
    ),
    CHECK (
        (decision_kind = 'incident_pair'
         AND record_a_kind = 'incident' AND record_b_kind = 'incident'
         AND record_a_id < record_b_id AND lineage_type IS NULL)
        OR
        (decision_kind = 'source_dependency'
         AND record_a_kind = 'source' AND record_b_kind = 'source'
         AND record_a_id < record_b_id AND lineage_type IS NULL)
        OR
        (decision_kind = 'source_lineage'
         AND record_a_kind = 'source' AND record_b_kind = 'source'
         AND lineage_type IS NOT NULL)
    ),
    FOREIGN KEY (record_a_id, record_a_kind)
        REFERENCES records (record_id, record_kind) ON DELETE RESTRICT,
    FOREIGN KEY (record_b_id, record_b_kind)
        REFERENCES records (record_id, record_kind) ON DELETE RESTRICT,
    FOREIGN KEY (origin_suggestion_id)
        REFERENCES research_suggestions (suggestion_id) ON DELETE RESTRICT,
    FOREIGN KEY (
        origin_suggestion_event_id, origin_suggestion_id, origin_suggestion_status
    )
        REFERENCES suggestion_events (event_id, suggestion_id, status)
        DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (initial_event_id, decision_id)
        REFERENCES decision_events (event_id, decision_id)
        DEFERRABLE INITIALLY DEFERRED
);

CREATE UNIQUE INDEX research_decisions_incident_pair_identity_idx
    ON research_decisions (record_a_id, record_b_id)
    WHERE decision_kind = 'incident_pair';

CREATE UNIQUE INDEX research_decisions_source_dependency_identity_idx
    ON research_decisions (record_a_id, record_b_id)
    WHERE decision_kind = 'source_dependency';

CREATE UNIQUE INDEX research_decisions_source_lineage_identity_idx
    ON research_decisions (record_a_id, record_b_id, lineage_type)
    WHERE decision_kind = 'source_lineage';

CREATE TABLE decision_events (
    event_id TEXT PRIMARY KEY NOT NULL,
    decision_id TEXT NOT NULL REFERENCES research_decisions (decision_id)
        ON DELETE RESTRICT,
    event_kind TEXT NOT NULL CHECK (event_kind IN ('recorded', 'superseded', 'reversed')),
    outcome TEXT CHECK (
        outcome IS NULL OR outcome IN (
            'same_event',
            'related_distinct',
            'not_same_event',
            'dependent',
            'assessed_independent',
            'unknown',
            'copied_from',
            'derived_from',
            'quotes',
            'repeats',
            'references'
        )
    ),
    predecessor_event_id TEXT,
    occurred_at TEXT NOT NULL,
    actor_kind TEXT NOT NULL CHECK (actor_kind = 'researcher'),
    actor_name TEXT,
    rationale TEXT NOT NULL CHECK (length(trim(rationale)) > 0),
    UNIQUE (event_id, decision_id),
    CHECK (
        (event_kind = 'reversed' AND outcome IS NULL)
        OR (event_kind IN ('recorded', 'superseded') AND outcome IS NOT NULL)
    ),
    FOREIGN KEY (predecessor_event_id, decision_id)
        REFERENCES decision_events (event_id, decision_id) ON DELETE RESTRICT
);

CREATE UNIQUE INDEX decision_events_one_root_idx
    ON decision_events (decision_id)
    WHERE predecessor_event_id IS NULL;

CREATE UNIQUE INDEX decision_events_one_successor_idx
    ON decision_events (predecessor_event_id)
    WHERE predecessor_event_id IS NOT NULL;

CREATE TABLE decision_event_references (
    reference_id TEXT PRIMARY KEY NOT NULL,
    event_id TEXT NOT NULL REFERENCES decision_events (event_id) ON DELETE RESTRICT,
    record_id TEXT NOT NULL,
    record_kind TEXT NOT NULL CHECK (
        record_kind IN ('incident', 'source', 'evidence_item', 'claim', 'relationship',
                        'discrepancy', 'finding')
    ),
    reference_role TEXT NOT NULL CHECK (length(trim(reference_role)) > 0),
    locator_json TEXT CHECK (locator_json IS NULL OR json_valid(locator_json)),
    note TEXT,
    FOREIGN KEY (record_id, record_kind)
        REFERENCES records (record_id, record_kind) ON DELETE RESTRICT
);

CREATE INDEX decision_event_references_event_idx
    ON decision_event_references (event_id);

CREATE INDEX decision_event_references_record_idx
    ON decision_event_references (record_id, record_kind);

CREATE TABLE suggestion_events (
    event_id TEXT PRIMARY KEY NOT NULL,
    suggestion_id TEXT NOT NULL REFERENCES research_suggestions (suggestion_id)
        ON DELETE RESTRICT,
    status TEXT NOT NULL CHECK (
        status IN ('proposed', 'dismissed', 'superseded', 'resolved')
    ),
    predecessor_event_id TEXT,
    occurred_at TEXT NOT NULL,
    actor_kind TEXT NOT NULL CHECK (actor_kind IN ('researcher', 'system', 'ai')),
    actor_name TEXT,
    rationale TEXT,
    replacement_suggestion_id TEXT REFERENCES research_suggestions (suggestion_id)
        ON DELETE RESTRICT,
    decision_id TEXT REFERENCES research_decisions (decision_id)
        DEFERRABLE INITIALLY DEFERRED,
    UNIQUE (event_id, suggestion_id),
    UNIQUE (event_id, suggestion_id, status),
    CHECK (
        (status = 'proposed'
         AND predecessor_event_id IS NULL
         AND rationale IS NULL
         AND replacement_suggestion_id IS NULL
         AND decision_id IS NULL)
        OR
        (status = 'dismissed'
         AND predecessor_event_id IS NOT NULL
         AND rationale IS NOT NULL
         AND length(trim(rationale)) > 0
         AND replacement_suggestion_id IS NULL
         AND decision_id IS NULL)
        OR
        (status = 'superseded'
         AND predecessor_event_id IS NOT NULL
         AND rationale IS NOT NULL
         AND length(trim(rationale)) > 0
         AND replacement_suggestion_id IS NOT NULL
         AND decision_id IS NULL)
        OR
        (status = 'resolved'
         AND predecessor_event_id IS NOT NULL
         AND replacement_suggestion_id IS NULL
         AND decision_id IS NOT NULL)
    ),
    FOREIGN KEY (predecessor_event_id, suggestion_id)
        REFERENCES suggestion_events (event_id, suggestion_id) ON DELETE RESTRICT
);

CREATE UNIQUE INDEX suggestion_events_one_root_idx
    ON suggestion_events (suggestion_id)
    WHERE predecessor_event_id IS NULL;

CREATE UNIQUE INDEX suggestion_events_one_successor_idx
    ON suggestion_events (predecessor_event_id)
    WHERE predecessor_event_id IS NOT NULL;

CREATE TRIGGER research_suggestions_no_update
BEFORE UPDATE ON research_suggestions
BEGIN
    SELECT RAISE(ABORT, 'research suggestions are append-only');
END;

CREATE TRIGGER research_suggestions_no_delete
BEFORE DELETE ON research_suggestions
BEGIN
    SELECT RAISE(ABORT, 'research suggestions are append-only');
END;

CREATE TRIGGER suggestion_references_no_update
BEFORE UPDATE ON suggestion_references
BEGIN
    SELECT RAISE(ABORT, 'suggestion references are append-only');
END;

CREATE TRIGGER suggestion_references_no_delete
BEFORE DELETE ON suggestion_references
BEGIN
    SELECT RAISE(ABORT, 'suggestion references are append-only');
END;

CREATE TRIGGER suggestion_events_no_update
BEFORE UPDATE ON suggestion_events
BEGIN
    SELECT RAISE(ABORT, 'suggestion events are append-only');
END;

CREATE TRIGGER suggestion_events_no_delete
BEFORE DELETE ON suggestion_events
BEGIN
    SELECT RAISE(ABORT, 'suggestion events are append-only');
END;

CREATE TRIGGER research_decisions_no_update
BEFORE UPDATE ON research_decisions
BEGIN
    SELECT RAISE(ABORT, 'research decisions are append-only');
END;

CREATE TRIGGER research_decisions_no_delete
BEFORE DELETE ON research_decisions
BEGIN
    SELECT RAISE(ABORT, 'research decisions are append-only');
END;

CREATE TRIGGER decision_events_no_update
BEFORE UPDATE ON decision_events
BEGIN
    SELECT RAISE(ABORT, 'decision events are append-only');
END;

CREATE TRIGGER decision_events_no_delete
BEFORE DELETE ON decision_events
BEGIN
    SELECT RAISE(ABORT, 'decision events are append-only');
END;

CREATE TRIGGER decision_event_references_no_update
BEFORE UPDATE ON decision_event_references
BEGIN
    SELECT RAISE(ABORT, 'decision event references are append-only');
END;

CREATE TRIGGER decision_event_references_no_delete
BEFORE DELETE ON decision_event_references
BEGIN
    SELECT RAISE(ABORT, 'decision event references are append-only');
END;

CREATE TRIGGER suggestion_events_validate_insert
BEFORE INSERT ON suggestion_events
BEGIN
    SELECT CASE
        WHEN NEW.predecessor_event_id IS NULL AND (
            NEW.status <> 'proposed'
            OR NEW.event_id <> (
                SELECT initial_event_id FROM research_suggestions
                WHERE suggestion_id = NEW.suggestion_id
            )
            OR EXISTS (
                SELECT 1 FROM suggestion_events
                WHERE suggestion_id = NEW.suggestion_id
            )
        ) THEN RAISE(ABORT, 'suggestion must start with its proposed event')
        WHEN NEW.predecessor_event_id IS NOT NULL AND (
            NEW.status = 'proposed'
            OR NOT EXISTS (
                SELECT 1 FROM suggestion_events previous
                WHERE previous.event_id = NEW.predecessor_event_id
                  AND previous.suggestion_id = NEW.suggestion_id
                  AND previous.status = 'proposed'
                  AND NOT EXISTS (
                      SELECT 1 FROM suggestion_events successor
                      WHERE successor.predecessor_event_id = previous.event_id
                  )
            )
        ) THEN RAISE(ABORT, 'suggestion transition requires its proposed current head')
        WHEN NEW.status = 'superseded' AND NOT EXISTS (
            SELECT 1
            FROM research_suggestions current
            JOIN research_suggestions replacement
              ON replacement.suggestion_id = NEW.replacement_suggestion_id
            WHERE current.suggestion_id = NEW.suggestion_id
              AND replacement.suggestion_id <> current.suggestion_id
              AND replacement.decision_kind = current.decision_kind
              AND replacement.record_a_id = current.record_a_id
              AND replacement.record_a_kind = current.record_a_kind
              AND replacement.record_b_id = current.record_b_id
              AND replacement.record_b_kind = current.record_b_kind
              AND replacement.lineage_type IS current.lineage_type
        ) THEN RAISE(ABORT, 'replacement suggestion must address the same question')
        WHEN NEW.status = 'resolved' AND NOT EXISTS (
            SELECT 1
            FROM research_decisions decision
            WHERE decision.decision_id = NEW.decision_id
              AND decision.origin_suggestion_id = NEW.suggestion_id
              AND decision.origin_suggestion_event_id = NEW.event_id
        ) THEN RAISE(ABORT, 'resolved suggestion must match its originating decision')
    END;
END;

CREATE TRIGGER research_decisions_validate_origin_insert
BEFORE INSERT ON research_decisions
WHEN NEW.origin_suggestion_id IS NOT NULL
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1
        FROM research_suggestions suggestion
        JOIN suggestion_events current
          ON current.suggestion_id = suggestion.suggestion_id
         AND current.status = 'proposed'
         AND current.predecessor_event_id IS NULL
        WHERE suggestion.suggestion_id = NEW.origin_suggestion_id
          AND suggestion.decision_kind = NEW.decision_kind
          AND suggestion.record_a_id = NEW.record_a_id
          AND suggestion.record_a_kind = NEW.record_a_kind
          AND suggestion.record_b_id = NEW.record_b_id
          AND suggestion.record_b_kind = NEW.record_b_kind
          AND suggestion.lineage_type IS NEW.lineage_type
          AND NOT EXISTS (
              SELECT 1 FROM suggestion_events successor
              WHERE successor.predecessor_event_id = current.event_id
          )
    ) THEN RAISE(ABORT, 'originating suggestion must be proposed and match the decision')
    END;
END;

CREATE TRIGGER decision_events_validate_insert
BEFORE INSERT ON decision_events
BEGIN
    SELECT CASE
        WHEN NEW.predecessor_event_id IS NULL AND (
            NEW.event_kind <> 'recorded'
            OR NEW.event_id <> (
                SELECT initial_event_id FROM research_decisions
                WHERE decision_id = NEW.decision_id
            )
            OR EXISTS (
                SELECT 1 FROM decision_events
                WHERE decision_id = NEW.decision_id
            )
        ) THEN RAISE(ABORT, 'decision thread must start with its recorded event')
        WHEN NEW.predecessor_event_id IS NOT NULL AND NOT EXISTS (
            SELECT 1
            FROM decision_events previous
            WHERE previous.event_id = NEW.predecessor_event_id
              AND previous.decision_id = NEW.decision_id
              AND NOT EXISTS (
                  SELECT 1 FROM decision_events successor
                  WHERE successor.predecessor_event_id = previous.event_id
              )
              AND (
                  (previous.event_kind = 'reversed' AND NEW.event_kind = 'recorded')
                  OR
                  (previous.event_kind IN ('recorded', 'superseded')
                   AND NEW.event_kind IN ('superseded', 'reversed'))
              )
        ) THEN RAISE(ABORT, 'decision transition requires a valid current head')
        WHEN NEW.outcome IS NOT NULL AND NOT EXISTS (
            SELECT 1
            FROM research_decisions decision
            WHERE decision.decision_id = NEW.decision_id
              AND (
                  (decision.decision_kind = 'incident_pair'
                   AND NEW.outcome IN ('same_event', 'related_distinct', 'not_same_event'))
                  OR
                  (decision.decision_kind = 'source_dependency'
                   AND NEW.outcome IN ('dependent', 'assessed_independent', 'unknown'))
                  OR
                  (decision.decision_kind = 'source_lineage'
                   AND NEW.outcome = decision.lineage_type)
              )
        ) THEN RAISE(ABORT, 'decision outcome is incompatible with its thread')
        WHEN NEW.actor_kind <> 'researcher'
          OR length(trim(NEW.rationale)) = 0
          THEN RAISE(ABORT, 'reviewed decisions require a researcher and rationale')
    END;
END;

CREATE TRIGGER suggestion_events_validate_resolved_decision
BEFORE INSERT ON suggestion_events
WHEN NEW.status = 'resolved'
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1
        FROM decision_events initial
        JOIN research_decisions decision ON decision.decision_id = initial.decision_id
        WHERE decision.decision_id = NEW.decision_id
          AND decision.origin_suggestion_id = NEW.suggestion_id
          AND decision.origin_suggestion_event_id = NEW.event_id
          AND initial.event_id = decision.initial_event_id
          AND initial.event_kind = 'recorded'
          AND initial.predecessor_event_id IS NULL
    ) THEN RAISE(ABORT, 'resolved suggestion requires its originating decision event')
    END;
END;
