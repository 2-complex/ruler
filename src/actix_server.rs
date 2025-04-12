use actix_multipart::Multipart;
use actix_web::
{
    delete,
    get,
    post,
    web,
    App,
    HttpResponse,
    HttpServer,
    Responder
};
use futures::
{
    StreamExt,
    TryStreamExt
};

use tokio::{io::{self, AsyncBufReadExt, BufReader}};
use std::fs;
use std::path::Path;
use std::sync::Mutex;

use crate::system::real::RealSystem;
use crate::server::ServerError;
use crate::directory;
use crate::directory::Elements;
use crate::ticket::Ticket;

use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;

use std::io::Read;
use std::io::Write;

#[post("/upload")]
async fn upload(data: web::Data<AppStateWithCounter>, mut payload: Multipart) -> impl Responder
{
    let mut inbox_file = match data.elements.cache.clone().open_inbox_file()
    {
        Ok(inbox_file) => inbox_file,
        Err(_) =>
            return HttpResponse::InternalServerError().body("Inbox Start Error"),
    };

    while let Ok(Some(mut field)) = payload.try_next().await
    {
        while let Some(chunk) = field.next().await
        {
            let data = chunk.unwrap();
            match inbox_file.write(&data)
            {
                Ok(_) => {},
                Err(_) =>
                    return HttpResponse::InternalServerError().body("Inbox Write Error"),
            }
        }
    }

    let ticket = match inbox_file.finish()
    {
        Ok(ticket) => ticket,
        Err(_) =>
            return HttpResponse::InternalServerError().body("Inbox Finish Error"),
    };

    HttpResponse::Ok().body(format!("{}", ticket))
}

struct AppStateWithCounter
{
    counter : Mutex<i32>,
    elements : Elements<RealSystem>
}

#[get("/")]
async fn home(data: web::Data<AppStateWithCounter>) -> impl Responder
{
    let mut counter = data.counter.lock().unwrap();
    *counter += 1;
    let body_string = format!("WOW!  Text! {:?}", *counter);
    HttpResponse::Ok().body(body_string)
}

#[get("/files")]
async fn files(_data: web::Data<AppStateWithCounter>) -> impl Responder
{
    HttpResponse::Ok().body("Files!")
}

#[get("/files/{hash}")]
async fn files_hash(data: web::Data<AppStateWithCounter>, hash: web::Path<String>) -> impl Responder
{
    let mut counter = data.counter.lock().unwrap();
    *counter += 1;
    let hash_str = hash.to_string();

    let ticket = match Ticket::from_human_readable(&hash_str)
    {
        Err(error) => return HttpResponse::NotFound().body(format!("Error: {}", error)),
        Ok(ticket) => ticket,
    };

    let mut file = match data.elements.cache.open(&ticket)
    {
        Err(error) => return HttpResponse::NotFound().body(format!("Error opening file: {} {}", hash_str, error)),
        Ok(file) => file,
    };

    let mut buffer = vec![];
    match file.read_to_end(&mut buffer)
    {
        Err(error) => return HttpResponse::InternalServerError().body(
            format!("Error while reading file: {} {}", hash_str, error)),
        Ok(_size) => {},
    }

    HttpResponse::Ok().body(buffer)
}

#[delete("/{filename}")]
async fn delete(filename: web::Path<String>) -> impl Responder
{
    let filename = filename.into_inner();
    let filepath = format!("./{}", filename);

    if Path::new(&filepath).exists()
    {
        fs::remove_file(filepath).unwrap();
        HttpResponse::Ok().body("File deleted successfully")
    }
    else
    {
        HttpResponse::NotFound().body("File not found")
    }
}

#[tokio::main]
pub async fn serve
(
    mut system : RealSystem,
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

    let app_data = web::Data::new(AppStateWithCounter
    {
        counter: 0.into(),
        elements: elements,
    });

    // Create a new HTTP server
    let server = HttpServer::new(move || {
        App::new()
            .app_data(app_data.clone())
            .service(upload)
            .service(delete)
            .service(home)
            .service(files)
            .service(files_hash)
    });

    let socket_address = SocketAddr::new(IpAddr::V4(address), port);
    let server = match server.bind(socket_address)
    {
        Err(error) => return Err(ServerError::BindFailed(error.to_string())),
        Ok(server) => server,
    };

    let server = server.run();
    let (tx, rx) = tokio::sync::oneshot::channel();
    // Create a one-shot channel for shutting down the server

    // Spawn a new task that waits for a line from stdin and then sends a signal to the channel
    tokio::spawn(async move
    {
        let mut reader = BufReader::new(io::stdin());
        let mut buffer = String::new();
        reader.read_line(&mut buffer).await.expect("Failed to read line from stdin");
        tx.send(()).unwrap();
    });

    println!("Serving {} on ENTER to stop", socket_address);
    // Wait for either the server to finish or a signal from the channel
    tokio::select!
    {
        _ = server => {},
        _ = rx =>
        {
            println!("ENTER pressed, shutting down");
        }
    }

    Err(ServerError::Weird)
}

