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

pub type Updater<'a> = Option<&'a dyn Fn(UpdateStatus)>;
