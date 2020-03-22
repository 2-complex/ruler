extern crate reqwest;
extern crate hyper;
extern crate hyper_native_tls;
extern crate filesystem;

use std::io::Read;
use filesystem::FileSystem;

pub enum DownloadError
{
    ResponseBodyFailedToRead,
    RequestFailed,
    DownloadFailed,
    FileFailedToWrite,
}

pub fn download
<FileSystemType : FileSystem>
(
    url : &str,
    file_system : &FileSystemType,
    path : &str
)
-> Result<(), DownloadError>
{
    match reqwest::get(url)
    {
        Ok(mut response) =>
        {
            let mut body = Vec::new();
            if response.status().is_success()
            {
                match response.read_to_end(&mut body)
                {
                    Ok(_) =>
                    {
                        match file_system.write_file(path, body)
                        {
                            Ok(()) => Ok(()),
                            Err(_) => Err(DownloadError::FileFailedToWrite)
                        }
                    },
                    Err(_) => return Err(DownloadError::ResponseBodyFailedToRead),
                }
            }
            else
            {
                Err(DownloadError::DownloadFailed)
            }
        },
        Err(_) => Err(DownloadError::RequestFailed)
    }
}

#[cfg(test)]
mod test
{
}

