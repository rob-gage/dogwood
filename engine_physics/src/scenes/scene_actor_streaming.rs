use std::collections::HashMap;

use super::Scene;
use crate::actors::ActorPhysicalSnapshot;
use crate::actors::ActorPhysicalSpawn;
use crate::chunks::Chunk;
use crate::tiles::TileArea;
use crate::tiles::TileCoordinates;

impl Scene {
    pub(super) fn generate_actor_region(&mut self, region: TileCoordinates) {
        if !self.actor_initialized_regions.insert(region) {
            return;
        }
        let spawns: Vec<ActorPhysicalSpawn> =
            self.generator().generate_actor_spawns_with_seed(0, region);
        for spawn in spawns {
            self.actor_registry_mutable().spawn_physical_actor(
                spawn.configuration,
                spawn.position,
                spawn.velocity,
            );
        }
    }

    pub(super) fn actors_streaming_update(&mut self) {
        let retained: TileArea =
            self.area_streaming()
                .expanded(Chunk::WIDTH, Chunk::WIDTH, Chunk::WIDTH, Chunk::WIDTH);
        let regions: Vec<TileCoordinates> = self
            .actor_snapshots
            .keys()
            .copied()
            .filter(|region| retained.contains(*region))
            .collect();
        for region in regions {
            if let Some(snapshots) = self.actor_snapshots.remove(&region) {
                for snapshot in snapshots {
                    self.actor_registry_mutable()
                        .restore_physical_snapshot(snapshot);
                }
            }
        }

        let possessed: Option<crate::actors::Actor> = self.possessed_actor();
        let mut dormant: HashMap<TileCoordinates, Vec<ActorPhysicalSnapshot>> = HashMap::new();
        for snapshot in self.actor_registry().physical_snapshots() {
            if possessed == Some(snapshot.actor)
                || retained.contains(snapshot.position.tile_coordinates)
            {
                continue;
            }
            self.actor_registry_mutable().despawn(snapshot.actor);
            dormant
                .entry(snapshot.position.tile_coordinates.chunk_coordinates())
                .or_default()
                .push(snapshot);
        }
        for (region, snapshots) in dormant {
            self.actor_snapshots
                .entry(region)
                .or_default()
                .extend(snapshots);
        }
    }
}
