use crate::RenderFeature;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RenderApiVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl RenderApiVersion {
    #[inline]
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl Default for RenderApiVersion {
    #[inline]
    fn default() -> Self {
        Self::new(2, 0, 0)
    }
}

impl RenderApiVersion {
    /// Render protocol compatibility is strict at the major boundary.
    /// Minor/patch differences may negotiate to the backend stable version,
    /// but a v1 client can never bind a v2 provider (and vice versa).
    #[inline]
    pub const fn is_major_compatible_with(self, backend: Self) -> bool {
        self.major == backend.major
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderCapabilityNegotiationRequest {
    pub preferred_version: RenderApiVersion,
    #[serde(default)]
    pub required_features: Vec<RenderFeature>,
    #[serde(default)]
    pub optional_features: Vec<RenderFeature>,
}

impl Default for RenderCapabilityNegotiationRequest {
    #[inline]
    fn default() -> Self {
        Self {
            preferred_version: RenderApiVersion::default(),
            required_features: Vec::new(),
            optional_features: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderProtocolNotice {
    pub code: String,
    pub message: String,
}

impl RenderProtocolNotice {
    #[inline]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderCapabilityNegotiationResponse {
    pub accepted_version: RenderApiVersion,
    pub backend_version: RenderApiVersion,
    pub ok: bool,
    /// Full backend-neutral capability snapshot accepted for this protocol session.
    /// Runtime consumers use this as the authority for limits and execution semantics
    /// instead of reconstructing behavior from a backend id or implementation name.
    #[serde(default)]
    pub capabilities: crate::RenderBackendCapabilities,
    pub enabled_features: Vec<RenderFeature>,
    pub missing_required_features: Vec<RenderFeature>,
    #[serde(default)]
    pub notices: Vec<RenderProtocolNotice>,
}

/// Builds the canonical renderer protocol negotiation result.
///
/// This function is shared by concrete and null providers so compatibility
/// semantics cannot drift across routes. `ok` is false on any major-version
/// mismatch even when every requested feature is otherwise available.
pub fn negotiate_render_capabilities(
    request: RenderCapabilityNegotiationRequest,
    backend_version: RenderApiVersion,
    capabilities: &crate::RenderBackendCapabilities,
) -> RenderCapabilityNegotiationResponse {
    let major_compatible = request
        .preferred_version
        .is_major_compatible_with(backend_version);
    let missing_required_features = request
        .required_features
        .iter()
        .copied()
        .filter(|feature| !capabilities.supports(*feature))
        .collect::<Vec<_>>();
    let enabled_features = if major_compatible {
        request
            .optional_features
            .iter()
            .chain(request.required_features.iter())
            .copied()
            .filter(|feature| capabilities.supports(*feature))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let mut notices = Vec::new();
    if !major_compatible {
        notices.push(RenderProtocolNotice::new(
            "render.protocol.major_mismatch",
            format!(
                "requested render protocol v{}.{}.{} is incompatible with backend v{}.{}.{}",
                request.preferred_version.major,
                request.preferred_version.minor,
                request.preferred_version.patch,
                backend_version.major,
                backend_version.minor,
                backend_version.patch,
            ),
        ));
    } else if request.preferred_version != backend_version {
        notices.push(RenderProtocolNotice::new(
            "render.protocol.version_selected",
            format!(
                "requested render protocol v{}.{}.{} negotiated to backend v{}.{}.{}",
                request.preferred_version.major,
                request.preferred_version.minor,
                request.preferred_version.patch,
                backend_version.major,
                backend_version.minor,
                backend_version.patch,
            ),
        ));
    }

    RenderCapabilityNegotiationResponse {
        accepted_version: backend_version,
        backend_version,
        ok: major_compatible && missing_required_features.is_empty(),
        capabilities: capabilities.clone(),
        enabled_features,
        missing_required_features,
        notices,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderProblemDetails {
    pub code: String,
    pub title: String,
    pub detail: String,
    pub backend: Option<String>,
    pub phase: Option<String>,
    #[serde(default)]
    pub recoverable: bool,
}

impl RenderProblemDetails {
    #[inline]
    pub fn new(
        code: impl Into<String>,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            title: title.into(),
            detail: detail.into(),
            backend: None,
            phase: None,
            recoverable: true,
        }
    }

    #[inline]
    pub fn with_backend(mut self, backend: impl Into<String>) -> Self {
        self.backend = Some(backend.into());
        self
    }

    #[inline]
    pub fn with_phase(mut self, phase: impl Into<String>) -> Self {
        self.phase = Some(phase.into());
        self
    }

    #[inline]
    pub fn fatal(mut self) -> Self {
        self.recoverable = false;
        self
    }
}

#[cfg(test)]
mod negotiation_tests {
    use super::*;
    use crate::{RenderBackendCapabilities, RenderFeature};

    #[test]
    fn protocol_v1_client_is_rejected_by_v2_backend() {
        let response = negotiate_render_capabilities(
            RenderCapabilityNegotiationRequest {
                preferred_version: RenderApiVersion::new(1, 9, 9),
                required_features: Vec::new(),
                optional_features: vec![RenderFeature::UiComposite],
            },
            RenderApiVersion::default(),
            &RenderBackendCapabilities::raster_default(),
        );
        assert!(!response.ok);
        assert_eq!(response.accepted_version, RenderApiVersion::new(2, 0, 0));
        assert!(response.enabled_features.is_empty());
        assert!(response
            .notices
            .iter()
            .any(|notice| notice.code == "render.protocol.major_mismatch"));
    }

    #[test]
    fn protocol_v2_client_is_accepted_when_required_features_exist() {
        let response = negotiate_render_capabilities(
            RenderCapabilityNegotiationRequest {
                preferred_version: RenderApiVersion::new(2, 0, 0),
                required_features: vec![RenderFeature::UiComposite],
                optional_features: Vec::new(),
            },
            RenderApiVersion::default(),
            &RenderBackendCapabilities::raster_default(),
        );
        assert!(response.ok);
        assert_eq!(response.accepted_version, RenderApiVersion::new(2, 0, 0));
        assert_eq!(response.enabled_features, vec![RenderFeature::UiComposite]);
        assert!(response.notices.is_empty());
    }

    #[test]
    fn future_major_client_is_rejected_by_v2_backend() {
        let response = negotiate_render_capabilities(
            RenderCapabilityNegotiationRequest {
                preferred_version: RenderApiVersion::new(3, 0, 0),
                required_features: Vec::new(),
                optional_features: Vec::new(),
            },
            RenderApiVersion::default(),
            &RenderBackendCapabilities::raster_default(),
        );
        assert!(!response.ok);
        assert!(response
            .notices
            .iter()
            .any(|notice| notice.code == "render.protocol.major_mismatch"));
    }

    #[test]
    fn same_major_minor_difference_selects_backend_version() {
        let response = negotiate_render_capabilities(
            RenderCapabilityNegotiationRequest {
                preferred_version: RenderApiVersion::new(2, 7, 0),
                required_features: Vec::new(),
                optional_features: Vec::new(),
            },
            RenderApiVersion::default(),
            &RenderBackendCapabilities::raster_default(),
        );
        assert!(response.ok);
        assert_eq!(response.accepted_version, RenderApiVersion::new(2, 0, 0));
        assert!(response
            .notices
            .iter()
            .any(|notice| notice.code == "render.protocol.version_selected"));
    }
}
