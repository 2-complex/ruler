use std::fmt;
use std::io::
{
    self,
    Read,
};

use crate::rule::
{
    parse_all,
    ParseError,
    Node,
    topological_sort,
    topological_sort_all,
    TopologicalSortError,
};
use crate::packet::Packet;
use crate::work::
{
    TargetFileInfo,
    WorkOption,
    WorkResult,
    WorkError,
    FileResolution,
    handle_node,
    clean_targets,
};

use crate::memory::{Memory, MemoryError};
use crate::cache::LocalCache;
use crate::printer::Printer;

use termcolor::
{
    Color,
};

use crate::system::
{
    System,
    SystemError
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


/*   */
pub fn serve
<
    SystemType : System + Clone + Send + 'static,
    PrinterType : Printer,
>
(
    mut system : SystemType,
    directory : &str,
    printer: &mut PrinterType,
)
-> Result<(), ServerError>
{
    println!("SERVING! or rather, pretending to serve");
    Err(ServerError::Weird) 
}

#[cfg(test)]
mod test
{
}
