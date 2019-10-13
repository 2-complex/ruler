extern crate reqwest;
extern crate filesystem;

use filesystem::FileSystem;

use std::io::Read;

enum DownloadError
{
    ResponseBodyFailedToRead,
    RequestFailed,
    DownloadFailed,
    FileFailedToWrite,
}

enum UploadError
{
    UploadFailed,
    FileFailedToRead,
    NotStatus200
}

fn upload
<FileSystemType : FileSystem>
(
    url : &str,
    file_system : &FileSystemType,
    path : &str
)
-> Result<(), UploadError>
{
    let client = reqwest::Client::new();

    match file_system.read_file(path)
    {
        Ok(content) =>
        {
            match client.post(url).body(content).send()
            {
                Ok(response) =>
                {
                    if response.status() != 200
                    {
                        Err(UploadError::NotStatus200)
                    }
                    else
                    {
                        Ok(())
                    }
                },
                Err(_) => Err(UploadError::UploadFailed),
            }
        },
        Err(_) => Err(UploadError::FileFailedToRead),
    }
}

fn download
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
    use filesystem::{FakeFileSystem, FileSystem};
    use crate::internet::{download, DownloadError, upload};

    static SERVER: &str = "https://sha-da.com";

    #[test]
    fn upload_download_roundtrip()
    {
        let file_system = FakeFileSystem::new();

        match file_system.write_file("pears.txt",
            "Now is the time for all good men to come to the aid of their country.\n")
        {
            Ok(_) => {},
            Err(_error) => panic!("Write file failed"),
        }

        let upload_url = format!("{}/upload-data", SERVER);

        match upload(
            &upload_url,
            &file_system,
            "pears.txt")
        {
            Ok(_) => {},
            Err(_error) => panic!("Upload failed"),
        }

        let download_url = format!("{}/files/{}",
            SERVER,
            "3T13OaUBYrG_KaL6mlW-iviF-hyfs-9tqQ1jT6oCKzdoU8dTyoDCvadkkTqIoJDJnsNG_9DGVyW1rTSKsMhqww==");

        match download(
            &download_url,
            &file_system,
            "apples.txt")
        {
            Ok(_) =>
            {
                match file_system.read_file_to_string("apples.txt")
                {
                    Ok(content) => assert_eq!(content, "Now is the time for all good men to come to the aid of their country.\n"),
                    Err(_error) => panic!("File contents didn't convert to string : {}"),
                }
            },
            Err(_) => panic!("Download failed"),
        }
    }

    #[test]
    fn test_download_not_there()
    {
        let file_system = FakeFileSystem::new();

        match download(
            "https://sha-da.com/files/notarealurl==",
            &file_system,
            "apples.txt")
        {
            Ok(_) => panic!("Unexpected presence at made-up url"),
            Err(DownloadError::DownloadFailed) => {},
            Err(_) => panic!("Download failed"),
        }
    }
}
