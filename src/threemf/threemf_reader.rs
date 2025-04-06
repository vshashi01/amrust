use anyhow::Result;
use instant_xml::to_string;
use std::fs::File;
use std::path::PathBuf;
use threemf::io::threemf_package::ThreemfPackage;

pub fn get_threemf_package(path: &PathBuf) -> Result<ThreemfPackage> {
    let file = File::open(path).unwrap();
    let package = ThreemfPackage::from_reader(file, false);

    Ok(package.unwrap())
}

pub fn get_root_model_file_as_string(packages: &ThreemfPackage) -> Result<String> {
    let string = to_string(&packages.root).unwrap();

    Ok(string)
}
#[cfg(test)]
mod tests {

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
}
