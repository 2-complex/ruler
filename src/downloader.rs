use crate::system::
{
    System,
};
use reqwest::
{
    get,
    StatusCode
};
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

/*  Appeal to the given url to download a file.  If the download is successful up to the point where
    a stream of bytes can be created, then create a file in the file-system to hold the data.

    Then stream the file contents into the file, and if anything goes wrong during the stream, return
    an appropriate error, but keep the file inexistence.
*/
#[tokio::main]
pub async fn download_file
<
    SystemType : System
>(
    system : &mut SystemType,
    url : &str,
    path : &str) -> Result<(), DownloadError>
{
    let mut content =
    match get(url).await
    {
        Ok(response) =>
        {
            if response.status() != StatusCode::OK
            {
                return Err(DownloadError::UrlInaccessible(url.to_string()));
            }
            response.bytes_stream()
        },
        Err(_error) => return Err(DownloadError::UrlInaccessible(url.to_string())),
    };

    let mut file =
    match system.create_file(path)
    {
        Ok(file) => file,
        Err(_error) => return Err(DownloadError::FileWouldNotCreate(path.to_string())),
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
                    Err(_) => return Err(DownloadError::FileWriteDidNotFinish(path.to_string())),
                }
            }
            Err(_) => return Err(DownloadError::FailedMidDownload(url.to_string())),
        }
    }

    Ok(())
}

/*  Appeal to the url and just return the String that downloads,
    or an appropriae error.*/
#[tokio::main]
pub async fn download_string(url : &str) -> Result<String, DownloadError>
{
    match get(url).await
    {
        Ok(response) =>
        {
            if response.status() != StatusCode::OK
            {
                return Err(DownloadError::UrlInaccessible(url.to_string()));
            }
            match response.text().await
            {
                Ok(s) => Ok(s),
                Err(_) => return Err(DownloadError::FailedMidDownload(url.to_string())),
            }
        },
        Err(_error) => return Err(DownloadError::UrlInaccessible(url.to_string())),
    }
}
