use crate::vertex::VertexPC;

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
pub const VERTICES: &[VertexPC] = &[
    // back face
    VertexPC {
        position: [-1.0, -1.0, -1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [1.0, 1.0], // 0
    },
    VertexPC {
        position: [1.0, -1.0, -1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [0.0, 1.0], // 1
    },
    VertexPC {
        position: [1.0, 1.0, -1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [0.0, 0.0], // 2
    },
    VertexPC {
        position: [-1.0, 1.0, -1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [1.0, 0.0], // 3
    },
    // front face
    VertexPC {
        position: [-1.0, -1.0, 1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [0.0, 1.0], // 4
    },
    VertexPC {
        position: [-1.0, 1.0, 1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [0.0, 0.0], // 5
    },
    VertexPC {
        position: [1.0, 1.0, 1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [1.0, 0.0], // 6
    },
    VertexPC {
        position: [1.0, -1.0, 1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [1.0, 1.0], // 7
    },
    //bottom face
    VertexPC {
        position: [-1.0, -1.0, 1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [0.0, 0.0], // 8
    }, //4' bottom face
    VertexPC {
        position: [-1.0, -1.0, -1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [0.0, 1.0], // 9
    }, // 0' bottom face
    VertexPC {
        position: [1.0, -1.0, 1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [1.0, 0.0], // 10
    }, // 7' bottom face
    VertexPC {
        position: [1.0, -1.0, -1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [1.0, 1.0], // 11
    }, // 1' bottom face
    // top face
    VertexPC {
        position: [-1.0, 1.0, -1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [0.0, 0.0], // 12
    }, // 3' top face
    VertexPC {
        position: [-1.0, 1.0, 1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [0.0, 1.0], // 13
    }, // 5' top face
    VertexPC {
        position: [1.0, 1.0, -1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [1.0, 0.0], // 14
    }, // 2' top face
    VertexPC {
        position: [1.0, 1.0, 1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [1.0, 1.0], // 15
    }, // 6' top face
    // right face
    VertexPC {
        position: [1.0, 1.0, 1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [0.0, 0.0], // 16
    }, // 6' top face
    VertexPC {
        position: [1.0, -1.0, 1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [0.0, 1.0], // 17
    }, // 7' right face
    VertexPC {
        position: [1.0, 1.0, -1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [1.0, 0.0], // 18
    }, // 2' right face
    VertexPC {
        position: [1.0, -1.0, -1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [1.0, 1.0], // 19
    }, // 1' right face
    // left face
    VertexPC {
        position: [-1.0, 1.0, -1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [0.0, 0.0], // 20
    }, // 3' left face
    VertexPC {
        position: [-1.0, -1.0, -1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [0.0, 1.0], // 21
    }, // 0' left face
    VertexPC {
        position: [-1.0, 1.0, 1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [1.0, 0.0], // 22
    }, // 5' left face
    VertexPC {
        position: [-1.0, -1.0, 1.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [1.0, 1.0], // 23
    }, // 4' left face
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
