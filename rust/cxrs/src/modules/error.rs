use std::fmt;
use std::path::PathBuf;

pub type CxResult<T> = Result<T, CxError>;

#[derive(Debug)]
pub enum CxError {
    Io {
        context: String,
        source: std::io::Error,
    },
    JsonParse {
        context: String,
        source: serde_json::Error,
    },
    JsonLineParse {
        file: PathBuf,
        line: usize,
        content_preview: String,
        source: serde_json::Error,
    },
    InvalidData {
        context: String,
    },
}

impl CxError {
    pub fn invalid(context: impl Into<String>) -> Self {
        CxError::InvalidData {
            context: context.into(),
        }
    }

    pub fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        CxError::Io {
            context: context.into(),
            source,
        }
    }

    pub fn json(context: impl Into<String>, source: serde_json::Error) -> Self {
        CxError::JsonParse {
            context: context.into(),
            source,
        }
    }
}

impl fmt::Display for CxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CxError::Io { context, source } => write!(f, "{context}: {source}"),
            CxError::JsonParse { context, source } => write!(f, "{context}: {source}"),
            CxError::JsonLineParse {
                file,
                line,
                content_preview,
                source,
            } => write!(
                f,
                "failed to parse json line {} in {} (preview='{}'): {}",
                line,
                file.display(),
                content_preview,
                source
            ),
            CxError::InvalidData { context } => write!(f, "{context}"),
        }
    }
}

impl std::error::Error for CxError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CxError::Io { source, .. } => Some(source),
            CxError::JsonParse { source, .. } => Some(source),
            CxError::JsonLineParse { source, .. } => Some(source),
            CxError::InvalidData { .. } => None,
        }
    }
}
