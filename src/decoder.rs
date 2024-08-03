use anyhow_source_location::{format_context, format_error};
use std::io::Read;

use crate::driver::{Driver, UpdateStatus, Updater, SEVEN_Z_TAR_FILENAME};

use anyhow::Context;

enum DecoderDriver {
    GzipDecoder(flate2::read::GzDecoder<std::fs::File>),
    Bzip2Decoder(bzip2::read::BzDecoder<std::fs::File>),
    ZipDecoder(zip::ZipArchive<std::fs::File>),
    SevenZDecoder,
}

pub struct Decoder {
    decoder: DecoderDriver,
    output_directory: String,
    input_file_name: String,
    reader_size: u64,
    driver: Driver,
}

impl Decoder {
    pub fn new(input_file_path: &str, destination_directory: &str) -> anyhow::Result<Self> {
        let driver =
            Driver::from_filename(input_file_path).context(format_context!("{input_file_path}"))?;

        let reader_size = std::path::Path::new(input_file_path)
            .metadata()
            .context(format_context!("{input_file_path}"))?
            .len();

        let input_file =
            std::fs::File::open(input_file_path).context(format_context!("{input_file_path}"))?;

        let decoder = match driver {
            Driver::Gzip => DecoderDriver::GzipDecoder(flate2::read::GzDecoder::new(input_file)),
            Driver::Zip => DecoderDriver::ZipDecoder(
                zip::ZipArchive::new(input_file).context(format_context!("{input_file_path}"))?,
            ),
            Driver::Bzip2 => DecoderDriver::Bzip2Decoder(bzip2::read::BzDecoder::new(input_file)),
            Driver::SevenZ => DecoderDriver::SevenZDecoder,
        };

        let output_directory = destination_directory.to_string();

        Ok(Self {
            decoder,
            output_directory,
            reader_size,
            input_file_name: input_file_path.to_string(),
            driver,
        })
    }

    fn extract_to_tar_bytes<Decoder: std::io::Read>(
        mut decoder: Decoder,
        updater: Updater,
        reader_size: u64,
        driver: Driver,
    ) -> anyhow::Result<Vec<u8>> {
        let mut result = Vec::new();

        result.reserve(reader_size as usize);

        let mut buffer = [0; 8192];

        if let Some(updater) = updater.as_ref() {
            updater(UpdateStatus {
                brief: Some(format!("Extracting {}", driver.extension())),
                detail: Some("creating tar as binary blob".to_string()),
                total: Some(200),
                ..Default::default()
            });
        }

        while let Ok(bytes_read) = decoder.read(&mut buffer) {
            if bytes_read == 0 {
                break;
            }
            result.extend_from_slice(&buffer[..bytes_read]);

            if let Some(updater) = updater {
                updater(UpdateStatus {
                    increment: Some(1),
                    ..Default::default()
                });
            }
        }

        Ok(result)
    }

    pub fn extract(self, updater: Updater) -> anyhow::Result<()> {
        let reader_size = self.reader_size;
        let driver = self.driver;
        let input_file: String = self.input_file_name.clone();
        let output_directory = self.output_directory.clone();

        let tar_bytes = match self.decoder {
            DecoderDriver::GzipDecoder(decoder) => Some(Self::extract_to_tar_bytes(
                decoder,
                updater,
                reader_size,
                driver,
            )?),
            DecoderDriver::ZipDecoder(mut decoder) => {
                let file_names: Vec<String> = decoder.file_names().map(|e| e.to_string()).collect();

                if let Some(updater) = updater {
                    updater(UpdateStatus {
                        brief: Some("Extracting (zip)".to_string()),
                        total: Some(file_names.len() as u64),
                        ..Default::default()
                    });
                }

                for file in file_names {
                    let mut zip_file = decoder
                        .by_name(file.as_str())
                        .context(format_context!("{file:?}"))?;

                    if let Some(updater) = updater {
                        updater(UpdateStatus {
                            detail: Some(file.clone()),
                            increment: Some(1),
                            ..Default::default()
                        });
                    }
                    let mut buffer = Vec::new();
                    let destination_path = format!("{}/{}", self.output_directory, zip_file.name());
                    let mut file = std::fs::File::create(destination_path.as_str())
                        .context(format_context!("{destination_path}"))?;
                    use std::io::Write;
                    zip_file
                        .read_to_end(&mut buffer)
                        .context(format_context!("{destination_path}"))?;
                    file.write(buffer.as_slice())
                        .context(format_context!("{destination_path}"))?;
                }

                decoder
                    .extract(self.output_directory.as_str())
                    .context(format_context!("{output_directory}"))?;

                None
            }
            DecoderDriver::Bzip2Decoder(decoder) => Some(Self::extract_to_tar_bytes(
                decoder,
                updater,
                reader_size,
                driver,
            )?),
            DecoderDriver::SevenZDecoder => {
                if let Some(updater) = updater.as_ref() {
                    updater(UpdateStatus {
                        brief: Some(format!("Extracting {}", driver.extension())),
                        detail: Some("creating tar as binary blob".to_string()),
                        total: Some(200),
                        ..Default::default()
                    });
                }

                let handle = std::thread::spawn(move || -> anyhow::Result<Vec<u8>> {
                    let temporary_file_path =
                        format!("{output_directory}/{}", SEVEN_Z_TAR_FILENAME);
                    let input_file = std::fs::File::open(input_file.as_str())
                        .context(format_context!("{input_file}"))?;
                    sevenz_rust::decompress(input_file, output_directory.as_str()).context(
                        format_context!("{temporary_file_path} -> {output_directory}"),
                    )?;
                    let result = std::fs::read(temporary_file_path.as_str())
                        .context(format_context!("{temporary_file_path}"));

                    std::fs::remove_file(temporary_file_path.as_str())
                        .context(format_context!("{temporary_file_path}"))?;

                    result
                });

                while !handle.is_finished() {
                    if let Some(updater) = updater {
                        updater(UpdateStatus {
                            increment: Some(1),
                            ..Default::default()
                        });
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }

                let result = handle.join().map_err(|err| format_error!("{:?}", err))?;

                let tar_contents = result.context(format_context!(""))?;

                Some(tar_contents)
            }
        };

        let output_directory = self.output_directory.clone();

        if let Some(tar_bytes) = tar_bytes {
            let handle = std::thread::spawn(move || -> anyhow::Result<()> {
                let mut archive = tar::Archive::new(tar_bytes.as_slice());
                archive
                    .unpack(output_directory.as_str())
                    .context(format_context!("{output_directory}"))?;

                Ok(())
            });

            if let Some(updater) = updater.as_ref() {
                updater(UpdateStatus {
                    brief: Some("Unpacking (tar)".to_string()),
                    ..Default::default()
                });
            }

            while !handle.is_finished() {
                if let Some(updater) = updater {
                    updater(UpdateStatus {
                        increment: Some(1),
                        ..Default::default()
                    });
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }

            let result = handle
                .join()
                .map_err(|err| anyhow::anyhow!("failed to join thread: {:?}", err))?;

            result.context(format_context!(""))?;
        }

        Ok(())
    }
}
