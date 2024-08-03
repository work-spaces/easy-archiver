#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Driver {
    Zlib,
    Gzip,
    Bzip,
    Bzip2,
    Zip,
    SevenZ,
}

impl Driver {
    pub fn extension(&self) -> String {
        match &self {
            Driver::Zlib => "tar.gz".to_string(),
            Driver::Gzip => "tar.gz".to_string(),
            Driver::Bzip => "tar.bz".to_string(),
            Driver::Bzip2 => "tar.bz2".to_string(),
            Driver::Zip => "zip".to_string(),
            Driver::SevenZ => "tar.7z".to_string(),
        }
    }

    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension {
            "tar.gz" => Some(Driver::Gzip),
            "tar.bz" => Some(Driver::Bzip),
            "tar.bz2" => Some(Driver::Bzip2),
            "zip" => Some(Driver::Zip),
            "tar.7z" => Some(Driver::SevenZ),
            _ => None,
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
