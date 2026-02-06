use crate::vertex::{self, Position};

/// Full, self-contained prism axis gizmo mesh generator.
/// Produces 3 rectangular prisms for +X, +Y, +Z axes (triangle list).
/// Output is compatible with your MeshBuilder:
/// - positions -> vertex::Position stream
/// - colors    -> vertex::Color stream
/// - indices   -> mesh index stream (triangles)
///
/// Local space:
/// - Origin at (0,0,0)
/// - X axis prism: from x=0..L, thickness 2T in Y and Z
/// - Y axis prism: from y=0..L, thickness 2T in X and Z
/// - Z axis prism: from z=0..L, thickness 2T in X and Y
///
/// Notes:
/// - Uses 24 vertices per box (unique per face), 36 indices per box.
/// - Total: 72 vertices, 108 indices.
pub fn generate_axis_gizmo_prism_mesh(
    axis_length: f32,
    axis_half_thickness: f32,
) -> (Vec<vertex::Position>, Vec<vertex::Color>, Vec<u32>) {
    fn push_box(
        positions: &mut Vec<[f32; 3]>,
        colors: &mut Vec<[f32; 3]>,
        indices: &mut Vec<u32>,
        min: [f32; 3],
        max: [f32; 3],
        color: [f32; 3],
    ) {
        let [xmin, ymin, zmin] = min;
        let [xmax, ymax, zmax] = max;

        let base = positions.len() as u32;

        // 6 faces * 4 vertices (duplicated per face)
        let face_verts: [[f32; 3]; 24] = [
            // +X (x = xmax)
            [xmax, ymin, zmin],
            [xmax, ymax, zmin],
            [xmax, ymax, zmax],
            [xmax, ymin, zmax],
            // -X (x = xmin)
            [xmin, ymin, zmax],
            [xmin, ymax, zmax],
            [xmin, ymax, zmin],
            [xmin, ymin, zmin],
            // +Y (y = ymax)
            [xmin, ymax, zmin],
            [xmin, ymax, zmax],
            [xmax, ymax, zmax],
            [xmax, ymax, zmin],
            // -Y (y = ymin)
            [xmin, ymin, zmax],
            [xmin, ymin, zmin],
            [xmax, ymin, zmin],
            [xmax, ymin, zmax],
            // +Z (z = zmax)
            [xmin, ymin, zmax],
            [xmax, ymin, zmax],
            [xmax, ymax, zmax],
            [xmin, ymax, zmax],
            // -Z (z = zmin)
            [xmax, ymin, zmin],
            [xmin, ymin, zmin],
            [xmin, ymax, zmin],
            [xmax, ymax, zmin],
        ];

        positions.extend_from_slice(&face_verts);
        colors.extend(std::iter::repeat_n(color, 24));

        // Two triangles per face (0,1,2) (0,2,3), repeated for 6 faces
        for f in 0..6u32 {
            let i0 = base + f * 4;
            indices.extend_from_slice(&[i0, i0 + 1, i0 + 2, i0, i0 + 2, i0 + 3]);
        }
    }

    let l = axis_length;
    let t = axis_half_thickness;

    let mut positions = Vec::<[f32; 3]>::with_capacity(72);
    let mut colors = Vec::<[f32; 3]>::with_capacity(72);
    let mut indices = Vec::<u32>::with_capacity(108);

    // +X prism (red)
    push_box(
        &mut positions,
        &mut colors,
        &mut indices,
        [0.0, -t, -t],
        [l, t, t],
        [1.0, 0.0, 0.0],
    );

    // +Y prism (green)
    push_box(
        &mut positions,
        &mut colors,
        &mut indices,
        [-t, 0.0, -t],
        [t, l, t],
        [0.0, 1.0, 0.0],
    );

    // +Z prism (blue)
    push_box(
        &mut positions,
        &mut colors,
        &mut indices,
        [-t, -t, 0.0],
        [t, t, l],
        [0.0, 0.0, 1.0],
    );

    let pos = positions
        .iter()
        .map(|p| vertex::Position(*p))
        .collect::<Vec<Position>>();
    let color = colors.iter().map(|c| vertex::Color(*c)).collect::<Vec<_>>();

    (pos, color, indices)
}
