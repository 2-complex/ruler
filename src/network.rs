use std::net::
{
    UdpSocket,
    Ipv4Addr,
    SocketAddrV4
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
use std::time::Duration;
use crate::ticket::
{
    Ticket,
    TicketFactory,
};
use crate::rule::
{
    Rule,
    parse_all,
    get_rule_for_one_target,
};
use crate::build::
{
    init_directory,
    read_all_rules,
    InitDirectoryError,
    BuildError
};
use std::fmt;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
enum ReceivePacket
{
    FileData(u32, usize, Vec<u8>),
    FileIndexCount(u32, usize),
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
enum SendPacket
{
    WantRule(u32, Ticket, Ticket), // index, rule-ticket, sources-ticket
}

pub enum NetworkError
{
    InitDirectoryError(InitDirectoryError),
    SocketBindFailed,
    SendFailed,
    BuildErrorReadingRules(BuildError),
    RulesError,
    RuleNotFound,
}

impl fmt::Display for NetworkError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            NetworkError::InitDirectoryError(error) =>
                write!(formatter, "Failed to init directory: {}", error),

            NetworkError::SocketBindFailed =>
                write!(formatter, "Socket bind failed"),

            NetworkError::SendFailed =>
                write!(formatter, "Send failed"),

            NetworkError::BuildErrorReadingRules(error) =>
                write!(formatter, "Error reading rules: {}", error),

            NetworkError::RulesError =>
                write!(formatter, "Some error with the rules"),

            NetworkError::RuleNotFound =>
                write!(formatter, "Rule not found"),
        }
    }
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
    let (mut memory, cache, _memoryfile) =
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

        let all_rule_text =
        match read_all_rules(&mut system, rulefile_paths)
        {
            Ok(rule_text) => rule_text,
            Err(_) =>
            {
                printer.print(&format!("Not all rules read"));
                return Err(NetworkError::RulesError);
            },
        };

        let rules =
        match parse_all(all_rule_text)
        {
            Ok(rules) => rules,
            Err(error) => 
            {
                printer.print(&format!("Not all rules parsed"));
                return Err(NetworkError::RulesError);
            }
        };

        loop
        {
            printer.print("Receiving requests");

            let mut buf = [0; 256];
            /*  Receives a single datagram message on the socket. If `buf` is too small to hold
                the message, it will be cut off. */
            match socket.recv_from(&mut buf)
            {
                Ok((amt, src)) =>
                {
                    let decoded_packet : SendPacket = bincode::deserialize(&buf[..amt]).unwrap();

                    match decoded_packet
                    {
                        SendPacket::WantRule(fetch_id, rule_ticket, sources_ticket) =>
                        {
                            printer.print(&format!("Want: rule: {} sources: {}", rule_ticket, sources_ticket));

                            let rule_history = memory.take_rule_history(&rule_ticket);

                            match rule_history.get_target_tickets(&sources_ticket)
                            {
                                Some(target_tickets) =>
                                {
                                    for target_ticket in target_tickets.iter()
                                    {
                                        match cache.open(&mut system, target_ticket)
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
                                                            let packet = ReceivePacket::FileData(fetch_id, i, in_buffer[..size].to_vec());
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

                                                let packet = ReceivePacket::FileIndexCount(fetch_id, i);
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

fn find_rule_ticket_with_target(rules : Vec<Rule>, target : &str)
-> Result<Ticket, ()>
{
    for rule in rules.iter()
    {
        for t in rule.targets.iter()
        {
            if target == t
            {
                return Ok(Ticket::from_strings(
                    &rule.targets,
                    &rule.sources,
                    &rule.command));
            }
        }
    }

    Err(())
}

pub fn download
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
    goal_target : &str
)
-> Result<(), NetworkError>
{
    let (mut memory, cache, _memoryfile) =
    match init_directory(&mut system, directory_path)
    {
        Ok((memory, cache, memoryfile)) => (memory, cache, memoryfile),
        Err(error) =>
        {
            return Err(NetworkError::InitDirectoryError(error));
        },
    };

    let socket =
    match UdpSocket::bind("127.0.0.1:34255")
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

    let all_rule_text =
    match read_all_rules(&mut system, rulefile_paths)
    {
        Ok(rule_text) => rule_text,
        Err(_) =>
        {
            printer.print(&format!("Not all rules read"));
            return Err(NetworkError::RulesError);
        },
    };

    let rules =
    match parse_all(all_rule_text)
    {
        Ok(rules) => rules,
        Err(error) => 
        {
            printer.print(&format!("Not all rules parsed"));
            return Err(NetworkError::RulesError);
        }
    };

    let mut rule =
    match get_rule_for_one_target(rules, &goal_target)
    {
        Ok(rule) => rule,
        Err(error) => return Err(NetworkError::RulesError),
    };

    let rule_ticket = Ticket::from_strings(
        &rule.targets,
        &rule.sources,
        &rule.command);

    let mut sources_factory = TicketFactory::new();
    for source in rule.sources
    {
        match TicketFactory::from_file(&system, &source)
        {
            Ok(mut file_factory) => sources_factory.input_ticket(file_factory.result()),
            Err(error) => return Err(NetworkError::RulesError),
        }
    }

    // TODO: get this address from config
    let addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 3456);

    let packet = SendPacket::WantRule(1, rule_ticket, sources_factory.result());
    let encoded = bincode::serialize(&packet).unwrap();
    match socket.send_to(&encoded, &addr)
    {
        Ok(_) =>
        {
            println!("packet sent");
        },
        Err(error) =>
        {
            return Err(NetworkError::SendFailed);
        },
    }
    socket.set_read_timeout(Some(Duration::from_millis(1000)));

    Ok(())
}

