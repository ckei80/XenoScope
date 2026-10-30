-- Historical order is reconstructed by imported_at, then dataset_version for ties.
-- This deterministic order cannot recover the true insertion order of prior imports.
CREATE TABLE dataset_imports_with_sequence (
    dataset_id TEXT NOT NULL,
    dataset_version TEXT NOT NULL,
    import_sequence INTEGER NOT NULL CHECK (import_sequence > 0),
    format TEXT NOT NULL CHECK (format = 'xenoscope-research-dataset'),
    format_version INTEGER NOT NULL CHECK (format_version > 0),
    imported_at TEXT NOT NULL,
    metadata_json TEXT NOT NULL CHECK (
        json_valid(metadata_json) AND json_type(metadata_json) = 'object'
    ),
    record_keys_json TEXT NOT NULL CHECK (
        json_valid(record_keys_json) AND json_type(record_keys_json) = 'array'
    ),
    PRIMARY KEY (dataset_id, dataset_version),
    UNIQUE (dataset_id, import_sequence),
    FOREIGN KEY (dataset_id)
        REFERENCES research_datasets (dataset_id)
        ON DELETE RESTRICT
);

INSERT INTO dataset_imports_with_sequence (
    dataset_id,
    dataset_version,
    import_sequence,
    format,
    format_version,
    imported_at,
    metadata_json,
    record_keys_json
)
SELECT
    dataset_id,
    dataset_version,
    ROW_NUMBER() OVER (
        PARTITION BY dataset_id
        ORDER BY imported_at, dataset_version
    ),
    format,
    format_version,
    imported_at,
    metadata_json,
    record_keys_json
FROM dataset_imports;

DROP TABLE dataset_imports;
ALTER TABLE dataset_imports_with_sequence RENAME TO dataset_imports;

CREATE INDEX dataset_imports_imported_at_idx
    ON dataset_imports (imported_at);
