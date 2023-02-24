use std::fmt;

/*
use std::io::
{
    self,
    Read,
};
*/

use warp::http::
{
    Response,
    StatusCode,
};

// use crate::memory::{Memory, MemoryError};
// use crate::cache::LocalCache;
use crate::printer::Printer;
use crate::directory;

use crate::ticket::
{
    Ticket,
};
// use termcolor::Color;

use warp::Filter;

use crate::system::
{
    System,
};

pub enum ServerError
{
    Weird,
}

impl fmt::Display for ServerError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            ServerError::Weird =>
                write!(formatter, "Weird Server Error"),
        }
    }
}

#[tokio::main]
pub async fn serve
<
    SystemType : System + Clone + Send + 'static,
    PrinterType : Printer,
>
(
    mut system : SystemType,
    directory_path : &str,
    printer: &mut PrinterType,
)
-> Result<(), ServerError>
{
    let (memory, cache, _memoryfile) =
    match directory::init(&mut system, directory_path)
    {
        Ok((memory, cache, memoryfile)) => (memory, cache, memoryfile),
        Err(error) => panic!("Failed to init directory error: {}", error)
    };

    let files_endpoint = warp::get()
        .and(warp::path!("files" / String))
        .map(move |hash_str : String|
            {
                match Ticket::from_base64(&hash_str)
                {
                    Ok(ticket) =>
                    {
                        match cache.open(&ticket)
                        {
                            Ok(file) =>
                            {
                                Response::builder()
                                    .status(StatusCode::OK)
                                    .body(format!("Yes").into_bytes())
                            },
                            Err(error) =>
                            {
                                Response::builder()
                                    .status(StatusCode::NOT_FOUND)
                                    .body(format!("Error: {}", error).into_bytes())
                            }
                        }
                    },
                    Err(error) =>
                    {
                        Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(format!("Error: {}", error).into_bytes())
                    }
                }
            });

    let rules_endpoint = warp::get()
        .and(warp::path!("rules" / String))
        .map(move |hash_str : String|
            {
                match Ticket::from_base64(&hash_str)
                {
                    Ok(ticket) =>
                    {
                        Response::builder()
                            .status(StatusCode::OK)
                            .body(format!("RESPONSE {}", ticket).into_bytes())
                    },
                    Err(error) =>
                    {
                        Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(format!("Error: {}", error).into_bytes())
                    }
                }
            });

    warp::serve(files_endpoint.or(rules_endpoint))
        .run(([127, 0, 0, 1], 8080))
        .await;

    Err(ServerError::Weird) 
}

#[cfg(test)]
mod test
{

}
