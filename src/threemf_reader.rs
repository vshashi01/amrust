use anyhow::Result;
use std::{
    env, fs,
    io::{self, Read},
    path::PathBuf,
};

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
        }
    }

    Ok(list_string)
}

#[test]
fn test_load_threemf_get_root_model_file_as_string() {
    let root_dir = &env::var("CARGO_MANIFEST_DIR").expect("$CARGO_MANIFEST_DIR");
    let mut test_file_path = PathBuf::from(root_dir);
    test_file_path.push("test_resources\\fake-3mf.3mf");
    println!("{:?}", test_file_path);

    let file = fs::File::open(test_file_path).unwrap();
    let string = load_threemf_get_root_model_file_as_string(file).unwrap();
    assert!(
        string == "Test Passed",
        "3dmodel.model file is not read correctly"
    );
}
