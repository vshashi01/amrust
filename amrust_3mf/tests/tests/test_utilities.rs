use std::vec::IntoIter;

use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub(crate) struct TestFixture {
    pub filepath: String,
    pub skip_test: bool,
    pub large_test: bool,
    pub golden_thumbnail_path: String,
}

#[derive(Deserialize, Debug)]
pub(crate) struct TestFixtures {
    pub fixtures: Vec<TestFixture>,
}

impl IntoIterator for TestFixtures {
    type Item = TestFixture;

    type IntoIter = IntoIter<TestFixture>;

    fn into_iter(self) -> Self::IntoIter {
        self.fixtures.into_iter()
    }
}

pub(crate) fn get_test_fixtures() -> TestFixtures {
    let json = include_str!("../../tests/data/third-party/third-party-test-fixtures.json");
    let fixtures: TestFixtures = serde_json::from_str(json).unwrap();

    fixtures
}
