extern crate reqwest;
extern crate hyper;
extern crate multipart;
extern crate hyper_native_tls;
extern crate filesystem;

use std::io::Read;
use std::fmt;

use hyper::Client;
use hyper::net::HttpsConnector;
use hyper_native_tls::NativeTlsClient;
use multipart::client::lazy::Multipart;

use filesystem::FileSystem;


pub enum DownloadError
{
    ResponseBodyFailedToRead,
    RequestFailed,
    DownloadFailed,
    FileFailedToWrite,
}

pub enum UploadError
{
    UploadFailed,
    NotSupportedOnPlatform,
    UploadConstructionFailed,
    FileFailedToRead(String),
    NotStatus200(String),
}

impl fmt::Display for UploadError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            UploadError::NotSupportedOnPlatform =>
                write!(formatter, "Upload type not supported."),

            UploadError::UploadConstructionFailed =>
                write!(formatter, "Upload construction failed."),

            UploadError::UploadFailed =>
                write!(formatter, "Upload failed."),

            UploadError::FileFailedToRead(path) =>
                write!(formatter, "File failed to read for upload: {}", path),

            UploadError::NotStatus200(url) =>
                write!(formatter, "Upload request recieved non-200 code. URL={}", url),
        }
    }
}

fn upload_stream
<
    StreamType : Read
>
(
    url : &str,
    name : &str,
    filename : &str,
    stream : StreamType
)
-> Result<String, UploadError>
{
    let client =
    match NativeTlsClient::new()
    {
        Ok(client) => client,
        Err(_) =>
        {
            return Err(UploadError::NotSupportedOnPlatform);
        }
    };

    let connector = HttpsConnector::new(client);
    let client = Client::with_connector(connector);

    let mut response =
    match Multipart::new()
        .add_stream(name, stream, Some(filename), None)
        .client_request(&client, url)
    {
        Ok(response) => response,
        Err(_) =>
        {
            return Err(UploadError::UploadConstructionFailed);
        }
    };

    let mut s = String::new();
    response.read_to_string(&mut s).unwrap();

    Ok(s)
}

pub fn upload
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
                        Err(UploadError::NotStatus200(url.to_string()))
                    }
                    else
                    {
                        Ok(())
                    }
                },
                Err(_) => Err(UploadError::UploadFailed),
            }
        },
        Err(_) => Err(UploadError::FileFailedToRead(path.to_string())),
    }
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
    use filesystem::{FakeFileSystem, FileSystem};
    use crate::internet::{download, DownloadError, upload, upload_stream};

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

    use std::fs::File;

    #[test]
    fn upload_stream_round_trip()
    {
        let upload_url = format!("{}/upload-file", SERVER);
        let file = File::open("your-file.txt").unwrap();
        match upload_stream(&upload_url, "name.txt", "filename.txt", file)
        {
            Ok(result) =>
            {
                println!("result = {}", result);
            },
            Err(error) =>
            {
                panic!("Failed to upload: {}", error);
            }
        }
    }
}

