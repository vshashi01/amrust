#[cfg(test)]
pub mod tests {

    pub mod test_utilities;

    use thiserror::Error;

    use amrust_3mf::io::ThreemfPackage;
    use amrust_3mf::io::ThreemfUnpacked;
    use amrust_3mf::io::error::Error;
    use amrust_3mf::io::thumbnail;

    use std::cmp::Ordering;
    use std::fs::File;
    use std::path::PathBuf;

    #[derive(Debug, Error)]
    enum ImageTestError {
        #[error("Failed to generate image for file {0}")]
        ThumbnailGenerationFailed(#[from] Error),

        #[error("Image dont match for file {0} with a Mean error {1}")]
        ThumbnailComparisonFailed(PathBuf, f32),
    }

    #[test]
    pub fn can_load_thirdparty_3mf_package() {
        let folder_path = PathBuf::from("./tests/data/third-party/");
        let fixtures = test_utilities::get_test_fixtures();
        let mut failed_conditions: Vec<ImageTestError> = vec![];

        for fixture in fixtures {
            if fixture.skip_test || fixture.large_test {
                continue;
            }

            let filepath = folder_path.join(fixture.filepath.clone());
            println!("{:?}", filepath);
            let file = File::open(&filepath).unwrap();

            let package = ThreemfPackage::from_reader(file, true);

            let golden_thumbnail_path = folder_path.join(format!(
                "golden_thumbnails/{}",
                fixture.golden_thumbnail_path
            ));

            match package {
                Ok(threemf) => {
                    assert!(!threemf.content_types.defaults.is_empty());
                    assert!(!threemf.relationships.is_empty());
                    assert!(!threemf.root.build.item.is_empty());

                    if golden_thumbnail_path.is_file() {
                        match run_image_comparison(golden_thumbnail_path, threemf, fixture.filepath)
                        {
                            Ok(_) => {}
                            Err(err) => failed_conditions.push(err),
                        }
                    } else {
                        println!(
                            "Skipped thumbnail comparison for: {:?}",
                            golden_thumbnail_path
                        );
                    }
                }
                Err(err) => {
                    panic!(
                        "Failed to read the file: {:?} with err: {:?}",
                        &filepath, err
                    );
                }
            }
        }

        if !failed_conditions.is_empty() {
            panic!("Some thumbnail generations failed {:?}", failed_conditions)
        }
    }

    #[test]
    pub fn unpack_thirdparty_3mf_package() {
        let folder_path = PathBuf::from("./tests/data/third-party/");
        let fixtures = test_utilities::get_test_fixtures();

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

    fn run_image_comparison(
        path: PathBuf,
        threemf: ThreemfPackage,
        fixture_filepath: String,
    ) -> Result<(), ImageTestError> {
        const FLIP_MEAN_ERROR: f32 = 0.021;
        pollster::block_on(async {
            let ref_image_data = image::open(&path).unwrap().into_rgba8();

            let thumbnail = thumbnail::render_package_thumbnail(&threemf, 1280, 1080).await;

            match thumbnail {
                Ok(thumbnail) => {
                    let ref_image = nv_flip::FlipImageRgb8::with_data(1280, 1080, &ref_image_data);
                    let test_image = nv_flip::FlipImageRgb8::with_data(1280, 1080, &thumbnail);

                    let error_map =
                        nv_flip::flip(ref_image, test_image, nv_flip::DEFAULT_PIXELS_PER_DEGREE);
                    let pool = nv_flip::FlipPool::from_image(&error_map);
                    if let Some(Ordering::Greater) = pool.mean().partial_cmp(&FLIP_MEAN_ERROR) {
                        println!("Mean error {}", pool.mean());
                        thumbnail
                            .save(format!("{}_golden_thumbnail.png", fixture_filepath))
                            .unwrap();

                        Err(ImageTestError::ThumbnailComparisonFailed(path, pool.mean()))
                    } else {
                        Ok(())
                    }
                }
                Err(err) => Err(ImageTestError::ThumbnailGenerationFailed(err)),
            }
        })
    }
}
