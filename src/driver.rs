use anyhow::Context;
use anyhow_source_location::{format_context, format_error};

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Driver {
    Gzip,
    Bzip2,
    Zip,
    SevenZ,
}

pub(crate) const SEVEN_Z_TAR_FILENAME: &str = "swiss_army_archive_seven7_temp.tar";

impl Driver {
    pub fn extension(&self) -> String {
        match &self {
            Driver::Gzip => "tar.gz".to_string(),
            Driver::Bzip2 => "tar.bz2".to_string(),
            Driver::Zip => "zip".to_string(),
            Driver::SevenZ => "tar.7z".to_string(),
        }
    }

    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension {
            "tar.gz" => Some(Driver::Gzip),
            "tar.bz2" => Some(Driver::Bzip2),
            "zip" => Some(Driver::Zip),
            "tar.7z" => Some(Driver::SevenZ),
            _ => None,
        }
    }

    pub fn from_filename(filename: &str) -> Option<Self> {
        if filename.ends_with(".tar.gz") {
            Some(Driver::Gzip)
        } else if filename.ends_with(".tar.bz") {
            Some(Driver::Bzip2)
        } else if filename.ends_with(".tar.bz2") {
            Some(Driver::Bzip2)
        } else if filename.ends_with(".zip") {
            Some(Driver::Zip)
        } else if filename.ends_with(".tar.7z") {
            Some(Driver::SevenZ)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct UpdateStatus {
    pub brief: Option<String>,
    pub detail: Option<String>,
    pub increment: Option<u64>,
    pub total: Option<u64>,
}

#[cfg(feature = "printer")]
pub(crate) fn update_status(progress: &mut printer::MultiProgressBar, update_status: UpdateStatus) {
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

pub(crate) fn digest_file(
    file_path: &str,
    #[cfg(feature = "printer")] progress: &mut printer::MultiProgressBar,
) -> anyhow::Result<String> {
    #[cfg(feature = "printer")]
    update_status(
        progress,
        UpdateStatus {
            brief: Some(format!("Digesting")),
            detail: Some("...".to_string()),
            total: Some(200),
            ..Default::default()
        },
    );

    let file_path = file_path.to_owned();

    let handle = std::thread::spawn(move || -> anyhow::Result<String> {
        let file_contents = std::fs::read(&file_path).context(format_context!("{file_path}"))?;
        let digest = sha256::digest(file_contents);
        Ok(digest)
    });

    wait_handle(
        handle,
        #[cfg(feature = "printer")]
        progress,
    )
    .context(format_context!(""))
}

pub(crate) fn wait_handle<OkType>(
    handle: std::thread::JoinHandle<Result<OkType, anyhow::Error>>,
    #[cfg(feature = "printer")] progress: &mut printer::MultiProgressBar,
) -> anyhow::Result<OkType> {
    while !handle.is_finished() {
        #[cfg(feature = "printer")]
        update_status(
            progress,
            UpdateStatus {
                increment: Some(1),
                ..Default::default()
            },
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    let result = handle
        .join()
        .map_err(|err| format_error!("failed to join thread: {:?}", err))?;

    result.map_err(|err| format_error!("{:?}", err))
}
