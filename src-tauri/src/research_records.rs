use crate::database::Database;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    Researcher,
    System,
    Ai,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecordActor {
    pub kind: ActorKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceLocator {
    Page {
        page: u32,
    },
    Section {
        section: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        paragraph: Option<u32>,
    },
    Timecode {
        start: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        end: Option<String>,
    },
    TextFragment {
        text: String,
    },
    Other {
        description: String,
    },
}

impl SourceLocator {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Page { .. } => "page",
            Self::Section { .. } => "section",
            Self::Timecode { .. } => "timecode",
            Self::TextFragment { .. } => "text_fragment",
            Self::Other { .. } => "other",
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Page { page } if *page == 0 => {
                Err("Page locator must use a page number greater than zero".to_string())
            }
            Self::Section { section, paragraph } => {
                require_nonblank("Section locator section", section)?;
                if paragraph == &Some(0) {
                    return Err("Section locator paragraph must be greater than zero".to_string());
                }
                Ok(())
            }
            Self::Timecode { start, end } => {
                require_nonblank("Timecode locator start", start)?;
                if let Some(end) = end {
                    require_nonblank("Timecode locator end", end)?;
                }
                Ok(())
            }
            Self::TextFragment { text } => require_nonblank("Text-fragment locator text", text),
            Self::Other { description } => {
                require_nonblank("Other locator description", description)
            }
            Self::Page { .. } => Ok(()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    WebPage,
    Publication,
    OfficialRecord,
    Dataset,
    FirsthandAccount,
    Archive,
    Other,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceVerificationStatus {
    NotAssessed,
    Unverified,
    Verified,
    NeedsReview,
}

impl Default for SourceVerificationStatus {
    fn default() -> Self {
        Self::NotAssessed
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    pub created_at: String,
    pub created_by: RecordActor,
    pub source_kind: SourceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publication_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other_locator: Option<String>,
    pub verification_status: SourceVerificationStatus,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateSourceInput {
    pub source_kind: SourceKind,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub publisher: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub publication_date: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub other_locator: Option<String>,
    #[serde(default)]
    pub verification_status: SourceVerificationStatus,
}

impl CreateSourceInput {
    pub fn validate(&self) -> Result<(), String> {
        for (field, value) in [
            ("title", &self.title),
            ("publisher", &self.publisher),
            ("author", &self.author),
            ("publication date", &self.publication_date),
            ("URL", &self.url),
            ("other locator", &self.other_locator),
        ] {
            if let Some(value) = value {
                require_nonblank(field, value)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClaimCategory {
    Observation,
    Identification,
    Timing,
    Location,
    Context,
    Other,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClaimReviewStatus {
    Unreviewed,
    InReview,
    Reviewed,
    NeedsRevision,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewMetadata {
    pub status: ClaimReviewStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewed_by: Option<RecordActor>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthorStatedConfidence {
    pub stated_by: String,
    pub value_as_stated: String,
    pub source_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locator: Option<SourceLocator>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Claim {
    pub id: String,
    pub created_at: String,
    pub created_by: RecordActor,
    pub assertion_text: String,
    pub category: ClaimCategory,
    pub source_id: String,
    pub source_locator: SourceLocator,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incident_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_stated_confidence: Option<AuthorStatedConfidence>,
    pub review: ReviewMetadata,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateClaimInput {
    pub assertion_text: String,
    pub category: ClaimCategory,
    pub source_id: String,
    pub source_locator: SourceLocator,
    #[serde(default)]
    pub incident_id: Option<String>,
    #[serde(default)]
    pub author_stated_confidence: Option<AuthorStatedConfidence>,
}

impl CreateClaimInput {
    pub fn validate(&self) -> Result<(), String> {
        require_nonblank("Claim assertion text", &self.assertion_text)?;
        self.source_locator.validate()?;

        if let Some(confidence) = &self.author_stated_confidence {
            require_nonblank(
                "Author-stated confidence attribution",
                &confidence.stated_by,
            )?;
            require_nonblank(
                "Author-stated confidence value",
                &confidence.value_as_stated,
            )?;
            if let Some(locator) = &confidence.locator {
                locator.validate()?;
            }
        }

        Ok(())
    }
}

#[tauri::command]
pub fn create_source(
    database: State<'_, Database>,
    input: CreateSourceInput,
) -> Result<Source, String> {
    database.create_source(input)
}

#[tauri::command]
pub fn get_source(database: State<'_, Database>, id: String) -> Result<Option<Source>, String> {
    database.get_source(&id)
}

#[tauri::command]
pub fn create_claim(
    database: State<'_, Database>,
    input: CreateClaimInput,
) -> Result<Claim, String> {
    database.create_claim(input)
}

#[tauri::command]
pub fn get_claim(database: State<'_, Database>, id: String) -> Result<Option<Claim>, String> {
    database.get_claim(&id)
}

pub(crate) fn require_nonblank(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field} must not be empty"))
    } else {
        Ok(())
    }
}
