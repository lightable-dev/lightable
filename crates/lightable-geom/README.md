# lightable-geom

Pure, tested 2D geometry for CAD in Rust. No dependencies, no allocation surprises, `f64` throughout.

- `bulge`: polyline bulges ↔ arcs (`arc_from_bulge`, `bulge_from_sweep`).
- `curve`: points along ellipses and NURBS curves (`ellipse_points`, `nurbs_points`).
- `transform`: object coordinate systems by the arbitrary axis algorithm (`ocs_axes`) and 3D affine maps (`Affine3`), including whether a map keeps circles circular when seen from above.
- `fill`: triangles covering the even-odd region of closed loops (`even_odd_triangles`), for hatch fills with holes and islands.

```rust
use lightable_geom::{bulge, fill};

// A bulge of 1 is a half circle.
let arc = bulge::arc_from_bulge([0.0, 0.0], [2.0, 0.0], 1.0).unwrap();
assert!((arc.radius - 1.0).abs() < 1e-12);

// A 10 × 10 square with a 6 × 6 hole: 64 square units.
let square = |x: f64, s: f64| vec![[x, x], [x + s, x], [x + s, x + s], [x, x + s]];
let area: f64 = fill::even_odd_triangles(&[square(0.0, 10.0), square(2.0, 6.0)])
    .iter()
    .map(fill::triangle_area)
    .sum();
assert!((area - 64.0).abs() < 1e-9);
```

This crate is part of [Lightable](https://lightable.dev), an open-source 2D drafting app, where commands and rendering share these functions.

## Licence

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
