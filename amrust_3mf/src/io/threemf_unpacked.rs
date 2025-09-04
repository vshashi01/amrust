use instant_xml::{from_str, to_string};
use zip::ZipArchive;

use crate::io::{
    content_types::{ContentTypes, DefaultContentTypeEnum},
    error::Error,
    relationship::{RelationshipType, Relationships},
};

use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{self, Read};
use std::path::PathBuf;

/// Represents a 3mf package, the nested folder structure of the parts
/// in the 3mf package will be flattened into respective dictionaries with
/// the key being the path of the part in the archive package.
#[derive(Debug, PartialEq)]
pub struct ThreemfUnpacked {
    /// The root model of the 3mf package.
    /// Expected to always exist and be a valid model with a [Build](crate::core::build::Build) object.
    pub root: String,

    /// The sub models contained in the file. Usually this is to represent the [Object](crate::core::object::Object)
    /// that are to be referenced in the [root](ThreemfPackage::root) model part.
    /// The key is the path of the model in the archive package.
    pub sub_models: HashMap<String, String>,

    /// The thumbnails contained in the file.
    /// The key is the path of the thumbnail in the archive package.
    /// The thumbnail paths defined in the [Model](crate::core::model::Model) object should match the keys in this dictionary.
    pub thumbnails: HashMap<String, Vec<u8>>,

    /// Bytes of additional data found through Unknown relationship
    /// The key is the path of the thumbnail in the archive package.
    pub unknown_parts: HashMap<String, Vec<u8>>,

    /// The relationships between the different parts in the 3mf package.
    /// The key is the path of the relationship file in the archive package.
    /// Always expected to have at least 1 relationship file,
    /// the root relationship file placed within "_rels" folder at the root of the package
    pub relationships: HashMap<String, String>,

    /// A summary of all Default Content Types that exists in the current 3mf package.
    /// The reader/writer will still read and write data not currently known to library as
    /// unknown data.
    /// The extensions defined in the [ContentTypes.xml]
    ///  file should match the extensions of the parts in the package.
    pub content_types: String,
}

impl ThreemfUnpacked {
    /// Reads a 3mf package from a type [Read] + [io::Seek].
    /// Expected to deal with nested parts of the 3mf package and flatten them into the respective dictionaries.
    /// Only If [process_sub_models] is set to true, it will process the sub models and thumbnails associated with the sub models in the package.
    /// Will return an error if the package is not a valid 3mf package or if the package contains unsupported content types.
    pub fn from_reader<R: Read + io::Seek>(
        reader: R,
        process_sub_models: bool,
    ) -> Result<Self, Error> {
        let mut zip = ZipArchive::new(reader)?;

        let content_types: ContentTypes;
        {
            let content_types_file = zip.by_name("[Content_Types].xml");

            content_types = match content_types_file {
                Ok(mut file) => {
                    let mut xml_string: String = Default::default();
                    let _ = file.read_to_string(&mut xml_string)?;

                    from_str::<ContentTypes>(&xml_string)?
                }
                Err(err) => {
                    return Err(Error::Zip(err));
                }
            }
        }

        let rels_ext = {
            let rels_content = content_types
                .defaults
                .iter()
                .find(|t| t.content_type == DefaultContentTypeEnum::Relationship);

            match rels_content {
                Some(rels) => &rels.extension,
                None => "rels",
            }
        };

        let root_rels_filename: &str = &format!("_rels/.{rels_ext}");

        let mut relationships = HashMap::<String, Relationships>::new();

        let mut models = HashMap::<String, String>::new();
        let mut thumbnails = HashMap::<String, Vec<u8>>::new();
        let mut unknown_parts = HashMap::<String, Vec<u8>>::new();
        let mut root_model_path: &str = "";

        let root_rels: Relationships =
            relationships_from_zip_by_name(&mut zip, root_rels_filename)?;
        let mut rels_strings_map = HashMap::<String, String>::new();

        let root_model_processed = process_rels(
            &mut zip,
            &root_rels,
            &mut models,
            &mut thumbnails,
            &mut unknown_parts,
        );
        match root_model_processed {
            Ok(_) => {
                let model_rels = root_rels
                    .relationships
                    .iter()
                    .find(|rels| rels.relationship_type == RelationshipType::Model);

                if let Some(root_model) = model_rels {
                    root_model_path = &root_model.target;
                    relationships.insert(root_rels_filename.to_owned(), root_rels.clone());
                }
            }
            Err(err) => return Err(err),
        }

        if process_sub_models {
            {
                for value in 0..zip.len() {
                    let file = zip.by_index(value)?;

                    if file.is_file()
                        && let Some(path) = file.enclosed_name()
                        && Some(OsStr::new(rels_ext)) == path.extension()
                        && path != PathBuf::from(root_rels_filename)
                    {
                        match path.to_str() {
                            Some(path_str) => {
                                let rels = relationships_from_zipfile(file)?;
                                relationships.insert(format!("/{path_str}"), rels);
                            }
                            None => {
                                return Err(Error::ReadError(
                                    "Failed to read the relationship file path".to_owned(),
                                ));
                            }
                        }
                    }
                }
            }

            for (dir_path, rels) in relationships.iter() {
                process_rels(
                    &mut zip,
                    rels,
                    &mut models,
                    &mut thumbnails,
                    &mut unknown_parts,
                )?;
                let rels_string = to_string(rels)?;
                rels_strings_map.insert(dir_path.clone(), rels_string);
            }
        }

        let content_types_string = to_string(&content_types)?;
        if let Some(root_model) = models.remove(root_model_path) {
            Ok(Self {
                root: root_model,
                sub_models: models,
                thumbnails,
                unknown_parts,
                relationships: rels_strings_map,
                content_types: content_types_string,
            })
        } else {
            Err(Error::ReadError("Root model not found".to_owned()))
        }
    }
}

fn relationships_from_zip_by_name<R: Read + io::Seek>(
    zip: &mut ZipArchive<R>,
    zip_filename: &str,
) -> Result<Relationships, Error> {
    let rels_file = zip.by_name(zip_filename);
    match rels_file {
        Ok(file) => relationships_from_zipfile(file),
        Err(err) => Err(Error::Zip(err)),
    }
}

fn relationships_from_zipfile<R: Read>(
    mut file: zip::read::ZipFile<'_, R>,
) -> Result<Relationships, Error> {
    let mut xml_string: String = Default::default();
    let _ = file.read_to_string(&mut xml_string)?;
    let rels = from_str::<Relationships>(&xml_string)?;

    Ok(rels)
}

fn process_rels<R: Read + io::Seek>(
    zip: &mut ZipArchive<R>,
    rels: &Relationships,
    models: &mut HashMap<String, String>,
    thumbnails: &mut HashMap<String, Vec<u8>>,
    unknown_parts: &mut HashMap<String, Vec<u8>>,
) -> Result<(), Error> {
    for rel in &rels.relationships {
        let name = try_strip_leading_slash(&rel.target);
        let zip_file = zip.by_name(name);

        match zip_file {
            Ok(mut file) => {
                if file.is_dir() {
                    return Err(Error::ReadError(format!(
                        r#"Found a folder "{:?}" instead of a file"#,
                        file.enclosed_name()
                    )));
                }

                match rel.relationship_type {
                    RelationshipType::Thumbnail => {
                        let mut bytes: Vec<u8> = vec![];
                        let _ = file.read_to_end(&mut bytes)?;
                        // println!("Thumbnail read bytes: {:?}", bytes.len());

                        thumbnails.insert(rel.target.clone(), bytes);
                    }
                    RelationshipType::Model => {
                        let mut xml_string: String = Default::default();
                        let _ = file.read_to_string(&mut xml_string)?;
                        // println!("Model bytes: {:?}", xml_string.len());

                        models.insert(rel.target.clone(), xml_string);
                    }
                    RelationshipType::Unknown(_) => {
                        let mut bytes: Vec<u8> = vec![];
                        let _ = file.read_to_end(&mut bytes)?;
                        // println!("Unknown bytes: {:?}", bytes.len());

                        unknown_parts.insert(rel.target.clone(), bytes);
                    }
                }
            }
            Err(err) => return Err(Error::Zip(err)),
        }
    }

    Ok(())
}

fn try_strip_leading_slash(target: &str) -> &str {
    match target.strip_prefix("/") {
        Some(stripped) => stripped,
        None => target,
    }
}

#[cfg(test)]
pub mod tests {
    use pretty_assertions::assert_eq;

    use super::ThreemfUnpacked;

    use std::io::Cursor;

    #[test]
    pub fn from_reader_root_model_test() {
        let bytes = include_bytes!("../../tests/data/third-party/P_XPX_0702_02.3mf");
        let reader = Cursor::new(bytes);

        let result = ThreemfUnpacked::from_reader(reader, true);

        match result {
            Ok(threemf) => {
                assert_eq!(threemf.content_types.len(), 332);
                assert_eq!(threemf.root.len(), 988);
                assert_eq!(threemf.sub_models.len(), 1);
                assert_eq!(threemf.thumbnails.len(), 1);
                assert_eq!(threemf.relationships.len(), 2);

                assert!(threemf.sub_models.contains_key("/3D/midway.model"));

                assert!(threemf.relationships.contains_key("_rels/.rels"));
                assert!(
                    threemf
                        .relationships
                        .contains_key("/3D/_rels/3dmodel.model.rels")
                );
                assert!(
                    threemf
                        .thumbnails
                        .contains_key("/Thumbnails/P_XPX_0702_02.png")
                )
            }
            Err(err) => panic!("{:?}", err),
        }
    }
}
