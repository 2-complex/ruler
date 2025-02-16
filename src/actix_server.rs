use actix_multipart::Multipart;
use std::path::PathBuf;
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

use tokio::{fs::File, io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader}};
use std::fs;
use std::path::Path;
use tokio_util::io::ReaderStream;
use std::sync::Mutex;

use crate::system::real::RealSystem;
use crate::server::ServerError;
use crate::directory;
use crate::directory::Elements;

use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;

#[post("/upload")]
async fn upload(mut payload: Multipart) -> impl Responder
{
    println!("upload received");
    while let Ok(Some(mut field)) = payload.try_next().await
    {
        println!("field received");
        let content_disposition = field.content_disposition();
        match content_disposition.get_filename()
        {
            Ok(filename) =>
            {
                if filename.is_empty()
                {
                    println!("filename empty");
                }
                else
                {
                    println!("filename received");
                }
            },
            Err(_error) =>
            {
                eprintln!("get filename failed, ignoring");
            },
        }

        let filepath = "TEMPAPPLES.txt";

        let mut f = File::create(filepath).await.unwrap();
        while let Some(chunk) = field.next().await
        {
            println!("nested while");
            let data = chunk.unwrap();
            f.write_all(&data).await.unwrap();
        }
    }

    println!("upload 3");
    HttpResponse::Ok().body("File uploaded successfully")
}

#[get("/download/{filename}")]
async fn download(filename: web::Path<String>) -> impl Responder {
    let filename = filename.into_inner();
    let filepath = format!("./{}", filename);

    if Path::new(&filepath).exists() {
        let data = fs::read(filepath).unwrap();
        HttpResponse::Ok().body(data)
    } else {
        HttpResponse::NotFound().body("File not found")
    }
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

    let data = format!("WOW!  Text! {}", counter);
    HttpResponse::Ok().body(data)
}

#[get("/download-chunked/{filename:.*}")]
async fn chunked_download(path: web::Path<String>) -> impl Responder
{
    let filename = path.into_inner();
    let file_path = PathBuf::from("./").join(filename);

    if file_path.exists() {
        match File::open(&file_path).await {
            Ok(file) => HttpResponse::Ok().streaming(ReaderStream::new(file)),
            Err(_) => HttpResponse::InternalServerError().body("Could not read file"),
        }
    } else {
        HttpResponse::NotFound().body("File not found")
    }
}

#[delete("/{filename}")]
async fn delete(filename: web::Path<String>) -> impl Responder {
    let filename = filename.into_inner();
    let filepath = format!("./{}", filename);

    if Path::new(&filepath).exists() {
        fs::remove_file(filepath).unwrap();
        HttpResponse::Ok().body("File deleted successfully")
    } else {
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
            .service(download)
            .service(chunked_download)
            .service(delete)
            .service(home)
    });

    let socket_address = SocketAddr::new(IpAddr::V4(address), port);
    let server = match server.bind(socket_address)
    {
        Err(_) => {return Err(ServerError::Weird)},
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

