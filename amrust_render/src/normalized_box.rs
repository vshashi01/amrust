use crate::vertex::{Color, Position, TexCoords, UseTexture};

//
//
//                    5----------------------6
//                   /|                     /|
//                  / |                    / |
//                 /  |                   /  |
//                /   |                  /   |                             +z front
//               /    |                 /    |                             ^
//              4----------------------7     |                             |   ^ +y top
//              |     |                |     |                             |  /
//              |     |                |     |                             | /
//              |     |                |     |               -x left <---- o ----> +x right
//              |     |                |     |                            /|
//              |     |                |     |                           / |
//              |     3----------------------2                          /  |
//              |    /                 |    /                          v   v
//              |   /                  |   /                  -y bottom    -z back
//              |  /                   |  /
//              | /                    | /
//              0----------------------1
//
//
//
//
//
// Triangles order
//
//          3-----2
//          |   / |
//          | /   |
//    3-----5-----6-----2-----3
//    |   / |   / |   / |   / |
//    | /   | /   | /   | /   |
//    0-----4-----7-----1-----0
//          |   / |
//          | /   |
//          0-----1
//

pub const POSITIONS: &[Position] = &[
    // back face
    Position([-1.0, -1.0, -1.0]),
    Position([1.0, -1.0, -1.0]),
    Position([1.0, 1.0, -1.0]),
    Position([-1.0, 1.0, -1.0]),
    // front face
    Position([-1.0, -1.0, 1.0]),
    Position([-1.0, 1.0, 1.0]),
    Position([1.0, 1.0, 1.0]),
    Position([1.0, -1.0, 1.0]),
    // bottom face
    Position([-1.0, -1.0, 1.0]),
    Position([-1.0, -1.0, -1.0]),
    Position([1.0, -1.0, 1.0]),
    Position([1.0, -1.0, -1.0]),
    // top face
    Position([-1.0, 1.0, -1.0]),
    Position([-1.0, 1.0, 1.0]),
    Position([1.0, 1.0, -1.0]),
    Position([1.0, 1.0, 1.0]),
    // right face
    Position([1.0, 1.0, 1.0]),
    Position([1.0, -1.0, 1.0]),
    Position([1.0, 1.0, -1.0]),
    Position([1.0, -1.0, -1.0]),
    // left face
    Position([-1.0, 1.0, -1.0]),
    Position([-1.0, -1.0, -1.0]),
    Position([-1.0, 1.0, 1.0]),
    Position([-1.0, -1.0, 1.0]),
];

pub const COLORS: &[Color] = &[
    // All faces use the same color
    Color([0.5, 0.0, 0.5]); 24
];

pub const TEX_COORDS: &[TexCoords] = &[
    // back face
    TexCoords([1.0, 1.0]),
    TexCoords([0.0, 1.0]),
    TexCoords([0.0, 0.0]),
    TexCoords([1.0, 0.0]),
    // front face
    TexCoords([0.0, 1.0]),
    TexCoords([0.0, 0.0]),
    TexCoords([1.0, 0.0]),
    TexCoords([1.0, 1.0]),
    // bottom face
    TexCoords([0.0, 0.0]),
    TexCoords([0.0, 1.0]),
    TexCoords([1.0, 0.0]),
    TexCoords([1.0, 1.0]),
    // top face
    TexCoords([0.0, 0.0]),
    TexCoords([0.0, 1.0]),
    TexCoords([1.0, 0.0]),
    TexCoords([1.0, 1.0]),
    // right face
    TexCoords([0.0, 0.0]),
    TexCoords([0.0, 1.0]),
    TexCoords([1.0, 0.0]),
    TexCoords([1.0, 1.0]),
    // left face
    TexCoords([0.0, 0.0]),
    TexCoords([0.0, 1.0]),
    TexCoords([1.0, 0.0]),
    TexCoords([1.0, 1.0]),
];

pub const USE_TEXTURE: &[UseTexture] = &[
    // back face
    UseTexture::yes(),
    UseTexture::yes(),
    UseTexture::yes(),
    UseTexture::yes(),
    // front face
    UseTexture::yes(),
    UseTexture::yes(),
    UseTexture::yes(),
    UseTexture::yes(),
    // bottom face
    UseTexture::yes(),
    UseTexture::yes(),
    UseTexture::yes(),
    UseTexture::yes(),
    // top face
    UseTexture::yes(),
    UseTexture::yes(),
    UseTexture::yes(),
    UseTexture::yes(),
    // right face
    UseTexture::no(),
    UseTexture::no(),
    UseTexture::no(),
    UseTexture::no(),
    // left face
    UseTexture::yes(),
    UseTexture::yes(),
    UseTexture::yes(),
    UseTexture::yes(),
];

pub fn get_use_texture_vertices(
    back_face: UseTexture,
    front_face: UseTexture,
    bottom_face: UseTexture,
    top_face: UseTexture,
    right_face: UseTexture,
    left_face: UseTexture,
) -> [UseTexture; 24] {
    [
        // back face
        back_face,
        back_face,
        back_face,
        back_face,
        // front face
        front_face,
        front_face,
        front_face,
        front_face,
        // bottom face
        bottom_face,
        bottom_face,
        bottom_face,
        bottom_face,
        // top face
        top_face,
        top_face,
        top_face,
        top_face,
        // right face
        right_face,
        right_face,
        right_face,
        right_face,
        // left face
        left_face,
        left_face,
        left_face,
        left_face,
    ]
}

#[rustfmt::skip]
pub const INDICES: &[u16] = &[
    // Front face //done
    5,4,6, 6,4,7,
    // Left face
    20,21,22, 22,21,23, 
    // Right face //done
    16,17,18, 18,17,19,
    // Top face, //done
    12,13,14, 14,13,15,
    // Bottom face //done
    8,9,10, 10,9,11,
    // Back face //done
    2,1,3,3,1,0,
];

pub const ORDERED_POSITIONS: &[Position] = &[
    // Front face
    Position([-1.0, 1.0, 1.0]),  // 5
    Position([-1.0, -1.0, 1.0]), // 4
    Position([1.0, 1.0, 1.0]),   // 6
    Position([1.0, 1.0, 1.0]),   // 6
    Position([-1.0, -1.0, 1.0]), // 4
    Position([1.0, -1.0, 1.0]),  // 7
    // Left face
    Position([-1.0, 1.0, -1.0]),  // 20
    Position([-1.0, -1.0, -1.0]), // 21
    Position([-1.0, 1.0, 1.0]),   // 22
    Position([-1.0, 1.0, 1.0]),   // 22
    Position([-1.0, -1.0, -1.0]), // 21
    Position([-1.0, -1.0, 1.0]),  // 23
    // Right face
    Position([1.0, 1.0, 1.0]),   // 16
    Position([1.0, -1.0, 1.0]),  // 17
    Position([1.0, 1.0, -1.0]),  // 18
    Position([1.0, 1.0, -1.0]),  // 18
    Position([1.0, -1.0, 1.0]),  // 17
    Position([1.0, -1.0, -1.0]), // 19
    // Top face
    Position([-1.0, 1.0, -1.0]), // 12
    Position([-1.0, 1.0, 1.0]),  // 13
    Position([1.0, 1.0, -1.0]),  // 14
    Position([1.0, 1.0, -1.0]),  // 14
    Position([-1.0, 1.0, 1.0]),  // 13
    Position([1.0, 1.0, 1.0]),   // 15
    // Bottom face
    Position([-1.0, -1.0, 1.0]),  // 8
    Position([-1.0, -1.0, -1.0]), // 9
    Position([1.0, -1.0, 1.0]),   // 10
    Position([1.0, -1.0, 1.0]),   // 10
    Position([-1.0, -1.0, -1.0]), // 9
    Position([1.0, -1.0, -1.0]),  // 11
    // Back face
    Position([1.0, 1.0, -1.0]),   // 2
    Position([1.0, -1.0, -1.0]),  // 1
    Position([-1.0, 1.0, -1.0]),  // 3
    Position([-1.0, 1.0, -1.0]),  // 3
    Position([1.0, -1.0, -1.0]),  // 1
    Position([-1.0, -1.0, -1.0]), // 0
];
