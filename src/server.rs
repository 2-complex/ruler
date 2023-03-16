use std::fmt;
use std::io::Read;

use warp::http::
{
    Response,
    StatusCode,
};

use crate::directory;

use crate::ticket::
{
    Ticket,
};

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
>
(
    mut system : SystemType,
    directory_path : &str,
)
-> Result<(), ServerError>
{
    let elements =
    match directory::init(&mut system, directory_path)
    {
        Ok(elements) => elements,
        Err(error) => panic!("Failed to init directory error: {}", error)
    };

    let cache = elements.cache;

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
                        Ok(mut file) =>
                        {
                            let mut buffer = vec![];
                            match file.read_to_end(&mut buffer)
                            {
                                Ok(size) =>
                                {
                                    println!("Serving file: {} size: {}", hash_str, size);
                                Response::builder()
                                    .status(StatusCode::OK)
                                        .body(buffer)
                                },
                                Err(error) =>
                                {
                                    let message = format!("Error: {}", error);
                                    println!("{}", &message);

                                    Response::builder()
                                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                                        .body(message.into_bytes())
                                },
                            }
                        },
                        Err(error) =>
                        {
                            let message = format!("Error: {}", error);
                            println!("{}", &message);

                            Response::builder()
                                .status(StatusCode::NOT_FOUND)
                                .body(message.into_bytes())
                        }
                    }
                },
                Err(error) =>
                {
                    let message = format!("Error: {}", error);
                    println!("{}", &message);

                    Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(message.into_bytes())
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
