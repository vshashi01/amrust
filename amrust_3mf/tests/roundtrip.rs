use pretty_assertions::assert_eq;

use amrust_3mf::{
    core::{
        build::{Build, Item},
        mesh::{Mesh, Triangle, Triangles, Vertex, Vertices},
        model::{Model, Unit},
        object::{Object, ObjectType},
        resources::Resources,
    },
    io::{
        ThreemfPackage,
        content_types::{ContentTypes, DefaultContentTypeEnum, DefaultContentTypes},
        relationship::{Relationship, RelationshipType, Relationships},
    },
};

use std::{collections::HashMap, io::Cursor};

#[test]
fn roundtrip_threemfpackage_test() {
    let vertices = Vertices {
        vertex: vec![
            Vertex {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            Vertex {
                x: 0.0,
                y: 2.0,
                z: 0.0,
            },
            Vertex {
                x: 0.0,
                y: 1.0,
                z: 1.0,
            },
        ],
    };

    let triangles = Triangles {
        triangle: vec![Triangle {
            v1: 0,
            v2: 1,
            v3: 2,
            p1: None,
            p2: None,
            p3: None,
            pid: None,
        }],
    };

    let mesh = Mesh {
        triangles,
        vertices,
        trianglesets: None,
    };

    let write_package = ThreemfPackage {
        root: Model {
            xmlns: None,
            unit: Some(Unit::Millimeter),
            requiredextensions: None,
            recommendedextensions: None,
            metadata: vec![],
            resources: Resources {
                object: vec![Object {
                    id: 1,
                    objecttype: Some(ObjectType::Model),
                    thumbnail: None,
                    partnumber: None,
                    name: Some("Mesh".to_owned()),
                    pid: None,
                    pindex: None,
                    uuid: None,
                    mesh: Some(mesh.clone()),
                    components: None,
                }],
                basematerials: vec![],
            },
            build: Build {
                uuid: None,
                item: vec![Item {
                    objectid: 1,
                    transform: None,
                    partnumber: None,
                    path: None,
                    uuid: None,
                }],
            },
        },
        sub_models: HashMap::new(),
        thumbnails: HashMap::new(),
        unknown_parts: HashMap::new(),
        relationships: HashMap::from([(
            "_rels/.rels".to_owned(),
            Relationships {
                relationships: vec![Relationship {
                    id: "rel0".to_owned(),
                    target: "3D/3Dmodel.model".to_owned(),
                    relationship_type: RelationshipType::Model,
                }],
            },
        )]),
        content_types: ContentTypes {
            defaults: vec![
                DefaultContentTypes {
                    extension: "rels".to_owned(),
                    content_type: DefaultContentTypeEnum::Relationship,
                },
                DefaultContentTypes {
                    extension: "model".to_owned(),
                    content_type: DefaultContentTypeEnum::Model,
                },
            ],
        },
    };

    let mut buf = Cursor::new(Vec::new());

    write_package
        .write(&mut buf)
        .expect("Error writing package");
    let models = ThreemfPackage::from_reader(&mut buf, false).expect("Error reading package");
    assert_eq!(models, write_package);
}
