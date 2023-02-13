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
/*
use crate::ticket::
{
    Ticket,
    // From64Error,
};
*/
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
    let (mut _memory, cache, _memoryfile) =
    match directory::init(&mut system, "ruler-directory")
    {
        Ok((memory, cache, memoryfile)) => (memory, cache, memoryfile),
        Err(error) => panic!("Failed to init directory error: {}", error)
    };

    // GET /hello/warp => 200 OK with body "Hello, warp!"
    let hello = warp::path!("hello" / String)
        .map(|name| format!("Hello, {}!", name));

    warp::serve(hello)
        .run(([127, 0, 0, 1], 3030))
        .await;

    Err(ServerError::Weird) 
}

#[cfg(test)]
mod test
{

}
