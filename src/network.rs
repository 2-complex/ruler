use std::net::
{
    UdpSocket
};
use serde::
{
    Serialize,
    Deserialize
};
use crate::system::
{
    System
};
use crate::printer::Printer;
use std::io::Read;
use std::io::Write;
use std::time::Duration;
use crate::ticket::
{
    Ticket,
};
use crate::rule::
{
    Rule,
    parse_all,
};
use crate::build::
{
    init_directory,
    read_all_rules,
    InitDirectoryError,
    BuildError
};
use crate::memory::{Memory, MemoryError};


#[derive(Serialize, Deserialize, PartialEq, Debug)]
enum ReceivePacket
{
    FileData(String, usize, Vec<u8>),
    FileIndexCount(String, usize),
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
enum SendPacket
{
    WantRule(Ticket, Ticket),
}

pub enum NetworkError
{
    InitDirectoryError(InitDirectoryError),
    SocketBindFailed,
    SendFailed,
    BuildErrorReadingRules(BuildError),
}

pub fn serve
<
    SystemType : System + Clone + Send + 'static,
    PrinterType : Printer,
>
(
    mut system : SystemType,
    directory_path : &str,
    rulefile_paths : Vec<String>,
    printer : &mut PrinterType,
    address : &str,
)
->
Result<(), NetworkError>
{
    let (mut memory, _cache, _memoryfile) =
    match init_directory(&mut system, directory_path)
    {
        Ok((memory, cache, memoryfile)) => (memory, cache, memoryfile),
        Err(error) =>
        {
            return Err(NetworkError::InitDirectoryError(error));
        },
    };

    {
        let socket =
        match UdpSocket::bind(address)
        {
            Ok(socket) =>
            {
                socket
            },
            Err(_) =>
            {
                return Err(NetworkError::SocketBindFailed);
            },
        };

        /*  Receives a single datagram message on the socket. If `buf` is too small to hold
            the message, it will be cut off. */
        let mut buf = [0; 256];

        loop
        {
            printer.print("Receiving requests");

            match socket.recv_from(&mut buf)
            {
                Ok((amt, src)) =>
                {
                    let decoded_packet : SendPacket = bincode::deserialize(&buf[..amt]).unwrap();

                    match decoded_packet
                    {
                        SendPacket::WantRule(rule_ticket, sources_ticket) =>
                        {
                            printer.print(&format!("Want: rule: {} sources: {}", rule_ticket, sources_ticket));

                            let all_rule_text = read_all_rules(&system, rulefile_paths)?;
                            let rules =
                            match parse_all(all_rule_text)
                            {
                                Ok(rules) => rules,
                                Err(error) => return Err(BuildError::RuleFileFailedToParse(error)),
                            };

                            let rule_history =  match &rule_ticket
                            {
                                Some(ticket) => Some(memory.take_rule_history(ticket)),
                                None => None,
                            };

                            match rule_history.get_target_tickets(sources_ticket)
                            {
                                Some(target_tickets) =>
                                {
                                    for target_ticket in target_tickets.iter()
                                    {
                                        match system.open(&path_str)
                                        {
                                            Ok(mut reader) =>
                                            {
                                                printer.print("about to send the file back");
                                                let mut in_buffer = [0u8; 256];
                                                let mut i = 0usize;
                                                loop
                                                {
                                                    match reader.read(&mut in_buffer)
                                                    {
                                                        Ok(0) =>
                                                        {
                                                            break;
                                                        }
                                                        Ok(size) =>
                                                        {
                                                            let packet = ReceivePacket::FileData(path_str.clone(), i, in_buffer[..size].to_vec());
                                                            i+=1;
                                                            let encoded = bincode::serialize(&packet).unwrap();
                                                            match socket.send_to(&encoded, &src)
                                                            {
                                                                Ok(_) => {},
                                                                Err(_) => return Err(NetworkError::SendFailed),
                                                            }
                                                        },
                                                        Err(error) =>
                                                        {
                                                            printer.print(&format!("file io error: {}", error));
                                                        },
                                                    }
                                                }

                                                let packet = ReceivePacket::FileIndexCount(path_str.clone(), i);
                                                let encoded = bincode::serialize(&packet).unwrap();
                                                match socket.send_to(&encoded, &src)
                                                {
                                                    Ok(_) => {},
                                                    Err(_) => return Err(NetworkError::SendFailed),
                                                }
                                            },
                                            Err(error) =>
                                            {
                                                printer.print(&format!("File read error: {}", error));
                                            }
                                        }
                                    }
                                },
                                None => 
                                {
                                    printer.print(&format!("Not found"));
                                    continue;
                                }
                            }
                        },
                    }
                },

                Err(_) =>
                {
                    printer.print(&format!("breaking"));
                    break;
                }
            }
        }
    }
    Ok(())
}

pub fn download()
{
    println!("download\n");
}

