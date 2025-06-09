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
    UseTexture::new(true),
    UseTexture::new(true),
    UseTexture::new(true),
    UseTexture::new(true),
    // front face
    UseTexture::new(true),
    UseTexture::new(true),
    UseTexture::new(true),
    UseTexture::new(true),
    // bottom face
    UseTexture::new(true),
    UseTexture::new(true),
    UseTexture::new(true),
    UseTexture::new(true),
    // top face
    UseTexture::new(true),
    UseTexture::new(true),
    UseTexture::new(true),
    UseTexture::new(true),
    // right face
    UseTexture::new(false),
    UseTexture::new(false),
    UseTexture::new(false),
    UseTexture::new(false),
    // left face
    UseTexture::new(true),
    UseTexture::new(true),
    UseTexture::new(true),
    UseTexture::new(true),
];

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
