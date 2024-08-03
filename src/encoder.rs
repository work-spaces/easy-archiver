use crate::driver::{Driver, UpdateStatus, Updater, SEVEN_Z_TAR_FILENAME};
use anyhow_source_location::format_context;
use std::io::Write;

use anyhow::Context;

pub struct Entry {
    pub archive_path: String,
    pub file_path: String,
}

enum EncoderDriver {
    GzipEncoder(tar::Builder<Vec<u8>>),
    Bzip2Encoder(tar::Builder<Vec<u8>>),
    ZipEncoder(zip::ZipWriter<std::fs::File>),
    SevenZEncoder(tar::Builder<Vec<u8>>),
}

pub struct Encoder {
    encoder: EncoderDriver,
    driver: Driver,
    output_directory: String,
    output_filename: String,
}

impl Encoder {
    fn get_output_file_path(output_directory: &str, output_filename: &str) -> String {
        format!("{output_directory}/{output_filename}")
    }

    fn get_encoder_output_file_path(&self) -> String {
        Self::get_output_file_path(
            self.output_directory.as_str(),
            self.output_filename.as_str(),
        )
    }

    pub fn new(output_directory: &str, output_filename: &str) -> anyhow::Result<Self> {
        let driver = Driver::from_filename(output_filename).ok_or(anyhow::anyhow!(
            "could not determine compression type from {output_filename} suffix"
        ))?;

        let encoder = match driver {
            Driver::Gzip => {
                let archiver = tar::Builder::new(Vec::new());
                EncoderDriver::GzipEncoder(archiver)
            }
            Driver::Zip => {
                let file_path = Self::get_output_file_path(output_directory, output_filename);
                let file = std::fs::File::create(file_path.as_str())
                    .context(format_context!("{file_path}"))?;
                let encoder = zip::ZipWriter::new(file);
                EncoderDriver::ZipEncoder(encoder)
            }
            Driver::Bzip2 => {
                let archiver = tar::Builder::new(Vec::new());
                EncoderDriver::Bzip2Encoder(archiver)
            }
            Driver::SevenZ => {
                let archiver = tar::Builder::new(Vec::new());
                EncoderDriver::SevenZEncoder(archiver)
            }
        };

        Ok(Self {
            encoder,
            driver,
            output_directory: output_directory.to_string(),
            output_filename: output_filename.to_string(),
        })
    }

    pub fn add_entries(&mut self, entries: &Vec<Entry>, updater: Updater) -> anyhow::Result<()> {
        if let Some(updater) = updater.as_ref() {
            updater(UpdateStatus {
                brief: Some(format!("Archiving ({})", self.driver.extension())),
                ..Default::default()
            });
        }

        for entry in entries.iter() {
            if let Some(updater) = updater {
                updater(UpdateStatus {
                    detail: Some(entry.archive_path.clone()),
                    increment: Some(1),
                    total: Some(entries.len() as u64),
                    ..Default::default()
                });
            }
            self.add_file(&entry.archive_path, &entry.file_path)
                .context(format_context!("{}", entry.archive_path))?;
        }

        if let Some(updater) = updater {
            updater(UpdateStatus {
                detail: Some("...".to_string()),
                ..Default::default()
            });
        }
        Ok(())
    }

    pub fn add_file(&mut self, archive_path: &str, file_path: &str) -> anyhow::Result<()> {
        match &mut self.encoder {
            EncoderDriver::GzipEncoder(archiver)
            | EncoderDriver::Bzip2Encoder(archiver)
            | EncoderDriver::SevenZEncoder(archiver) => {
                let mut file =
                    std::fs::File::open(file_path).context(format_context!("{file_path}"))?;
                archiver
                    .append_file(archive_path, &mut file)
                    .context(format_context!(""))?;
            }
            EncoderDriver::ZipEncoder(encoder) => {
                let options = zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated)
                    .unix_permissions(0o755);

                let contents = std::fs::read(file_path).context(format_context!(
                    "Failed to read file for zip archive {file_path}"
                ))?;
                encoder
                    .start_file(archive_path, options)
                    .context(format_context!("{file_path}"))?;
                encoder
                    .write_all(contents.as_slice())
                    .context(format_context!("{file_path}"))?;
            }
        }
        Ok(())
    }

    fn encode_in_chunks<Encoder: std::io::Write>(
        archiver: tar::Builder<Vec<u8>>,
        mut encoder: Encoder,
        updater: Updater,
        driver: Driver,
    ) -> anyhow::Result<()> {
        let contents = archiver
            .into_inner()
            .context(format_context!("{driver:?}"))?;

        let total_chunks = contents.len() / 4096;

        if let Some(updater) = updater.as_ref() {
            updater(UpdateStatus {
                brief: Some(format!("Compressing ({})", driver.extension())),
                ..Default::default()
            });
        }

        for chunk in contents.as_slice().chunks(total_chunks) {
            if let Some(updater) = updater {
                updater(UpdateStatus {
                    increment: Some(1),
                    total: Some((contents.len() / total_chunks) as u64),
                    ..Default::default()
                });
            }
            encoder
                .write_all(chunk)
                .context(format_context!("{driver:?}"))?;
        }
        Ok(())
    }

    pub fn finish(self, updater: Updater) -> anyhow::Result<()> {
        let driver = self.driver;
        let output_directory = self.output_directory.clone();
        let output_path = self.get_encoder_output_file_path();

        match self.encoder {
            EncoderDriver::GzipEncoder(archiver) => {
                let output_file = std::fs::File::create(output_path.as_str())
                    .context(format_context!("{output_path}"))?;
                let encoder =
                    flate2::write::GzEncoder::new(output_file, flate2::Compression::default());
                Self::encode_in_chunks(archiver, encoder, updater, driver)?;
            }
            EncoderDriver::ZipEncoder(encoder) => {
                encoder.finish().context(format_context!(
                    "{output_path}"
                ))?;
            }
            EncoderDriver::Bzip2Encoder(archiver) => {
                let output_file = std::fs::File::create(output_path.as_str())
                    .context(format_context!("{output_path}"))?;
                let encoder =
                    bzip2::write::BzEncoder::new(output_file, bzip2::Compression::default());
                Self::encode_in_chunks(archiver, encoder, updater, driver)?;
            }
            EncoderDriver::SevenZEncoder(archiver) => {
                let contents = archiver
                    .into_inner()
                    .context("tar.7z")?;

                if let Some(updater) = updater.as_ref() {
                    updater(UpdateStatus {
                        brief: Some(format!("Compressing ({})", driver.extension())),
                        total: Some(500),
                        ..Default::default()
                    });
                }

                let handle = std::thread::spawn(move || -> anyhow::Result<()> {
                    let output_file = std::fs::File::create(output_path.as_str())
                        .context(format_context!("{output_path}"))?;

                    let temporary_tar_path = format!("{output_directory}/{}", SEVEN_Z_TAR_FILENAME);
                    // create a temporary tar file
                    std::fs::write(temporary_tar_path.as_str(), contents)
                        .context(format_context!("{temporary_tar_path}"))?;

                    sevenz_rust::compress(temporary_tar_path.as_str(), output_file)
                        .context(format_context!("{temporary_tar_path} -> {output_path}"))?;

                    //std::fs::remove_file(temporary_tar_path.as_str()).context(format_context!(""))?;

                    Ok(())
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

                let result = handle.join();
                match result {
                    Ok(result) => result,
                    Err(err) => Err(anyhow::anyhow!("{:?}", err)),
                }?;
            }
        }
        Ok(())
    }
}
