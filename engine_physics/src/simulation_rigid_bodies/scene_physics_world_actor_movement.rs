// Copyright Rob Gage 2026

use super::*;

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
        let exclude = self.pawn_collider_at(shape, position);
        self.move_actor_excluding(
            shape,
            position,
            desired,
            up,
            walkable_normal,
            snap_distance,
            exclude,
            collisions,
        )
    }

    pub(crate) fn move_actor_excluding(
        &self,
        shape: ActorCollisionShape,
        position: Vector,
        desired: Vector,
        up: Vector,
        walkable_normal: f32,
        snap_distance: f32,
        exclude: Option<ColliderHandle>,
        collisions: &mut impl FnMut(Vector),
    ) -> (Vector, bool) {
        let (mut translation, mut grounded) = self.resolve_actor_translation(
            shape,
            position,
            desired,
            up,
            walkable_normal,
            exclude,
            collisions,
        );
        if snap_distance > 0.0 && desired.dot(up) <= 0.0 && !grounded {
            let (snap, snapped) = self.resolve_actor_support_excluding(
                shape,
                position + translation,
                snap_distance,
                up,
                exclude,
            );
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
        exclude: Option<ColliderHandle>,
        collisions: &mut impl FnMut(Vector),
    ) -> (Vector, bool) {
        #[cfg(debug_assertions)]
        let started = Instant::now();
        let offset = 1.0 / 1024.0;
        let primitive = shape.rapier_shape();
        let mut consumed = Vector::ZERO;
        let mut remaining = desired;
        let mut grounded = false;
        for _ in 0..4 {
            if remaining.length_squared() < 1e-12 {
                break;
            }
            let from = position + consumed;
            let moving_pose = shape.pose(from, up);
            let options = ShapeCastOptions {
                max_time_of_impact: 1.0,
                target_distance: offset,
                stop_at_penetration: false,
                compute_impact_geometry_on_penetration: true,
            };
            let mut earliest = 2.0f32;
            let mut normals = [Vector::ZERO; 4];
            let mut normal_count = 0usize;
            if let Some((_handle, hit)) = self.rapier.cast_shape(
                &moving_pose,
                remaining,
                primitive.as_ref(),
                options,
                exclude.map_or_else(
                    || QueryFilter::default().groups(Self::pawn_query_groups()),
                    |h| {
                        QueryFilter::default()
                            .groups(Self::pawn_query_groups())
                            .exclude_collider(h)
                    },
                ),
            ) {
                if hit.time_of_impact + 1e-4 < earliest {
                    earliest = hit.time_of_impact;
                    normal_count = 0;
                }
                if (hit.time_of_impact - earliest).abs() <= 1e-4
                    && normal_count < normals.len()
                    && !normals[..normal_count]
                        .iter()
                        .any(|current| current.dot(hit.normal1) > 0.999)
                {
                    normals[normal_count] = hit.normal1;
                    normal_count += 1;
                }
            }
            if earliest > 1.0 {
                consumed += remaining;
                break;
            }
            let advance = remaining * earliest.max(0.0);
            consumed += advance;
            remaining -= advance;
            let mut active_normals = 0usize;
            for index in 0..normal_count {
                let normal = normals[index];
                active_normals += 1;
                collisions(normal);
                grounded |= normal.dot(up) >= walkable_normal;
                let inward = remaining.dot(normal);
                if inward < 0.0 {
                    remaining -= normal * inward;
                }
            }
            if active_normals == 0 {
                consumed += remaining;
                break;
            }
        }
        #[cfg(debug_assertions)]
        tracing::trace!(
            elapsed_us = started.elapsed().as_micros(),
            "pawn rigid/terrain shape casts"
        );
        (consumed, grounded)
    }

    /// Casts only along gravity-relative down without allowing a support correction to slide.
    pub(crate) fn resolve_actor_support(
        &self,
        shape: ActorCollisionShape,
        position: Vector,
        distance: f32,
        up: Vector,
    ) -> (Vector, bool) {
        self.resolve_actor_support_excluding(shape, position, distance, up, None)
    }

    fn resolve_actor_support_excluding(
        &self,
        shape: ActorCollisionShape,
        position: Vector,
        distance: f32,
        up: Vector,
        exclude: Option<ColliderHandle>,
    ) -> (Vector, bool) {
        #[cfg(debug_assertions)]
        let started = Instant::now();
        if !distance.is_finite() || distance <= 0.0 {
            return (Vector::ZERO, false);
        }
        let offset = 1.0 / 1024.0;
        let desired = -up * distance;
        let primitive = shape.rapier_shape();
        let moving_pose = shape.pose(position, up);
        let options = ShapeCastOptions {
            max_time_of_impact: 1.0,
            target_distance: offset,
            stop_at_penetration: false,
            compute_impact_geometry_on_penetration: true,
        };
        let mut earliest = 1.0f32;
        let mut support = false;
        if let Some((_handle, hit)) = self.rapier.cast_shape(
            &moving_pose,
            desired,
            primitive.as_ref(),
            options,
            exclude.map_or_else(
                || QueryFilter::default().groups(Self::pawn_query_groups()),
                |h| {
                    QueryFilter::default()
                        .groups(Self::pawn_query_groups())
                        .exclude_collider(h)
                },
            ),
        ) {
            if hit.normal1.dot(up) > 1e-4 && hit.time_of_impact <= earliest {
                earliest = hit.time_of_impact;
                support = true;
            }
        }
        #[cfg(debug_assertions)]
        tracing::trace!(
            elapsed_us = started.elapsed().as_micros(),
            "pawn support shape cast"
        );
        (-up * (distance * earliest), support)
    }
}
