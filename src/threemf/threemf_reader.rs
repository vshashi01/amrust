use anyhow::Result;
use quick_xml::de::Deserializer;
use serde::Deserialize;
use std::io::{self, Read};
use threemf::model::Model;
use xml_dom::{level2::RefNode, parser::read_xml};

use zip::ZipArchive;

pub fn load_threemf_get_root_model_file_as_string<R: io::Read + io::Seek>(
    reader: R,
) -> Result<String> {
    let mut zip = ZipArchive::new(reader)?;
    let mut list_string = String::new();

    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        // let message = format!("File {} is {}", i + 1, file.name());
        if file.name().ends_with(".model") {
            file.read_to_string(&mut list_string)?;
            break;
        }
    }

    Ok(list_string)
}

pub fn get_model_from_3mf_model_file_string(xml_content: &String) -> Result<Model> {
    let mut de = Deserializer::from_str(&xml_content);
    let model = Model::deserialize(&mut de)?;

    Ok(model)
}

pub fn get_xml_dom_from_3mf_model_file_string(xml_content: &String) -> Result<RefNode> {
    let dom = read_xml(xml_content)?;

    Ok(dom)
}

#[cfg(test)]
mod tests {
    use threemf::model::ObjectData;
    use xml_dom::level2::Node;

    use super::*;
    use std::{
        env::{self},
        fs::{self, File},
        path::PathBuf,
    };

    fn open_file_from_test_resource(file_name: &str) -> File {
        let root_dir = &env::var("CARGO_MANIFEST_DIR").expect("$CARGO_MANIFEST_DIR");
        let mut test_file_path = PathBuf::from(root_dir);
        test_file_path.push("test_resources\\");
        test_file_path.push(file_name);
        // println!("{:?}", test_file_path);

        fs::File::open(test_file_path).unwrap()
    }

    #[test]
    fn test_load_threemf_get_root_model_file_as_string() {
        let file = open_file_from_test_resource("fake-3mf.3mf");
        let string = load_threemf_get_root_model_file_as_string(file).unwrap();
        assert!(
            string == "Test Passed",
            "3dmodel.model file is not read correctly"
        );
    }

    #[test]
    fn test_get_model_from_3mf_model_file_string() {
        let file = open_file_from_test_resource("box.3mf");
        let string = load_threemf_get_root_model_file_as_string(file).unwrap();
        let model = get_model_from_3mf_model_file_string(&string).unwrap();

        assert!(
            model.resources.object.len() == 1,
            "Number of objects is wrong"
        );

        assert!(
            model.build.item.len() == 1,
            "Number of build items do not match"
        );

        if let ObjectData::Mesh(mesh) = &model.resources.object[0].object {
            assert!(
                mesh.vertices.vertex.last().unwrap().z == 30.0,
                "Some vertex is wrong"
            );
            assert!(
                mesh.triangles.triangle.last().unwrap().v3 == 3,
                "Some triangle index is wrong"
            );
        } else {
            assert!(false, "Not a mesh data");
        }
    }

    #[test]
    fn test_get_xml_dom_from_3mf_model_file_string() {
        let file = open_file_from_test_resource("box.3mf");
        let string = load_threemf_get_root_model_file_as_string(file).unwrap();

        let dom = get_xml_dom_from_3mf_model_file_string(&string);
        assert!(dom.is_ok(), "XML Dom generaated");
        // println!("{}", &dom.as_ref().unwrap().child_nodes()[0].local_name());
        assert!(
            dom.unwrap().child_nodes()[0].local_name() == "model",
            "3mf model is not found"
        );
    }
}
