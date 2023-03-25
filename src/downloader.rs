use crate::system::
{
    System,
};
use reqwest;
use std::fmt;
use futures::StreamExt;
use std::io::Write;

pub enum DownloadError
{
    UrlInaccessible(String),
    FailedMidDownload(String),
    FileWouldNotCreate(String),
    FileWriteDidNotFinish(String),
}

impl fmt::Display for DownloadError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            DownloadError::UrlInaccessible(url) =>
                write!(formatter, "Url did not work: {}", url),

            DownloadError::FailedMidDownload(url) =>
                write!(formatter, "Failed mid download: {}", url),

            DownloadError::FileWouldNotCreate(path) =>
                write!(formatter, "Failed to create file at path: {}", path),

            DownloadError::FileWriteDidNotFinish(path) =>
                write!(formatter, "File write did not finish: {}", path),
        }
    }
}

#[tokio::main]
pub async fn download
<
    SystemType : System
>(
    system : &mut SystemType,
    url : String,
    path : String) -> Result<(), DownloadError>
{
    let mut file =
    match system.create_file(&path)
    {
        Ok(file) => file,
        Err(_error) => return Err(DownloadError::FileWouldNotCreate(path)),
    };

    let mut content =
    match reqwest::get(&url).await
    {
        Ok(response) => response.bytes_stream(),
        Err(_error) => return Err(DownloadError::UrlInaccessible(url)),
    };

    while let Some(item) = content.next().await
    {
        match item
        {
            Ok(chunk) =>
            {
                match file.write(&chunk)
                {
                    Ok(_) => {},
                    Err(_) => return Err(DownloadError::FileWriteDidNotFinish(path)),
                }
            }
            Err(_) => return Err(DownloadError::FailedMidDownload(url)),
        }
    }

    Ok(())
}
