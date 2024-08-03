use crate::driver::{Driver, UpdateStatus, Updater};
use std::io::Write;

use anyhow::Context;
use sevenz_rust::SevenZArchiveEntry;

pub struct Entry {
    pub archive_path: String,
    pub file_path: String,
}

enum EncoderDriver<Writer: std::io::Write + std::io::Seek + std::marker::Send> {
    ZlibEncoder(tar::Builder<Vec<u8>>, flate2::write::ZlibEncoder<Writer>),
    GzipEncoder(tar::Builder<Vec<u8>>, flate2::write::GzEncoder<Writer>),
    BzipEncoder(tar::Builder<Vec<u8>>, bzip2::write::BzEncoder<Writer>),
    Bzip2Encoder(tar::Builder<Vec<u8>>, bzip2::write::BzEncoder<Writer>),
    ZipEncoder(zip::ZipWriter<Writer>),
    SevenZEncoder(tar::Builder<Vec<u8>>, sevenz_rust::SevenZWriter<Writer>),
}

pub struct Encoder<Writer: std::io::Write + std::io::Seek + std::marker::Send> {
    encoder: EncoderDriver<Writer>,
    driver: Driver,
}

impl<Writer: std::io::Write + std::io::Seek + std::marker::Send> Encoder<Writer> {
    pub fn new(driver: Driver, writer: Writer) -> anyhow::Result<Self> {
        let encoder = match driver {
            Driver::Zlib => {
                let archiver = tar::Builder::new(Vec::new());
                let encoder =
                    flate2::write::ZlibEncoder::new(writer, flate2::Compression::default());
                EncoderDriver::ZlibEncoder(archiver, encoder)
            }
            Driver::Gzip => {
                let archiver = tar::Builder::new(Vec::new());
                let encoder = flate2::write::GzEncoder::new(writer, flate2::Compression::default());
                EncoderDriver::GzipEncoder(archiver, encoder)
            }
            Driver::Zip => {
                let encoder = zip::ZipWriter::new(writer);
                EncoderDriver::ZipEncoder(encoder)
            }
            Driver::Bzip => {
                let archiver = tar::Builder::new(Vec::new());
                let encoder = bzip2::write::BzEncoder::new(writer, bzip2::Compression::default());
                EncoderDriver::BzipEncoder(archiver, encoder)
            }
            Driver::Bzip2 => {
                let archiver = tar::Builder::new(Vec::new());
                let encoder = bzip2::write::BzEncoder::new(writer, bzip2::Compression::default());
                EncoderDriver::Bzip2Encoder(archiver, encoder)
            }
            Driver::SevenZ => {
                let archiver = tar::Builder::new(Vec::new());
                let encoder = sevenz_rust::SevenZWriter::new(writer)
                    .context(format!("Failed to create 7z encoder"))?;

                EncoderDriver::SevenZEncoder(archiver, encoder)
            }
        };

        Ok(Self { encoder, driver })
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
                .context(format!("Failed to add {}", entry.archive_path))?;
        }
        Ok(())
    }

    pub fn add_file(&mut self, archive_path: &str, file_path: &str) -> anyhow::Result<()> {
        match &mut self.encoder {
            EncoderDriver::ZlibEncoder(archiver, _)
            | EncoderDriver::GzipEncoder(archiver, _)
            | EncoderDriver::BzipEncoder(archiver, _)
            | EncoderDriver::Bzip2Encoder(archiver, _)
            | EncoderDriver::SevenZEncoder(archiver, _) => {
                let mut file = std::fs::File::open(file_path)
                    .context(format!("Failed to open file {file_path}"))?;
                archiver
                    .append_file(archive_path, &mut file)
                    .context("Failed to append file to archive")?;
            }
            EncoderDriver::ZipEncoder(encoder) => {
                let options = zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated)
                    .unix_permissions(0o755);

                let contents = std::fs::read(file_path)
                    .context(format!("Failed to read file for zip archive {file_path}"))?;
                encoder
                    .start_file(archive_path, options)
                    .context(format!("Failed to start file {file_path} in zip archive"))?;
                encoder
                    .write_all(contents.as_slice())
                    .context(format!("Failed to write {file_path} to zip archive"))?;
            }
        }
        Ok(())
    }

    fn encode_in_chunks<Encoder: std::io::Write>(
        archiver: tar::Builder<Vec<u8>>,
        mut encoder: Encoder,
        updater: Updater,
        driver: Driver
    ) -> anyhow::Result<()> {
        let contents = archiver
            .into_inner()
            .context("Failed to finish zlib tar archive")?;

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
                .context("Failed to write zlib tar archive")?;
        }
        Ok(())
    }

    pub fn finish(self, updater: Updater) -> anyhow::Result<()> {
        let driver = self.driver;
        match self.encoder {
            EncoderDriver::ZlibEncoder(archiver, encoder) => {
                Self::encode_in_chunks(archiver, encoder, updater, driver)?;
            }
            EncoderDriver::GzipEncoder(archiver, encoder) => {
                Self::encode_in_chunks(archiver, encoder, updater, driver)?;
            }
            EncoderDriver::ZipEncoder(encoder) => {
                encoder.finish().context("Failed to finish zip archive")?;
            }
            EncoderDriver::BzipEncoder(archiver, encoder) => {
                Self::encode_in_chunks(archiver, encoder, updater, driver)?;
            }
            EncoderDriver::Bzip2Encoder(archiver, encoder) => {
                Self::encode_in_chunks(archiver, encoder, updater, driver)?;
            }
            EncoderDriver::SevenZEncoder(archiver, mut encoder) => {
                let contents = archiver
                    .into_inner()
                    .context("Failed to finish zlib tar archive")?;
                let mut entry = sevenz_rust::SevenZArchiveEntry::new();
                entry.name = "archive.tar".to_string();

                std::thread::scope(move |s| -> anyhow::Result<()> {
                    let handle = s.spawn(move || -> anyhow::Result<()> {
                        encoder
                            .push_archive_entry(
                                SevenZArchiveEntry::new(),
                                Some(contents.as_slice()),
                            )
                            .context(format!("Failed to push archive.tar to 7z encoder"))?;

                        Ok(())
                    });

                    if let Some(updater) = updater.as_ref() {
                        updater(UpdateStatus {
                            brief: Some(format!("Compressing ({})", driver.extension())),
                            total: Some(500),
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

                    let result = handle.join();
                    match result {
                        Ok(result) => result,
                        Err(err) => Err(anyhow::anyhow!("{:?}", err)),
                    }
                })
                .context(format!("Thread scope failed"))?;
            }
        }
        Ok(())
    }
}
