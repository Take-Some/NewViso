# newengine-ui-draw

UI draw-list structures used by render extraction.

## Architecture notes

This crate is part of the North Star Engine host/plugin architecture. Runtime-facing code should prefer engine gateways and typed adapters over concrete provider implementation crates.

<!-- NORTHSTAR-DIR-README:BEGIN -->

## Directory purpose

**Path:** `NewEngine/neocore2/crates/newengine-ui-draw`

**Role:** User interface runtime assets or UI provider implementation data.

**Local contents:** 1 direct subdirectories, 2 direct files.

**Direct file examples:** `Cargo.toml`

## Working rules

- Do not put transient build output in this directory unless the directory is explicitly a runtime output/cache location.
- Keep runtime assets and editable source assets separate: source assets are packed into runtime formats through explicit tools/manifests.
- Do not introduce hidden provider/backend coupling here; use declared descriptors, gateways, DTOs, and explicit maintenance scripts.

<!-- NORTHSTAR-DIR-README:END -->
