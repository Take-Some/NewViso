#![forbid(unsafe_op_in_unsafe_fn)]

use serde_json::{json, Value};

/// Stable, typed AssetManager error class.
///
/// The ABI still transports service errors through `RString`, but the payload is
/// no longer a free-form sentence. Providers should use `encode_asset_error_wire`,
/// clients should use `decode_asset_error_wire` / `AssetError::from_wire_or_message`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetErrorKind {
    NotReady,
    NotFound,
    DecodeFailed,
    UnsupportedFormat,
    Io,
    InvalidRequest,
    ServiceUnavailable,
    Internal,
}

impl AssetErrorKind {
    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotReady => "not_ready",
            Self::NotFound => "not_found",
            Self::DecodeFailed => "decode_failed",
            Self::UnsupportedFormat => "unsupported_format",
            Self::Io => "io",
            Self::InvalidRequest => "invalid_request",
            Self::ServiceUnavailable => "service_unavailable",
            Self::Internal => "internal",
        }
    }

    #[inline]
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "not_ready" | "notready" | "asset_not_ready" => Some(Self::NotReady),
            "not_found" | "notfound" | "missing" => Some(Self::NotFound),
            "decode_failed" | "decode" | "decodefailed" => Some(Self::DecodeFailed),
            "unsupported_format" | "unsupported" | "unsupportedformat" => {
                Some(Self::UnsupportedFormat)
            }
            "io" | "i/o" => Some(Self::Io),
            "invalid_request" | "bad_request" | "invalid" => Some(Self::InvalidRequest),
            "service_unavailable" | "unavailable" => Some(Self::ServiceUnavailable),
            "internal" | "unknown" => Some(Self::Internal),
            _ => None,
        }
    }
}

impl core::fmt::Display for AssetErrorKind {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Engine-facing typed asset error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetError {
    pub kind: AssetErrorKind,
    pub message: String,
    pub logical_path: Option<String>,
    pub id_hex32: Option<String>,
    pub detail: Option<String>,
}

impl AssetError {
    #[inline]
    pub fn new(kind: AssetErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            logical_path: None,
            id_hex32: None,
            detail: None,
        }
    }

    #[inline]
    pub fn not_ready(message: impl Into<String>) -> Self {
        Self::new(AssetErrorKind::NotReady, message)
    }

    #[inline]
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(AssetErrorKind::NotFound, message)
    }

    #[inline]
    pub fn decode_failed(message: impl Into<String>) -> Self {
        Self::new(AssetErrorKind::DecodeFailed, message)
    }

    #[inline]
    pub fn unsupported_format(message: impl Into<String>) -> Self {
        Self::new(AssetErrorKind::UnsupportedFormat, message)
    }

    #[inline]
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(AssetErrorKind::InvalidRequest, message)
    }

    #[inline]
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(AssetErrorKind::Internal, message)
    }

    #[inline]
    pub fn with_logical_path(mut self, logical_path: impl Into<String>) -> Self {
        let logical_path = logical_path.into();
        if !logical_path.trim().is_empty() {
            self.logical_path = Some(logical_path);
        }
        self
    }

    #[inline]
    pub fn with_id_hex32(mut self, id_hex32: impl Into<String>) -> Self {
        let id_hex32 = id_hex32.into();
        if id_hex32.len() == 32 && id_hex32.chars().all(|c| c.is_ascii_hexdigit()) {
            self.id_hex32 = Some(id_hex32);
        }
        self
    }

    #[inline]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        if !detail.trim().is_empty() {
            self.detail = Some(detail);
        }
        self
    }

    #[inline]
    pub fn from_wire_or_message(message: impl Into<String>) -> Self {
        let message = message.into();
        decode_asset_error_wire(&message).unwrap_or_else(|| classify_text_asset_error(message))
    }

    pub fn to_json_value(&self) -> Value {
        let mut value = json!({
            "kind": self.kind.as_str(),
            "message": self.message.clone(),
        });
        if let Some(path) = &self.logical_path {
            value["logical_path"] = Value::String(path.clone());
        }
        if let Some(id) = &self.id_hex32 {
            value["id_hex32"] = Value::String(id.clone());
        }
        if let Some(detail) = &self.detail {
            value["detail"] = Value::String(detail.clone());
        }
        value
    }

    pub fn from_json_value(value: &Value) -> Option<Self> {
        let kind = value
            .get("kind")
            .and_then(Value::as_str)
            .and_then(AssetErrorKind::from_str)?;
        let message = value
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or(kind.as_str())
            .to_owned();
        let logical_path = value
            .get("logical_path")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .filter(|it| !it.trim().is_empty());
        let id_hex32 = value
            .get("id_hex32")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .filter(|it| it.len() == 32 && it.chars().all(|c| c.is_ascii_hexdigit()));
        let detail = value
            .get("detail")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .filter(|it| !it.trim().is_empty());
        Some(Self {
            kind,
            message,
            logical_path,
            id_hex32,
            detail,
        })
    }
}

impl core::fmt::Display for AssetError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match (&self.logical_path, &self.id_hex32, &self.detail) {
            (Some(path), Some(id), Some(detail)) => write!(
                f,
                "{}: {} path='{}' id={} detail='{}'",
                self.kind, self.message, path, id, detail
            ),
            (Some(path), Some(id), None) => write!(
                f,
                "{}: {} path='{}' id={}",
                self.kind, self.message, path, id
            ),
            (Some(path), None, Some(detail)) => write!(
                f,
                "{}: {} path='{}' detail='{}'",
                self.kind, self.message, path, detail
            ),
            (None, Some(id), Some(detail)) => write!(
                f,
                "{}: {} id={} detail='{}'",
                self.kind, self.message, id, detail
            ),
            (Some(path), None, None) => {
                write!(f, "{}: {} path='{}'", self.kind, self.message, path)
            }
            (None, Some(id), None) => write!(f, "{}: {} id={}", self.kind, self.message, id),
            (None, None, Some(detail)) => {
                write!(f, "{}: {} detail='{}'", self.kind, self.message, detail)
            }
            (None, None, None) => write!(f, "{}: {}", self.kind, self.message),
        }
    }
}

impl std::error::Error for AssetError {}

pub type AssetResult<T> = Result<T, AssetError>;

pub const ASSET_ERROR_WIRE_PREFIX: &str = "NEASSETERR1:";

#[inline]
pub fn encode_asset_error_wire(error: &AssetError) -> String {
    format!("{}{}", ASSET_ERROR_WIRE_PREFIX, error.to_json_value())
}

#[inline]
pub fn decode_asset_error_wire(value: &str) -> Option<AssetError> {
    let payload = value.strip_prefix(ASSET_ERROR_WIRE_PREFIX)?;
    let json = serde_json::from_str::<Value>(payload).ok()?;
    AssetError::from_json_value(&json)
}

pub fn classify_text_asset_error(message: impl Into<String>) -> AssetError {
    let message = message.into();
    let lower = message.to_ascii_lowercase();
    let kind = if lower.contains("not ready")
        || lower.contains("loading")
        || lower.contains("queued")
    {
        AssetErrorKind::NotReady
    } else if lower.contains("not found") || lower.contains("missing") || lower.contains("no such")
    {
        AssetErrorKind::NotFound
    } else if lower.contains("unsupported")
        || lower.contains("bad magic")
        || lower.contains("bad format")
    {
        AssetErrorKind::UnsupportedFormat
    } else if lower.contains("decode")
        || lower.contains("parse")
        || lower.contains("bad json")
        || lower.contains("non-utf8")
    {
        AssetErrorKind::DecodeFailed
    } else if lower.contains("io")
        || lower.contains("failed to read")
        || lower.contains("permission denied")
    {
        AssetErrorKind::Io
    } else if lower.contains("bad id") || lower.contains("empty path") || lower.contains("invalid")
    {
        AssetErrorKind::InvalidRequest
    } else {
        AssetErrorKind::Internal
    };
    AssetError::new(kind, message)
}
