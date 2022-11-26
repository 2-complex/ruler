use std::fmt;

/*
use std::io::
{
    self,
    Read,
};
*/

// use crate::memory::{Memory, MemoryError};
// use crate::cache::LocalCache;
use crate::printer::Printer;
use crate::directory;
use crate::ticket::
{
    Ticket,
    // From64Error,
};
// use termcolor::Color;

use actix_web::
{
    web,
    App,
    HttpServer,
    HttpResponse,
//    Responder,
    rt,
    http::StatusCode,
};

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

/*  Creates an HTTP server */
pub fn serve
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
    let (mut _memory, cache, _memoryfile) =
    match directory::init(&mut system, "ruler-directory")
    {
        Ok((memory, cache, memoryfile)) => (memory, cache, memoryfile),
        Err(error) => panic!("Failed to init directory error: {}", error)
    };

    let mut runtime = rt::Runtime::new().unwrap();
    runtime.block_on(async {
        HttpServer::new(|| {
            App::new()
                .route(
                    "/files/{hash}",
                    web::get().to(
                        |h: web::Path<String>| async move
                        {
                            let status_ok = StatusCode::from_u16(200).unwrap();

                            match Ticket::from_base64(&h)
                            {
                                Ok(ticket) =>
                                {
                                    match cache.open(&mut system, &ticket)
                                    {
                                        Ok(file) =>
                                        {
                                            HttpResponse::new(status_ok).set_body("stuff".to_string())
                                        },
                                        Err(err) => 
                                        {
                                            HttpResponse::new(status_ok).set_body("ERROR".to_string())
                                        }
                                    }
                                },

                                Err(_) =>
                                {
                                    HttpResponse::new(status_ok).set_body("ERROR".to_string())
                                },
                            }
                        }
                    ))})

/*
                    web::get().to(
                        |hash: web::Path<String>| async move
                        {
                            let status_ok = StatusCode::from_u16(200).unwrap();
                            HttpResponse::new(status_ok).set_body("something that should be the file");

                            let status_ok = StatusCode::from_u16(200).unwrap();
                            let status_not_found = StatusCode::from_u16(404).unwrap();
                            match Ticket::from_base64(&hash)
                            {
                                Ok(ticket) =>
                                {
                                    match cache.open(&mut system, &ticket)
                                    {
                                        Ok(file) =>
                                            HttpResponse::new(status_ok).set_body("something that should be the file"),
                                        Err(error) =>
                                            HttpResponse::new(status_not_found).set_body(""),
                                    }
                                },

                                Err(error) =>
                                {
                                    println!("{}", error);
                                    HttpResponse::new(status_not_found).set_body("")
                                },
                            }
                        }
                    )
                )
        })
                            */
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
    });

    Err(ServerError::Weird) 
}

#[cfg(test)]
mod test
{

}
