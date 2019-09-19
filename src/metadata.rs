use std::fs;
use std::time::SystemTime;

pub trait MetadataGetter
{
    fn get_modified(&self, path: &str) -> Result<SystemTime, String>;
}

pub struct OsMetadataGetter
{
}

impl OsMetadataGetter
{
    pub fn new() -> OsMetadataGetter
    {
        OsMetadataGetter
        {
        }
    }
}

impl MetadataGetter for OsMetadataGetter
{
    fn get_modified(&self, path: &str) -> Result<SystemTime, String>
    {
        match fs::metadata(path)
        {
            Ok(metadata) =>
            {
                match metadata.modified()
                {
                    Ok(timestamp) => Ok(timestamp),
                    Err(_) => Err(format!("Could not get modified date for file: {}", path))
                }
            },
            Err(_) => Err(format!("Could not get metadata for file: {}", path))
        }
    }
}

#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::time::Duration;

#[cfg(test)]
pub struct FakeMetadataGetter
{
    path_to_time: HashMap<String, SystemTime>,
}

#[cfg(test)]
impl FakeMetadataGetter
{
    pub fn new() -> FakeMetadataGetter
    {
        FakeMetadataGetter
        {
            path_to_time: HashMap::new(),
        }
    }

    pub fn insert_timestamp(&mut self, path: &str, timestamp: u64)
    {
        self.path_to_time.insert(path.to_string(),
            SystemTime::UNIX_EPOCH
                + Duration::from_secs(timestamp / 1_000_000u64)
                + Duration::from_micros(timestamp % 1_000_000u64));
    }
}

#[cfg(test)]
impl MetadataGetter for FakeMetadataGetter
{
    fn get_modified(&self, path: &str) -> Result<SystemTime, String>
    {
        match self.path_to_time.get(path)
        {
            Some(time) => Ok(*time),
            None => Err(format!("Couldn't get modified date for {}", path)),
        }
    }
}
