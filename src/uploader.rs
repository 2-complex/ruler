use crate::system::System;
use std::fmt;
use reqwest::{multipart, Body};
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};

pub enum UploadError
{
    UrlInaccessible(String),
    FileInaccessible(String),
    FileReadDidNotFinish(String),
    HttpError(String),
}

impl fmt::Display for UploadError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            UploadError::UrlInaccessible(url) =>
                write!(formatter, "Url inaccessible: {}", url),

            UploadError::FileInaccessible(path) =>
                write!(formatter, "Failed to open file at path: {}", path),

            UploadError::FileReadDidNotFinish(path) =>
                write!(formatter, "File write did not finish: {}", path),

            UploadError::HttpError(message) =>
                write!(formatter, "Upload failed with HTTP error: {}", message),
        }
    }
}

#[tokio::main]
pub async fn upload_file
<
    SystemType : System
>(
    system : &mut SystemType,
    url : &str,
    path : &str) -> Result<(), UploadError>
{
    let client = reqwest::Client::new();

    let mut file = match File::open(path).await
    {
        Ok(file) => file,
        Err(error) =>
        {
            return Err(UploadError::FileInaccessible(path.to_string()));
        },
    };

    let stream = FramedRead::new(file, BytesCodec::new());
    let file_body = Body::wrap_stream(stream);

    let some_file = match multipart::Part::stream(file_body)
        .mime_str("text/plain")
    {
        Ok(fome) => fome,
        Err(_) => return Err(UploadError::FileInaccessible(path.to_string())),
    };

    let form = multipart::Form::new()
        .part("file", some_file);

    let response = match client.post(url).multipart(form).send().await
    {
        Ok(response) => response,
        Err(_) => return Err(UploadError::UrlInaccessible(url.to_string())),
    };

    let status_code = response.status();
    if ! status_code.is_success()
    {
        return Err(UploadError::HttpError(status_code.to_string()))
    }

    let result = match response.text().await
    {
        Ok(result) => result,
        Err(_) => return Err(UploadError::UrlInaccessible(url.to_string())),
    };

    println!("{:?}", result);

    Ok(())
}
