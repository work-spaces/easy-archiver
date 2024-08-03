use std::io::Read;

use crate::driver::{Driver, UpdateStatus, Updater};

use anyhow::Context;
use sevenz_rust::Password;

enum DecoderDriver<Reader: std::io::Read + std::io::Seek + std::marker::Send> {
    ZlibDecoder(flate2::read::ZlibDecoder<Reader>),
    GzipDecoder(flate2::read::GzDecoder<Reader>),
    BzipDecoder(bzip2::read::BzDecoder<Reader>),
    Bzip2Decoder(bzip2::read::BzDecoder<Reader>),
    ZipDecoder(zip::ZipArchive<Reader>),
    SevenZDecoder(sevenz_rust::SevenZReader<Reader>),
}

pub struct Decoder<Reader: std::io::Read + std::io::Seek + std::marker::Send> {
    decoder: DecoderDriver<Reader>,
    output_directory: String,
    reader_size: u64,
    driver: Driver,
}

impl<Reader: std::io::Read + std::io::Seek + std::marker::Send> Decoder<Reader> {
    pub fn new(
        driver: Driver,
        destination_directory: &str,
        reader: Reader,
        reader_size: u64,
    ) -> anyhow::Result<Self> {
        let decoder = match driver {
            Driver::Zlib => DecoderDriver::ZlibDecoder(flate2::read::ZlibDecoder::new(reader)),
            Driver::Gzip => DecoderDriver::GzipDecoder(flate2::read::GzDecoder::new(reader)),
            Driver::Zip => DecoderDriver::ZipDecoder(
                zip::ZipArchive::new(reader)
                    .context(format!("Failed to create zip archive decoder"))?,
            ),
            Driver::Bzip => DecoderDriver::BzipDecoder(bzip2::read::BzDecoder::new(reader)),
            Driver::Bzip2 => DecoderDriver::Bzip2Decoder(bzip2::read::BzDecoder::new(reader)),
            Driver::SevenZ => DecoderDriver::SevenZDecoder(
                sevenz_rust::SevenZReader::new(reader, reader_size, Password::empty()).context(
                    format!("Failed to create 7z reader with {reader_size} bytes"),
                )?,
            ),
        };

        let output_directory = destination_directory.to_string();

        Ok(Self {
            decoder,
            output_directory,
            reader_size,
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
        let tar_bytes = match self.decoder {
            DecoderDriver::ZlibDecoder(decoder) => Some(Self::extract_to_tar_bytes(
                decoder,
                updater,
                reader_size,
                driver,
            )?),
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
                        .context("Failed to read file")?;

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
                        .context("Failed to create file")?;
                    use std::io::Write;
                    zip_file
                        .read_to_end(&mut buffer)
                        .context("Failed to read file")?;
                    file.write(buffer.as_slice())
                        .context("Failed to write file")?;
                }

                decoder
                    .extract(self.output_directory.as_str())
                    .context("Failed to extract zip archive")?;

                None
            }
            DecoderDriver::BzipDecoder(decoder) => Some(Self::extract_to_tar_bytes(
                decoder,
                updater,
                reader_size,
                driver,
            )?),
            DecoderDriver::Bzip2Decoder(decoder) => Some(Self::extract_to_tar_bytes(
                decoder,
                updater,
                reader_size,
                driver,
            )?),
            DecoderDriver::SevenZDecoder(_) => None,
        };

        let output_directory = self.output_directory.clone();

        if let Some(tar_bytes) = tar_bytes {
            let handle = std::thread::spawn(move || -> anyhow::Result<()> {
                let mut archive = tar::Archive::new(tar_bytes.as_slice());
                archive
                    .unpack(output_directory.as_str())
                    .context("Failed to unpack tar archive")?;

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

            result.context(format!("Failed to unpack tar contents"))?;
        }

        Ok(())
    }
}
