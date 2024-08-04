pub mod decoder;
pub mod driver;
pub mod encoder;

pub use decoder::Decoder;
pub use driver::UpdateStatus;
pub use encoder::Encoder;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const FILE_COUNT: usize = 500;
    const LINE_COUNT: usize = 500;

    fn verify_generated_files(output_directory: &str) {
        for i in 0..FILE_COUNT {
            let archive_path = format!("file_{i}.txt");
            let file_path = format!("{output_directory}/{archive_path}");
            let contents = std::fs::read_to_string(file_path.as_str()).unwrap();

            for (number, line) in contents.lines().enumerate() {
                let expected = format!("This is line #{number}");
                assert_eq!(line, expected);
            }
        }
    }

    fn generate_tmp_files() -> Vec<encoder::Entry> {
        let mut result = Vec::new();
        std::fs::create_dir_all("tmp/files").unwrap();
        for i in 0..FILE_COUNT {
            let archive_path = format!("file_{i}.txt");
            let file_path = format!("tmp/files/{archive_path}");
            let path = std::path::Path::new(file_path.as_str());
            let mut file = if !path.exists() {
                Some(std::fs::File::create(file_path.as_str()).unwrap())
            } else {
                None
            };
            result.push(encoder::Entry {
                archive_path,
                file_path,
            });

            if let Some(file) = file.as_mut() {
                for j in 0..LINE_COUNT {
                    file.write(format!("This is line #{j}\n").as_bytes())
                        .unwrap();
                }
            }
        }
        result
    }

    #[test]
    fn compress_test() {
        let entries = generate_tmp_files();

        let mut printer = printer::Printer::new_stdout();

        const DRIVERS: &[driver::Driver] = &[
            driver::Driver::Gzip,
            driver::Driver::Bzip2,
            driver::Driver::Zip,
            driver::Driver::SevenZ,
        ];

        let mut multi_progress = printer::MultiProgress::new(&mut printer);

        for driver in DRIVERS {
            let output_directory = "./tmp";
            let output_filename = format!("test.{}", driver.extension());

            let progress_bar = multi_progress.add_progress(&driver.extension(), Some(100), None);

            let mut encoder =
                encoder::Encoder::new(output_directory, &output_filename, progress_bar).unwrap();

            encoder.add_entries(&entries).unwrap();

            let _digest = encoder.compress().unwrap().digest().unwrap();
        }

        for driver in DRIVERS {
            let output_dir = format!("tmp/extract_test.{}", driver.extension());
            std::fs::create_dir_all(output_dir.as_str()).unwrap();

            let archive_path_string = format!("tmp/test.{}", driver.extension());

            let digest = {
                let contents = std::fs::read(archive_path_string.as_str()).unwrap();
                sha256::digest(contents)
            };

            let progress_bar = multi_progress.add_progress(&driver.extension(), Some(100), None);

            let decoder = decoder::Decoder::new(
                archive_path_string.as_str(),
                Some(digest),
                output_dir.as_str(),
                progress_bar,
            )
            .unwrap();
            decoder.extract().unwrap();

            verify_generated_files(output_dir.as_str());
        }
    }
}
