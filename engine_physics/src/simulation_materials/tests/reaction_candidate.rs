// Copyright Rob Gage 2026

use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReactionCandidate {
    pub anchor: u32,
    pub reaction_index: u32,
    pub priority: i32,
    pub authoring_order: u32,
    pub extent: f32,
    pub authorities: [Option<u64>; 2],
}

pub(crate) fn resolve_contention(mut candidates: Vec<ReactionCandidate>) -> Vec<ReactionCandidate> {
    candidates.sort_by_key(candidate_order_key);
    let mut claimed: BTreeSet<u64> = BTreeSet::new();
    candidates
        .into_iter()
        .filter(|candidate| {
            let keys: Vec<u64> = candidate.authorities.iter().flatten().copied().collect();
            if keys.iter().any(|key| claimed.contains(key)) {
                return false;
            }
            claimed.extend(keys);
            true
        })
        .collect()
}

fn candidate_order_key(candidate: &ReactionCandidate) -> (std::cmp::Reverse<i32>, u32, u32, u32) {
    (
        std::cmp::Reverse(candidate.priority),
        candidate.authoring_order,
        candidate.anchor,
        candidate.reaction_index,
    )
}
