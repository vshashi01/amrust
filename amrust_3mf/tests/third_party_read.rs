#[cfg(test)]
pub mod tests {
    use serde::*;

    use amrust_3mf::io::ThreemfPackage;
    use amrust_3mf::io::ThreemfUnpacked;

    use std::fs::File;
    use std::path::PathBuf;
    use std::vec::IntoIter;

    #[test]
    pub fn can_load_thirdparty_3mf_package() {
        let folder_path = PathBuf::from("./tests/data/third-party/");
        let fixtures = get_test_fixtures();

        for fixture in fixtures {
            if fixture.skip_test || fixture.large_test {
                continue;
            }

            let filepath = folder_path.join(fixture.filepath);
            let file = File::open(&filepath).unwrap();

            let package = ThreemfPackage::from_reader(file, true);

            match package {
                Ok(threemf) => {
                    assert!(!threemf.content_types.defaults.is_empty());
                    assert!(!threemf.relationships.is_empty());
                    assert!(!threemf.root.build.item.is_empty());
                }
                Err(err) => {
                    panic!(
                        "Failed to read the file: {:?} with err: {:?}",
                        &filepath, err
                    );
                }
            }
        }
    }

    #[test]
    pub fn unpack_thirdparty_3mf_package() {
        let folder_path = PathBuf::from("./tests/data/third-party/");
        let fixtures = get_test_fixtures();

        for fixture in fixtures {
            if fixture.skip_test {
                continue;
            }

            let filepath = folder_path.join(fixture.filepath);
            let file = File::open(&filepath).unwrap();

            let package = ThreemfUnpacked::from_reader(file, true);

            match package {
                Ok(threemf) => {
                    assert!(!threemf.content_types.is_empty());
                    assert!(!threemf.relationships.is_empty());
                    assert!(!threemf.root.is_empty());
                }
                Err(err) => {
                    panic!(
                        "Failed to read the file: {:?} with err: {:?}",
                        &filepath, err
                    );
                }
            }
        }
    }

    #[derive(Deserialize, Debug)]
    struct TestFixture {
        pub filepath: String,
        pub skip_test: bool,
        pub large_test: bool,
    }

    #[derive(Deserialize, Debug)]
    struct TestFixtures {
        pub fixtures: Vec<TestFixture>,
    }

    impl IntoIterator for TestFixtures {
        type Item = TestFixture;

        type IntoIter = IntoIter<TestFixture>;

        fn into_iter(self) -> Self::IntoIter {
            self.fixtures.into_iter()
        }
    }

    fn get_test_fixtures() -> TestFixtures {
        let json = include_str!("../tests/data/third-party/third-party-test-fixtures.json");
        let fixtures: TestFixtures = serde_json::from_str(json).unwrap();

        fixtures
    }
}
