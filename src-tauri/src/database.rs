use crate::research_records::{
    ActorKind, AuthorStatedConfidence, Claim, ClaimReviewStatus, CreateClaimInput,
    CreateSourceInput, RecordActor, ReviewMetadata, Source, SourceLocator,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs,
    path::Path,
    sync::{Mutex, MutexGuard},
};
use uuid::Uuid;

const LATEST_SCHEMA_VERSION: i64 = 4;
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../migrations/0001_initial.sql")),
    (2, include_str!("../migrations/0002_dataset_identity.sql")),
    (
        3,
        include_str!("../migrations/0003_dataset_import_sequence.sql"),
    ),
    (
        4,
        include_str!("../migrations/0004_research_assessments.sql"),
    ),
];

pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Could not create database directory {}: {error}",
                    parent.display()
                )
            })?;
        }

        let mut connection = Connection::open(path)
            .map_err(|error| format!("Could not open database {}: {error}", path.display()))?;
        initialize(&mut connection)?;

        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn connection(&self) -> Result<MutexGuard<'_, Connection>, String> {
        self.connection
            .lock()
            .map_err(|_| "Database connection lock was poisoned".to_string())
    }

    #[cfg(test)]
    pub(crate) fn in_memory() -> Result<Self, String> {
        let mut connection = Connection::open_in_memory()
            .map_err(|error| format!("Could not open test database: {error}"))?;
        initialize(&mut connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn create_source(&self, input: CreateSourceInput) -> Result<Source, String> {
        input.validate()?;

        let id = Uuid::new_v4().to_string();
        let created_by = researcher_actor();
        let verification_status = input.verification_status.clone();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| format!("Could not begin source creation: {error}"))?;
        let created_at = current_timestamp(&transaction)?;

        insert_record(&transaction, &id, "source", &created_at, &created_by)?;
        transaction
            .execute(
                "INSERT INTO sources (
                    record_id, record_kind, source_kind, title, publisher, author,
                    publication_date, url, other_locator, verification_status
                 ) VALUES (?1, 'source', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    id,
                    enum_value(&input.source_kind)?,
                    input.title,
                    input.publisher,
                    input.author,
                    input.publication_date,
                    input.url,
                    input.other_locator,
                    enum_value(&verification_status)?,
                ],
            )
            .map_err(|error| format!("Could not save source record: {error}"))?;
        insert_creation_history(&transaction, &id, "source", "Source record created")?;
        transaction
            .commit()
            .map_err(|error| format!("Could not commit source record: {error}"))?;

        Ok(Source {
            id,
            created_at,
            created_by,
            source_kind: input.source_kind,
            title: input.title,
            publisher: input.publisher,
            author: input.author,
            publication_date: input.publication_date,
            url: input.url,
            other_locator: input.other_locator,
            verification_status,
        })
    }

    pub fn get_source(&self, id: &str) -> Result<Option<Source>, String> {
        let id = canonical_uuid("Source ID", id)?;
        let connection = self.connection()?;
        let raw_source = connection
            .query_row(
                "SELECT
                    r.record_id, r.created_at, r.created_by_kind, r.created_by_name,
                    s.source_kind, s.title, s.publisher, s.author, s.publication_date,
                    s.url, s.other_locator, s.verification_status
                 FROM records r
                 JOIN sources s ON s.record_id = r.record_id
                 WHERE r.record_id = ?1 AND r.record_kind = 'source'",
                [&id],
                |row| {
                    Ok(RawSource {
                        id: row.get(0)?,
                        created_at: row.get(1)?,
                        created_by_kind: row.get(2)?,
                        created_by_name: row.get(3)?,
                        source_kind: row.get(4)?,
                        title: row.get(5)?,
                        publisher: row.get(6)?,
                        author: row.get(7)?,
                        publication_date: row.get(8)?,
                        url: row.get(9)?,
                        other_locator: row.get(10)?,
                        verification_status: row.get(11)?,
                    })
                },
            )
            .optional()
            .map_err(|error| format!("Could not retrieve source {id}: {error}"))?;

        raw_source.map(RawSource::into_source).transpose()
    }

    pub fn create_claim(&self, mut input: CreateClaimInput) -> Result<Claim, String> {
        input.validate()?;
        input.source_id = canonical_uuid("Source ID", &input.source_id)?;
        if let Some(incident_id) = &mut input.incident_id {
            *incident_id = canonical_uuid("Incident ID", incident_id)?;
        }
        if let Some(confidence) = &mut input.author_stated_confidence {
            confidence.source_id = canonical_uuid("Confidence source ID", &confidence.source_id)?;
        }

        let id = Uuid::new_v4().to_string();
        let created_by = researcher_actor();
        let review = ReviewMetadata {
            status: ClaimReviewStatus::Unreviewed,
            reviewed_at: None,
            reviewed_by: None,
        };
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| format!("Could not begin claim creation: {error}"))?;

        require_existing_source(&transaction, &input.source_id)?;
        if let Some(incident_id) = &input.incident_id {
            require_existing_incident(&transaction, incident_id)?;
        }
        if let Some(confidence) = &input.author_stated_confidence {
            require_existing_source(&transaction, &confidence.source_id)?;
        }

        let created_at = current_timestamp(&transaction)?;
        insert_record(&transaction, &id, "claim", &created_at, &created_by)?;
        transaction
            .execute(
                "INSERT INTO claims (
                    record_id, record_kind, assertion_text, category, source_id,
                    source_locator_kind, source_locator_json, incident_id, review_status
                 ) VALUES (?1, 'claim', ?2, ?3, ?4, ?5, ?6, ?7, 'unreviewed')",
                params![
                    id,
                    input.assertion_text,
                    enum_value(&input.category)?,
                    input.source_id,
                    input.source_locator.kind(),
                    serde_json::to_string(&input.source_locator)
                        .map_err(|error| format!("Could not encode claim locator: {error}"))?,
                    input.incident_id,
                ],
            )
            .map_err(|error| format!("Could not save claim record: {error}"))?;

        if let Some(confidence) = &input.author_stated_confidence {
            insert_author_confidence(&transaction, &id, confidence)?;
        }

        insert_creation_history(&transaction, &id, "claim", "Claim record created")?;
        transaction
            .commit()
            .map_err(|error| format!("Could not commit claim record: {error}"))?;

        Ok(Claim {
            id,
            created_at,
            created_by,
            assertion_text: input.assertion_text,
            category: input.category,
            source_id: input.source_id,
            source_locator: input.source_locator,
            incident_id: input.incident_id,
            author_stated_confidence: input.author_stated_confidence,
            review,
        })
    }

    pub fn get_claim(&self, id: &str) -> Result<Option<Claim>, String> {
        let id = canonical_uuid("Claim ID", id)?;
        let connection = self.connection()?;
        let raw_claim = connection
            .query_row(
                "SELECT
                    r.record_id, r.created_at, r.created_by_kind, r.created_by_name,
                    c.assertion_text, c.category, c.source_id, c.source_locator_kind,
                    c.source_locator_json, c.incident_id, c.review_status, c.reviewed_at,
                    c.reviewed_by_kind, c.reviewed_by_name, ac.stated_by,
                    ac.value_as_stated, ac.source_id, ac.locator_kind, ac.locator_json
                 FROM records r
                 JOIN claims c ON c.record_id = r.record_id
                 LEFT JOIN claim_author_confidence ac ON ac.claim_id = c.record_id
                 WHERE r.record_id = ?1 AND r.record_kind = 'claim'",
                [&id],
                |row| {
                    Ok(RawClaim {
                        id: row.get(0)?,
                        created_at: row.get(1)?,
                        created_by_kind: row.get(2)?,
                        created_by_name: row.get(3)?,
                        assertion_text: row.get(4)?,
                        category: row.get(5)?,
                        source_id: row.get(6)?,
                        source_locator_kind: row.get(7)?,
                        source_locator_json: row.get(8)?,
                        incident_id: row.get(9)?,
                        review_status: row.get(10)?,
                        reviewed_at: row.get(11)?,
                        reviewed_by_kind: row.get(12)?,
                        reviewed_by_name: row.get(13)?,
                        confidence_stated_by: row.get(14)?,
                        confidence_value: row.get(15)?,
                        confidence_source_id: row.get(16)?,
                        confidence_locator_kind: row.get(17)?,
                        confidence_locator_json: row.get(18)?,
                    })
                },
            )
            .optional()
            .map_err(|error| format!("Could not retrieve claim {id}: {error}"))?;

        raw_claim.map(RawClaim::into_claim).transpose()
    }
}

struct RawSource {
    id: String,
    created_at: String,
    created_by_kind: String,
    created_by_name: Option<String>,
    source_kind: String,
    title: Option<String>,
    publisher: Option<String>,
    author: Option<String>,
    publication_date: Option<String>,
    url: Option<String>,
    other_locator: Option<String>,
    verification_status: String,
}

impl RawSource {
    fn into_source(self) -> Result<Source, String> {
        Ok(Source {
            id: self.id,
            created_at: self.created_at,
            created_by: parse_actor(self.created_by_kind, self.created_by_name)?,
            source_kind: parse_enum(&self.source_kind, "source kind")?,
            title: self.title,
            publisher: self.publisher,
            author: self.author,
            publication_date: self.publication_date,
            url: self.url,
            other_locator: self.other_locator,
            verification_status: parse_enum(
                &self.verification_status,
                "source verification status",
            )?,
        })
    }
}

struct RawClaim {
    id: String,
    created_at: String,
    created_by_kind: String,
    created_by_name: Option<String>,
    assertion_text: String,
    category: String,
    source_id: String,
    source_locator_kind: String,
    source_locator_json: String,
    incident_id: Option<String>,
    review_status: String,
    reviewed_at: Option<String>,
    reviewed_by_kind: Option<String>,
    reviewed_by_name: Option<String>,
    confidence_stated_by: Option<String>,
    confidence_value: Option<String>,
    confidence_source_id: Option<String>,
    confidence_locator_kind: Option<String>,
    confidence_locator_json: Option<String>,
}

impl RawClaim {
    fn into_claim(self) -> Result<Claim, String> {
        let source_locator: SourceLocator = serde_json::from_str(&self.source_locator_json)
            .map_err(|error| format!("Stored claim locator is invalid: {error}"))?;
        source_locator.validate()?;
        if source_locator.kind() != self.source_locator_kind {
            return Err("Stored claim locator type does not match its saved kind".to_string());
        }

        let author_stated_confidence = match (
            self.confidence_stated_by,
            self.confidence_value,
            self.confidence_source_id,
            self.confidence_locator_kind,
            self.confidence_locator_json,
        ) {
            (None, None, None, None, None) => None,
            (Some(stated_by), Some(value_as_stated), Some(source_id), kind, json) => {
                let locator = match (kind, json) {
                    (None, None) => None,
                    (Some(kind), Some(json)) => {
                        let locator: SourceLocator =
                            serde_json::from_str(&json).map_err(|error| {
                                format!("Stored confidence locator is invalid: {error}")
                            })?;
                        locator.validate()?;
                        if locator.kind() != kind {
                            return Err(
                                "Stored confidence locator type does not match its saved kind"
                                    .to_string(),
                            );
                        }
                        Some(locator)
                    }
                    _ => return Err("Stored confidence locator is incomplete".to_string()),
                };
                Some(AuthorStatedConfidence {
                    stated_by,
                    value_as_stated,
                    source_id,
                    locator,
                })
            }
            _ => return Err("Stored author-stated confidence is incomplete".to_string()),
        };

        let reviewed_by = self
            .reviewed_by_kind
            .map(|kind| parse_actor(kind, self.reviewed_by_name))
            .transpose()?;

        Ok(Claim {
            id: self.id,
            created_at: self.created_at,
            created_by: parse_actor(self.created_by_kind, self.created_by_name)?,
            assertion_text: self.assertion_text,
            category: parse_enum(&self.category, "claim category")?,
            source_id: self.source_id,
            source_locator,
            incident_id: self.incident_id,
            author_stated_confidence,
            review: ReviewMetadata {
                status: parse_enum(&self.review_status, "claim review status")?,
                reviewed_at: self.reviewed_at,
                reviewed_by,
            },
        })
    }
}

pub(crate) fn researcher_actor() -> RecordActor {
    RecordActor {
        kind: ActorKind::Researcher,
        display_name: None,
    }
}

fn parse_actor(kind: String, display_name: Option<String>) -> Result<RecordActor, String> {
    Ok(RecordActor {
        kind: parse_enum(&kind, "record actor")?,
        display_name,
    })
}

pub(crate) fn enum_value(value: &impl Serialize) -> Result<String, String> {
    serde_json::to_value(value)
        .map_err(|error| format!("Could not encode domain value: {error}"))?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "Domain enum did not serialize as a string".to_string())
}

fn parse_enum<T: DeserializeOwned>(value: &str, field: &str) -> Result<T, String> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .map_err(|error| format!("Stored {field} is invalid: {error}"))
}

fn canonical_uuid(field: &str, value: &str) -> Result<String, String> {
    Uuid::parse_str(value)
        .map(|id| id.hyphenated().to_string())
        .map_err(|_| format!("{field} must be a valid UUID"))
}

pub(crate) fn current_timestamp(connection: &Connection) -> Result<String, String> {
    connection
        .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| {
            row.get::<_, Option<String>>(0)
        })
        .map_err(|error| format!("Could not generate record creation time: {error}"))?
        .ok_or_else(|| "SQLite could not generate a UTC creation time".to_string())
}

pub(crate) fn insert_record(
    transaction: &Transaction<'_>,
    id: &str,
    record_kind: &str,
    created_at: &str,
    created_by: &RecordActor,
) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO records (record_id, record_kind, created_at, created_by_kind, created_by_name)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id,
                record_kind,
                created_at,
                enum_value(&created_by.kind)?,
                created_by.display_name,
            ],
        )
        .map_err(|error| format!("Could not save {record_kind} record identity: {error}"))?;
    Ok(())
}

pub(crate) fn insert_creation_history(
    transaction: &Transaction<'_>,
    subject_id: &str,
    subject_kind: &str,
    summary: &str,
) -> Result<(), String> {
    insert_creation_history_with_actor(
        transaction,
        subject_id,
        subject_kind,
        summary,
        &researcher_actor(),
    )
}

pub(crate) fn insert_creation_history_with_actor(
    transaction: &Transaction<'_>,
    subject_id: &str,
    subject_kind: &str,
    summary: &str,
    actor: &RecordActor,
) -> Result<(), String> {
    let history_id = Uuid::new_v4().to_string();
    let created_at = current_timestamp(transaction)?;
    insert_record(transaction, &history_id, "history", &created_at, actor)?;

    transaction
        .execute(
            "INSERT INTO record_history (
                record_id, record_kind, subject_record_id, subject_record_kind,
                event_kind, summary
             ) VALUES (?1, 'history', ?2, ?3, 'record_created', ?4)",
            params![history_id, subject_id, subject_kind, summary],
        )
        .map_err(|error| format!("Could not save {subject_kind} creation history: {error}"))?;
    Ok(())
}

pub(crate) fn insert_author_confidence(
    transaction: &Transaction<'_>,
    claim_id: &str,
    confidence: &AuthorStatedConfidence,
) -> Result<(), String> {
    let (locator_kind, locator_json) = confidence
        .locator
        .as_ref()
        .map(|locator| {
            Ok::<_, String>((
                Some(locator.kind()),
                Some(
                    serde_json::to_string(locator)
                        .map_err(|error| format!("Could not encode confidence locator: {error}"))?,
                ),
            ))
        })
        .transpose()?
        .unwrap_or((None, None));

    transaction
        .execute(
            "INSERT INTO claim_author_confidence (
                claim_id, stated_by, value_as_stated, source_id, locator_kind, locator_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                claim_id,
                confidence.stated_by,
                confidence.value_as_stated,
                confidence.source_id,
                locator_kind,
                locator_json,
            ],
        )
        .map_err(|error| format!("Could not save author-stated confidence: {error}"))?;
    Ok(())
}

fn require_existing_source(transaction: &Transaction<'_>, source_id: &str) -> Result<(), String> {
    let exists: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sources WHERE record_id = ?1)",
            [source_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("Could not validate source reference {source_id}: {error}"))?;
    if exists {
        Ok(())
    } else {
        Err(format!("Source {source_id} does not exist"))
    }
}

fn require_existing_incident(
    transaction: &Transaction<'_>,
    incident_id: &str,
) -> Result<(), String> {
    let exists: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM incidents WHERE record_id = ?1)",
            [incident_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("Could not validate incident reference {incident_id}: {error}"))?;
    if exists {
        Ok(())
    } else {
        Err(format!("Incident {incident_id} does not exist"))
    }
}

fn initialize(connection: &mut Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA recursive_triggers = ON;
             PRAGMA busy_timeout = 5000;",
        )
        .map_err(|error| format!("Could not configure SQLite connection: {error}"))?;

    let current_version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| format!("Could not read database schema version: {error}"))?;

    if current_version > LATEST_SCHEMA_VERSION {
        return Err(format!(
            "Database schema version {current_version} is newer than the supported version \
             {LATEST_SCHEMA_VERSION}"
        ));
    }

    for &(version, sql) in MIGRATIONS {
        if version > current_version {
            apply_migration(connection, version, sql)?;
        }
    }

    Ok(())
}

fn apply_migration(connection: &mut Connection, version: i64, sql: &str) -> Result<(), String> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| format!("Could not begin migration {version}: {error}"))?;

    transaction
        .execute_batch(sql)
        .map_err(|error| format!("Migration {version} failed and was rolled back: {error}"))?;

    transaction
        .pragma_update(None, "user_version", version)
        .map_err(|error| format!("Could not record migration {version}: {error}"))?;

    transaction
        .commit()
        .map_err(|error| format!("Could not commit migration {version}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research_records::{
        AuthorStatedConfidence, ClaimCategory, CreateClaimInput, CreateSourceInput, SourceKind,
        SourceLocator, SourceVerificationStatus,
    };
    use rusqlite::Connection;
    use std::{fs, path::PathBuf};

    fn test_database() -> Database {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        initialize(&mut connection).expect("initialize schema");
        Database {
            connection: Mutex::new(connection),
        }
    }

    fn sample_source_input() -> CreateSourceInput {
        CreateSourceInput {
            source_kind: SourceKind::WebPage,
            title: Some("Research document".to_string()),
            publisher: Some("Example publisher".to_string()),
            author: Some("Example author".to_string()),
            publication_date: Some("2025-03".to_string()),
            url: Some("https://example.invalid/report".to_string()),
            other_locator: None,
            verification_status: SourceVerificationStatus::Verified,
        }
    }

    fn sample_claim_input(source_id: String) -> CreateClaimInput {
        CreateClaimInput {
            assertion_text: "A statement attributed to the cited source.".to_string(),
            category: ClaimCategory::Observation,
            source_id: source_id.clone(),
            source_locator: SourceLocator::Section {
                section: "Methods".to_string(),
                paragraph: Some(2),
            },
            incident_id: None,
            author_stated_confidence: Some(AuthorStatedConfidence {
                stated_by: "Example author".to_string(),
                value_as_stated: "High confidence".to_string(),
                source_id,
                locator: Some(SourceLocator::Page { page: 4 }),
            }),
        }
    }

    fn record_count(database: &Database, record_kind: &str) -> i64 {
        database
            .connection()
            .expect("lock database")
            .query_row(
                "SELECT COUNT(*) FROM records WHERE record_kind = ?1",
                [record_kind],
                |row| row.get(0),
            )
            .expect("count records")
    }

    fn initialized_connection() -> Connection {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        initialize(&mut connection).expect("initialize schema");
        connection
    }

    fn version_three_connection_with_research_data() -> Connection {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");
        apply_migration(
            &mut connection,
            1,
            include_str!("../migrations/0001_initial.sql"),
        )
        .expect("apply migration one");
        apply_migration(
            &mut connection,
            2,
            include_str!("../migrations/0002_dataset_identity.sql"),
        )
        .expect("apply migration two");
        apply_migration(
            &mut connection,
            3,
            include_str!("../migrations/0003_dataset_import_sequence.sql"),
        )
        .expect("apply migration three");

        connection
            .execute(
                "INSERT INTO research_datasets (dataset_id) VALUES ('dataset.one'), ('dataset.two')",
                [],
            )
            .expect("insert dataset identities");
        connection
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES
                    ('dataset.one', 'v1', 1, 'xenoscope-research-dataset', 1,
                     '2026-01-01T00:00:00Z', '{}',
                     '[\"source:a\",\"source:b\",\"claim:a\",\"claim:b\",\"evidence:a\",\"evidence:b\",\"evidence:c\",\"evidence:d\",\"evidence:e\",\"evidence:f\"]'),
                    ('dataset.one', 'v2', 2, 'xenoscope-research-dataset', 1,
                     '2026-02-01T00:00:00Z', '{}',
                     '[\"source:a\",\"source:b\",\"claim:a\",\"evidence:a\",\"evidence:b\",\"evidence:c\",\"evidence:d\",\"evidence:e\"]'),
                    ('dataset.two', 'v1', 1, 'xenoscope-research-dataset', 1,
                     '2026-01-01T00:00:00Z', '{}', '[]')",
                [],
            )
            .expect("insert versioned dataset imports");

        for (id, kind) in [
            ("source-a", "source"),
            ("source-b", "source"),
            ("claim-a", "claim"),
            ("claim-b", "claim"),
            ("evidence-a", "evidence_item"),
            ("evidence-b", "evidence_item"),
            ("evidence-c", "evidence_item"),
            ("evidence-d", "evidence_item"),
            ("evidence-e", "evidence_item"),
            ("evidence-f", "evidence_item"),
        ] {
            insert_record(&connection, id, kind);
        }
        for id in ["source-a", "source-b"] {
            connection
                .execute(
                    "INSERT INTO sources (record_id, record_kind, source_kind, verification_status)
                     VALUES (?1, 'source', 'web_page', 'not_assessed')",
                    [id],
                )
                .expect("insert source domain data");
        }
        for (id, source_id) in [("claim-a", "source-a"), ("claim-b", "source-b")] {
            connection
                .execute(
                    "INSERT INTO claims (
                        record_id, record_kind, assertion_text, category, source_id,
                        source_locator_kind, source_locator_json
                     ) VALUES (?1, 'claim', 'Test claim', 'observation', ?2,
                               'page', '{\"kind\":\"page\",\"page\":1}')",
                    params![id, source_id],
                )
                .expect("insert claim domain data");
        }
        for id in [
            "evidence-a",
            "evidence-b",
            "evidence-c",
            "evidence-d",
            "evidence-e",
            "evidence-f",
        ] {
            connection
                .execute(
                    "INSERT INTO evidence_items (
                        record_id, record_kind, source_id, locator_kind, locator_json,
                        evidence_kind, description
                     ) VALUES (?1, 'evidence_item', 'source-a', 'page',
                               '{\"kind\":\"page\",\"page\":1}', 'document', 'Test evidence')",
                    [id],
                )
                .expect("insert evidence domain data");
        }

        for (key, id, kind) in [
            ("source:a", "source-a", "source"),
            ("source:b", "source-b", "source"),
            ("claim:a", "claim-a", "claim"),
            ("claim:b", "claim-b", "claim"),
            ("evidence:a", "evidence-a", "evidence_item"),
            ("evidence:b", "evidence-b", "evidence_item"),
            ("evidence:c", "evidence-c", "evidence_item"),
            ("evidence:d", "evidence-d", "evidence_item"),
            ("evidence:e", "evidence-e", "evidence_item"),
            ("evidence:f", "evidence-f", "evidence_item"),
        ] {
            connection
                .execute(
                    "INSERT INTO dataset_record_mappings (
                        dataset_id, record_key, record_kind, record_id, content_snapshot_json
                     ) VALUES ('dataset.one', ?1, ?2, ?3, '{}')",
                    params![key, kind, id],
                )
                .expect("insert stable record mapping");
        }
        connection
    }

    fn migrated_assessment_test_connection() -> Connection {
        let mut connection = version_three_connection_with_research_data();
        initialize(&mut connection).expect("apply assessment schema migration");
        connection
    }

    fn insert_test_assessment(
        connection: &Connection,
        assessment_id: &str,
        dataset_id: &str,
        dataset_version: &str,
        import_sequence: i64,
        status: &str,
        score: Option<i64>,
        supersedes: Option<&str>,
        revision_reason: Option<&str>,
    ) -> rusqlite::Result<usize> {
        connection.execute(
            "INSERT INTO research_assessments (
                assessment_id, dataset_id, dataset_version, import_sequence, created_at,
                created_by_kind, status, score, scoring_profile_version, scoring_profile_json,
                algorithm_version, input_snapshot_json, supersedes_assessment_id, revision_reason
             ) VALUES (?1, ?2, ?3, ?4, '2026-03-01T00:00:00Z', 'researcher', ?5, ?6,
                       'profile-1', '{\"version\":1}', 'algorithm-1', '{\"inputs\":[]}',
                       ?7, ?8)",
            params![
                assessment_id,
                dataset_id,
                dataset_version,
                import_sequence,
                status,
                score,
                supersedes,
                revision_reason
            ],
        )
    }

    fn seal_test_assessment(connection: &Connection, assessment_id: &str) {
        connection
            .execute(
                "UPDATE research_assessments SET is_sealed = 1 WHERE assessment_id = ?1",
                [assessment_id],
            )
            .expect("seal assessment");
    }

    fn insert_record(connection: &Connection, id: &str, kind: &str) {
        connection
            .execute(
                "INSERT INTO records (record_id, record_kind, created_at, created_by_kind)
                 VALUES (?1, ?2, '2026-01-01T00:00:00Z', 'researcher')",
                (id, kind),
            )
            .expect("insert base record");
    }

    #[test]
    fn migrations_are_repeatable_and_do_not_seed_records_or_dataset_imports() {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        initialize(&mut connection).expect("first initialization");
        initialize(&mut connection).expect("repeat initialization");

        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version");
        let record_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))
            .expect("count records");
        let dataset_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM research_datasets", [], |row| {
                row.get(0)
            })
            .expect("count datasets");
        let import_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM dataset_imports", [], |row| row.get(0))
            .expect("count imports");
        let mapping_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM dataset_record_mappings", [], |row| {
                row.get(0)
            })
            .expect("count mappings");

        assert_eq!(version, LATEST_SCHEMA_VERSION);
        assert_eq!(record_count, 0);
        assert_eq!(dataset_count, 0);
        assert_eq!(import_count, 0);
        assert_eq!(mapping_count, 0);
    }

    #[test]
    fn migration_two_preserves_version_one_records_and_history() {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");
        apply_migration(
            &mut connection,
            1,
            include_str!("../migrations/0001_initial.sql"),
        )
        .expect("apply version one migration");

        insert_record(&connection, "source-before-upgrade", "source");
        connection
            .execute(
                "INSERT INTO sources (record_id, record_kind, source_kind, verification_status)
                 VALUES ('source-before-upgrade', 'source', 'web_page', 'verified')",
                [],
            )
            .expect("insert version one source");
        insert_record(&connection, "history-before-upgrade", "history");
        connection
            .execute(
                "INSERT INTO record_history (
                    record_id, record_kind, subject_record_id, subject_record_kind,
                    event_kind, summary
                 ) VALUES (
                    'history-before-upgrade', 'history', 'source-before-upgrade',
                    'source', 'record_created', 'Created before upgrade'
                 )",
                [],
            )
            .expect("insert version one history");

        initialize(&mut connection).expect("upgrade to version two");

        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version");
        let source_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sources WHERE record_id = 'source-before-upgrade')",
                [],
                |row| row.get(0),
            )
            .expect("check preserved source");
        let history_exists: bool = connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM record_history
                    WHERE record_id = 'history-before-upgrade'
                )",
                [],
                |row| row.get(0),
            )
            .expect("check preserved history");

        assert_eq!(version, LATEST_SCHEMA_VERSION);
        assert!(source_exists);
        assert!(history_exists);
    }

    fn apply_migrations_through_two(connection: &mut Connection) {
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");
        apply_migration(
            connection,
            1,
            include_str!("../migrations/0001_initial.sql"),
        )
        .expect("apply migration one");
        apply_migration(
            connection,
            2,
            include_str!("../migrations/0002_dataset_identity.sql"),
        )
        .expect("apply migration two");
    }

    fn insert_dataset_import(
        connection: &Connection,
        dataset_id: &str,
        dataset_version: &str,
        imported_at: &str,
    ) {
        connection
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES (
                    ?1, ?2, 'xenoscope-research-dataset', 1, ?3,
                    '{\"preserved\":true}', '[\"stable:key\"]'
                 )",
                params![dataset_id, dataset_version, imported_at],
            )
            .expect("insert existing import ledger row");
    }

    #[test]
    fn migration_three_preserves_ledger_rows_and_assigns_deterministic_sequences() {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        apply_migrations_through_two(&mut connection);
        connection
            .execute(
                "INSERT INTO research_datasets (dataset_id) VALUES ('dataset.one'), ('dataset.two')",
                [],
            )
            .expect("insert dataset identities");
        insert_dataset_import(&connection, "dataset.one", "v2", "2026-02-01T00:00:00Z");
        insert_dataset_import(&connection, "dataset.one", "v1", "2026-01-01T00:00:00Z");
        insert_dataset_import(&connection, "dataset.one", "v3", "2026-02-01T00:00:00Z");
        insert_dataset_import(&connection, "dataset.two", "v1", "2026-01-15T00:00:00Z");

        initialize(&mut connection).expect("apply migration three");

        let rows: Vec<(String, String, i64, String, String, String)> = {
            let mut statement = connection
                .prepare(
                    "SELECT dataset_id, dataset_version, import_sequence, imported_at,
                            metadata_json, record_keys_json
                     FROM dataset_imports
                     ORDER BY dataset_id, import_sequence",
                )
                .expect("prepare imported rows query");
            statement
                .query_map([], |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                })
                .expect("query preserved imports")
                .collect::<Result<_, _>>()
                .expect("collect preserved imports")
        };
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version");

        assert_eq!(
            rows,
            vec![
                (
                    "dataset.one".to_string(),
                    "v1".to_string(),
                    1,
                    "2026-01-01T00:00:00Z".to_string(),
                    "{\"preserved\":true}".to_string(),
                    "[\"stable:key\"]".to_string()
                ),
                (
                    "dataset.one".to_string(),
                    "v2".to_string(),
                    2,
                    "2026-02-01T00:00:00Z".to_string(),
                    "{\"preserved\":true}".to_string(),
                    "[\"stable:key\"]".to_string()
                ),
                (
                    "dataset.one".to_string(),
                    "v3".to_string(),
                    3,
                    "2026-02-01T00:00:00Z".to_string(),
                    "{\"preserved\":true}".to_string(),
                    "[\"stable:key\"]".to_string()
                ),
                (
                    "dataset.two".to_string(),
                    "v1".to_string(),
                    1,
                    "2026-01-15T00:00:00Z".to_string(),
                    "{\"preserved\":true}".to_string(),
                    "[\"stable:key\"]".to_string()
                ),
            ]
        );
        assert_eq!(version, LATEST_SCHEMA_VERSION);
    }

    #[test]
    fn import_sequence_must_be_positive_and_unique_per_dataset() {
        let connection = initialized_connection();
        connection
            .execute(
                "INSERT INTO research_datasets (dataset_id) VALUES ('dataset.one'), ('dataset.two')",
                [],
            )
            .expect("insert dataset identities");
        connection
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES (
                    'dataset.one', 'v1', 1, 'xenoscope-research-dataset', 1,
                    '2026-01-01T00:00:00Z', '{}', '[]'
                 )",
                [],
            )
            .expect("insert first sequence");
        connection
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES (
                    'dataset.one', 'v2', 1, 'xenoscope-research-dataset', 1,
                    '2026-01-02T00:00:00Z', '{}', '[]'
                 )",
                [],
            )
            .expect_err("sequence must be unique within one dataset");
        connection
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES (
                    'dataset.one', 'v0', 0, 'xenoscope-research-dataset', 1,
                    '2025-12-31T00:00:00Z', '{}', '[]'
                 )",
                [],
            )
            .expect_err("sequence must be positive");
        connection
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES (
                    'dataset.two', 'v1', 1, 'xenoscope-research-dataset', 1,
                    '2026-01-01T00:00:00Z', '{}', '[]'
                 )",
                [],
            )
            .expect("different datasets may reuse a sequence");
    }

    #[test]
    fn failed_sequence_migration_rolls_back_schema_and_user_version() {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        apply_migrations_through_two(&mut connection);
        let failing_sql = format!(
            "{}\nINVALID SQL;",
            include_str!("../migrations/0003_dataset_import_sequence.sql")
        );

        let error = apply_migration(&mut connection, 3, &failing_sql)
            .expect_err("invalid migration suffix should fail");

        assert!(error.contains("rolled back"));
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version");
        let sequence_column_exists: bool = connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM pragma_table_info('dataset_imports')
                    WHERE name = 'import_sequence'
                )",
                [],
                |row| row.get(0),
            )
            .expect("check sequence column rollback");
        assert_eq!(version, 2);
        assert!(!sequence_column_exists);
    }

    #[test]
    fn failed_dataset_identity_migration_rolls_back_tables_and_user_version() {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");
        apply_migration(
            &mut connection,
            1,
            include_str!("../migrations/0001_initial.sql"),
        )
        .expect("apply version one migration");
        let failing_sql = format!(
            "{}\nINVALID SQL;",
            include_str!("../migrations/0002_dataset_identity.sql")
        );

        let error = apply_migration(&mut connection, 2, &failing_sql)
            .expect_err("invalid migration suffix should fail");

        assert!(error.contains("rolled back"));
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version");
        let table_exists: bool = connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM sqlite_master WHERE type = 'table'
                    AND name = 'research_datasets'
                )",
                [],
                |row| row.get(0),
            )
            .expect("check migration table rollback");

        assert_eq!(version, 1);
        assert!(!table_exists);
    }

    #[test]
    fn migration_four_preserves_v3_research_data_and_adds_empty_assessment_tables() {
        let mut connection = version_three_connection_with_research_data();
        let version_three: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read v3 schema version");
        let before: (i64, i64, i64, i64, i64) = connection
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM records),
                    (SELECT COUNT(*) FROM sources),
                    (SELECT COUNT(*) FROM claims),
                    (SELECT COUNT(*) FROM dataset_imports),
                    (SELECT COUNT(*) FROM dataset_record_mappings)",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .expect("count v3 data");
        let existing_rows_before: (String, String, String, String) = connection
            .query_row(
                "SELECT
                    (SELECT source_kind || ':' || verification_status
                     FROM sources WHERE record_id = 'source-a'),
                    (SELECT assertion_text || ':' || category || ':' || source_locator_json
                     FROM claims WHERE record_id = 'claim-a'),
                    (SELECT dataset_version || ':' || imported_at || ':' || metadata_json ||
                            ':' || record_keys_json
                     FROM dataset_imports
                     WHERE dataset_id = 'dataset.one' AND dataset_version = 'v2'),
                    (SELECT record_kind || ':' || record_id || ':' || content_snapshot_json
                     FROM dataset_record_mappings
                     WHERE dataset_id = 'dataset.one' AND record_key = 'claim:a')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("read representative v3 row values");
        assert_eq!(version_three, 3);

        initialize(&mut connection).expect("migrate v3 database to v4");

        let version_four: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read v4 schema version");
        let after: (i64, i64, i64, i64, i64) = connection
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM records),
                    (SELECT COUNT(*) FROM sources),
                    (SELECT COUNT(*) FROM claims),
                    (SELECT COUNT(*) FROM dataset_imports),
                    (SELECT COUNT(*) FROM dataset_record_mappings)",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .expect("count preserved data");
        let existing_rows_after: (String, String, String, String) = connection
            .query_row(
                "SELECT
                    (SELECT source_kind || ':' || verification_status
                     FROM sources WHERE record_id = 'source-a'),
                    (SELECT assertion_text || ':' || category || ':' || source_locator_json
                     FROM claims WHERE record_id = 'claim-a'),
                    (SELECT dataset_version || ':' || imported_at || ':' || metadata_json ||
                            ':' || record_keys_json
                     FROM dataset_imports
                     WHERE dataset_id = 'dataset.one' AND dataset_version = 'v2'),
                    (SELECT record_kind || ':' || record_id || ':' || content_snapshot_json
                     FROM dataset_record_mappings
                     WHERE dataset_id = 'dataset.one' AND record_key = 'claim:a')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("read preserved v3 row values");
        let assessment_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM research_assessments", [], |row| {
                row.get(0)
            })
            .expect("count new assessments");
        let foreign_key_violations: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .expect("check migrated foreign keys");

        assert_eq!(version_four, 4);
        assert_eq!(after, before);
        assert_eq!(existing_rows_after, existing_rows_before);
        assert_eq!(assessment_count, 0);
        assert_eq!(foreign_key_violations, 0);
    }

    #[test]
    fn assessment_status_score_snapshot_and_predecessor_constraints_are_enforced() {
        let connection = migrated_assessment_test_connection();
        insert_test_assessment(
            &connection,
            "assessment-v1",
            "dataset.one",
            "v1",
            1,
            "preliminary",
            None,
            None,
            None,
        )
        .expect("insert preliminary assessment without score");
        assert!(
            insert_test_assessment(
                &connection,
                "assessment-v2-too-early",
                "dataset.one",
                "v2",
                2,
                "preliminary",
                None,
                Some("assessment-v1"),
                Some("Parent is not sealed"),
            )
            .is_err()
        );
        seal_test_assessment(&connection, "assessment-v1");
        insert_test_assessment(
            &connection,
            "assessment-v2",
            "dataset.one",
            "v2",
            2,
            "limited_by_missing_information",
            Some(1),
            Some("assessment-v1"),
            Some("Added newer evidence"),
        )
        .expect("insert successor assessment");
        insert_test_assessment(
            &connection,
            "assessment-v3",
            "dataset.one",
            "v2",
            2,
            "reviewed_for_scope",
            Some(100),
            None,
            None,
        )
        .expect("insert assessment with maximum score");

        assert!(
            insert_test_assessment(
                &connection,
                "score-zero",
                "dataset.one",
                "v1",
                1,
                "preliminary",
                Some(0),
                None,
                None,
            )
            .is_err()
        );
        assert!(
            insert_test_assessment(
                &connection,
                "score-over-max",
                "dataset.one",
                "v1",
                1,
                "preliminary",
                Some(101),
                None,
                None,
            )
            .is_err()
        );
        assert!(
            insert_test_assessment(
                &connection,
                "invalid-status",
                "dataset.one",
                "v1",
                1,
                "complete",
                None,
                None,
                None,
            )
            .is_err()
        );
        assert!(
            insert_test_assessment(
                &connection,
                "mismatched-import",
                "dataset.one",
                "v1",
                2,
                "preliminary",
                None,
                None,
                None,
            )
            .is_err()
        );
        assert!(
            insert_test_assessment(
                &connection,
                "self-reference",
                "dataset.one",
                "v1",
                1,
                "preliminary",
                None,
                Some("self-reference"),
                Some("Self-reference"),
            )
            .is_err()
        );
        assert!(
            insert_test_assessment(
                &connection,
                "missing-predecessor",
                "dataset.one",
                "v1",
                1,
                "preliminary",
                None,
                Some("does-not-exist"),
                Some("Missing parent"),
            )
            .is_err()
        );
        assert!(
            insert_test_assessment(
                &connection,
                "cross-case-predecessor",
                "dataset.two",
                "v1",
                1,
                "preliminary",
                None,
                Some("assessment-v1"),
                Some("Wrong case"),
            )
            .is_err()
        );
        assert!(
            insert_test_assessment(
                &connection,
                "missing-revision-reason",
                "dataset.one",
                "v1",
                1,
                "preliminary",
                None,
                Some("assessment-v1"),
                None,
            )
            .is_err()
        );
        connection
            .execute(
                "INSERT INTO research_assessments (
                    assessment_id, dataset_id, dataset_version, import_sequence, created_at,
                    created_by_kind, status, scoring_profile_version, scoring_profile_json,
                    algorithm_version, input_snapshot_json
                 ) VALUES (
                    'invalid-snapshot', 'dataset.one', 'v1', 1, '2026-03-01T00:00:00Z',
                    'researcher', 'preliminary', 'profile-1', '[]',
                    'algorithm-1', '{\"inputs\":[]}'
                 )",
                [],
            )
            .expect_err("profile JSON must be an object");
        connection
            .execute(
                "INSERT INTO research_assessments (
                    assessment_id, dataset_id, dataset_version, import_sequence, created_at,
                    created_by_kind, status, scoring_profile_version, scoring_profile_json,
                    algorithm_version, input_snapshot_json
                 ) VALUES (
                    'invalid-input-json', 'dataset.one', 'v1', 1, '2026-03-01T00:00:00Z',
                    'researcher', 'preliminary', 'profile-1', '{}',
                    'algorithm-1', '{invalid'
                 )",
                [],
            )
            .expect_err("input snapshot must contain valid JSON");
    }

    #[test]
    fn assessment_claims_and_evidence_enforce_scope_and_valid_states() {
        let connection = migrated_assessment_test_connection();
        insert_test_assessment(
            &connection,
            "assessment-evidence",
            "dataset.one",
            "v1",
            1,
            "preliminary",
            None,
            None,
            None,
        )
        .expect("insert assessment");
        connection
            .execute(
                "INSERT INTO assessment_claims (
                    assessment_id, dataset_id, claim_key, claim_record_id, scope_role, outcome
                 ) VALUES (
                    'assessment-evidence', 'dataset.one', 'claim:a', 'claim-a', 'core', 'supported'
                 )",
                [],
            )
            .expect("select a claim");
        connection
            .execute(
                "INSERT INTO assessment_claims (
                    assessment_id, dataset_id, claim_key, claim_record_id, scope_role, outcome
                 ) VALUES (
                    'assessment-evidence', 'dataset.one', 'claim:a', 'claim-a', 'primary', 'supported'
                 )",
                [],
            )
            .expect_err("invalid scope role must be rejected");
        connection
            .execute(
                "INSERT INTO assessment_claims (
                    assessment_id, dataset_id, claim_key, claim_record_id, scope_role, outcome
                 ) VALUES (
                    'assessment-evidence', 'dataset.one', 'claim:a', 'claim-a', 'context', 'unknown'
                 )",
                [],
            )
            .expect_err("invalid claim outcome must be rejected");

        let evidence_states = [
            ("evidence-a", "supports"),
            ("evidence-b", "conflicts"),
            ("evidence-c", "not_examined"),
            ("evidence-d", "irrelevant_to_scope"),
            ("evidence-e", "indeterminate"),
        ];
        for (index, (evidence_id, state)) in evidence_states.iter().enumerate() {
            let key = format!("evidence:{}", (b'a' + index as u8) as char);
            connection
                .execute(
                    "INSERT INTO assessment_claim_evidence (
                        assessment_evidence_id, assessment_id, dataset_id, claim_record_id,
                        evidence_key, evidence_record_id, state
                     ) VALUES (?1, 'assessment-evidence', 'dataset.one', 'claim-a', ?2, ?3, ?4)",
                    params![format!("evidence-state-{state}"), key, evidence_id, state],
                )
                .expect("insert valid evidence state");
        }
        connection
            .execute(
                "INSERT INTO assessment_claim_evidence (
                    assessment_evidence_id, assessment_id, dataset_id, claim_record_id,
                    state, note
                 ) VALUES (
                    'evidence-unavailable', 'assessment-evidence', 'dataset.one',
                    'claim-a', 'unavailable', 'Referenced evidence is not available for review'
                 )",
                [],
            )
            .expect("record unavailable evidence without a fabricated record ID");
        connection
            .execute(
                "INSERT INTO assessment_claim_evidence (
                    assessment_evidence_id, assessment_id, dataset_id,
                    claim_record_id, evidence_key, state
                 ) VALUES (
                    'evidence-unavailable-key', 'assessment-evidence', 'dataset.one',
                    'claim-a', 'expected:evidence-key', 'unavailable'
                 )",
                [],
            )
            .expect("a descriptive stable reference can identify unavailable evidence");
        connection
            .execute(
                "INSERT INTO assessment_claim_evidence (
                    assessment_evidence_id, assessment_id, dataset_id, claim_record_id, state
                 ) VALUES (
                    'evidence-unavailable-no-note', 'assessment-evidence', 'dataset.one',
                    'claim-a', 'unavailable'
                 )",
                [],
            )
            .expect_err("unavailable evidence needs a meaningful note or reference");
        connection
            .execute(
                "INSERT INTO assessment_claim_evidence (
                    assessment_evidence_id, assessment_id, dataset_id, claim_record_id,
                    evidence_key, evidence_record_id, state
                 ) VALUES (
                    'evidence-wrong-claim', 'assessment-evidence', 'dataset.one',
                    'claim-b', 'evidence:a', 'evidence-a', 'supports'
                 )",
                [],
            )
            .expect_err("evidence must reference a claim selected for this assessment");
        connection
            .execute(
                "INSERT INTO assessment_claim_evidence (
                    assessment_evidence_id, assessment_id, dataset_id, claim_record_id,
                    evidence_key, evidence_record_id, state
                 ) VALUES (
                    'evidence-missing-record', 'assessment-evidence', 'dataset.one',
                    'claim-a', 'evidence:missing', 'evidence-missing', 'supports'
                 )",
                [],
            )
            .expect_err("evidence record references must satisfy foreign keys");
        connection
            .execute(
                "INSERT INTO assessment_claim_evidence (
                    assessment_evidence_id, assessment_id, dataset_id, claim_record_id,
                    evidence_key, evidence_record_id, state
                 ) VALUES (
                    'evidence-invalid-state', 'assessment-evidence', 'dataset.one',
                    'claim-a', 'evidence:f', 'evidence-f', 'uncertain'
                 )",
                [],
            )
            .expect_err("invalid evidence state must be rejected");

        insert_test_assessment(
            &connection,
            "assessment-new-version",
            "dataset.one",
            "v2",
            2,
            "preliminary",
            None,
            None,
            None,
        )
        .expect("insert assessment for newer import");
        connection
            .execute(
                "INSERT INTO assessment_claims (
                    assessment_id, dataset_id, claim_key, claim_record_id, scope_role, outcome
                 ) VALUES (
                    'assessment-new-version', 'dataset.one', 'claim:a', 'claim-a',
                    'core', 'unexamined'
                 )",
                [],
            )
            .expect("select claim present in the newer manifest");
        connection
            .execute(
                "INSERT INTO assessment_claims (
                    assessment_id, dataset_id, claim_key, claim_record_id, scope_role, outcome
                 ) VALUES (
                    'assessment-new-version', 'dataset.one', 'claim:b', 'claim-b',
                    'core', 'supported'
                 )",
                [],
            )
            .expect_err("claim absent from the selected import manifest must be rejected");
        connection
            .execute(
                "INSERT INTO assessment_claim_evidence (
                    assessment_evidence_id, assessment_id, dataset_id, claim_record_id,
                    evidence_key, evidence_record_id, state
                 ) VALUES (
                    'evidence-removed-from-manifest', 'assessment-new-version',
                    'dataset.one', 'claim-a', 'evidence:f', 'evidence-f', 'not_examined'
                 )",
                [],
            )
            .expect_err("evidence absent from the selected version manifest must be rejected");
    }

    #[test]
    fn assessment_factor_and_source_dependency_constraints_are_enforced() {
        let connection = migrated_assessment_test_connection();
        insert_test_assessment(
            &connection,
            "assessment-factors",
            "dataset.one",
            "v1",
            1,
            "preliminary",
            None,
            None,
            None,
        )
        .expect("insert assessment");

        for factor in [
            "evidence_quality",
            "provenance",
            "independent_corroboration",
            "consistency",
        ] {
            connection
                .execute(
                    "INSERT INTO assessment_factor_ratings (
                        assessment_id, factor_key, rating_json, rationale, references_json
                     ) VALUES ('assessment-factors', ?1, NULL, NULL, '[]')",
                    [factor],
                )
                .expect("insert valid nullable factor rating");
        }
        connection
            .execute(
                "INSERT INTO assessment_factor_ratings (assessment_id, factor_key)
                 VALUES ('assessment-factors', 'evidence_quality')",
                [],
            )
            .expect_err("factor key must be unique per assessment");
        connection
            .execute(
                "INSERT INTO assessment_factor_ratings (assessment_id, factor_key)
                 VALUES ('assessment-factors', 'source_quality')",
                [],
            )
            .expect_err("invalid factor key must be rejected");
        connection
            .execute(
                "INSERT INTO assessment_factor_ratings (
                    assessment_id, factor_key, rating_json, references_json
                 ) VALUES ('assessment-factors', 'provenance', 'not-json', '[]')",
                [],
            )
            .expect_err("factor rating JSON must be valid when present");
        connection
            .execute(
                "INSERT INTO assessment_factor_ratings (
                    assessment_id, factor_key, references_json
                 ) VALUES ('assessment-factors', 'consistency', '{}')",
                [],
            )
            .expect_err("factor references must be a JSON array");

        connection
            .execute(
                "INSERT INTO assessment_source_dependencies (
                    dependency_id, assessment_id, dataset_id,
                    source_a_key, source_a_id, source_b_key, source_b_id, state
                 ) VALUES (
                    'dependency-1', 'assessment-factors', 'dataset.one',
                    'source:a', 'source-a', 'source:b', 'source-b', 'unknown'
                 )",
                [],
            )
            .expect("insert unknown source dependency");
        insert_test_assessment(
            &connection,
            "assessment-dependency-independent",
            "dataset.one",
            "v1",
            1,
            "preliminary",
            None,
            None,
            None,
        )
        .expect("insert second dependency assessment");
        connection
            .execute(
                "INSERT INTO assessment_source_dependencies (
                    dependency_id, assessment_id, dataset_id,
                    source_a_key, source_a_id, source_b_key, source_b_id, state
                 ) VALUES (
                    'dependency-independent', 'assessment-dependency-independent',
                    'dataset.one', 'source:a', 'source-a', 'source:b', 'source-b',
                    'assessed_independent'
                 )",
                [],
            )
            .expect("insert assessed-independent dependency state");
        insert_test_assessment(
            &connection,
            "assessment-dependency-dependent",
            "dataset.one",
            "v1",
            1,
            "preliminary",
            None,
            None,
            None,
        )
        .expect("insert third dependency assessment");
        connection
            .execute(
                "INSERT INTO assessment_source_dependencies (
                    dependency_id, assessment_id, dataset_id,
                    source_a_key, source_a_id, source_b_key, source_b_id, state
                 ) VALUES (
                    'dependency-dependent', 'assessment-dependency-dependent',
                    'dataset.one', 'source:a', 'source-a', 'source:b', 'source-b',
                    'dependent'
                 )",
                [],
            )
            .expect("insert dependent source state");
        connection
            .execute(
                "INSERT INTO assessment_source_dependencies (
                    dependency_id, assessment_id, dataset_id,
                    source_a_key, source_a_id, source_b_key, source_b_id, state
                 ) VALUES (
                    'dependency-duplicate', 'assessment-factors', 'dataset.one',
                    'source:a', 'source-a', 'source:b', 'source-b', 'dependent'
                 )",
                [],
            )
            .expect_err("source pair must be unique per assessment");
        connection
            .execute(
                "INSERT INTO assessment_source_dependencies (
                    dependency_id, assessment_id, dataset_id,
                    source_a_key, source_a_id, source_b_key, source_b_id, state
                 ) VALUES (
                    'dependency-reversed', 'assessment-factors', 'dataset.one',
                    'source:b', 'source-b', 'source:a', 'source-a', 'dependent'
                 )",
                [],
            )
            .expect_err("source pairs must use normalized ascending record IDs");
        connection
            .execute(
                "INSERT INTO assessment_source_dependencies (
                    dependency_id, assessment_id, dataset_id,
                    source_a_key, source_a_id, source_b_key, source_b_id, state
                 ) VALUES (
                    'dependency-self', 'assessment-factors', 'dataset.one',
                    'source:a', 'source-a', 'source:a', 'source-a', 'dependent'
                 )",
                [],
            )
            .expect_err("self-dependencies must be rejected");
        connection
            .execute(
                "INSERT INTO assessment_source_dependencies (
                    dependency_id, assessment_id, dataset_id,
                    source_a_key, source_a_id, source_b_key, source_b_id, state
                 ) VALUES (
                    'dependency-invalid-state', 'assessment-factors', 'dataset.one',
                    'source:a', 'source-a', 'source:b', 'source-b', 'probably_independent'
                 )",
                [],
            )
            .expect_err("invalid dependency state must be rejected");
    }

    #[test]
    fn assessment_rows_and_inputs_cannot_be_overwritten_or_deleted() {
        let connection = migrated_assessment_test_connection();
        let recursive_triggers: bool = connection
            .pragma_query_value(None, "recursive_triggers", |row| row.get(0))
            .expect("read recursive trigger setting");
        assert!(recursive_triggers);

        insert_test_assessment(
            &connection,
            "assessment-immutable",
            "dataset.one",
            "v1",
            1,
            "preliminary",
            None,
            None,
            None,
        )
        .expect("insert assessment");
        connection
            .execute(
                "INSERT INTO assessment_claims (
                    assessment_id, dataset_id, claim_key, claim_record_id, scope_role, outcome,
                    rationale
                 ) VALUES (
                    'assessment-immutable', 'dataset.one', 'claim:a', 'claim-a',
                    'core', 'supported', 'Original claim rationale'
                 )",
                [],
            )
            .expect("insert assessment claim");
        connection
            .execute(
                "INSERT INTO assessment_factor_ratings (
                    assessment_id, factor_key, rationale
                 ) VALUES (
                    'assessment-immutable', 'evidence_quality', 'Original factor rationale'
                 )",
                [],
            )
            .expect("insert assessment factor");
        seal_test_assessment(&connection, "assessment-immutable");

        insert_test_assessment(
            &connection,
            "sealed-assessment-replace",
            "dataset.one",
            "v1",
            1,
            "preliminary",
            None,
            None,
            None,
        )
        .expect("insert assessment to isolate the replacement conflict");
        seal_test_assessment(&connection, "sealed-assessment-replace");

        let assessment_replace = connection.execute(
            "INSERT OR REPLACE INTO research_assessments (
                assessment_id, dataset_id, dataset_version, import_sequence, created_at,
                created_by_kind, status, score, scoring_profile_version, scoring_profile_json,
                algorithm_version, input_snapshot_json, is_sealed
             ) VALUES (
                'sealed-assessment-replace', 'dataset.one', 'v1', 1,
                '2026-03-02T00:00:00Z', 'researcher', 'reviewed_for_scope', 99,
                'profile-2', '{\"version\":2}', 'algorithm-2', '{\"inputs\":[\"replaced\"]}', 1
             )",
            [],
        );
        assert!(
            assessment_replace.is_err(),
            "INSERT OR REPLACE must not replace a sealed assessment: {assessment_replace:?}"
        );

        let factor_replace = connection.execute(
            "INSERT OR REPLACE INTO assessment_factor_ratings (
                assessment_id, factor_key, rationale, references_json
             ) VALUES (
                'assessment-immutable', 'evidence_quality', 'Replaced rationale', '[]'
             )",
            [],
        );
        assert!(
            factor_replace.is_err(),
            "INSERT OR REPLACE must not replace a sealed factor row: {factor_replace:?}"
        );

        let claim_replace = connection.execute(
            "INSERT OR REPLACE INTO assessment_claims (
                assessment_id, dataset_id, claim_key, claim_record_id, scope_role, outcome,
                rationale
             ) VALUES (
                'assessment-immutable', 'dataset.one', 'claim:a', 'claim-a',
                'core', 'contradicted', 'Replaced claim rationale'
             )",
            [],
        );
        assert!(
            claim_replace.is_err(),
            "INSERT OR REPLACE must not replace a sealed claim input: {claim_replace:?}"
        );

        let stored_snapshot: String = connection
            .query_row(
                "SELECT input_snapshot_json FROM research_assessments
                 WHERE assessment_id = 'sealed-assessment-replace'",
                [],
                |row| row.get(0),
            )
            .expect("read original sealed assessment");
        let stored_factor_rationale: String = connection
            .query_row(
                "SELECT rationale FROM assessment_factor_ratings
                 WHERE assessment_id = 'assessment-immutable'
                   AND factor_key = 'evidence_quality'",
                [],
                |row| row.get(0),
            )
            .expect("read original sealed factor");
        let stored_claim: (String, String) = connection
            .query_row(
                "SELECT outcome, rationale FROM assessment_claims
                 WHERE assessment_id = 'assessment-immutable' AND claim_record_id = 'claim-a'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read original sealed claim input");
        assert_eq!(stored_snapshot, "{\"inputs\":[]}");
        assert_eq!(stored_factor_rationale, "Original factor rationale");
        assert_eq!(
            stored_claim,
            (
                "supported".to_string(),
                "Original claim rationale".to_string()
            )
        );

        assert!(
            connection
                .execute(
                    "UPDATE research_assessments SET score = 50
                     WHERE assessment_id = 'assessment-immutable'",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "INSERT INTO assessment_factor_ratings (assessment_id, factor_key)
                     VALUES ('assessment-immutable', 'provenance')",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "DELETE FROM research_assessments WHERE assessment_id = 'assessment-immutable'",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "UPDATE assessment_factor_ratings SET rationale = 'changed'
                     WHERE assessment_id = 'assessment-immutable'",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "DELETE FROM assessment_factor_ratings
                     WHERE assessment_id = 'assessment-immutable'",
                    [],
                )
                .is_err()
        );
    }

    #[test]
    fn dataset_identity_constraints_enforce_dataset_keys_kinds_and_record_identity() {
        let connection = initialized_connection();
        insert_record(&connection, "mapped-source", "source");
        connection
            .execute(
                "INSERT INTO sources (record_id, record_kind, source_kind, verification_status)
                 VALUES ('mapped-source', 'source', 'web_page', 'verified')",
                [],
            )
            .expect("insert mapped source");
        connection
            .execute(
                "INSERT INTO research_datasets (dataset_id) VALUES ('dataset.one')",
                [],
            )
            .expect("insert dataset identity");
        connection
            .execute(
                "INSERT INTO dataset_record_mappings (
                    dataset_id, record_key, record_kind, record_id, content_snapshot_json
                 ) VALUES (
                    'dataset.one', 'dataset.one:source:report', 'source',
                    'mapped-source', '{}'
                 )",
                [],
            )
            .expect("insert valid record mapping");

        connection
            .execute(
                "INSERT INTO dataset_record_mappings (
                    dataset_id, record_key, record_kind, record_id, content_snapshot_json
                 ) VALUES (
                    'dataset.one', 'dataset.one:source:wrong-kind', 'claim',
                    'mapped-source', '{}'
                 )",
                [],
            )
            .expect_err("mapping kind must match the existing record kind");
        connection
            .execute(
                "INSERT INTO dataset_record_mappings (
                    dataset_id, record_key, record_kind, record_id, content_snapshot_json
                 ) VALUES (
                    'missing-dataset', 'missing-dataset:source:report', 'source',
                    'mapped-source', '{}'
                 )",
                [],
            )
            .expect_err("mapping must reference an existing dataset");
        connection
            .execute(
                "INSERT INTO dataset_record_mappings (
                    dataset_id, record_key, record_kind, record_id, content_snapshot_json
                 ) VALUES (
                    'dataset.one', 'dataset.one:history:entry', 'history',
                    'mapped-source', '{}'
                 )",
                [],
            )
            .expect_err("history records cannot be dataset-mapped");
        connection
            .execute(
                "INSERT INTO dataset_record_mappings (
                    dataset_id, record_key, record_kind, record_id, content_snapshot_json
                 ) VALUES (
                    'dataset.one', 'dataset.one:source:second-key', 'source',
                    'mapped-source', '{}'
                 )",
                [],
            )
            .expect_err("one dataset record cannot be mapped to multiple stable keys");
        insert_record(&connection, "another-source", "source");
        connection
            .execute(
                "INSERT INTO sources (record_id, record_kind, source_kind, verification_status)
                 VALUES ('another-source', 'source', 'web_page', 'verified')",
                [],
            )
            .expect("insert second source");
        connection
            .execute(
                "INSERT INTO dataset_record_mappings (
                    dataset_id, record_key, record_kind, record_id, content_snapshot_json
                 ) VALUES (
                    'dataset.one', 'dataset.one:source:report', 'source',
                    'another-source', '{}'
                 )",
                [],
            )
            .expect_err("stable key must be unique within its dataset");
        connection
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES (
                    'dataset.one', '1.0.0', 1, 'xenoscope-research-dataset', 1,
                    '2026-01-01T00:00:00Z', '{}', '[]'
                 )",
                [],
            )
            .expect("insert successful import ledger row");
        connection
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES (
                    'dataset.one', '1.0.0', 2, 'xenoscope-research-dataset', 1,
                    '2026-01-02T00:00:00Z', '{}', '[]'
                 )",
                [],
            )
            .expect_err("dataset version must be unique");
    }

    #[test]
    fn dataset_identity_json_columns_reject_invalid_json() {
        let connection = initialized_connection();
        connection
            .execute(
                "INSERT INTO research_datasets (dataset_id) VALUES ('dataset.json')",
                [],
            )
            .expect("insert dataset identity");

        connection
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES (
                    'dataset.json', '1.0.0', 1, 'xenoscope-research-dataset', 1,
                    '2026-01-01T00:00:00Z', '{invalid', '[]'
                 )",
                [],
            )
            .expect_err("metadata must be valid JSON");
        connection
            .execute(
                "INSERT INTO dataset_imports (
                    dataset_id, dataset_version, import_sequence, format, format_version,
                    imported_at, metadata_json, record_keys_json
                 ) VALUES (
                    'dataset.json', '1.0.0', 2, 'xenoscope-research-dataset', 1,
                    '2026-01-01T00:00:00Z', '{}', '{invalid'
                 )",
                [],
            )
            .expect_err("record manifest must be valid JSON");
        insert_record(&connection, "json-source", "source");
        connection
            .execute(
                "INSERT INTO sources (record_id, record_kind, source_kind, verification_status)
                 VALUES ('json-source', 'source', 'web_page', 'verified')",
                [],
            )
            .expect("insert source for mapping");
        connection
            .execute(
                "INSERT INTO dataset_record_mappings (
                    dataset_id, record_key, record_kind, record_id, content_snapshot_json
                 ) VALUES (
                    'dataset.json', 'dataset.json:source:report', 'source',
                    'json-source', '{invalid'
                 )",
                [],
            )
            .expect_err("record snapshot must be valid JSON");
    }

    #[test]
    fn claims_require_existing_sources_and_typed_relationship_endpoints() {
        let connection = initialized_connection();
        insert_record(&connection, "claim-1", "claim");
        connection
            .execute(
                "INSERT INTO claims (
                    record_id, record_kind, assertion_text, category,
                    source_id, source_locator_kind, source_locator_json, review_status
                 ) VALUES (
                    'claim-1', 'claim', 'Example assertion', 'context',
                    'missing-source', 'page', '{\"page\": 1}', 'unreviewed'
                 )",
                [],
            )
            .expect_err("claim without a source must fail");

        insert_record(&connection, "source-1", "source");
        connection
            .execute(
                "INSERT INTO sources (record_id, record_kind, source_kind, verification_status)
                 VALUES ('source-1', 'source', 'web_page', 'unverified')",
                [],
            )
            .expect("insert source");
        connection
            .execute(
                "INSERT INTO claims (
                    record_id, record_kind, assertion_text, category,
                    source_id, source_locator_kind, source_locator_json, review_status
                 ) VALUES (
                    'claim-1', 'claim', 'Example assertion', 'context',
                    'source-1', 'page', '{\"page\": 1}', 'unreviewed'
                 )",
                [],
            )
            .expect("insert source-attributed claim");

        insert_record(&connection, "relationship-1", "relationship");
        connection
            .execute(
                "INSERT INTO relationships (
                    record_id, record_kind, relationship_kind,
                    from_record_id, from_record_kind, to_record_id, to_record_kind
                 ) VALUES (
                    'relationship-1', 'relationship', 'corrects',
                    'source-1', 'claim', 'claim-1', 'claim'
                 )",
                [],
            )
            .expect_err("relationship endpoint kind must match its record");
    }

    #[test]
    fn discrepancies_and_findings_reference_existing_typed_records() {
        let connection = initialized_connection();
        insert_record(&connection, "discrepancy-1", "discrepancy");
        connection
            .execute(
                "INSERT INTO discrepancies (record_id, record_kind, description, status)
                 VALUES ('discrepancy-1', 'discrepancy', 'A documented difference', 'open')",
                [],
            )
            .expect("insert discrepancy");
        connection
            .execute(
                "INSERT INTO discrepancy_claims (discrepancy_id, claim_id)
                 VALUES ('discrepancy-1', 'missing-claim')",
                [],
            )
            .expect_err("discrepancy claim reference must exist");

        insert_record(&connection, "finding-1", "finding");
        connection
            .execute(
                "INSERT INTO findings (
                    record_id, record_kind, assessment, attribution_kind, review_status
                 ) VALUES ('finding-1', 'finding', 'Assessment text', 'researcher', 'unreviewed')",
                [],
            )
            .expect("insert finding");
        connection
            .execute(
                "INSERT INTO finding_claim_references (finding_id, claim_id, reference_role)
                 VALUES ('finding-1', 'missing-claim', 'supporting')",
                [],
            )
            .expect_err("finding claim reference must exist");
        connection
            .execute(
                "INSERT INTO finding_evidence_references (finding_id, evidence_item_id, reference_role)
                 VALUES ('finding-1', 'missing-evidence', 'conflicting')",
                [],
            )
            .expect_err("finding evidence reference must exist");
    }

    #[test]
    fn failed_migration_rolls_back_schema_and_version() {
        let mut connection = initialized_connection();
        let error = apply_migration(
            &mut connection,
            LATEST_SCHEMA_VERSION + 1,
            "CREATE TABLE migration_probe (id TEXT); INVALID SQL;",
        )
        .expect_err("invalid migration must fail");

        assert!(error.contains("rolled back"));
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version");
        let probe_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'migration_probe')",
                [],
                |row| row.get(0),
            )
            .expect("check rolled-back table");

        assert_eq!(version, LATEST_SCHEMA_VERSION);
        assert!(!probe_exists);
    }

    #[test]
    fn source_verification_and_claim_review_are_separate_fields() {
        let connection = initialized_connection();
        let source_columns: Vec<String> = connection
            .prepare("PRAGMA table_info(sources)")
            .expect("prepare source schema query")
            .query_map([], |row| row.get(1))
            .expect("query source columns")
            .collect::<Result<_, _>>()
            .expect("collect source columns");
        let claim_columns: Vec<String> = connection
            .prepare("PRAGMA table_info(claims)")
            .expect("prepare claim schema query")
            .query_map([], |row| row.get(1))
            .expect("query claim columns")
            .collect::<Result<_, _>>()
            .expect("collect claim columns");

        assert!(source_columns.contains(&"verification_status".to_string()));
        assert!(!source_columns.contains(&"review_status".to_string()));
        assert!(claim_columns.contains(&"review_status".to_string()));
        assert!(!claim_columns.contains(&"verification_status".to_string()));
    }

    #[test]
    fn newer_database_schema_is_rejected() {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .pragma_update(None, "user_version", LATEST_SCHEMA_VERSION + 1)
            .expect("set future schema version");

        let error = initialize(&mut connection).expect_err("future schema must be rejected");

        assert!(error.contains("newer than the supported version"));
    }

    #[test]
    fn record_history_cannot_be_updated_or_deleted() {
        let connection = initialized_connection();
        insert_record(&connection, "source-1", "source");
        connection
            .execute(
                "INSERT INTO sources (record_id, record_kind, source_kind, verification_status)
                 VALUES ('source-1', 'source', 'web_page', 'unverified')",
                [],
            )
            .expect("insert source");
        insert_record(&connection, "history-1", "history");
        connection
            .execute(
                "INSERT INTO record_history (
                    record_id, record_kind, subject_record_id, subject_record_kind,
                    event_kind, summary
                 ) VALUES (
                    'history-1', 'history', 'source-1', 'source', 'record_created', 'Created'
                 )",
                [],
            )
            .expect("insert history event");

        assert!(
            connection
                .execute(
                    "UPDATE record_history SET summary = 'Changed' WHERE record_id = 'history-1'",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "UPDATE records SET created_by_name = 'Changed' WHERE record_id = 'history-1'",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "DELETE FROM record_history WHERE record_id = 'history-1'",
                    []
                )
                .is_err()
        );
    }

    #[test]
    fn source_creation_and_retrieval_preserve_fields_and_provenance() {
        let database = test_database();
        let created = database
            .create_source(sample_source_input())
            .expect("create source");
        let retrieved = database
            .get_source(&created.id)
            .expect("retrieve source")
            .expect("source exists");

        assert_eq!(retrieved, created);
        assert_eq!(retrieved.created_by.kind, ActorKind::Researcher);
        assert!(retrieved.created_at.ends_with('Z'));
        assert_eq!(record_count(&database, "source"), 1);
        assert_eq!(record_count(&database, "history"), 1);
    }

    #[test]
    fn claim_creation_and_retrieval_preserve_locator_confidence_and_attribution() {
        let database = test_database();
        let source = database
            .create_source(sample_source_input())
            .expect("create source");
        let created = database
            .create_claim(sample_claim_input(source.id.clone()))
            .expect("create claim");
        let retrieved = database
            .get_claim(&created.id)
            .expect("retrieve claim")
            .expect("claim exists");

        assert_eq!(retrieved, created);
        assert_eq!(retrieved.source_id, source.id);
        assert_eq!(
            retrieved.source_locator,
            SourceLocator::Section {
                section: "Methods".to_string(),
                paragraph: Some(2),
            }
        );
        assert_eq!(
            retrieved
                .author_stated_confidence
                .expect("confidence stored")
                .value_as_stated,
            "High confidence"
        );
        assert_eq!(retrieved.review.status, ClaimReviewStatus::Unreviewed);
        assert_eq!(record_count(&database, "claim"), 1);
        assert_eq!(record_count(&database, "history"), 2);
    }

    #[test]
    fn claim_rejects_missing_sources_and_invalid_locators_without_writing() {
        let database = test_database();
        let missing_source_id = Uuid::new_v4().to_string();
        let error = database
            .create_claim(sample_claim_input(missing_source_id))
            .expect_err("missing source must be rejected");

        assert!(error.contains("does not exist"));
        assert_eq!(record_count(&database, "claim"), 0);
        assert_eq!(record_count(&database, "history"), 0);

        let mut invalid_locator_input = sample_claim_input(Uuid::new_v4().to_string());
        invalid_locator_input.source_locator = SourceLocator::Page { page: 0 };
        let error = database
            .create_claim(invalid_locator_input)
            .expect_err("zero page locator must be rejected");

        assert!(error.contains("greater than zero"));
        assert_eq!(record_count(&database, "claim"), 0);

        let malformed_locator =
            serde_json::from_str::<SourceLocator>(r#"{"kind":"page","paragraph":3}"#);
        assert!(malformed_locator.is_err());
        assert!(
            SourceLocator::Section {
                section: "  ".to_string(),
                paragraph: None,
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn failed_creation_history_rolls_back_source_and_claim_records() {
        let database = test_database();
        let source = database
            .create_source(sample_source_input())
            .expect("create source");
        database
            .connection()
            .expect("lock database")
            .execute_batch(
                "CREATE TRIGGER reject_creation_history
                 BEFORE INSERT ON record_history
                 BEGIN
                     SELECT RAISE(ABORT, 'test history failure');
                 END;",
            )
            .expect("install failing test trigger");

        assert!(database.create_source(sample_source_input()).is_err());
        assert_eq!(record_count(&database, "source"), 1);

        let claims_before = record_count(&database, "claim");
        assert!(
            database
                .create_claim(sample_claim_input(source.id))
                .is_err()
        );
        assert_eq!(record_count(&database, "claim"), claims_before);
        assert_eq!(record_count(&database, "history"), 1);
    }

    #[test]
    fn created_sources_and_claims_persist_after_database_reopen() {
        let path: PathBuf = std::env::temp_dir().join(format!(
            "xenoscope-record-persistence-{}.sqlite3",
            Uuid::new_v4()
        ));
        let (source, claim) = {
            let database = Database::open(&path).expect("open temporary database");
            let source = database
                .create_source(sample_source_input())
                .expect("create persistent source");
            let claim = database
                .create_claim(sample_claim_input(source.id.clone()))
                .expect("create persistent claim");
            (source, claim)
        };

        let reopened = Database::open(&path).expect("reopen temporary database");
        assert_eq!(
            reopened
                .get_source(&source.id)
                .expect("retrieve persistent source")
                .expect("source exists after reopen"),
            source
        );
        assert_eq!(
            reopened
                .get_claim(&claim.id)
                .expect("retrieve persistent claim")
                .expect("claim exists after reopen"),
            claim
        );
        drop(reopened);
        fs::remove_file(path).expect("remove temporary database");
    }
}
