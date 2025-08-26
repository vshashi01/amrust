use instant_xml::{Error, FromXml, Kind, ToXml};

use crate::threemf_namespaces::CORE_NS;

//ToDo: Add additional optional fields on Metadata
#[derive(FromXml, ToXml, Debug, PartialEq, Eq)]
#[xml(ns(CORE_NS), rename = "metadata")]
pub struct Metadata {
    #[xml(attribute)]
    pub name: String,

    #[xml(attribute)]
    pub preserve: Option<Preserve>,

    #[xml(direct)]
    pub value: Option<String>,
}

#[derive(Debug, ToXml, FromXml, PartialEq, Eq)]
#[xml(ns(CORE_NS), rename = "metadatagroup")]
pub struct MetadataGroup {
    pub metadata: Vec<Metadata>,
}

#[derive(ToXml, Debug, PartialEq, Eq)]
#[xml(ns(CORE_NS), rename = "preserve")]
pub struct Preserve(bool);

impl<'xml> FromXml<'xml> for Preserve {
    fn matches(id: instant_xml::Id<'_>, field: Option<instant_xml::Id<'_>>) -> bool {
        match field {
            Some(field) => id == field,
            None => false,
        }
    }

    fn deserialize<'cx>(
        into: &mut Self::Accumulator,
        field: &'static str,
        deserializer: &mut instant_xml::Deserializer<'cx, 'xml>,
    ) -> Result<(), instant_xml::Error> {
        if into.is_some() {
            return Err(Error::DuplicateValue(field));
        }

        let value = match deserializer.take_str()? {
            Some(value) => value,
            None => return Err(Error::MissingValue("No Must Preserve value found")),
        };

        if let Ok(must_preserve) = value.parse::<bool>() {
            *into = Some(Preserve(must_preserve));
            Ok(())
        } else {
            Err(Error::MissingValue("Not a valid boolean value"))
        }
    }

    type Accumulator = Option<Self>;

    const KIND: Kind = Kind::Scalar;
}

#[cfg(test)]
pub mod tests {
    use instant_xml::{from_str, to_string};
    use pretty_assertions::assert_eq;

    use crate::{core::metadata::Preserve, threemf_namespaces::CORE_NS};

    use super::{Metadata, MetadataGroup};

    #[test]
    pub fn fromxml_metadata_test() {
        let xml_string = format!(
            r#"<metadata xmlns="{}" name="Copyright">Copyright (c) 2018 3MF Consortium. All rights reserved.</metadata>"#,
            CORE_NS
        );
        let metadata = from_str::<Metadata>(&xml_string).unwrap();

        assert_eq!(
            metadata,
            Metadata {
                name: "Copyright".to_string(),
                preserve: None,
                value: Some("Copyright (c) 2018 3MF Consortium. All rights reserved.".to_string())
            }
        )
    }

    #[test]
    pub fn toxml_metadata_test() {
        let xml_string = format!(
            r#"<metadata xmlns="{}" name="Copyright">Copyright (c) 2018 3MF Consortium. All rights reserved.</metadata>"#,
            CORE_NS
        );
        let metadata = Metadata {
            name: "Copyright".to_string(),
            preserve: None,
            value: Some("Copyright (c) 2018 3MF Consortium. All rights reserved.".to_string()),
        };
        let metadata_string = to_string(&metadata).unwrap();

        assert_eq!(metadata_string, xml_string);
    }

    #[test]
    pub fn fromxml_simple_metadata_test() {
        let xml_string = format!(r#"<metadata xmlns="{}" name="From Test"/>"#, CORE_NS);
        let metadata = from_str::<Metadata>(&xml_string).unwrap();

        assert_eq!(
            metadata,
            Metadata {
                name: "From Test".to_string(),
                preserve: None,
                value: None,
            }
        )
    }

    #[test]
    pub fn toxml_simple_metadata_test() {
        let xml_string = format!(r#"<metadata xmlns="{}" name="From Test" />"#, CORE_NS);
        let metadata = Metadata {
            name: "From Test".to_string(),
            preserve: None,
            value: None,
        };
        let metadata_string = to_string(&metadata).unwrap();

        assert_eq!(metadata_string, xml_string);
    }

    #[test]
    pub fn fromxml_metadatagroup_test() {
        let xml_string = format!(
            r#"<metadatagroup xmlns="{}"><metadata name="From Test"></metadata><metadata name="From Test 2"></metadata></metadatagroup>"#,
            CORE_NS
        );
        let metadatagroup = from_str::<MetadataGroup>(&xml_string).unwrap();

        assert_eq!(
            metadatagroup,
            MetadataGroup {
                metadata: vec![
                    Metadata {
                        name: "From Test".to_string(),
                        preserve: None,
                        value: None,
                    },
                    Metadata {
                        name: "From Test 2".to_string(),
                        preserve: None,
                        value: None,
                    }
                ]
            }
        )
    }

    #[test]
    pub fn toxml_metadatagroup_test() {
        let xml_string = format!(
            r#"<metadatagroup xmlns="{}"><metadata name="From Test"></metadata><metadata name="From Test 2"></metadata></metadatagroup>"#,
            CORE_NS
        );
        let metadatagroup = MetadataGroup {
            metadata: vec![
                Metadata {
                    name: "From Test".to_string(),
                    preserve: None,
                    value: Some("".to_string()),
                },
                Metadata {
                    name: "From Test 2".to_string(),
                    preserve: None,
                    value: Some("".to_string()),
                },
            ],
        };
        let metadatagroup_string = to_string(&metadatagroup).unwrap();

        assert_eq!(metadatagroup_string, xml_string);
    }
}
