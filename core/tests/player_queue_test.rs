use spfy_core::model::TrackId;
use spfy_core::player::queue::{Queue, AdvanceResult};

fn ids(strs: &[&str]) -> Vec<TrackId> {
    strs.iter().map(|s| TrackId((*s).into())).collect()
}

#[test]
fn play_context_loads_at_start_index() {
    let mut q = Queue::default();
    let current = q.set(ids(&["a", "b", "c"]), 1);
    assert_eq!(current.unwrap().0, "b");
}

#[test]
fn next_advances_or_stops() {
    let mut q = Queue::default();
    q.set(ids(&["a", "b"]), 0);
    assert_eq!(q.next(), AdvanceResult::Loaded(TrackId("b".into())));
    assert_eq!(q.next(), AdvanceResult::EndReached);
}

#[test]
fn prev_walks_backwards_clamped_to_zero() {
    let mut q = Queue::default();
    q.set(ids(&["a", "b", "c"]), 2);
    assert_eq!(q.previous(), AdvanceResult::Loaded(TrackId("b".into())));
    assert_eq!(q.previous(), AdvanceResult::Loaded(TrackId("a".into())));
    assert_eq!(q.previous(), AdvanceResult::Loaded(TrackId("a".into()))); // clamps
}
