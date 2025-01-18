use std::fmt;
use std::fs;
use std::io::Read;
use std::net::
{
    SocketAddr,
    IpAddr,
    Ipv4Addr
};

use warp::http::Response;
use warp::http::StatusCode;
use warp::multipart::FormData;
use warp::Reply;
use warp::Rejection;
use warp::Filter;
use crate::directory;
use crate::ticket::Ticket;
use futures::TryStreamExt;
use futures::StreamExt;
use crate::cache::SysCache;
use crate::history::History;
use crate::system::System;
use std::path::PathBuf;
use std::path::Path;
use bytes::buf::Buf;


fn to_path_buf(path: &str) -> PathBuf
{
    Path::new(".").join(path.split("/").map(|s|{s.to_string()}).collect::<PathBuf>())
}

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

fn get_history_endpoint<SystemType : System + Clone + Send + 'static>
(history : History<SystemType>) -> warp::filters::BoxedFilter<(impl warp::Reply,)>
{
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
        ).boxed()
}

fn get_rule_history_endpoint<SystemType : System + Clone + Send + 'static>
(history : History<SystemType>) -> warp::filters::BoxedFilter<(impl warp::Reply,)>
{
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
        ).boxed()
}

fn get_files_endpoint<SystemType : System + Clone + Send + 'static>
(cache : SysCache<SystemType>) -> warp::filters::BoxedFilter<(impl warp::Reply,)>
{
    warp::get()
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
        )
    .boxed()
}

fn get_rules_endpoint<SystemType : System + Clone + Send + 'static>
(history : History<SystemType>) -> warp::filters::BoxedFilter<(impl warp::Reply,)>
{
    warp::get()
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
            })
    .boxed()
}

async fn process_upload<SystemType : System + Clone + 'static>(_cache : SysCache<SystemType>, form: FormData) -> Result<impl Reply, Rejection>
{
    let mut parts = form.into_stream();
    loop
    {
        match parts.next().await
        {
            Some(part_result) =>
            {
                match part_result
                {
                    Ok(mut p) =>
                    {
                        println!("------Part Received-------");
                        println!("name: {:?}", p.name());
                        println!("filename: {:?}", p.filename());
                        println!("content-type: {:?}", p.content_type());

                        let target_filename =
                        match p.filename()
                        {
                            Some(name) =>
                            {
                                format!(".files/{}", name)
                            },
                            None =>
                            {
                                println!("Not a file actually");
                                continue;
                            },
                        };

                        println!("Making sure the .files directory is there...");
                        match fs::create_dir(".files")
                        {
                            Ok(_) => {},
                            Err(err) =>
                            {
                                eprintln!("create directory error: {}", err);
                                warp::reject::reject();
                            }
                        }

                        println!("proceeding with target_filename = {}", target_filename);

                        match p.data().await
                        {
                            Some(p_result) =>
                            {
                                match p_result
                                {
                                    Ok(data_buf) =>
                                    {
                                        println!("{:?}", data_buf.remaining());
                                        match fs::File::create(to_path_buf(&target_filename))
                                        {
                                            Ok(mut file) =>
                                            {
                                                println!("AH HA we're here, let's write the file!");
                                                println!("{:?}", file);
                                                let _ = std::io::copy(&mut data_buf.reader(), &mut file);
                                            },
                                            Err(err) =>
                                            {
                                                eprintln!("create file error: {}", err);
                                                warp::reject::reject();
                                            }
                                        }
                                    },

                                    Err(err) =>
                                    {
                                        println!("error getting databuff: {:?}", err);
                                    },
                                }
                            },

                            None =>
                            {
                                println!("nodata");
                            }
                        };
                    },

                    Err(err) =>
                    {
                        println!("{:?}", err);
                    }
                }
            },

            None => break,
        }
    }

    Ok("success\n")
}

fn get_upload_endpoint<SystemType : System + Clone + Send + 'static>
(cache : SysCache<SystemType>) -> warp::filters::BoxedFilter<(impl warp::Reply,)>
{
    warp::path("upload")
        .and(warp::post())
        .and(warp::multipart::form())
        .and_then(move |form| process_upload(cache.clone(), form))
    .boxed()
}

#[tokio::main]
pub async fn serve
<
    SystemType : System + Clone + Send + 'static,
>
(
    mut system : SystemType,
    directory_path : &str,
    address : Ipv4Addr,
    port : u16
) -> Result<(), ServerError>
{
    let elements =
    match directory::init(&mut system, directory_path)
    {
        Ok(elements) => elements,
        Err(error) => panic!("Failed to init directory error: {}", error)
    };

    let history_endpoint = get_history_endpoint(elements.history.clone());
    let rule_history_endpoint = get_rule_history_endpoint(elements.history.clone());
    let files_endpoint = get_files_endpoint(elements.cache.clone());
    let rules_endpoint = get_rules_endpoint(elements.history);
    let upload_endpoint = get_upload_endpoint(elements.cache);

    let socket_address = SocketAddr::new(IpAddr::V4(address), port);
    println!("Serving on {}", socket_address);

    warp::serve(
        upload_endpoint
        .or(history_endpoint)
        .or(rule_history_endpoint)
        .or(files_endpoint)
        .or(rules_endpoint))
    .run(socket_address).await;

    Err(ServerError::Weird)
}

#[cfg(test)]
mod test
{

}
