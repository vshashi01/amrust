use pretty_assertions::assert_eq;

use amrust_3mf::io::ThreemfPackage;

use std::{fs::File, path::PathBuf};

#[test]
fn read_threemf_package() {
    let path = PathBuf::from("./tests/data/third-party/mgx-core-prod-beamlattice-material.3mf");
    let reader = File::open(path).unwrap();

    let result = ThreemfPackage::from_reader(reader, true);

    assert!(result.is_ok());

    match result {
        Ok(package) => {
            assert_eq!(package.relationships.len(), 2);
            for rels in package.relationships.keys() {
                println!("Relationship file at {}", rels);
            }
            assert!(package.relationships.contains_key("_rels/.rels"));
            assert!(
                package
                    .relationships
                    .contains_key("/3D/_rels/3dmodel.model.rels")
            );

            let sub_rels = package.relationships.get("/3D/_rels/3dmodel.model.rels");

            assert!(package.unknown_parts.contains_key("/3D/Disp2D/lines.png"));
            assert!(sub_rels.is_some());
        }
        Err(err) => {
            panic!("read failed {:?}", err);
        }
    }
}
