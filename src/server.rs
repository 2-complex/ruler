use std::fmt;
use std::io::Read;
use std::net::SocketAddr;
use std::net::Ipv4Addr;
use std::net::IpAddr;

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
    port : u16
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
                match Ticket::from_human_readable(&hash_str)
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
                                        let message = format!("Error while reading file: {} {}", hash_str, error);
                                        println!("{}", &message);
                                        Response::builder()
                                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                                            .body(message.into_bytes())
                                    },
                                }
                            },
                            Err(error) =>
                            {
                                let message = format!("Error opening file: {} {}", hash_str, error);
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
            }
        );

    let history = elements.history.clone();
    let rules_endpoint = warp::get()
        .and(warp::path!("rules" / String / String))
        .map(
            move |rule_hash_str : String, source_hash_str : String|
            {
                let rule_ticket =
                match Ticket::from_human_readable(&rule_hash_str)
                {
                    Ok(ticket) => ticket,
                    Err(error) =>
                    {
                        return Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(format!("Error: {}", error).into_bytes())
                    }
                };

                let source_ticket =
                match Ticket::from_human_readable(&source_hash_str)
                {
                    Ok(ticket) => ticket,
                    Err(error) =>
                    {
                        return Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(format!("Error: {}", error).into_bytes())
                    }
                };

                let rule_history =
                match history.read_rule_history(&rule_ticket)
                {
                    Ok(rule_history) => rule_history,
                    Err(error) => return
                        Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(format!("Error: {}", error).into_bytes()),
                };

                let target_tickets =
                match rule_history.get_file_state_vec(&source_ticket)
                {
                    Some(target_tickets) => target_tickets,
                    None => return
                        Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(format!("No entry for source: {}", source_ticket).into_bytes()),
                };

                Response::builder()
                    .status(StatusCode::OK)
                    .body(format!("{}", target_tickets.download_string()).into_bytes())
            });

    let home_endpoint =
    {
        let history = elements.history.clone();
        warp::get()
            .and(warp::path!("history"))
            .map(
                move||
                {
                    let message =
                    match history.list()
                    {
                        Ok(rule_hashes) =>
                        {
                            let mut message_vec = vec!["<html><head></head><body style = \"font-family: monospace;\">".to_string()];
                            for hash in rule_hashes
                            {
                                message_vec.push(format!("<a href= \"rule_history/{}\">{}</a>", hash, hash));
                            }
                            message_vec.push("</body></html>".to_string());
                            message_vec.join("\n")
                        },
                        Err(error) =>
                        {
                            format!("{}", error)
                        }
                    };

                    Response::builder()
                        .status(StatusCode::OK)
                        .body(message)
                }
            )
    };

    let rule_history_endpoint =
    {
        let history = elements.history.clone();
        warp::get()
            .and(warp::path!("rule_history" / String))
            .map(
                move|rule_hash_str : String|
                {
                    let rule_ticket =
                    match Ticket::from_human_readable(&rule_hash_str)
                    {
                        Ok(ticket) => ticket,
                        Err(error) =>
                        {
                            return Response::builder()
                                .status(StatusCode::NOT_FOUND)
                                .body(format!("Error: {}", error).into_bytes())
                        }
                    };

                    let rule_history =
                    match history.read_rule_history(&rule_ticket)
                    {
                        Ok(rule_history) => rule_history,
                        Err(error) => return
                            Response::builder()
                                .status(StatusCode::NOT_FOUND)
                                .body(format!("Error: {}", error).into_bytes()),
                    };

                    let mut message_vec = vec!["<html><head></head><body style = \"font-family: monospace;\">".to_string()];
                    let source_to_targets = &rule_history.get_source_to_targets();

                    for (source_ticket, file_state_vec) in source_to_targets.iter()
                    {
                        let file_state_vec_download_string = file_state_vec.download_string();
                        message_vec.push(format!("{} <a href = \"/files/{}\">{}</a><br/>", source_ticket, file_state_vec_download_string, file_state_vec_download_string));
                    }

                    Response::builder()
                        .status(StatusCode::OK)
                        .body(message_vec.join("\n").into_bytes())
                }
            )
    };

    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);
    println!("Serving on {}", address);

    warp::serve(home_endpoint
            .or(files_endpoint)
            .or(rules_endpoint)
            .or(rule_history_endpoint))
        .run(address)
        .await;

    Err(ServerError::Weird) 
}

#[cfg(test)]
mod test
{

}
