`rapier2d` is rapier2d 0.36.1 from crates.io with `rapier2d-voxel-contact.patch` applied, dropping
only its tests and development dependencies. The patch fixes the panic of dimforge/rapier issue 1043
(`stale ContactRef manifold ordinal`), where a dynamic body touching a `Voxels` collider left the
solver graph referring to a dropped manifold. Replace this copy with the crates.io release once
upstream fixes that issue.
