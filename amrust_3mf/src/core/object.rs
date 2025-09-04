use instant_xml::{FromXml, ToXml};

use crate::{
    core::{Mesh, component::Components},
    threemf_namespaces::{CORE_NS, PROD_NS},
};

#[derive(FromXml, ToXml, PartialEq, Debug)]
#[xml(ns(CORE_NS, p=PROD_NS), rename="object")]
pub struct Object {
    #[xml(attribute)]
    pub id: usize,

    #[xml(rename = "type", attribute)]
    pub objecttype: Option<ObjectType>,

    #[xml(rename = "thumbnail", attribute)]
    pub thumbnail: Option<String>,

    #[xml(attribute)]
    pub partnumber: Option<String>,

    #[xml(attribute)]
    pub name: Option<String>,

    #[xml(attribute)]
    pub pid: Option<usize>,

    #[xml(attribute)]
    pub pindex: Option<usize>,

    #[xml(attribute, ns(PROD_NS), rename = "UUID")]
    pub uuid: Option<String>,

    pub mesh: Option<Mesh>,

    pub components: Option<Components>,
}

#[derive(Debug, ToXml, FromXml, Default, PartialEq, Eq)]
#[xml(scalar, rename_all = "lowercase")]
pub enum ObjectType {
    #[default]
    Model,
    Support,
    SolidSupport,
    Surface,
    Other,
}

#[cfg(test)]
pub mod test {
    use instant_xml::{FromXml, ToXml, from_str, to_string};
    use pretty_assertions::assert_eq;

    use crate::{
        core::{
            Mesh, Triangles, Vertices,
            component::{Component, Components},
        },
        threemf_namespaces::{
            CORE_NS, CORE_TRIANGLESET_NS, CORE_TRIANGLESET_PREFIX, PROD_NS, PROD_PREFIX,
        },
    };

    use super::{Object, ObjectType};

    use std::vec;

    #[test]
    pub fn toxml_simple_object_test() {
        let xml_string = format!(
            r#"<object xmlns="{}" xmlns:{}="{}" id="4"></object>"#,
            CORE_NS, PROD_PREFIX, PROD_NS
        );
        let object = Object {
            id: 4,
            objecttype: None,
            thumbnail: None,
            partnumber: None,
            name: None,
            pid: None,
            pindex: None,
            uuid: None,
            mesh: None,
            components: None,
        };
        let object_string = to_string(&object).unwrap();

        assert_eq!(object_string, xml_string);
    }

    #[test]
    pub fn fromxml_simple_object_test() {
        let xml_string = format!(r#"<object xmlns="{}" id="4"></object>"#, CORE_NS);
        let object = from_str::<Object>(&xml_string).unwrap();

        assert_eq!(
            object,
            Object {
                id: 4,
                objecttype: None,
                thumbnail: None,
                partnumber: None,
                name: None,
                pid: None,
                pindex: None,
                uuid: None,
                mesh: None,
                components: None,
            }
        );
    }

    #[test]
    pub fn toxml_production_object_test() {
        let xml_string = format!(
            r#"<object xmlns="{}" xmlns:{}="{}" id="4" {}:UUID="someUUID"></object>"#,
            CORE_NS, PROD_PREFIX, PROD_NS, PROD_PREFIX
        );
        let object = Object {
            id: 4,
            objecttype: None,
            thumbnail: None,
            partnumber: None,
            name: None,
            pid: None,
            pindex: None,
            uuid: Some("someUUID".to_owned()),
            mesh: None,
            components: None,
        };
        let object_string = to_string(&object).unwrap();

        assert_eq!(object_string, xml_string);
    }

    #[test]
    pub fn fromxml_production_object_test() {
        const CUSTOM_PROD_PREFIX: &str = "custom";
        let xml_string = format!(
            r#"<object xmlns="{}" xmlns:{}="{}" id="4" {}:UUID="someUUID"></object>"#,
            CORE_NS, CUSTOM_PROD_PREFIX, PROD_NS, CUSTOM_PROD_PREFIX,
        );
        let object = from_str::<Object>(&xml_string).unwrap();

        assert_eq!(
            object,
            Object {
                id: 4,
                objecttype: None,
                thumbnail: None,
                partnumber: None,
                name: None,
                pid: None,
                pindex: None,
                uuid: Some("someUUID".to_owned()),
                mesh: None,
                components: None,
            }
        );
    }

    #[test]
    pub fn toxml_intermediate_object_test() {
        let xml_string = format!(
            r#"<object xmlns="{}" xmlns:{}="{}" id="4" type="model" thumbnail="\thumbnail\part_thumbnail.png" partnumber="part_1" name="Object Part"></object>"#,
            CORE_NS, PROD_PREFIX, PROD_NS
        );
        let object = Object {
            id: 4,
            objecttype: Some(ObjectType::Model),
            thumbnail: Some("\\thumbnail\\part_thumbnail.png".to_string()),
            partnumber: Some("part_1".to_string()),
            name: Some("Object Part".to_string()),
            pid: None,
            pindex: None,
            uuid: None,
            mesh: None,
            components: None,
        };
        let object_string = to_string(&object).unwrap();
        println!("{}", object_string);

        assert_eq!(object_string, xml_string);
    }

    #[test]
    pub fn fromxml_intermediate_object_test() {
        let xml_string = format!(
            r#"<object xmlns="{}" id="4" type="model" thumbnail="\thumbnail\part_thumbnail.png" partnumber="part_1" name="Object Part" pid="123" pindex="123"></object>"#,
            CORE_NS
        );
        let object = from_str::<Object>(&xml_string).unwrap();

        assert_eq!(
            object,
            Object {
                id: 4,
                objecttype: Some(ObjectType::Model),
                thumbnail: Some("\\thumbnail\\part_thumbnail.png".to_string()),
                partnumber: Some("part_1".to_string()),
                name: Some("Object Part".to_string()),
                pid: Some(123),
                pindex: Some(123),
                uuid: None,
                mesh: None,
                components: None,
            }
        );
    }

    #[test]
    pub fn roundtrip_advanced_mesh_object_test() {
        let xml_string = format!(
            r##"<object xmlns="{}" xmlns:{}="{}" id="4" type="model" thumbnail="\thumbnail\part_thumbnail.png" partnumber="part_1" name="Object Part"><mesh xmlns:{}="{}"><vertices></vertices><triangles></triangles></mesh></object>"##,
            CORE_NS, PROD_PREFIX, PROD_NS, CORE_TRIANGLESET_PREFIX, CORE_TRIANGLESET_NS
        );
        let object = Object {
            id: 4,
            objecttype: Some(ObjectType::Model),
            thumbnail: Some("\\thumbnail\\part_thumbnail.png".to_string()),
            partnumber: Some("part_1".to_string()),
            name: Some("Object Part".to_string()),
            pid: None,
            pindex: None,
            uuid: None,
            mesh: Some(Mesh {
                vertices: Vertices { vertex: vec![] },
                triangles: Triangles { triangle: vec![] },
                trianglesets: None,
            }),
            components: None,
        };
        let object_string = to_string(&object).unwrap();
        let roundtrip_object = from_str::<Object>(&object_string).unwrap();

        assert_eq!(object_string, xml_string);
        assert_eq!(roundtrip_object, object);
    }

    #[test]
    pub fn roundtrip_advanced_component_object_test() {
        let xml_string = format!(
            r##"<object xmlns="{}" xmlns:{}="{}" id="4" type="model" thumbnail="\thumbnail\part_thumbnail.png" partnumber="part_1" name="Object Part"><components><component objectid="23" /></components></object>"##,
            CORE_NS, PROD_PREFIX, PROD_NS
        );
        let object = Object {
            id: 4,
            objecttype: Some(ObjectType::Model),
            thumbnail: Some("\\thumbnail\\part_thumbnail.png".to_string()),
            partnumber: Some("part_1".to_string()),
            name: Some("Object Part".to_string()),
            pid: None,
            pindex: None,
            uuid: None,
            mesh: None,
            components: Some(Components {
                component: vec![Component {
                    objectid: 23,
                    transform: None,
                    path: None,
                    uuid: None,
                }],
            }),
        };
        let object_string = to_string(&object).unwrap();
        let roundtrip_object = from_str::<Object>(&object_string).unwrap();

        assert_eq!(object_string, xml_string);
        assert_eq!(roundtrip_object, object);
    }

    #[derive(Debug, ToXml, FromXml)]
    pub struct ObjectTypes {
        objecttype: Vec<ObjectType>,
    }

    #[test]
    pub fn toxml_objecttype_test() {
        let xml_string = format!(
            r#"<ObjectTypes><{s}>model</{s}><{s}>support</{s}><{s}>solidsupport</{s}><{s}>support</{s}><{s}>other</{s}></ObjectTypes>"#,
            s = "objecttype"
        );
        let objecttypes = ObjectTypes {
            objecttype: vec![
                ObjectType::Model,
                ObjectType::Support,
                ObjectType::SolidSupport,
                ObjectType::Support,
                ObjectType::Other,
            ],
        };
        let objecttype_string = to_string(&objecttypes).unwrap();

        assert_eq!(objecttype_string, xml_string);
    }
}
