use std::io::Write;

use anyhow::Context;
use sevenz_rust::SevenZArchiveEntry;

pub enum Driver {
    Zlib,
    Gzip,
    Bzip,
    Bzip2,
    Zip,
    SevenZ,
}

pub struct Entry {
    pub archive_path: String,
    pub file_path: String,
}

enum EncoderDriver<Writer: std::io::Write + std::io::Seek + std::marker::Send> {
    ZlibEncoder(tar::Builder<Vec<u8>>, flate2::write::ZlibEncoder<Writer>),
    GzipEncoder(tar::Builder<Vec<u8>>, flate2::write::GzEncoder<Writer>),
    ZipEncoder(zip::ZipWriter<Writer>),
    SevenZEncoder(tar::Builder<Vec<u8>>, sevenz_rust::SevenZWriter<Writer>),
}

pub struct Encoder<Writer: std::io::Write + std::io::Seek + std::marker::Send> {
    driver: EncoderDriver<Writer>,
}

impl<Writer: std::io::Write + std::io::Seek + std::marker::Send> Encoder<Writer> {
    pub fn new(driver: Driver, writer: Writer) -> anyhow::Result<Self> {
        match driver {
            Driver::Zlib => {
                let archiver = tar::Builder::new(Vec::new());
                let encoder =
                    flate2::write::ZlibEncoder::new(writer, flate2::Compression::default());
                Ok(Self {
                    driver: EncoderDriver::ZlibEncoder(archiver, encoder),
                })
            }
            Driver::Gzip => {
                let archiver = tar::Builder::new(Vec::new());
                let encoder = flate2::write::GzEncoder::new(writer, flate2::Compression::default());
                Ok(Self {
                    driver: EncoderDriver::GzipEncoder(archiver, encoder),
                })
            }
            Driver::Zip => {
                let encoder = zip::ZipWriter::new(writer);
                Ok(Self {
                    driver: EncoderDriver::ZipEncoder(encoder),
                })
            }
            Driver::SevenZ => {
                let archiver = tar::Builder::new(Vec::new());
                let encoder = sevenz_rust::SevenZWriter::new(writer)
                    .context(format!("Failed to create 7z encoder"))?;
                Ok(Self {
                    driver: EncoderDriver::SevenZEncoder(archiver, encoder),
                })
            }
        }
    }

    pub fn add_entries(
        &mut self,
        entries: &Vec<Entry>,
        updater: Option<&dyn Fn(usize, usize)>,
    ) -> anyhow::Result<()> {
        for (index, entry) in entries.iter().enumerate() {
            if let Some(updater) = updater {
                updater(index, entries.len());
            }
            self.add_file(&entry.archive_path, &entry.file_path)
                .context(format!("Failed to add {}", entry.archive_path))?;
        }
        Ok(())
    }

    pub fn add_file(&mut self, archive_path: &str, file_path: &str) -> anyhow::Result<()> {
        match &mut self.driver {
            EncoderDriver::ZlibEncoder(archiver, _)
            | EncoderDriver::GzipEncoder(archiver, _)
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
        updater: Option<&dyn Fn(usize, usize)>,
    ) -> anyhow::Result<()> {
        let contents = archiver
            .into_inner()
            .context("Failed to finish zlib tar archive")?;

        let total_chunks = contents.len() / 4096;

        for (index, chunk) in contents.as_slice().chunks(total_chunks).enumerate() {
            if let Some(updater) = updater {
                updater(index, contents.len() / total_chunks);
            }
            encoder
                .write_all(chunk)
                .context("Failed to write zlib tar archive")?;
        }
        Ok(())
    }

    pub fn finish(self, updater: Option<&dyn Fn(usize, usize)>) -> anyhow::Result<()> {
        match self.driver {
            EncoderDriver::ZlibEncoder(archiver, mut encoder) => {
                Self::encode_in_chunks(archiver, encoder, updater)?;
            }
            EncoderDriver::GzipEncoder(archiver, mut encoder) => {
                Self::encode_in_chunks(archiver, encoder, updater)?;
            }
            EncoderDriver::ZipEncoder(encoder) => {
                encoder.finish().context("Failed to finish zip archive")?;
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

                    let mut count = 0_usize;
                    while !handle.is_finished() {
                        if let Some(updater) = updater {
                            updater(count, 0);
                            count += 1;
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

    pub fn get_file_suffix(&self) -> String {
        match &self.driver {
            EncoderDriver::ZlibEncoder(_, _) => "zlib".to_string(),
            EncoderDriver::GzipEncoder(_, _) => "tar.gz".to_string(),
            EncoderDriver::ZipEncoder(_) => "zip".to_string(),
            EncoderDriver::SevenZEncoder(_, _) => "tar.7z".to_string(),
        }
    }
}
