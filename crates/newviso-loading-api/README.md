# newengine-loading-api

<!-- NORTHSTAR-DIR-README:BEGIN -->

## Directory purpose

**Path:** `NewEngine/neocore2/crates/newengine-loading-api`

**Role:** Engine repository directory. Keep files here scoped to this directory's domain and avoid cross-domain ownership drift.

**Local contents:** 1 direct subdirectories, 1 direct files.

**Direct file examples:** `Cargo.toml`

## Working rules

- Do not put transient build output in this directory unless the directory is explicitly a runtime output/cache location.
- Keep runtime assets and editable source assets separate: source assets are packed into runtime formats through explicit tools/manifests.
- Do not introduce hidden provider/backend coupling here; use declared descriptors, gateways, DTOs, and explicit maintenance scripts.

<!-- NORTHSTAR-DIR-README:END -->
