CREATE TABLE research_datasets (
    dataset_id TEXT PRIMARY KEY NOT NULL
);

CREATE TABLE dataset_imports (
    dataset_id TEXT NOT NULL,
    dataset_version TEXT NOT NULL,
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
    FOREIGN KEY (dataset_id)
        REFERENCES research_datasets (dataset_id)
        ON DELETE RESTRICT
);

CREATE TABLE dataset_record_mappings (
    dataset_id TEXT NOT NULL,
    record_key TEXT NOT NULL,
    record_kind TEXT NOT NULL CHECK (
        record_kind IN (
            'incident',
            'source',
            'evidence_item',
            'claim',
            'relationship',
            'discrepancy',
            'finding'
        )
    ),
    record_id TEXT NOT NULL,
    content_snapshot_json TEXT NOT NULL CHECK (
        json_valid(content_snapshot_json) AND json_type(content_snapshot_json) = 'object'
    ),
    PRIMARY KEY (dataset_id, record_key),
    UNIQUE (dataset_id, record_id),
    FOREIGN KEY (dataset_id)
        REFERENCES research_datasets (dataset_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (record_id, record_kind)
        REFERENCES records (record_id, record_kind)
        ON DELETE RESTRICT
);

CREATE INDEX dataset_imports_imported_at_idx
    ON dataset_imports (imported_at);

CREATE INDEX dataset_record_mappings_record_id_idx
    ON dataset_record_mappings (record_id);
