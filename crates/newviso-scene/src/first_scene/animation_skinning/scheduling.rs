//! Bounded animation scheduling: select the best N jobs instead of sorting every
//! visible entity. The ordering remains identical to a full priority sort.
use std::cmp::Ordering;

pub(super) type Candidate = (f64, f32, u64, f64);

fn candidate_priority(a: &Candidate, b: &Candidate) -> Ordering {
    b.0.partial_cmp(&a.0)
        .unwrap_or(Ordering::Equal)
        .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal))
        .then_with(|| a.2.cmp(&b.2))
}

pub(super) fn select_best_candidates(candidates: &mut Vec<Candidate>, budget: usize) {
    if budget == 0 {
        candidates.clear();
        return;
    }
    if candidates.len() > budget {
        // The best budget entries are moved into the prefix in O(n) average
        // time, with no change to the deterministic urgency/distance/id key.
        candidates.select_nth_unstable_by(budget, candidate_priority);
        candidates.truncate(budget);
    }
    candidates.sort_unstable_by(candidate_priority);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_schedule_matches_full_sort() {
        let original: Vec<_> = (0..512)
            .map(|id| (
                ((id * 37) % 13) as f64 / 4.0,
                ((id * 61) % 97) as f32,
                id as u64,
                id as f64,
            ))
            .collect();
        let mut expected = original.clone();
        expected.sort_unstable_by(candidate_priority);
        for budget in [0, 1, 2, 8, 37, 512, 1024] {
            let mut actual = original.clone();
            select_best_candidates(&mut actual, budget);
            assert_eq!(actual, expected[..expected.len().min(budget)]);
        }
    }

    /// Run manually: cargo test -p newviso-scene benchmark_skinning --offline -- --ignored --nocapture
    /// This isolates the selection algorithm; it is not an end-to-end FPS benchmark.
    #[test]
    #[ignore]
    fn benchmark_skinning_selection_vs_full_sort() {
        use std::{hint::black_box, time::Instant};

        let candidates = (0..20_000u64).map(|id| (
            ((id * 37) % 211) as f64 / 16.0,
            ((id * 61) % 997) as f32,
            id,
            id as f64,
        )).collect::<Vec<_>>();
        let repetitions = 50;
        let full_started = Instant::now();
        for _ in 0..repetitions {
            let mut old = candidates.clone();
            old.sort_unstable_by(candidate_priority);
            black_box(&old[..8]);
        }
        let full_ms = full_started.elapsed().as_secs_f64() * 1000.0;

        let selected_started = Instant::now();
        for _ in 0..repetitions {
            let mut next = candidates.clone();
            select_best_candidates(&mut next, 8);
            black_box(&next);
        }
        let selected_ms = selected_started.elapsed().as_secs_f64() * 1000.0;
        println!("SKINNING_BENCH candidates=20000 budget=8 repetitions={} full_sort_ms={:.3} bounded_select_ms={:.3} relative_speedup={:.2}x",
            repetitions, full_ms, selected_ms, full_ms / selected_ms);
    }

    #[test]
    fn overdue_infinite_priority_and_equal_scores_remain_deterministic() {
        let original = vec![
            (f64::INFINITY, 4.0, 8, 0.0),
            (2.0, 4.0, 11, 0.0),
            (f64::INFINITY, 4.0, 2, 0.0),
            (f64::INFINITY, 3.0, 7, 0.0),
            (2.0, 4.0, 10, 0.0),
        ];
        let mut actual = original;
        select_best_candidates(&mut actual, 3);
        assert_eq!(actual.iter().map(|x| x.2).collect::<Vec<_>>(), [7, 2, 8]);
    }
}
