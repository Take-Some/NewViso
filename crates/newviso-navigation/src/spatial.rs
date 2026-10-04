use super::{BTreeMap, BTreeSet, Vec3};

// A broad phase only: exact 3D triangle distance and obstacle rules remain in
// search. A polygon belongs to every cell its XZ bounds touch, so large faces
// and stacked floors cannot disappear when a query is far from their centroid.
const CELL_SIZE: f32 = 16.0;
const MAX_CELLS: u64 = 256;
type Cell = (i32, i32);

#[derive(Clone, Debug, Default)]
pub(super) struct PolygonSpatialIndex {
    cells: BTreeMap<Cell, BTreeSet<u64>>,
    oversized: BTreeSet<u64>,
}

impl PolygonSpatialIndex {
    pub(super) fn insert(&mut self, id: u64, vertices: [Vec3; 3]) {
        if let Some(range) = polygon_cells(vertices) {
            for cell in range.cells() {
                self.cells.entry(cell).or_default().insert(id);
            }
        } else {
            self.oversized.insert(id);
        }
    }

    pub(super) fn remove(&mut self, id: u64, vertices: [Vec3; 3]) {
        self.oversized.remove(&id);
        if let Some(range) = polygon_cells(vertices) {
            for cell in range.cells() {
                if let Some(ids) = self.cells.get_mut(&cell) {
                    ids.remove(&id);
                    if ids.is_empty() {
                        self.cells.remove(&cell);
                    }
                }
            }
        }
    }

    // Repeated IDs across cells are harmless: selection uses (distance, id),
    // not visitation order. This avoids building an allocated set per query.
    // Very large query windows fall back to a complete scan in the caller.
    pub(super) fn candidates(
        &self,
        point: Vec3,
        radius: f32,
    ) -> Option<impl Iterator<Item = u64> + '_> {
        let range = CellRange::new(
            [point[0] - radius, point[2] - radius],
            [point[0] + radius, point[2] + radius],
        )?;
        Some(
            range
                .cells()
                .filter_map(|cell| self.cells.get(&cell))
                .flat_map(|ids| ids.iter().copied())
                .chain(self.oversized.iter().copied()),
        )
    }

    pub(super) fn cell_count(&self) -> usize {
        self.cells.len()
    }

    pub(super) fn oversized_count(&self) -> usize {
        self.oversized.len()
    }
}

struct CellRange {
    min: Cell,
    max: Cell,
}

impl CellRange {
    fn new(min: [f32; 2], max: [f32; 2]) -> Option<Self> {
        if min.iter().chain(max.iter()).any(|v| !v.is_finite()) {
            return None;
        }
        let cell = |v: f32| (v / CELL_SIZE).floor() as i32;
        let min = (cell(min[0]), cell(min[1]));
        let max = (cell(max[0]), cell(max[1]));
        let width = (i64::from(max.0) - i64::from(min.0) + 1) as u64;
        let depth = (i64::from(max.1) - i64::from(min.1) + 1) as u64;
        if width.checked_mul(depth)? > MAX_CELLS {
            return None;
        }
        Some(Self { min, max })
    }

    fn cells(self) -> impl Iterator<Item = Cell> {
        let Self { min, max } = self;
        (min.0..=max.0).flat_map(move |x| (min.1..=max.1).map(move |z| (x, z)))
    }
}

fn polygon_cells(vertices: [Vec3; 3]) -> Option<CellRange> {
    let min = [
        vertices.iter().map(|v| v[0]).fold(f32::INFINITY, f32::min),
        vertices.iter().map(|v| v[2]).fold(f32::INFINITY, f32::min),
    ];
    let max = [
        vertices
            .iter()
            .map(|v| v[0])
            .fold(f32::NEG_INFINITY, f32::max),
        vertices
            .iter()
            .map(|v| v[2])
            .fold(f32::NEG_INFINITY, f32::max),
    ];
    CellRange::new(min, max)
}
