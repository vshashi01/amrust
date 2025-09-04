use instant_xml::{Error, FromXml, Id, Serializer, ToXml};

use crate::threemf_namespaces::CORE_TRIANGLESET_NS;

#[derive(FromXml, Debug, Clone, PartialEq, Eq)]
#[xml(ns(CORE_TRIANGLESET_NS), rename = "trianglesets")]
pub struct TriangleSets {
    // #[xml(ns(CORE_TRIANGLESET_NS))]
    pub trianglesets: Vec<TriangleSet>,
}

impl ToXml for TriangleSets {
    fn serialize<W: std::fmt::Write + ?Sized>(
        &self,
        field: Option<Id<'_>>,
        serializer: &mut Serializer<W>,
    ) -> Result<(), Error> {
        let prefix = match field {
            Some(id) => {
                let prefix = serializer.write_start(id.name, id.ns)?;
                serializer.end_start()?;
                Some((prefix, id.name))
            }
            None => {
                let _ = serializer.write_start("trianglesets", CORE_TRIANGLESET_NS)?;
                serializer.push(instant_xml::ser::Context {
                    default_ns: CORE_TRIANGLESET_NS,
                    prefixes: [],
                })?;

                serializer.end_start()?;
                Some((None, "trianglesets"))
            }
        };

        for set in &self.trianglesets {
            set.serialize(
                Some(Id {
                    ns: CORE_TRIANGLESET_NS,
                    name: "triangleset",
                }),
                serializer,
            )?;
        }

        if let Some((prefix, name)) = prefix {
            serializer.write_close(prefix, name)?;
        }

        Ok(())
    }
}

#[derive(FromXml, Debug, Clone, PartialEq, Eq)]
#[xml(ns(CORE_TRIANGLESET_NS), rename = "triangleset")]
pub struct TriangleSet {
    #[xml(attribute)]
    pub name: String,

    #[xml(attribute)]
    pub identifier: String,

    // #[xml(ns(CORE_TRIANGLESET_NS))]
    pub triangle_ref: Vec<TriangleRef>,

    // #[xml(ns(CORE_TRIANGLESET_NS))]
    pub triangle_refrange: Vec<TriangleRefRange>,
}

impl ToXml for TriangleSet {
    fn serialize<W: std::fmt::Write + ?Sized>(
        &self,
        field: Option<Id<'_>>,
        serializer: &mut Serializer<W>,
    ) -> Result<(), Error> {
        let prefix = match field {
            Some(id) => {
                let prefix = serializer.write_start(id.name, id.ns)?;
                Some((prefix, id.name))
            }
            None => None,
        };

        //work around to ensure the attributes do not get the prefix
        //in the case that default root namespace is not triangleset namespace
        let attr_ns = if let CORE_TRIANGLESET_NS = serializer.default_ns() {
            CORE_TRIANGLESET_NS
        } else {
            serializer.default_ns()
        };

        serializer.write_attr("name", attr_ns, &self.name)?;
        serializer.write_attr("identifier", attr_ns, &self.identifier)?;
        serializer.end_start()?;

        for triangle_ref in &self.triangle_ref {
            triangle_ref.serialize(field, serializer)?;
        }

        for triangle_refrange in &self.triangle_refrange {
            triangle_refrange.serialize(field, serializer)?;
        }

        if let Some((prefix, name)) = prefix {
            serializer.write_close(prefix, name)?;
        }

        Ok(())
    }
}

#[derive(ToXml, FromXml, Debug, Clone, PartialEq, Eq)]
#[xml(ns(CORE_TRIANGLESET_NS), rename = "ref")]
pub struct TriangleRef {
    #[xml(attribute)]
    pub index: usize,
}

#[derive(ToXml, FromXml, Debug, Clone, PartialEq, Eq)]
#[xml(ns(CORE_TRIANGLESET_NS), rename = "refrange")]
pub struct TriangleRefRange {
    #[xml(attribute)]
    pub startindex: usize,

    #[xml(attribute)]
    pub endindex: usize,
}

#[cfg(test)]
pub mod tests {
    use instant_xml::{from_str, to_string};
    use pretty_assertions::assert_eq;

    use crate::{
        core::{
            Mesh, Triangle, Triangles, Vertex, Vertices,
            triangle_set::{TriangleRef, TriangleRefRange, TriangleSet, TriangleSets},
        },
        threemf_namespaces::{CORE_NS, CORE_TRIANGLESET_NS, CORE_TRIANGLESET_PREFIX},
    };

    #[test]
    pub fn toxml_mesh_with_triangletest_test() {
        let xml_string = format!(
            r##"<mesh xmlns="{}" xmlns:{}="{}"><vertices><vertex x="-1" y="-1" z="0" /><vertex x="1" y="-1" z="0" /><vertex x="1" y="1" z="0" /><vertex x="-1" y="1" z="0" /></vertices><triangles><triangle v1="0" v2="1" v3="2" /><triangle v1="0" v2="2" v3="3" /></triangles><t:trianglesets><t:triangleset name="Triangle Set 1" identifier="someUniqueID1"><t:ref index="2" /><t:refrange startindex="22" endindex="102" /></t:triangleset><t:triangleset name="Triangle Set 2" identifier="someUniqueID2"><t:refrange startindex="1" endindex="12" /><t:refrange startindex="100236" endindex="4566893" /></t:triangleset></t:trianglesets></mesh>"##,
            CORE_NS, CORE_TRIANGLESET_PREFIX, CORE_TRIANGLESET_NS
        );
        let mesh = Mesh {
            vertices: Vertices {
                vertex: vec![
                    Vertex {
                        x: -1.0,
                        y: -1.0,
                        z: 0.0,
                    },
                    Vertex {
                        x: 1.0,
                        y: -1.0,
                        z: 0.0,
                    },
                    Vertex {
                        x: 1.0,
                        y: 1.0,
                        z: 0.0,
                    },
                    Vertex {
                        x: -1.0,
                        y: 1.0,
                        z: 0.0,
                    },
                ],
            },
            triangles: Triangles {
                triangle: vec![
                    Triangle {
                        v1: 0,
                        v2: 1,
                        v3: 2,
                        p1: None,
                        p2: None,
                        p3: None,
                        pid: None,
                    },
                    Triangle {
                        v1: 0,
                        v2: 2,
                        v3: 3,
                        p1: None,
                        p2: None,
                        p3: None,
                        pid: None,
                    },
                ],
            },
            trianglesets: Some(TriangleSets {
                trianglesets: vec![
                    TriangleSet {
                        name: "Triangle Set 1".to_owned(),
                        identifier: "someUniqueID1".to_owned(),
                        triangle_ref: vec![TriangleRef { index: 2 }],
                        triangle_refrange: vec![TriangleRefRange {
                            startindex: 22,
                            endindex: 102,
                        }],
                    },
                    TriangleSet {
                        name: "Triangle Set 2".to_owned(),
                        identifier: "someUniqueID2".to_owned(),
                        triangle_ref: vec![],
                        triangle_refrange: vec![
                            TriangleRefRange {
                                startindex: 1,
                                endindex: 12,
                            },
                            TriangleRefRange {
                                startindex: 100236,
                                endindex: 4566893,
                            },
                        ],
                    },
                ],
            }),
        };
        let mesh_string = to_string(&mesh).unwrap();

        assert_eq!(mesh_string, xml_string);
    }

    #[test]
    pub fn fromxml_mesh_with_triangleset_test() {
        let xml_string = format!(
            r##"<mesh xmlns="{}" xmlns:{}="{}"><vertices><vertex x="-1" y="-1" z="0" /><vertex x="1" y="-1" z="0" /><vertex x="1" y="1" z="0" /><vertex x="-1" y="1" z="0" /></vertices><triangles><triangle v1="0" v2="1" v3="2" /><triangle v1="0" v2="2" v3="3" /></triangles><t:trianglesets><t:triangleset name="Triangle Set 1" identifier="someUniqueID1"><t:ref index="2" /><t:refrange startindex="22" endindex="102" /></t:triangleset><t:triangleset name="Triangle Set 2" identifier="someUniqueID2"><t:refrange startindex="1" endindex="12" /><t:refrange startindex="100236" endindex="4566893" /></t:triangleset></t:trianglesets></mesh>"##,
            CORE_NS, CORE_TRIANGLESET_PREFIX, CORE_TRIANGLESET_NS
        );

        let mesh = from_str::<Mesh>(&xml_string).unwrap();

        assert_eq!(
            mesh,
            Mesh {
                vertices: Vertices {
                    vertex: vec![
                        Vertex {
                            x: -1.0,
                            y: -1.0,
                            z: 0.0
                        },
                        Vertex {
                            x: 1.0,
                            y: -1.0,
                            z: 0.0
                        },
                        Vertex {
                            x: 1.0,
                            y: 1.0,
                            z: 0.0
                        },
                        Vertex {
                            x: -1.0,
                            y: 1.0,
                            z: 0.0
                        }
                    ]
                },
                triangles: Triangles {
                    triangle: vec![
                        Triangle {
                            v1: 0,
                            v2: 1,
                            v3: 2,
                            p1: None,
                            p2: None,
                            p3: None,
                            pid: None,
                        },
                        Triangle {
                            v1: 0,
                            v2: 2,
                            v3: 3,
                            p1: None,
                            p2: None,
                            p3: None,
                            pid: None,
                        }
                    ]
                },
                trianglesets: Some(TriangleSets {
                    trianglesets: vec![
                        TriangleSet {
                            name: "Triangle Set 1".to_owned(),
                            identifier: "someUniqueID1".to_owned(),
                            triangle_ref: vec![TriangleRef { index: 2 }],
                            triangle_refrange: vec![TriangleRefRange {
                                startindex: 22,
                                endindex: 102,
                            }],
                        },
                        TriangleSet {
                            name: "Triangle Set 2".to_owned(),
                            identifier: "someUniqueID2".to_owned(),
                            triangle_ref: vec![],
                            triangle_refrange: vec![
                                TriangleRefRange {
                                    startindex: 1,
                                    endindex: 12,
                                },
                                TriangleRefRange {
                                    startindex: 100236,
                                    endindex: 4566893,
                                },
                            ],
                        },
                    ],
                })
            }
        )
    }

    #[test]
    pub fn toxml_trianglesets_test() {
        let xml_string = format!(
            r##"<trianglesets xmlns="{ns}"><triangleset name="Triangle Set 1" identifier="someUniqueID1"><ref index="2" /><refrange startindex="22" endindex="102" /></triangleset><triangleset name="Triangle Set 2" identifier="someUniqueID2"><refrange startindex="1" endindex="12" /><refrange startindex="100236" endindex="4566893" /></triangleset></trianglesets>"##,
            ns = CORE_TRIANGLESET_NS
        );
        let trianglesets = TriangleSets {
            trianglesets: vec![
                TriangleSet {
                    name: "Triangle Set 1".to_owned(),
                    identifier: "someUniqueID1".to_owned(),
                    triangle_ref: vec![TriangleRef { index: 2 }],
                    triangle_refrange: vec![TriangleRefRange {
                        startindex: 22,
                        endindex: 102,
                    }],
                },
                TriangleSet {
                    name: "Triangle Set 2".to_owned(),
                    identifier: "someUniqueID2".to_owned(),
                    triangle_ref: vec![],
                    triangle_refrange: vec![
                        TriangleRefRange {
                            startindex: 1,
                            endindex: 12,
                        },
                        TriangleRefRange {
                            startindex: 100236,
                            endindex: 4566893,
                        },
                    ],
                },
            ],
        };

        let trianglesets_string = to_string(&trianglesets).unwrap();

        assert_eq!(trianglesets_string, xml_string);
    }

    #[test]
    pub fn fromxml_trianglesets_test() {
        let xml_string = format!(
            r##"<trianglesets xmlns="{ns}"><triangleset name="Triangle Set 1" identifier="someUniqueID1"><ref index="2" /><refrange startindex="22" endindex="102" /></triangleset><triangleset name="Triangle Set 2" identifier="someUniqueID2"><refrange startindex="1" endindex="12" /><refrange startindex="100236" endindex="4566893" /></triangleset></trianglesets>"##,
            ns = CORE_TRIANGLESET_NS
        );
        let trianglesets = from_str::<TriangleSets>(&xml_string).unwrap();

        assert_eq!(
            trianglesets,
            TriangleSets {
                trianglesets: vec![
                    TriangleSet {
                        name: "Triangle Set 1".to_owned(),
                        identifier: "someUniqueID1".to_owned(),
                        triangle_ref: vec![TriangleRef { index: 2 }],
                        triangle_refrange: vec![TriangleRefRange {
                            startindex: 22,
                            endindex: 102,
                        }],
                    },
                    TriangleSet {
                        name: "Triangle Set 2".to_owned(),
                        identifier: "someUniqueID2".to_owned(),
                        triangle_ref: vec![],
                        triangle_refrange: vec![
                            TriangleRefRange {
                                startindex: 1,
                                endindex: 12,
                            },
                            TriangleRefRange {
                                startindex: 100236,
                                endindex: 4566893,
                            },
                        ],
                    },
                ],
            }
        );
    }
}
