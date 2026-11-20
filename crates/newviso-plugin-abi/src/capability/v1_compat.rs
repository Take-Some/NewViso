#[repr(C)]
#[derive(Debug, Clone, StableAbi)]
pub struct CapabilityDesc {
    pub id: CapabilityId,
    pub role: CapabilityRole,
    pub kind: CapabilityKind,
    pub version: u32,
    pub describe_json: RString,
}

impl CapabilityDesc {
    #[inline]
    pub fn new(
        id: impl Into<CapabilityId>,
        role: CapabilityRole,
        kind: CapabilityKind,
        version: u32,
    ) -> Self {
        Self {
            id: id.into(),
            role,
            kind,
            version,
            describe_json: RString::new(),
        }
    }

    #[inline]
    pub fn with_json(mut self, json: impl Into<RString>) -> Self {
        self.describe_json = json.into();
        self
    }

    #[inline]
    pub fn backend_route(id: impl Into<CapabilityId>, descriptor: BackendRouteDescriptor) -> Self {
        Self::new(id, CapabilityRole::Provides, CapabilityKind::Other, 1)
            .with_json(descriptor.to_json_string())
    }

    #[inline]
    pub fn with_backend_route(mut self, descriptor: BackendRouteDescriptor) -> Self {
        self.describe_json = RString::from(descriptor.to_json_string());
        self
    }

    #[inline]
    pub fn to_v2_compat(&self) -> CapabilityDescV2 {
        CapabilityDescV2::from_legacy(self)
    }

    #[inline]
    pub fn has_tag(&self, tag: &str) -> bool {
        capability_has_tag(self, tag)
    }
}

#[repr(C)]
#[derive(Debug, Clone, StableAbi)]
pub struct PluginDescriptor {
    pub id: RString,
    pub name: RString,
    pub version: RString,
    pub kind: PluginKind,
    pub capabilities: RVec<CapabilityDesc>,
}

impl PluginDescriptor {
    #[inline]
    pub fn builder(
        id: impl Into<RString>,
        name: impl Into<RString>,
        version: impl Into<RString>,
        kind: PluginKind,
    ) -> PluginDescriptorBuilder {
        PluginDescriptorBuilder {
            id: id.into(),
            name: name.into(),
            version: version.into(),
            kind,
            capabilities: RVec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PluginDescriptorBuilder {
    id: RString,
    name: RString,
    version: RString,
    kind: PluginKind,
    capabilities: RVec<CapabilityDesc>,
}

impl PluginDescriptorBuilder {
    #[inline]
    pub fn push(mut self, cap: CapabilityDesc) -> Self {
        self.capabilities.push(cap);
        self
    }

    #[inline]
    pub fn provides_service(
        self,
        id: impl Into<CapabilityId>,
        version: u32,
        describe_json: impl Into<RString>,
    ) -> Self {
        self.push(
            CapabilityDesc::new(
                id,
                CapabilityRole::Provides,
                CapabilityKind::ServiceV1,
                version,
            )
            .with_json(describe_json),
        )
    }

    #[inline]
    pub fn requires_service(
        self,
        id: impl Into<CapabilityId>,
        version: u32,
        describe_json: impl Into<RString>,
    ) -> Self {
        self.push(
            CapabilityDesc::new(
                id,
                CapabilityRole::Requires,
                CapabilityKind::ServiceV1,
                version,
            )
            .with_json(describe_json),
        )
    }

    #[inline]
    pub fn provides_events(
        self,
        id: impl Into<CapabilityId>,
        version: u32,
        describe_json: impl Into<RString>,
    ) -> Self {
        self.push(
            CapabilityDesc::new(
                id,
                CapabilityRole::Provides,
                CapabilityKind::EventsV1,
                version,
            )
            .with_json(describe_json),
        )
    }

    #[inline]
    pub fn provides_asset_importer(
        self,
        id: impl Into<CapabilityId>,
        version: u32,
        describe_json: impl Into<RString>,
    ) -> Self {
        self.push(
            CapabilityDesc::new(
                id,
                CapabilityRole::Provides,
                CapabilityKind::AssetImporterV1,
                version,
            )
            .with_json(describe_json),
        )
    }

    #[inline]
    pub fn provides_scene_contribution(
        self,
        id: impl Into<CapabilityId>,
        version: u32,
        describe_json: impl Into<RString>,
    ) -> Self {
        self.push(
            CapabilityDesc::new(
                id,
                CapabilityRole::Provides,
                CapabilityKind::SceneContributionV1,
                version,
            )
            .with_json(describe_json),
        )
    }

    #[inline]
    pub fn requires_scene_contribution(
        self,
        id: impl Into<CapabilityId>,
        version: u32,
        describe_json: impl Into<RString>,
    ) -> Self {
        self.push(
            CapabilityDesc::new(
                id,
                CapabilityRole::Requires,
                CapabilityKind::SceneContributionV1,
                version,
            )
            .with_json(describe_json),
        )
    }

    #[inline]
    pub fn build(self) -> PluginDescriptor {
        PluginDescriptor {
            id: self.id,
            name: self.name,
            version: self.version,
            kind: self.kind,
            capabilities: self.capabilities,
        }
    }
}
