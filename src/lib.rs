mod encoder;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::Write;

    fn generate_tmp_files() -> Vec<encoder::Entry> {
        let mut result = Vec::new();
        std::fs::create_dir_all("tmp/files").unwrap();
        for i in 0..100 {
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
                for j in 0..100 {
                    file.write(format!("This is line #{j}\n").as_bytes())
                        .unwrap();
                }
            }
        }
        result
    }

    fn update_progress(current: usize, total: usize){
        println!("adding {}/{}", current+1, total);
    }

    #[test]
    fn compress_test() {
        let entries = generate_tmp_files();

        let mut encoder = encoder::Encoder::new(
            encoder::Driver::Zip,
            std::fs::File::create("tmp/test.zip").unwrap(),
        )
        .unwrap();
        encoder.add_entries(&entries, None).unwrap();
        encoder.finish(Some(&update_progress)).unwrap();

        let mut encoder = encoder::Encoder::new(
            encoder::Driver::Gzip,
            std::fs::File::create("tmp/test.tar.gz").unwrap(),
        )
        .unwrap();
        encoder.add_entries(&entries, None).unwrap();
        encoder.finish(Some(&update_progress)).unwrap();

        let mut encoder = encoder::Encoder::new(
            encoder::Driver::SevenZ,
            std::fs::File::create("tmp/test.tar.7z").unwrap(),
        )
        .unwrap();
        encoder
            .add_entries(&entries, Some(&update_progress))
            .unwrap();
        encoder.finish(Some(&update_progress)).unwrap();
    }
}
