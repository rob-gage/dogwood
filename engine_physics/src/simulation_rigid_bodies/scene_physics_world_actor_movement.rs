// Copyright Rob Gage 2026

#[cfg(debug_assertions)]
use std::time::Instant;

use rapier2d::parry::query::ShapeCastOptions;
use rapier2d::prelude::Pose;
use rapier2d::prelude::QueryFilter;
use rapier2d::prelude::SharedShape;
use rapier2d::prelude::Vector;

use super::ScenePhysicsWorld;
use crate::actors::ActorCollisionShape;

impl ScenePhysicsWorld {
    /// Resolves authoritative actor motion through Rapier terrain and rigid-body queries.
    pub(crate) fn move_actor(
        &self,
        shape: ActorCollisionShape,
        position: Vector,
        desired: Vector,
        up: Vector,
        walkable_normal: f32,
        snap_distance: f32,
        collisions: &mut impl FnMut(Vector),
    ) -> (Vector, bool) {
        let (mut translation, mut grounded): (Vector, bool) = self.resolve_actor_translation(
            shape,
            position,
            desired,
            up,
            walkable_normal,
            collisions,
        );
        if snap_distance > 0.0 && desired.dot(up) <= 0.0 && !grounded {
            let (snap, snapped): (Vector, bool) =
                self.resolve_actor_support(shape, position + translation, snap_distance, up);
            if snapped {
                translation += snap;
                grounded = true;
            }
        }
        (translation, grounded)
    }

    fn resolve_actor_translation(
        &self,
        shape: ActorCollisionShape,
        position: Vector,
        desired: Vector,
        up: Vector,
        walkable_normal: f32,
        collisions: &mut impl FnMut(Vector),
    ) -> (Vector, bool) {
        #[cfg(debug_assertions)]
        let started: Instant = Instant::now();
        let collision_target_distance: f32 = 1.0 / 1024.0;
        let collision_shape: SharedShape = shape.rapier_shape();
        let mut consumed_translation: Vector = Vector::ZERO;
        let mut remaining_translation: Vector = desired;
        let mut is_grounded: bool = false;
        for _ in 0..4 {
            if remaining_translation.length_squared() < 1e-12 {
                break;
            }
            let cast_start_position: Vector = position + consumed_translation;
            let moving_pose: Pose = shape.pose(cast_start_position, up);
            let shape_cast_options: ShapeCastOptions = ShapeCastOptions {
                max_time_of_impact: 1.0,
                target_distance: collision_target_distance,
                stop_at_penetration: false,
                compute_impact_geometry_on_penetration: true,
            };
            let mut earliest_time_of_impact: f32 = 2.0;
            let mut collision_normals: [Vector; 4] = [Vector::ZERO; 4];
            let mut collision_normal_count: usize = 0;
            if let Some((_collider_handle, shape_cast_hit)) = self.rapier.cast_shape(
                &moving_pose,
                remaining_translation,
                collision_shape.as_ref(),
                shape_cast_options,
                QueryFilter::default().groups(Self::pawn_query_groups()),
            ) {
                if shape_cast_hit.time_of_impact + 1e-4 < earliest_time_of_impact {
                    earliest_time_of_impact = shape_cast_hit.time_of_impact;
                    collision_normal_count = 0;
                }
                if (shape_cast_hit.time_of_impact - earliest_time_of_impact).abs() <= 1e-4
                    && collision_normal_count < collision_normals.len()
                    && !collision_normals[..collision_normal_count]
                        .iter()
                        .any(|current_normal| current_normal.dot(shape_cast_hit.normal1) > 0.999)
                {
                    collision_normals[collision_normal_count] = shape_cast_hit.normal1;
                    collision_normal_count += 1;
                }
            }
            if earliest_time_of_impact > 1.0 {
                consumed_translation += remaining_translation;
                break;
            }
            let advance_translation: Vector =
                remaining_translation * earliest_time_of_impact.max(0.0);
            consumed_translation += advance_translation;
            remaining_translation -= advance_translation;
            let mut active_collision_normal_count: usize = 0;
            for collision_normal in collision_normals[..collision_normal_count].iter().copied() {
                active_collision_normal_count += 1;
                collisions(collision_normal);
                is_grounded |= collision_normal.dot(up) >= walkable_normal;
                let inward_translation: f32 = remaining_translation.dot(collision_normal);
                if inward_translation < 0.0 {
                    remaining_translation -= collision_normal * inward_translation;
                }
            }
            if active_collision_normal_count == 0 {
                consumed_translation += remaining_translation;
                break;
            }
        }
        #[cfg(debug_assertions)]
        tracing::trace!(
            elapsed_us = started.elapsed().as_micros(),
            "pawn rigid/terrain shape casts"
        );
        (consumed_translation, is_grounded)
    }

    /// Casts only along gravity-relative down without allowing a support correction to slide.
    pub(crate) fn resolve_actor_support(
        &self,
        shape: ActorCollisionShape,
        position: Vector,
        distance: f32,
        up: Vector,
    ) -> (Vector, bool) {
        #[cfg(debug_assertions)]
        let started: Instant = Instant::now();
        if !distance.is_finite() || distance <= 0.0 {
            return (Vector::ZERO, false);
        }
        let collision_target_distance: f32 = 1.0 / 1024.0;
        let desired_translation: Vector = -up * distance;
        let collision_shape: SharedShape = shape.rapier_shape();
        let moving_pose: Pose = shape.pose(position, up);
        let shape_cast_options: ShapeCastOptions = ShapeCastOptions {
            max_time_of_impact: 1.0,
            target_distance: collision_target_distance,
            stop_at_penetration: false,
            compute_impact_geometry_on_penetration: true,
        };
        let mut earliest_time_of_impact: f32 = 1.0;
        let mut has_support: bool = false;
        if let Some((_collider_handle, shape_cast_hit)) = self.rapier.cast_shape(
            &moving_pose,
            desired_translation,
            collision_shape.as_ref(),
            shape_cast_options,
            QueryFilter::default().groups(Self::pawn_query_groups()),
        ) && shape_cast_hit.normal1.dot(up) > 1e-4
            && shape_cast_hit.time_of_impact <= earliest_time_of_impact
        {
            earliest_time_of_impact = shape_cast_hit.time_of_impact;
            has_support = true;
        }
        #[cfg(debug_assertions)]
        tracing::trace!(
            elapsed_us = started.elapsed().as_micros(),
            "pawn support shape cast"
        );
        (-up * (distance * earliest_time_of_impact), has_support)
    }
}
