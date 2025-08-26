#![allow(unused)]

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
    // back face                  // vertices ~ index
    Position([-1.0, -1.0, -1.0]), // 0 ~ 0
    Position([1.0, -1.0, -1.0]),  // 1 ~ 1
    Position([1.0, 1.0, -1.0]),   // 2 ~ 2
    Position([-1.0, 1.0, -1.0]),  // 3 ~ 3
    // front face
    Position([-1.0, -1.0, 1.0]), // 4 ~ 4
    Position([-1.0, 1.0, 1.0]),  // 5 ~ 5
    Position([1.0, 1.0, 1.0]),   // 6 ~ 6
    Position([1.0, -1.0, 1.0]),  // 7 ~ 7
    // bottom face
    Position([-1.0, -1.0, 1.0]),  // 4 ~ 8
    Position([-1.0, -1.0, -1.0]), // 0 ~ 9
    Position([1.0, -1.0, 1.0]),   // 7 ~ 10
    Position([1.0, -1.0, -1.0]),  // 1 ~ 11
    // top face
    Position([-1.0, 1.0, -1.0]), // 3 ~ 12
    Position([-1.0, 1.0, 1.0]),  // 5 ~ 13
    Position([1.0, 1.0, -1.0]),  // 2 ~ 14
    Position([1.0, 1.0, 1.0]),   // 6 ~ 15
    // right face
    Position([1.0, 1.0, 1.0]),   // 6 ~ 16
    Position([1.0, -1.0, 1.0]),  // 7 ~ 17
    Position([1.0, 1.0, -1.0]),  // 2 ~ 18
    Position([1.0, -1.0, -1.0]), // 1 ~ 19
    // left face
    Position([-1.0, 1.0, -1.0]),  // 3 ~ 20
    Position([-1.0, -1.0, -1.0]), // 0 ~ 21
    Position([-1.0, 1.0, 1.0]),   // 5 ~ 22
    Position([-1.0, -1.0, 1.0]),  // 4 ~ 23
];

// pub const COLORS: &[Color] = &[
//     // All faces use the same color
//     Color([0.5, 0.0, 0.5]); 24
// ];

pub const COLORS: &[Color] = &[
    // back face                  // vertices ~ index
    Color([1.0, 0.5, 0.0]), // 0 ~ 0
    Color([1.0, 0.5, 0.0]), // 1 ~ 1
    Color([1.0, 0.5, 0.0]), // 2 ~ 2
    Color([1.0, 0.5, 0.0]), // 3 ~ 3
    // front face
    Color([1.0, 0.0, 0.0]), // 4 ~ 4
    Color([1.0, 0.0, 0.0]), // 5 ~ 5
    Color([1.0, 0.0, 0.0]), // 6 ~ 6
    Color([1.0, 0.0, 0.0]), // 7 ~ 7
    // bottom face
    Color([1.0, 0.0, 0.0]), // 4 ~ 8
    Color([1.0, 0.5, 0.0]), // 0 ~ 9
    Color([1.0, 0.0, 0.0]), // 7 ~ 10
    Color([1.0, 0.5, 0.0]), // 1 ~ 11
    // top face
    Color([1.0, 0.5, 0.0]), // 3 ~ 12
    Color([1.0, 0.0, 0.0]), // 5 ~ 13
    Color([1.0, 0.5, 0.0]), // 2 ~ 14
    Color([1.0, 0.0, 0.0]), // 6 ~ 15
    // right face
    Color([1.0, 0.0, 0.0]), // 6 ~ 16
    Color([1.0, 0.0, 0.0]), // 7 ~ 17
    Color([1.0, 0.5, 0.0]), // 2 ~ 18
    Color([1.0, 0.5, 0.0]), // 1 ~ 19
    // left face
    Color([1.0, 0.5, 0.0]), // 3 ~ 20
    Color([1.0, 0.5, 0.0]), // 0 ~ 21
    Color([1.0, 0.0, 0.0]), // 5 ~ 22
    Color([1.0, 0.0, 0.0]), // 4 ~ 23
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
pub const INDICES: &[u32] = &[
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

#[rustfmt::skip]
pub const TRI_EDGE_INDICES: &[u32] = &[
    //front face
    5,4,4,6,5,6, 6,4,4,7,6,7,
    //left face
    20,21,21,22,20,22, 22,21,21,23,22,23,
    //right face
    16,17,17,18,16,18, 18,17,17,17,18,19,
    //top face
    12,13,13,14,12,14, 14,13,13,15,14,15,
    //bottom face
    8,9,9,10,8,10, 10,9,9,11,10,11,
    //back face
    2,1,1,3,2,3, 3,1,1,0,3,0
];

#[rustfmt::skip]
pub const INDEXED_POSITIONS_BOX_EDGE_INDICES: &[u32] = &[
    //left face
    21, 20, 20, 22, 22, 23, 23, 21, 
    //bottom face
    9, 8, 8, 10, 10, 11, 11, 9, 
    //right face
    17, 16, 16, 18, 18, 19, 19, 17, 
    //top face
    13, 12, 12, 14, 14, 15, 15, 13,
];

pub const ORDERED_POSITIONS: &[Position] = &[
    // Front face                // vertices ~ actual index
    Position([-1.0, 1.0, 1.0]),  // 5 ~ 0
    Position([-1.0, -1.0, 1.0]), // 4 ~ 1
    Position([1.0, 1.0, 1.0]),   // 6 ~ 2
    Position([1.0, 1.0, 1.0]),   // 6 ~ 3
    Position([-1.0, -1.0, 1.0]), // 4 ~ 4
    Position([1.0, -1.0, 1.0]),  // 7 ~ 5
    // Left face
    Position([-1.0, 1.0, -1.0]),  // 3 ~ 6
    Position([-1.0, -1.0, -1.0]), // 0 ~ 7
    Position([-1.0, 1.0, 1.0]),   // 5 ~ 8
    Position([-1.0, 1.0, 1.0]),   // 5 ~ 9
    Position([-1.0, -1.0, -1.0]), // 0 ~ 10
    Position([-1.0, -1.0, 1.0]),  // 4 ~ 11
    // Right face
    Position([1.0, 1.0, 1.0]),   // 6 ~ 12
    Position([1.0, -1.0, 1.0]),  // 7 ~ 13
    Position([1.0, 1.0, -1.0]),  // 2 ~ 14
    Position([1.0, 1.0, -1.0]),  // 2 ~ 15
    Position([1.0, -1.0, 1.0]),  // 7 ~ 16
    Position([1.0, -1.0, -1.0]), // 1 ~ 17
    // Top face
    Position([-1.0, 1.0, -1.0]), // 3 ~ 18
    Position([-1.0, 1.0, 1.0]),  // 5 ~ 19
    Position([1.0, 1.0, -1.0]),  // 2 ~ 20
    Position([1.0, 1.0, -1.0]),  // 2 ~ 21
    Position([-1.0, 1.0, 1.0]),  // 5 ~ 22
    Position([1.0, 1.0, 1.0]),   // 6 ~ 23
    // Bottom face
    Position([-1.0, -1.0, 1.0]),  // 4 ~ 24
    Position([-1.0, -1.0, -1.0]), // 0 ~ 25
    Position([1.0, -1.0, 1.0]),   // 7 ~ 26
    Position([1.0, -1.0, 1.0]),   // 7 ~ 27
    Position([-1.0, -1.0, -1.0]), // 0 ~ 28
    Position([1.0, -1.0, -1.0]),  // 1 ~ 29
    // Back face
    Position([1.0, 1.0, -1.0]),   // 2 ~ 30
    Position([1.0, -1.0, -1.0]),  // 1 ~ 31
    Position([-1.0, 1.0, -1.0]),  // 3 ~ 32
    Position([-1.0, 1.0, -1.0]),  // 3 ~ 33
    Position([1.0, -1.0, -1.0]),  // 1 ~ 34
    Position([-1.0, -1.0, -1.0]), // 0 ~ 35
];

#[rustfmt::skip]
pub const ORDERED_POSITIONS_TRI_EDGE_INDICES: &[u16] = &[
    // Triangle 0
    0, 1, 1, 2, 2, 0,
    // Triangle 1
    3, 4, 4, 5, 5, 3,
    // Triangle 2
    6, 7, 7, 8, 8, 6,
    // Triangle 3
    9, 10, 10, 11, 11, 9,
    // Triangle 4
    12, 13, 13, 14, 14, 12,
    // Triangle 5
    15, 16, 16, 17, 17, 15,
    // Triangle 6
    18, 19, 19, 20, 20, 18,
    // Triangle 7
    21, 22, 22, 23, 23, 21,
    // Triangle 8
    24, 25, 25, 26, 26, 24,
    // Triangle 9
    27, 28, 28, 29, 29, 27,
    // Triangle 10
    30, 31, 31, 32, 32, 30,
    // Triangle 11
    33, 34, 34, 35, 35, 33,
];

#[rustfmt::skip]
pub const ORDERED_POSITIONS_BOX_EDGE_INDICES: &[u32] = &[
    //left face
    7, 6, 6, 8, 8, 11, 11, 7, 
    //bottom face
    25, 24, 24, 26, 26, 29, 29, 25, 
    //right face
    13, 12, 12, 14, 14, 17, 17, 13, 
    //top face
    19, 18, 18, 20, 20, 23, 23, 19,
];
