use super::*;

fn access_actor(
    application: &EngineApplication,
    command: &Value,
    index: usize,
    op: &str,
) -> Result<u64, String> {
    command
        .get("actor_entity")
        .and_then(Value::as_u64)
        .or_else(|| {
            command
                .get("actor_id")
                .and_then(Value::as_str)
                .and_then(|id| application.scene.runtime_entity_stable_id(id))
        })
        .ok_or_else(|| {
            format!("script command[{index}] {op} requires actor_entity or existing actor_id")
        })
}

fn requested_names(command: &Value, singular: &str, plural: &str) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    if let Some(value) = command.get(singular) {
        if !value.is_null() {
            let name = value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("{singular} must be a non-empty string or null"))?;
            names.push(name.to_owned());
        }
    }
    if let Some(values) = command.get(plural) {
        let values = values
            .as_array()
            .ok_or_else(|| format!("{plural} must be an array of strings"))?;
        for value in values {
            let name = value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("{plural} entries must be non-empty strings"))?;
            if !names
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(name))
            {
                names.push(name.to_owned());
            }
        }
    }
    Ok(names)
}

fn canonical_access_part(
    binding: &VehiclePresentationBinding,
    requested: &str,
    role: ModelFragmentPartRole,
) -> Option<String> {
    binding
        .parts
        .iter()
        .find(|part| {
            part.role == role
                && part.name.eq_ignore_ascii_case(requested)
                && part.visible
                && part.detached_entity.is_none()
        })
        .map(|part| part.name.clone())
}

impl EngineApplication {
    pub(crate) fn set_vehicle_access_layout_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        let entity = command
            .get("entity")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                format!(
                    "script command[{index}] vehicle.access.layout.set requires resolved entity"
                )
            })?;
        let name = command
            .get("layout")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let driver_requested = command
            .get("driver_seat")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!("script command[{index}] vehicle.access.layout.set requires driver_seat")
            })?;
        let seats = command
            .get("seats")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                format!("script command[{index}] vehicle.access.layout.set requires object seats")
            })?;

        let binding = self
            .vehicle_presentations
            .get_mut(&entity)
            .ok_or_else(|| format!("vehicle entity {entity} has no fragment presentation"))?;
        let driver_seat =
            canonical_access_part(binding, driver_requested, ModelFragmentPartRole::Seat)
                .ok_or_else(|| {
                    format!("vehicle driver seat '{driver_requested}' is unavailable")
                })?;

        let mut links = BTreeMap::new();
        for (requested, raw) in seats {
            let Some(seat) = canonical_access_part(binding, requested, ModelFragmentPartRole::Seat)
            else {
                // Metadata layouts can describe optional seats that a specific
                // fragment variant does not materialize. Preserve only live bones.
                continue;
            };
            let resolve_link = |key: &str| -> Option<String> {
                raw.get(key).and_then(Value::as_str).and_then(|requested| {
                    canonical_access_part(binding, requested, ModelFragmentPartRole::Seat)
                })
            };
            links.insert(
                seat,
                VehicleSeatAccessLinks {
                    shuffle: resolve_link("shuffle"),
                    rear: resolve_link("rear"),
                },
            );
        }
        if !links.contains_key(&driver_seat) {
            links.insert(driver_seat.clone(), VehicleSeatAccessLinks::default());
        }
        binding.access_layout = VehicleAccessLayoutState {
            name,
            driver_seat: Some(driver_seat),
            seats: links,
        };
        Ok(())
    }

    pub(crate) fn reserve_vehicle_access_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        let entity = command
            .get("entity")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                format!("script command[{index}] vehicle.access.reserve requires resolved entity")
            })?;
        let actor = access_actor(self, command, index, "vehicle.access.reserve")?;
        let requested_seats = requested_names(command, "seat", "seats")?;
        let requested_doors = requested_names(command, "door", "doors")?;
        if requested_seats.is_empty() {
            return Err(format!(
                "script command[{index}] vehicle.access.reserve requires seat or seats"
            ));
        }

        let binding = self
            .vehicle_presentations
            .get_mut(&entity)
            .ok_or_else(|| format!("vehicle entity {entity} has no fragment presentation"))?;

        let seats = requested_seats
            .iter()
            .map(|requested| {
                canonical_access_part(binding, requested, ModelFragmentPartRole::Seat)
                    .ok_or_else(|| format!("vehicle seat '{requested}' is unavailable"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let doors = requested_doors
            .iter()
            .map(|requested| {
                canonical_access_part(binding, requested, ModelFragmentPartRole::Door)
                    .ok_or_else(|| format!("vehicle door '{requested}' is unavailable"))
            })
            .collect::<Result<Vec<_>, _>>()?;

        for seat in &seats {
            if binding
                .occupants
                .get(seat)
                .is_some_and(|occupant| occupant.entity != actor)
            {
                return Err(format!("vehicle seat '{seat}' is already occupied"));
            }
            if binding
                .seat_reservations
                .get(seat)
                .is_some_and(|owner| *owner != actor)
            {
                return Err(format!(
                    "vehicle seat '{seat}' is reserved by another actor"
                ));
            }
        }
        for door in &doors {
            if binding
                .door_reservations
                .get(door)
                .is_some_and(|owner| *owner != actor)
            {
                return Err(format!(
                    "vehicle door '{door}' is reserved by another actor"
                ));
            }
        }

        // One access task owns one coherent component set. Re-picking an entry
        // atomically replaces only this actor's previous claims.
        binding.seat_reservations.retain(|_, owner| *owner != actor);
        binding.door_reservations.retain(|_, owner| *owner != actor);
        for seat in seats {
            binding.seat_reservations.insert(seat, actor);
        }
        for door in doors {
            binding.door_reservations.insert(door, actor);
        }
        Ok(())
    }

    pub(crate) fn release_vehicle_access_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        let entity = command
            .get("entity")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                format!("script command[{index}] vehicle.access.release requires resolved entity")
            })?;
        let actor = access_actor(self, command, index, "vehicle.access.release")?;
        let requested_seats = requested_names(command, "seat", "seats")?;
        let requested_doors = requested_names(command, "door", "doors")?;
        let Some(binding) = self.vehicle_presentations.get_mut(&entity) else {
            return Ok(());
        };

        if requested_seats.is_empty() {
            binding.seat_reservations.retain(|_, owner| *owner != actor);
        } else {
            binding.seat_reservations.retain(|name, owner| {
                *owner != actor
                    || !requested_seats
                        .iter()
                        .any(|requested| name.eq_ignore_ascii_case(requested))
            });
        }

        if requested_doors.is_empty() {
            binding.door_reservations.retain(|_, owner| *owner != actor);
        } else {
            binding.door_reservations.retain(|name, owner| {
                *owner != actor
                    || !requested_doors
                        .iter()
                        .any(|requested| name.eq_ignore_ascii_case(requested))
            });
        }
        Ok(())
    }
}
