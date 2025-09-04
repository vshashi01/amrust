use instant_xml::{Error, FromXml, Kind, ToXml};

const RELATIONSHIP_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

/// Represents a relationship of a single part in the 3mf package along with its [RelationshipType]
/// and target path of the part in the archive.
#[derive(ToXml, FromXml, Debug, Clone, PartialEq, Eq)]
#[xml(ns(RELATIONSHIP_NS))]
pub struct Relationship {
    /// The unique identifier of the relationship.
    #[xml(attribute, rename = "Id")]
    pub id: String,

    /// Target path of the part in the archive.
    #[xml(attribute, rename = "Target")]
    pub target: String,

    /// The actual relationship of the target part
    #[xml(attribute, rename = "Type")]
    pub relationship_type: RelationshipType,
}

/// Represents a collection of [Relationship]s where each collection is an independent
/// relationship part in the 3mf package. A single 3mf package may contain multiple [Relationships].
#[derive(ToXml, FromXml, Debug, Clone, PartialEq, Eq)]
#[xml(ns(RELATIONSHIP_NS))]
pub struct Relationships {
    pub relationships: Vec<Relationship>,
}

/// Represents the type of relationship of a part in the 3mf package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationshipType {
    /// Represents a thumbnail part in the package.
    Thumbnail,

    /// Represents a model part in the package.
    Model,

    /// Represents an unknown part currently by this library
    /// The namespaces of the relationship type is stored in the tuple.
    Unknown(String),
}

const THUMBNAIL_TYPE_NS: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail";
const MODEL_TYPE_NS: &str = "http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel";

impl ToXml for RelationshipType {
    fn serialize<W: std::fmt::Write + ?Sized>(
        &self,
        _: Option<instant_xml::Id<'_>>,
        serializer: &mut instant_xml::Serializer<W>,
    ) -> Result<(), Error> {
        let ns_str = match self {
            Self::Thumbnail => THUMBNAIL_TYPE_NS,
            Self::Model => MODEL_TYPE_NS,
            Self::Unknown(value) => value,
        };

        serializer.write_str(ns_str)?;
        Ok(())
    }
}

impl<'xml> FromXml<'xml> for RelationshipType {
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
    ) -> Result<(), Error> {
        if into.is_some() {
            return Err(Error::DuplicateValue(field));
        }

        let value = match deserializer.take_str()? {
            Some(value) => value,
            None => return Err(Error::MissingValue("No RelationshipType string found")),
        };

        match value.into_owned().as_ref() {
            THUMBNAIL_TYPE_NS => *into = Some(Self::Thumbnail),
            MODEL_TYPE_NS => *into = Some(Self::Model),
            value => *into = Some(Self::Unknown(value.to_owned())),
        }

        Ok(())
    }

    type Accumulator = Option<Self>;

    const KIND: Kind = Kind::Scalar;
}

#[cfg(test)]
pub mod tests {
    use instant_xml::{from_str, to_string};
    use pretty_assertions::assert_eq;

    use super::{
        MODEL_TYPE_NS, RELATIONSHIP_NS, Relationship, RelationshipType, Relationships,
        THUMBNAIL_TYPE_NS,
    };

    #[test]
    pub fn toxml_relationships_test() {
        let xml_string = format!(
            r#"<Relationships xmlns="{}"><Relationship Id="someId" Target="//somePath//Of//Resources" Type="{}" /><Relationship Id="someId1" Target="//somePath//Of//Resources" Type="{}" /><Relationship Id="someId2" Target="//somePath//Of//Unknown" Type="unknown" /></Relationships>"#,
            RELATIONSHIP_NS, MODEL_TYPE_NS, THUMBNAIL_TYPE_NS
        );
        let relationships = Relationships {
            relationships: vec![
                Relationship {
                    id: "someId".to_owned(),
                    target: "//somePath//Of//Resources".to_owned(),
                    relationship_type: RelationshipType::Model,
                },
                Relationship {
                    id: "someId1".to_owned(),
                    target: "//somePath//Of//Resources".to_owned(),
                    relationship_type: RelationshipType::Thumbnail,
                },
                Relationship {
                    id: "someId2".to_owned(),
                    target: "//somePath//Of//Unknown".to_owned(),
                    relationship_type: RelationshipType::Unknown("unknown".to_owned()),
                },
            ],
        };
        let relationships_string = to_string(&relationships).unwrap();

        assert_eq!(relationships_string, xml_string);
    }

    #[test]
    pub fn fromxml_relationships_test() {
        let xml_string = format!(
            r#"<Relationships xmlns="{}"><Relationship Id="someId" Target="//somePath//Of//Resources" Type="{}" /><Relationship Id="someId1" Target="//somePath//Of//Resources" Type="{}" /><Relationship Id="someId2" Target="//somePath//Of//Unknown" Type="unknown" /></Relationships>"#,
            RELATIONSHIP_NS, MODEL_TYPE_NS, THUMBNAIL_TYPE_NS
        );
        let relationships = from_str::<Relationships>(&xml_string).unwrap();

        assert_eq!(
            relationships,
            Relationships {
                relationships: vec![
                    Relationship {
                        id: "someId".to_owned(),
                        target: "//somePath//Of//Resources".to_owned(),
                        relationship_type: RelationshipType::Model,
                    },
                    Relationship {
                        id: "someId1".to_owned(),
                        target: "//somePath//Of//Resources".to_owned(),
                        relationship_type: RelationshipType::Thumbnail,
                    },
                    Relationship {
                        id: "someId2".to_owned(),
                        target: "//somePath//Of//Unknown".to_owned(),
                        relationship_type: RelationshipType::Unknown("unknown".to_owned()),
                    },
                ],
            }
        );
    }
}
