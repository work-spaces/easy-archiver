use crate::driver::{self, Driver, UpdateStatus, SEVEN_Z_TAR_FILENAME};
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
    XzEncoder(tar::Builder<Vec<u8>>),
    ZipEncoder(zip::ZipWriter<std::fs::File>),
    SevenZEncoder(tar::Builder<Vec<u8>>),
}

pub struct Digestable {
    path: String,
    #[cfg(feature = "printer")]
    progress_bar: printer::MultiProgressBar,
}

pub struct Digested {
    pub sha256: String,
    #[cfg(feature = "printer")]
    pub progress_bar: printer::MultiProgressBar,
}

impl Digestable {
    pub fn digest(self) -> anyhow::Result<Digested> {
        let mut progress_bar = self.progress_bar;

        let digest = driver::digest_file(
            self.path.as_str(),
            #[cfg(feature = "printer")]
            &mut progress_bar,
        );

        Ok(Digested {
            sha256: digest?,
            #[cfg(feature = "printer")]
            progress_bar,
        })
    }
}

pub struct Encoder {
    encoder: EncoderDriver,
    driver: Driver,
    output_directory: String,
    output_filename: String,
    #[cfg(feature = "printer")]
    progress: printer::MultiProgressBar,
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

    #[allow(unused)]
    fn update_status(&mut self, update_status: UpdateStatus) {
        #[cfg(feature = "printer")]
        driver::update_status(&mut self.progress, update_status);
    }

    pub fn new(
        output_directory: &str,
        output_filename: &str,
        #[cfg(feature = "printer")] progress: printer::MultiProgressBar,
    ) -> anyhow::Result<Self> {
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
            Driver::Xz => {
                let archiver = tar::Builder::new(Vec::new());
                EncoderDriver::XzEncoder(archiver)
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
            #[cfg(feature = "printer")]
            progress,
        })
    }

    pub fn add_entries(&mut self, entries: &Vec<Entry>) -> anyhow::Result<()> {
        self.update_status(UpdateStatus {
            brief: Some(format!("Archiving ({})", self.driver.extension())),
            ..Default::default()
        });

        for entry in entries.iter() {
            self.update_status(UpdateStatus {
                detail: Some(entry.archive_path.clone()),
                increment: Some(1),
                total: Some(entries.len() as u64),
                ..Default::default()
            });

            self.add_file(&entry.archive_path, &entry.file_path)
                .context(format_context!("{}", entry.archive_path))?;
        }

        self.update_status(UpdateStatus {
            detail: Some("...".to_string()),
            ..Default::default()
        });

        Ok(())
    }

    pub fn add_file(&mut self, archive_path: &str, file_path: &str) -> anyhow::Result<()> {
        match &mut self.encoder {
            EncoderDriver::GzipEncoder(archiver)
            | EncoderDriver::Bzip2Encoder(archiver)
            | EncoderDriver::XzEncoder(archiver)
            | EncoderDriver::SevenZEncoder(archiver) => {
                let mut file =
                    std::fs::File::open(file_path).context(format_context!("{file_path}"))?;
                archiver
                    .append_file(archive_path, &mut file)
                    .context(format_context!("appending {archive_path}"))?;
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
        driver: Driver,
        #[cfg(feature = "printer")] progress: &mut printer::MultiProgressBar,
    ) -> anyhow::Result<()> {
        let contents = archiver
            .into_inner()
            .context(format_context!("{driver:?}"))?;

        let total_chunks = contents.len() / 4096;

        #[cfg(feature = "printer")]
        driver::update_status(
            progress,
            UpdateStatus {
                brief: Some(format!("Compressing ({})", driver.extension())),
                ..Default::default()
            },
        );

        for chunk in contents.as_slice().chunks(total_chunks) {
            #[cfg(feature = "printer")]
            driver::update_status(
                progress,
                UpdateStatus {
                    increment: Some(1),
                    total: Some((contents.len() / total_chunks) as u64),
                    ..Default::default()
                },
            );

            encoder
                .write_all(chunk)
                .context(format_context!("{driver:?}"))?;
        }
        Ok(())
    }

    pub fn compress(self) -> anyhow::Result<Digestable> {
        let driver = self.driver;
        let output_directory = self.output_directory.clone();
        let output_path = self.get_encoder_output_file_path();
        let output_path_result = output_path.clone();
        let mut progress_bar = self.progress;

        match self.encoder {
            EncoderDriver::GzipEncoder(archiver) => {
                let output_file = std::fs::File::create(output_path.as_str())
                    .context(format_context!("cannot create {output_path}"))?;
                let encoder =
                    flate2::write::GzEncoder::new(output_file, flate2::Compression::default());
                Self::encode_in_chunks(
                    archiver,
                    encoder,
                    driver,
                    #[cfg(feature = "printer")]
                    &mut progress_bar,
                )?;
            }
            EncoderDriver::ZipEncoder(encoder) => {
                encoder.finish().context(format_context!("{output_path}"))?;
            }
            EncoderDriver::XzEncoder(archiver) => {
                let output_file = std::fs::File::create(output_path.as_str())
                    .context(format_context!("{output_path}"))?;
                let encoder = xz2::write::XzEncoder::new(output_file, 9);
                Self::encode_in_chunks(
                    archiver,
                    encoder,
                    driver,
                    #[cfg(feature = "printer")]
                    &mut progress_bar,
                )?;
            }
            EncoderDriver::Bzip2Encoder(archiver) => {
                let output_file = std::fs::File::create(output_path.as_str())
                    .context(format_context!("{output_path}"))?;
                let encoder =
                    bzip2::write::BzEncoder::new(output_file, bzip2::Compression::default());
                Self::encode_in_chunks(
                    archiver,
                    encoder,
                    driver,
                    #[cfg(feature = "printer")]
                    &mut progress_bar,
                )?;
            }
            EncoderDriver::SevenZEncoder(archiver) => {
                let contents = archiver.into_inner().context("tar.7z")?;

                #[cfg(feature = "printer")]
                driver::update_status(
                    &mut progress_bar,
                    UpdateStatus {
                        brief: Some(format!("Compressing ({})", driver.extension())),
                        total: Some(200),
                        ..Default::default()
                    },
                );

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

                driver::wait_handle(
                    handle,
                    #[cfg(feature = "printer")]
                    &mut progress_bar,
                )
                .context(format_context!(""))?;
            }
        }
        Ok(Digestable {
            path: output_path_result,
            progress_bar,
        })
    }
}
