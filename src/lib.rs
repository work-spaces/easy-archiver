pub mod decoder;
pub mod driver;
pub mod encoder;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::UpdateStatus;
    use std::io::Write;



    fn generate_tmp_files() -> Vec<encoder::Entry> {
        const FILE_COUNT: usize = 1000;
        const LINE_COUNT: usize = 1000;
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

    fn update_progress(
        progress: &std::cell::RefCell<printer::MultiProgressBar>,
        update_status: UpdateStatus,
    ) {
        let mut progress = progress.borrow_mut();

        //println!("\n\nStatus {:?}", update_status);
        if let Some(brief) = update_status.brief {
            progress.set_prefix(brief.as_str());
        }

        if let Some(detail) = update_status.detail {
            progress.set_message(detail.as_str());
        }
        
        if let Some(total) = update_status.total {
            progress.set_total(total);
            if let Some(increment) = update_status.increment {
                progress.increment_with_overflow(increment);
            }
        } else {
            progress.set_total(100_u64);
            progress.increment_with_overflow(1);
        }
    }

    #[test]
    fn compress_test() {
        let entries = generate_tmp_files();

        let mut printer = printer::Printer::new_stdout();

        const DRIVERS: &[driver::Driver] = &[
            driver::Driver::Zlib,
            driver::Driver::Gzip,
            driver::Driver::Bzip2,
            driver::Driver::Zip,
            //driver::Driver::SevenZ,
        ];

        let mut multi_progress = printer::MultiProgress::new(&mut printer);

        for driver in DRIVERS {
            let progress = std::cell::RefCell::new(multi_progress.add_progress(
                &driver.extension(),
                Some(100),
                Some("Completed"),
            ));
            let mut encoder = encoder::Encoder::new(
                *driver,
                std::fs::File::create(format!("tmp/test.{}", driver.extension())).unwrap(),
            )
            .unwrap();
            encoder
                .add_entries(
                    &entries,
                    Some(&|update_status| {
                        update_progress(&progress, update_status);
                    }),
                )
                .unwrap();
            encoder
                .finish(Some(&|update_status| {
                    update_progress(&progress, update_status);
                }))
                .unwrap();
        }

        for driver in DRIVERS {
            let progress = std::cell::RefCell::new(multi_progress.add_progress(
                &driver.extension(),
                Some(100),
                None,
            ));

            let output_dir = format!("tmp/extract_test.{}", driver.extension());
            std::fs::create_dir_all(output_dir.as_str()).unwrap();

            let archive_path_string = format!("tmp/test.{}", driver.extension());
            let archive_path = std::path::Path::new(archive_path_string.as_str());
            if !archive_path.exists() {
                panic!("Archive not found {archive_path_string}");
            }

            let len = archive_path.metadata().unwrap().len();

            let decoder = decoder::Decoder::new(
                *driver,
                output_dir.as_str(),
                std::fs::File::open(archive_path_string.as_str()).unwrap(),
                len,
            )
            .unwrap();
            decoder
                .extract(Some(&|update_status| {
                    update_progress(&progress, update_status);
                }))
                .unwrap();
        }
    }
}
