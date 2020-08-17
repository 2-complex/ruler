use crate::system::real::RealSystem;
use crate::system::
{
    System,
    SystemError,
    // ReadWriteError
};
// use crate::ticket::Ticket;
// use regex::Regex;
use std::net::TcpStream;
use std::net::TcpListener;
use std::io::
{
    Read,
    Write,
};
use base64::
{
    DecodeError
};
use std::fmt;
use crate::directory::
{
    init_directory,
    // InitDirectoryError,
};
use std::cmp::min;
use crate::cache::LocalCache;

enum HandleConnectionError
{
    Base64DecodeError(DecodeError),
    ConvertIndexError,
}

impl fmt::Display for HandleConnectionError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            HandleConnectionError::Base64DecodeError(error) =>
                write!(formatter, "Base64 decode failed with error: {}", error),

            HandleConnectionError::ConvertIndexError =>
                write!(formatter, "Could not convert string to target-index"),
        }
    }
}

pub fn serve<SystemType : System + Clone + Send + 'static>(
    system : &mut SystemType,
    directory : &str,
    _rulefile_path: &str,
)
{
    let (mut _memory, cache, _memoryfile) =
    match init_directory(system, directory)
    {
        Ok((memory, cache, memoryfile)) => (memory, cache, memoryfile),
        Err(error) =>
        {
            panic!("serve getting memory error");
        }
    };

    let listener = TcpListener::bind("0.0.0.0:7979").unwrap();
    let system = RealSystem::new();
    for stream in listener.incoming()
    {
        let stream = stream.unwrap();
        match handle_connection(&system, &cache, stream)
        {
            Ok(_) => println!("request handled"),
            Err(error) => println!("Handle connection oops, error: {}\n", error),
        }
    }
}

fn handle_download_ticket<SystemType: System>(
    system : &SystemType,
    cache : &LocalCache,
    base64_ticket : &str,
    stream: &mut TcpStream)
{
    stream.write("\n".as_bytes());
}

enum HandleLineError
{
    InvalidInvocation,
    NotEnoughArguments,
}

fn handle_line<SystemType: System>(
    system : &SystemType,
    cache : &LocalCache,
    line_as_bytes : Vec<u8>,
    stream: &mut TcpStream
)
-> Result<(), HandleLineError>
{
    let line_as_str = std::str::from_utf8(&line_as_bytes).unwrap();

    let mut components = line_as_str.split_whitespace();
    match components.next()
    {
        Some("download") =>
        {
            let base64_ticket =
            match components.next()
            {
                Some(s) => s,
                None => return Err(HandleLineError::NotEnoughArguments)
            };

            handle_download_ticket(system, cache, base64_ticket, stream);
            Ok(())
        },

        _ => return Err(HandleLineError::InvalidInvocation),
    }
}

fn handle_connection<SystemType: System>(
    system : &SystemType,
    cache : &LocalCache,
    mut stream: TcpStream)
-> Result<(), HandleConnectionError>
{
    let mut next_line = vec![];
    let mut buffer = [0u8; 256];

    loop
    {
        match stream.read(&mut buffer)
        {
            Ok(0)=>
            {
                break;
            },
            Ok(ln) =>
            {
                let bvec = buffer[0..ln].to_vec();
                for b in bvec
                {
                    if b == 0x0a
                    {
                        handle_line(system, cache, next_line, &mut stream);
                        next_line = vec![];
                    }
                    else
                    {
                        next_line.push(b);
                    }
                }
            },
            Err(_) => panic!("oof"),
        }
    }

    Ok(())
}

const MAX_DOWNLOAD_FILE_SIZE : usize = 1000000000;

enum ReadSizeError
{
    NoFile,
    TooBig,
    NonDigitInSize,
    StreamError(std::io::Error)
}

fn read_size(stream : &mut TcpStream) -> Result<usize, ReadSizeError>
{
    let mut size_opt = None;
    let mut buffer = [0u8; 1];
    loop
    {
        match stream.read(&mut buffer)
        {
            Ok(ln) =>
            {
                let bvec = buffer[0..ln].to_vec();
                for b in bvec // There's only one
                {
                    println!("b = {}", b);

                    if b == 0x0a
                    {
                        return match size_opt
                        {
                            Some(size) => Ok(size),
                            None => Err(ReadSizeError::NoFile),
                        };
                    }
                    else
                    {
                        size_opt = Some(
                            match size_opt
                            {
                                None => ((b-0x30) as usize),
                                Some(size) =>
                                {
                                    if b < 0x30 || b > 0x39
                                    {
                                        return Err(ReadSizeError::NonDigitInSize);
                                    }

                                    if size > MAX_DOWNLOAD_FILE_SIZE / 10 - ((b-0x30) as usize)
                                    {
                                        return Err(ReadSizeError::TooBig);
                                    }

                                    10 * size + ((b-0x30) as usize)
                                }
                            }
                        );
                    }
                }
            },
            Err(error) => return Err(ReadSizeError::StreamError(error)),
        }
    }
}

pub enum DownloadError
{
    NoFile,
    TooBig,
    StreamDry,
    NonDigitInSize,
    StreamError(std::io::Error),
    SystemError(SystemError),
}


pub fn download<SystemType: System>(
    system : &mut SystemType,
    base64_ticket : String,
    download_path : String
)
-> Result<(), DownloadError>
{
    let mut stream =
    match TcpStream::connect("127.0.0.1:7979")
    {
        Ok(stream) => stream,
        Err(_) => panic!("stream didn't open"),
    };

    match stream.write(format!("download {}\n", base64_ticket).as_bytes())
    {
        Ok(_) => println!("request made\n"),
        Err(_) => panic!("stream didn't write to"),
    }

    stream.flush();

    let mut buffer = [0u8; 256];

    match read_size(&mut stream)
    {
        Ok(mut size) =>
        {
            println!("expecting {} bytes", size);

            match system.create_file(&download_path)
            {
                Ok(mut file) =>
                {
                    let mut buf = vec![];

                    while size > 0
                    {
                        buf.resize(min(size, 256usize), 0u8);
                        match stream.read(&mut buf)
                        {
                            Ok(0) => return Err(DownloadError::StreamDry),
                            Ok(n) =>
                            {
                                println!("got {} bytes : {}", n, std::str::from_utf8(&buf[0..n]).unwrap());

                                size -= n;
                                file.write(&buf[0..n]);

                                println!("size = {}", size);
                            },
                            Err(error) => return Err(DownloadError::StreamError(error)),
                        }
                    }

                    Ok(())
                }

                Err(error) => Err(DownloadError::SystemError(error)),
            }
        },

        Err(ReadSizeError::NoFile) => return Err(DownloadError::NoFile),
        Err(ReadSizeError::TooBig) => return Err(DownloadError::TooBig),
        Err(ReadSizeError::NonDigitInSize) => return Err(DownloadError::NonDigitInSize),
        Err(ReadSizeError::StreamError(error)) => return Err(DownloadError::StreamError(error)),
    }
}

