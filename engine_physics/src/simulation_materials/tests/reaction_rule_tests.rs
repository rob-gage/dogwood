// Copyright Rob Gage 2026

use super::reaction_candidate::{ReactionCandidate, resolve_contention};
use super::reaction_environment::{ReactionEnvironment, environment_matches};
use super::reaction_extent::extent;
use super::reaction_implicit_air::implicit_air;
use crate::materials::CompiledMaterialReaction;

#[test]
fn test_environment_bounds_are_independent() {
    let rule = CompiledMaterialReaction {
        minimum_temperature: 10.0,
        maximum_temperature: 20.0,
        minimum_pressure: 2.0,
        maximum_pressure: 4.0,
        minimum_air: 0.25,
        maximum_air: 0.75,
        ..Default::default()
    };
    assert!(environment_matches(
        &rule,
        ReactionEnvironment {
            temperature: 15.0,
            pressure: 3.0,
            air: 0.5
        }
    ));
    assert!(!environment_matches(
        &rule,
        ReactionEnvironment {
            temperature: 9.0,
            pressure: 3.0,
            air: 0.5
        }
    ));
    assert!(!environment_matches(
        &rule,
        ReactionEnvironment {
            temperature: 15.0,
            pressure: 5.0,
            air: 0.5
        }
    ));
    assert!(!environment_matches(
        &rule,
        ReactionEnvironment {
            temperature: 15.0,
            pressure: 3.0,
            air: 0.9
        }
    ));
}

#[test]
fn test_contention_is_priority_then_stable_and_atomic() {
    let candidates = vec![
        ReactionCandidate {
            anchor: 8,
            reaction_index: 1,
            priority: 1,
            authoring_order: 1,
            extent: 1.0,
            authorities: [Some(7), Some(9)],
        },
        ReactionCandidate {
            anchor: 2,
            reaction_index: 2,
            priority: 2,
            authoring_order: 0,
            extent: 1.0,
            authorities: [Some(7), Some(10)],
        },
        ReactionCandidate {
            anchor: 3,
            reaction_index: 3,
            priority: 1,
            authoring_order: 0,
            extent: 1.0,
            authorities: [Some(11), None],
        },
    ];
    let accepted = resolve_contention(candidates);
    assert_eq!(
        accepted
            .iter()
            .map(|candidate| candidate.reaction_index)
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
}

#[test]
fn test_contention_tie_uses_authoring_order() {
    let candidates = vec![
        ReactionCandidate {
            anchor: 4,
            reaction_index: 9,
            priority: 3,
            authoring_order: 8,
            extent: 1.0,
            authorities: [Some(12), None],
        },
        ReactionCandidate {
            anchor: 2,
            reaction_index: 3,
            priority: 3,
            authoring_order: 2,
            extent: 1.0,
            authorities: [Some(12), None],
        },
    ];
    let accepted = resolve_contention(candidates);
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].reaction_index, 3);
}

#[test]
fn test_contention_final_tie_uses_reaction_index() {
    let candidates = vec![
        ReactionCandidate {
            anchor: 2,
            reaction_index: 9,
            priority: 3,
            authoring_order: 2,
            extent: 1.0,
            authorities: [Some(12), None],
        },
        ReactionCandidate {
            anchor: 2,
            reaction_index: 3,
            priority: 3,
            authoring_order: 2,
            extent: 1.0,
            authorities: [Some(12), None],
        },
    ];
    let accepted = resolve_contention(candidates);
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].reaction_index, 3);
}

#[test]
fn test_extent_is_stoichiometric() {
    assert_eq!(extent(0.5, [1.0, 0.4], [1.0, 2.0]), 0.2);
    // Two slots resolving to one authority share its inventory budget.
    assert_eq!(extent(1.0, [0.7], [1.0 + 1.0]), 0.35);
}

#[test]
fn test_implicit_air_respects_local_occupancy() {
    assert_eq!(implicit_air(true, false, 0.0, 0.0), 1.0);
    assert_eq!(implicit_air(true, false, 0.0, 0.25), 0.75);
    assert_eq!(implicit_air(true, false, 0.0, 1.0), 0.0);
    assert_eq!(implicit_air(false, false, 0.0, 0.0), 0.0);
    assert_eq!(implicit_air(true, true, 0.0, 0.0), 0.0);
}
