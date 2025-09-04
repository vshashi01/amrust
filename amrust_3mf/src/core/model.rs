use instant_xml::{FromXml, ToXml};

use crate::{
    core::{
        Mesh,
        build::{Build, Item},
        metadata::Metadata,
        object::{Object, ObjectType},
        resources::Resources,
    },
    threemf_namespaces::{CORE_NS, CORE_TRIANGLESET_NS, PROD_NS},
};

use std::vec;

#[derive(FromXml, ToXml, Debug, PartialEq)]
#[xml(ns(CORE_NS, p = PROD_NS, t = CORE_TRIANGLESET_NS), rename = "model")]
pub struct Model {
    #[xml(attribute)]
    pub xmlns: Option<String>,

    #[xml(attribute)]
    pub unit: Option<Unit>,

    #[xml(attribute)]
    pub requiredextensions: Option<String>,

    #[xml(attribute)]
    pub recommendedextensions: Option<String>,

    pub metadata: Vec<Metadata>,

    pub resources: Resources,

    pub build: Build,
}

/// Model measurement unit, default is millimeter
#[derive(ToXml, FromXml, Default, Debug, PartialEq, Eq)]
#[xml(scalar, rename_all = "lowercase")]
pub enum Unit {
    Micron,
    #[default]
    Millimeter,
    Centimeter,
    Inch,
    Foot,
    Meter,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            xmlns: Some(CORE_NS.to_owned()),
            requiredextensions: None,
            recommendedextensions: None,
            metadata: Vec::new(),
            resources: Resources::default(),
            build: Build::default(),
            unit: Some(Unit::default()),
        }
    }
}

impl From<Mesh> for Model {
    fn from(mesh: Mesh) -> Self {
        let object = Object {
            id: 1,
            objecttype: Some(ObjectType::Model),
            thumbnail: None,
            partnumber: None,
            name: Some("Mesh".to_owned()),
            pid: None,
            pindex: None,
            uuid: None,
            mesh: Some(mesh),
            components: None,
        };
        let resources = Resources {
            object: vec![object],
            basematerials: vec![],
        };
        let build = Build {
            uuid: None,
            item: vec![Item {
                objectid: 1,
                transform: None,
                partnumber: None,
                path: None,
                uuid: None,
            }],
        };
        Model {
            resources,
            build,
            ..Default::default()
        }
    }
}

#[cfg(test)]
pub mod test {
    use instant_xml::{FromXml, ToXml, from_str, to_string};
    use pretty_assertions::assert_eq;

    use crate::{
        core::{
            build::{Build, Item},
            component::{Component, Components},
            metadata::Metadata,
            object::{Object, ObjectType},
            resources::Resources,
        },
        threemf_namespaces::{
            CORE_NS, CORE_TRIANGLESET_NS, CORE_TRIANGLESET_PREFIX, PROD_NS, PROD_PREFIX,
        },
    };

    use super::{Model, Unit};

    #[test]
    pub fn toxml_simple_model_test() {
        let xml_string = format!(
            r#"<model xmlns="{}" xmlns:{}="{}" xmlns:{}="{}" unit="millimeter"><metadata name="Trial Metadata" /><resources><object id="346" type="model" name="test part"></object></resources><build><item objectid="346" /></build></model>"#,
            CORE_NS, PROD_PREFIX, PROD_NS, CORE_TRIANGLESET_PREFIX, CORE_TRIANGLESET_NS
        );
        let model = Model {
            xmlns: None,
            unit: Some(Unit::Millimeter),
            requiredextensions: None,
            recommendedextensions: None,
            metadata: vec![Metadata {
                name: "Trial Metadata".to_owned(),
                preserve: None,
                value: None,
            }],
            resources: Resources {
                basematerials: vec![],
                object: vec![Object {
                    id: 346,
                    objecttype: Some(ObjectType::Model),
                    thumbnail: None,
                    partnumber: None,
                    name: Some("test part".to_owned()),
                    pid: None,
                    pindex: None,
                    uuid: None,
                    mesh: None,
                    components: None,
                }],
            },
            build: Build {
                uuid: None,
                item: vec![Item {
                    objectid: 346,
                    transform: None,
                    partnumber: None,
                    path: None,
                    uuid: None,
                }],
            },
        };
        let model_string = to_string(&model).unwrap();

        assert_eq!(model_string, xml_string);
    }

    #[test]
    pub fn fromxml_simple_model_test() {
        let xml_string = format!(
            r#"<model xmlns="{}" xml:lang="en-us" unit="millimeter"><metadata name="Trial Metadata" /><resources><object id="346" type="model" name="test part"></object></resources><build><item objectid="346" /></build></model>"#,
            CORE_NS
        );
        let model = from_str::<Model>(&xml_string).unwrap();

        assert_eq!(
            model,
            Model {
                xmlns: None,
                unit: Some(Unit::Millimeter),
                requiredextensions: None,
                recommendedextensions: None,
                metadata: vec![Metadata {
                    name: "Trial Metadata".to_owned(),
                    preserve: None,
                    value: None,
                }],
                resources: Resources {
                    basematerials: vec![],
                    object: vec![Object {
                        id: 346,
                        objecttype: Some(ObjectType::Model),
                        thumbnail: None,
                        partnumber: None,
                        name: Some("test part".to_owned()),
                        pid: None,
                        pindex: None,
                        uuid: None,
                        mesh: None,
                        components: None,
                    }],
                },
                build: Build {
                    uuid: None,
                    item: vec![Item {
                        objectid: 346,
                        transform: None,
                        partnumber: None,
                        path: None,
                        uuid: None,
                    }],
                },
            }
        );
    }

    #[test]
    pub fn fromxml_production_model_test() {
        const CUSTOM_PROD_PREFIX: &str = "custom";
        let xml_string = format!(
            r#"<model xmlns="{}" xmlns:{}="{}" xml:lang="en-us" unit="millimeter"><metadata name="Trial Metadata" /><resources><object id="346" type="model" name="test part" {}:UUID="someObjectUUID"><components><component objectid="1" {}:path="//somePath//Component" {}:UUID="someComponentUUID" /></components></object></resources><build {}:UUID="someBuildUUID"><item objectid="346" {}:UUID="someItemUUID"/></build></model>"#,
            CORE_NS,
            CUSTOM_PROD_PREFIX,
            PROD_NS,
            CUSTOM_PROD_PREFIX,
            CUSTOM_PROD_PREFIX,
            CUSTOM_PROD_PREFIX,
            CUSTOM_PROD_PREFIX,
            CUSTOM_PROD_PREFIX,
        );
        let model = from_str::<Model>(&xml_string).unwrap();

        assert_eq!(
            model,
            Model {
                xmlns: None,
                unit: Some(Unit::Millimeter),
                requiredextensions: None,
                recommendedextensions: None,
                metadata: vec![Metadata {
                    name: "Trial Metadata".to_owned(),
                    preserve: None,
                    value: None,
                }],
                resources: Resources {
                    basematerials: vec![],
                    object: vec![Object {
                        id: 346,
                        objecttype: Some(ObjectType::Model),
                        thumbnail: None,
                        partnumber: None,
                        name: Some("test part".to_owned()),
                        pid: None,
                        pindex: None,
                        uuid: Some("someObjectUUID".to_owned()),
                        mesh: None,
                        components: Some(Components {
                            component: vec![Component {
                                objectid: 1,
                                transform: None,
                                path: Some("//somePath//Component".to_owned()),
                                uuid: Some("someComponentUUID".to_owned()),
                            }]
                        }),
                    }],
                },
                build: Build {
                    uuid: Some("someBuildUUID".to_owned()),
                    item: vec![Item {
                        objectid: 346,
                        transform: None,
                        partnumber: None,
                        path: None,
                        uuid: Some("someItemUUID".to_owned()),
                    }],
                },
            }
        );
    }

    #[derive(Debug, ToXml, FromXml, PartialEq, Eq)]
    struct UnitsVector {
        unit: Vec<Unit>,
    }

    #[test]
    pub fn toxml_units_test() {
        let xml_string = "<UnitsVector><unit>micron</unit><unit>millimeter</unit><unit>centimeter</unit><unit>inch</unit><unit>foot</unit><unit>meter</unit></UnitsVector>";
        let unitsvector = UnitsVector {
            unit: vec![
                Unit::Micron,
                Unit::Millimeter,
                Unit::Centimeter,
                Unit::Inch,
                Unit::Foot,
                Unit::Meter,
            ],
        };
        let unitsvector_string = to_string(&unitsvector).unwrap();

        assert_eq!(unitsvector_string, xml_string);
    }

    #[test]
    pub fn fromxml_units_test() {
        let xml_string = r#"<UnitsVector><unit>micron</unit><unit>millimeter</unit><unit>centimeter</unit><unit>inch</unit><unit>foot</unit><unit>meter</unit></UnitsVector>"#;
        let unitsvector = from_str::<UnitsVector>(xml_string).unwrap();

        assert_eq!(
            unitsvector,
            UnitsVector {
                unit: vec![
                    Unit::Micron,
                    Unit::Millimeter,
                    Unit::Centimeter,
                    Unit::Inch,
                    Unit::Foot,
                    Unit::Meter,
                ],
            }
        );
    }
}
