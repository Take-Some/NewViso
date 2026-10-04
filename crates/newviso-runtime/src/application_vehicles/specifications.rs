use super::*;
use newviso_vehicle::VehicleSpecification;

const SPECIFICATIONS_ASSET: &str = "config/vehicles/specifications.xml";

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct VehicleSpecificationCatalog {
    schema: String,
    vehicles: Vec<VehicleSpecification>,
}

fn decode_vehicle_specifications(
    text: &str,
) -> Result<BTreeMap<String, VehicleSpecification>, String> {
    let value = bootstrap_support::parse_engine_config_xml(
        text.as_bytes(),
        Path::new(SPECIFICATIONS_ASSET),
    )?;
    let catalog: VehicleSpecificationCatalog = serde_json::from_value(value)
        .map_err(|e| format!("invalid vehicle specification catalog: {e}"))?;
    if catalog.schema != "newviso.vehicle.specifications.v1" {
        return Err(format!(
            "unsupported vehicle specification schema '{}'",
            catalog.schema
        ));
    }
    let mut entries = BTreeMap::new();
    for mut specification in catalog.vehicles {
        specification.validate()?;
        specification.model_name = specification.model_name.to_ascii_lowercase();
        if specification.model_name.is_empty() {
            return Err("vehicle specification model_name must not be empty".into());
        }
        if entries
            .insert(specification.model_name.clone(), specification)
            .is_some()
        {
            return Err("duplicate vehicle specification model_name".into());
        }
    }
    Ok(entries)
}

impl EngineApplication {
    fn ensure_vehicle_specifications(&mut self) -> Result<(), String> {
        if self.vehicle_specifications.is_some() {
            return Ok(());
        }
        let assets = AssetClient::new();
        let listing = assets.vfs_list("config/vehicles")?;
        let entries = listing
            .get("entries")
            .and_then(Value::as_array)
            .ok_or("vehicle specification directory has no VFS entries array")?;
        let present = entries.iter().any(|entry| {
            ["name", "path", "logical_path"].iter().any(|key| {
                entry.get(key).and_then(Value::as_str).is_some_and(|name| {
                    name.replace('\\', "/").rsplit('/').next() == Some("specifications.xml")
                })
            })
        });
        self.vehicle_specifications = Some(if present {
            decode_vehicle_specifications(&assets.text(SPECIFICATIONS_ASSET)?)?
        } else {
            BTreeMap::new()
        });
        Ok(())
    }

    pub(super) fn apply_vehicle_model_specification(
        &mut self,
        entity: u64,
        model: &str,
    ) -> Result<(), String> {
        if self.vehicle_explicit_specifications.contains(&entity) || !self.vehicles.contains(entity)
        {
            return Ok(());
        }
        self.ensure_vehicle_specifications()?;
        let model = model
            .rsplit(['/', '\\', '@'])
            .next()
            .unwrap_or(model)
            .to_ascii_lowercase();
        let model = model.strip_suffix("_hi").unwrap_or(&model);
        if let Some(specification) = self
            .vehicle_specifications
            .as_ref()
            .and_then(|catalog| catalog.get(model))
            .cloned()
        {
            self.vehicles.set_specification(entity, specification)?;
        }
        Ok(())
    }

    pub(crate) fn set_vehicle_specification_from_script(
        &mut self,
        command: &Value,
        _index: usize,
    ) -> Result<(), String> {
        let entity = command
            .get("entity")
            .and_then(Value::as_u64)
            .ok_or("vehicle specification requires resolved entity")?;
        let specification: VehicleSpecification = serde_json::from_value(
            command
                .get("specification")
                .cloned()
                .ok_or("vehicle.specification.set requires specification object")?,
        )
        .map_err(|e| e.to_string())?;
        self.vehicles.set_specification(entity, specification)?;
        self.vehicle_explicit_specifications.insert(entity);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_profiles_decode_native_capabilities_and_preserve_source_flags() {
        let profiles = decode_vehicle_specifications(r#"<vehicle_specifications schema="newviso.vehicle.specifications.v1"><vehicles type="array"><item model_name="TEST" source_model_flags="33554432" powertrain="electric"><damage tyres_can_burst="false" oil_leaks="disabled"/><controls handbrake="false"/><source_flags type="array"><item value="FLAG_IS_ELECTRIC"/></source_flags></item></vehicles></vehicle_specifications>"#).unwrap();
        let profile = &profiles["test"];
        assert!(!profile.damage.tyres_can_burst);
        assert!(!profile.controls.handbrake);
        assert!(!profile.uses_combustion(VehicleClass::Automobile));
        assert_eq!(profile.source_model_flags, 0x0200_0000);
        assert_eq!(profile.source_flags, ["FLAG_IS_ELECTRIC"]);
    }

    #[test]
    fn invalid_xml_profiles_fail_instead_of_discarding_fields() {
        let wrap = |items: &str| {
            format!(
                "<vehicle_specifications schema=\"newviso.vehicle.specifications.v1\"><vehicles type=\"array\">{items}</vehicles></vehicle_specifications>"
            )
        };
        for items in [
            "<item model_name=\"a\"/><item model_name=\"A\"/>",
            "<item model_name=\"a\"><damage typo=\"true\"/></item>",
            "<item model_name=\"a\"><damage body_damage_scale=\"-1\"/></item>",
        ] {
            assert!(decode_vehicle_specifications(&wrap(items)).is_err());
        }
    }
}
