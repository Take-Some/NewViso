#[repr(C)]
#[derive(Debug, Clone, StableAbi)]
pub struct PluginDescriptorV2 {
    pub id: RString,
    pub name: RString,
    pub version: RString,
    pub kind: PluginKind,
    pub capabilities: RVec<CapabilityDescV2>,
    pub extension_json: RString,
}

impl PluginDescriptorV2 {
    /// Native V2 authoring entrypoint. First-party providers should build their
    /// discovery descriptor through this API instead of normalizing a V1 descriptor.
    #[inline]
    pub fn builder(
        id: impl Into<RString>,
        name: impl Into<RString>,
        version: impl Into<RString>,
        kind: PluginKind,
    ) -> PluginDescriptorV2Builder {
        PluginDescriptorV2Builder {
            id: id.into(),
            name: name.into(),
            version: version.into(),
            kind,
            capabilities: RVec::new(),
            extension_json: RString::new(),
        }
    }

    /// Compatibility-only V1 -> V2 normalization. Production first-party plugins
    /// must export a descriptor authored natively as V2.
    pub fn from_legacy(descriptor: &PluginDescriptor) -> Self {
        Self {
            id: descriptor.id.clone(),
            name: descriptor.name.clone(),
            version: descriptor.version.clone(),
            kind: descriptor.kind,
            capabilities: descriptor
                .capabilities
                .iter()
                .map(CapabilityDescV2::from_legacy)
                .collect::<Vec<_>>()
                .into(),
            extension_json: RString::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PluginDescriptorV2Builder {
    id: RString,
    name: RString,
    version: RString,
    kind: PluginKind,
    capabilities: RVec<CapabilityDescV2>,
    extension_json: RString,
}

impl PluginDescriptorV2Builder {
    #[inline]
    pub fn push(mut self, capability: CapabilityDescV2) -> Self {
        self.capabilities.push(capability);
        self
    }

    #[inline]
    pub fn provides_service(
        self,
        id: impl Into<CapabilityId>,
        version: u32,
        extension_json: impl Into<RString>,
    ) -> Self {
        self.push(
            CapabilityDescV2::new(
                id,
                CapabilityRole::Provides,
                CapabilityKind::ServiceV1,
                version,
            )
            .with_extension_json(extension_json),
        )
    }

    #[inline]
    pub fn requires_service(
        self,
        id: impl Into<CapabilityId>,
        version: u32,
        extension_json: impl Into<RString>,
    ) -> Self {
        self.push(
            CapabilityDescV2::new(
                id,
                CapabilityRole::Requires,
                CapabilityKind::ServiceV1,
                version,
            )
            .with_extension_json(extension_json),
        )
    }

    #[inline]
    pub fn provides_events(
        self,
        id: impl Into<CapabilityId>,
        version: u32,
        extension_json: impl Into<RString>,
    ) -> Self {
        self.push(
            CapabilityDescV2::new(
                id,
                CapabilityRole::Provides,
                CapabilityKind::EventsV1,
                version,
            )
            .with_extension_json(extension_json),
        )
    }

    #[inline]
    pub fn provides_asset_importer(
        self,
        id: impl Into<CapabilityId>,
        version: u32,
        extension_json: impl Into<RString>,
    ) -> Self {
        self.push(
            CapabilityDescV2::new(
                id,
                CapabilityRole::Provides,
                CapabilityKind::AssetImporterV1,
                version,
            )
            .with_extension_json(extension_json),
        )
    }

    #[inline]
    pub fn provides_scene_contribution(
        self,
        id: impl Into<CapabilityId>,
        version: u32,
        extension_json: impl Into<RString>,
    ) -> Self {
        self.push(
            CapabilityDescV2::new(
                id,
                CapabilityRole::Provides,
                CapabilityKind::SceneContributionV1,
                version,
            )
            .with_extension_json(extension_json),
        )
    }

    #[inline]
    pub fn requires_scene_contribution(
        self,
        id: impl Into<CapabilityId>,
        version: u32,
        extension_json: impl Into<RString>,
    ) -> Self {
        self.push(
            CapabilityDescV2::new(
                id,
                CapabilityRole::Requires,
                CapabilityKind::SceneContributionV1,
                version,
            )
            .with_extension_json(extension_json),
        )
    }

    #[inline]
    pub fn with_extension_json(mut self, extension_json: impl Into<RString>) -> Self {
        self.extension_json = extension_json.into();
        self
    }

    #[inline]
    pub fn build(self) -> PluginDescriptorV2 {
        PluginDescriptorV2 {
            id: self.id,
            name: self.name,
            version: self.version,
            kind: self.kind,
            capabilities: self.capabilities,
            extension_json: self.extension_json,
        }
    }
}
