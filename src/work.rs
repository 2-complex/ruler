extern crate filesystem;
extern crate chrono;

use chrono::offset::Utc;
use chrono::DateTime;

use crate::packet::Packet;
use crate::ticket::TicketFactory;
use crate::rule::Record;
use crate::station::Station;
use crate::executor::{CommandLineOutput, Executor};

use filesystem::FileSystem;
use std::process::Command;
use std::sync::mpsc::{Sender, Receiver};
use std::collections::VecDeque;
use std::fs;
use std::time::SystemTime;

pub struct OsExecutor
{
}

impl OsExecutor
{
    pub fn new() -> OsExecutor
    {
        OsExecutor{}
    }
}

impl Executor for OsExecutor
{
    fn execute_command(&self, command_list: Vec<String>) -> Result<CommandLineOutput, String>
    {
        let mut command_queue = VecDeque::from(command_list.clone());
        let command_opt = match command_queue.pop_front()
        {
            Some(first) =>
            {
                let mut command = Command::new(first);
                while let Some(argument) = command_queue.pop_front()
                {
                    command.arg(argument);
                }
                Some(command)
            },
            None => None
        };

        match command_opt
        {
            Some(mut command) =>
            {
                match command.output()
                {
                    Ok(out) => Ok(CommandLineOutput::from_output(out)),
                    Err(why) => Err(why.to_string()),
                }
            },
            None => Ok(CommandLineOutput::new()),
        }
    }
}

pub trait MetadataGetter
{
    fn get_modified(&self, path: &str) -> Result<SystemTime, String>;
}

pub struct OsMetadataGetter
{
}

impl OsMetadataGetter
{
    pub fn new() -> OsMetadataGetter
    {
        OsMetadataGetter
        {
        }
    }
}

impl MetadataGetter for OsMetadataGetter
{
    fn get_modified(&self, path: &str) -> Result<SystemTime, String>
    {
        match fs::metadata(path)
        {
            Ok(metadata) =>
            {
                match metadata.modified()
                {
                    Ok(timestamp) => Ok(timestamp),
                    Err(_) => Err(format!("Could not get modified date for file: {}", path))
                }
            },
            Err(_) => Err(format!("Could not get metadata for file: {}", path))
        }
    }
}


pub fn do_command<FSType: FileSystem, ExecType: Executor, MetadataGetterType: MetadataGetter>(
    record : Record,
    senders : Vec<(usize, Sender<Packet>)>,
    receivers : Vec<Receiver<Packet>>,
    station : Station<FSType>,
    executor : ExecType,
    metadata_getter: MetadataGetterType)
    -> Result<CommandLineOutput, String>
{
    let mut factory = TicketFactory::new();

    for rcv in receivers
    {
        match rcv.recv()
        {
            Ok(packet) => 
            {
                match packet.get_ticket()
                {
                    Ok(ticket) => factory.input_ticket(ticket),
                    Err(error) => return Err(format!("Received error from source rule: {}", error)),
                }
            },
            Err(why) => return Err(format!("ERROR {}", why)),
        }
    }

    let mut target_tickets = Vec::new();
    for target_path in record.targets.iter()
    {
        match station.get_file_ticket(target_path)
        {
            Ok(ticket) =>
            {
                target_tickets.push(ticket);
            },
            Err(why) => return Err(format!("TICKET ALIGNMENT ERROR {}", why)),
        }
    }

    let remembered_target_tickets = station.remember_target_tickets(&factory.result());

    let result = if target_tickets != remembered_target_tickets
    {
        match executor.execute_command(record.command)
        {
            Ok(command_result) => Ok(command_result),
            Err(why) =>
            {
                return Err(format!("Error in command to build: {} {}", record.targets.join(" "), why))
            },
        }
    }
    else
    {
        Ok(CommandLineOutput::new())
    };

    for target_path in record.targets.iter()
    {
        match metadata_getter.get_modified(&target_path)
        {
            Ok(timestamp) =>
            {
                let datetime: DateTime<Utc> = timestamp.into();
                println!("{} : {}", target_path, datetime.format("%d/%m/%Y %T"));
            }
            Err(_) => {},
        }

        if ! station.is_file(&target_path)
        {
            return Err(format!("File not found: {}", target_path))
        }
    }


    for (sub_index, sender) in senders
    {
        // Someone should cache the value of the ticket as an optimization,
        // it could be here, or it could be in station, but someone..
        match station.get_file_ticket(&record.targets[sub_index])
        {
            Ok(ticket) =>
            {
                match sender.send(Packet::from_ticket(ticket))
                {
                    Ok(_) => {},
                    Err(_error) => eprintln!("CHANNEL SEND ERROR"),
                }
            },
            Err(error) => return Err(format!("FILE IO ERROR {}", error)),
        }
    }

    result
}

#[cfg(test)]
mod test
{
    use crate::rule::Record;
    use crate::work::{Station, do_command};
    use crate::ticket::TicketFactory;
    use crate::memory::RuleHistory;
    use crate::executor::{Executor, CommandLineOutput};
    use crate::packet::Packet;
    use crate::work::MetadataGetter;

    use filesystem::{FileSystem, FakeFileSystem};
    use std::path::Path;
    use std::sync::mpsc::{self, Sender, Receiver};
    use std::str::from_utf8;
    use std::thread::{self, JoinHandle};
    use std::collections::HashMap;
    use std::time::SystemTime;

    struct FakeExecutor
    {
        file_system: FakeFileSystem
    }

    impl FakeExecutor
    {
        fn new(file_system: FakeFileSystem) -> FakeExecutor
        {
            FakeExecutor
            {
                file_system: file_system
            }
        }
    }

    impl Executor for FakeExecutor
    {
        fn execute_command(&self, command_list : Vec<String>) -> Result<CommandLineOutput, String>
        {
            let n = command_list.len();
            let mut output = String::new();

            if n > 1
            {
                match command_list[0].as_str()
                {
                    "mycat" =>
                    {
                        for file in command_list[1..(n-1)].iter()
                        {
                            match self.file_system.read_file(file)
                            {
                                Ok(content) =>
                                {
                                    match from_utf8(&content)
                                    {
                                        Ok(content_string) =>
                                        {
                                            output.push_str(content_string);
                                        }
                                        Err(_) => return Err(format!("File contained non utf8 bytes: {}", file)),
                                    }
                                }
                                Err(_) =>
                                {
                                    return Err(format!("File failed to open: {}", file));
                                }
                            }
                        }

                        match self.file_system.write_file(Path::new(&command_list[n-1]), output)
                        {
                            Ok(_) => Ok(CommandLineOutput::new()),
                            Err(why) =>
                            {
                                Err(format!("Filed to cat into file: {}: {}", command_list[n-1], why))
                            }
                        }
                    },
                    _=> Err(format!("Non command given: {}", command_list[0]))
                }
            }
            else
            {
                Ok(CommandLineOutput::new())
            }
        }
    }

    struct FakeMetadataGetter
    {
        path_to_time: HashMap<String, SystemTime>,
    }

    impl FakeMetadataGetter
    {
        fn new() -> FakeMetadataGetter
        {
            FakeMetadataGetter
            {
                path_to_time: HashMap::new(),
            }
        }

        fn insert(&mut self, path: &str, time: SystemTime)
        {
            self.path_to_time.insert(path.to_string(), time);
        }
    }

    impl MetadataGetter for FakeMetadataGetter
    {
        fn get_modified(&self, path: &str) -> Result<SystemTime, String>
        {
            match self.path_to_time.get(path)
            {
                Some(time) => Ok(*time),
                None => Err(format!("Couldn't get modified date for {}", path)),
            }
        }
    }

    #[test]
    fn do_empty_command()
    {
        let file_system = FakeFileSystem::new();
        match file_system.write_file("A", "A-content")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match do_command(
            Record
            {
                targets: vec!["A".to_string()],
                source_indices: vec![],
                ticket: TicketFactory::new().result(),
                command: vec!["noop".to_string()],
            },
            Vec::new(),
            Vec::new(),
            Station::new(file_system.clone(), RuleHistory::new()),
            FakeExecutor::new(file_system.clone()),
            FakeMetadataGetter::new())
        {
            Ok(result) =>
            {
                assert_eq!(result.out, "");
                assert_eq!(result.err, "");
                assert_eq!(result.code, Some(0));
                assert_eq!(result.success, true);
            },
            Err(why) => panic!("Command failed: {}", why),
        }
    }

    #[test]
    fn wait_for_channels()
    {
        let (sender_a, receiver_a) = mpsc::channel();
        let (sender_b, receiver_b) = mpsc::channel();
        let (sender_c, receiver_c) = mpsc::channel();

        let file_system = FakeFileSystem::new();

        match file_system.write_file(Path::new(&"A.txt"), "")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match sender_a.send(Packet::from_ticket(TicketFactory::from_str("apples").result()))
        {
            Ok(p) => assert_eq!(p, ()),
            Err(e) => panic!("Unexpected error sending: {}", e),
        }

        match sender_b.send(Packet::from_ticket(TicketFactory::from_str("bananas").result()))
        {
            Ok(p) => assert_eq!(p, ()),
            Err(e) => panic!("Unexpected error sending: {}", e),
        }

        match do_command(
            Record
            {
                targets: vec!["A.txt".to_string()],
                source_indices: vec![],
                ticket: TicketFactory::new().result(),
                command: vec!["noop".to_string()],
            },
            vec![(0, sender_c)],
            vec![receiver_a, receiver_b],
            Station::new(file_system.clone(), RuleHistory::new()),
            FakeExecutor::new(file_system.clone()),
            FakeMetadataGetter::new())
        {
            Ok(result) =>
            {
                assert_eq!(result.out, "");
                assert_eq!(result.err, "");
                assert_eq!(result.code, Some(0));
                assert_eq!(result.success, true);

                match receiver_c.recv()
                {
                    Ok(packet) =>
                    {
                        match packet.get_ticket()
                        {
                            Ok(ticket) => assert_eq!(ticket, TicketFactory::new().result()),
                            Err(why) => panic!("Unexpected error doing command: {}", why),
                        }
                    }
                    Err(_) => panic!("Unexpected fail to receive"),
                }
            },
            Err(err) => panic!("Command failed: {}", err),
        }
    }

    #[test]
    fn poem_concatination()
    {
        let (sender_a, receiver_a) = mpsc::channel();
        let (sender_b, receiver_b) = mpsc::channel();
        let (sender_c, receiver_c) = mpsc::channel();

        let file_system = FakeFileSystem::new();

        match file_system.write_file(Path::new(&"verse1.txt"), "Roses are red\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match file_system.write_file(Path::new(&"verse2.txt"), "Violets are violet\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match sender_a.send(Packet::from_ticket(TicketFactory::from_str("Roses are red\n").result()))
        {
            Ok(p) => assert_eq!(p, ()),
            Err(e) => panic!("Unexpected error sending: {}", e),
        }

        match sender_b.send(Packet::from_ticket(TicketFactory::from_str("Violets are violet\n").result()))
        {
            Ok(p) => assert_eq!(p, ()),
            Err(e) => panic!("Unexpected error sending: {}", e),
        }

        match do_command(
            Record
            {
                targets: vec!["poem.txt".to_string()],
                source_indices: vec![],
                ticket: TicketFactory::new().result(),
                command: vec![
                    "mycat".to_string(),
                    "verse1.txt".to_string(),
                    "verse2.txt".to_string(),
                    "poem.txt".to_string()],
            },
            vec![(0, sender_c)],
            vec![receiver_a, receiver_b],
            Station::new(file_system.clone(), RuleHistory::new()),
            FakeExecutor::new(file_system.clone()),
            FakeMetadataGetter::new())
        {
            Ok(result) =>
            {
                assert_eq!(result.out, "");
                assert_eq!(result.err, "");
                assert_eq!(result.code, Some(0));
                assert_eq!(result.success, true);

                match receiver_c.recv()
                {
                    Ok(packet) =>
                    {
                        match packet.get_ticket()
                        {
                            Ok(ticket) =>
                            {
                                assert_eq!(
                                    ticket,
                                    TicketFactory::from_str("Roses are red\nViolets are violet\n").result());
                            },
                            Err(_) => panic!("Failed to receive ticket"),
                        }
                    }
                    Err(_) => panic!("Unexpected fail to receive"),
                }
            },
            Err(err) => panic!("Command failed: {}", err),
        }
    }

    #[test]
    fn poem_already_correct()
    {
        let (sender_a, receiver_a) = mpsc::channel();
        let (sender_b, receiver_b) = mpsc::channel();
        let (sender_c, receiver_c) = mpsc::channel();

        let mut rule_history = RuleHistory::new();

        let mut factory = TicketFactory::new();
        factory.input_ticket(TicketFactory::from_str("Roses are red\n").result());
        factory.input_ticket(TicketFactory::from_str("Violets are violet\n").result());

        rule_history.insert(
            factory.result(),
            vec![
                TicketFactory::from_str("Roses are red\nViolets are violet\n").result()
            ]
        );

        let file_system = FakeFileSystem::new();

        match file_system.write_file(Path::new(&"verse1.txt"), "Roses are red\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match file_system.write_file(Path::new(&"verse2.txt"), "Violets are violet\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match file_system.write_file(Path::new(&"poem.txt"), "Roses are red\nViolets are violet\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match sender_a.send(Packet::from_ticket(TicketFactory::from_str("Roses are red\n").result()))
        {
            Ok(p) => assert_eq!(p, ()),
            Err(e) => panic!("Unexpected error sending: {}", e),
        }

        match sender_b.send(Packet::from_ticket(TicketFactory::from_str("Violets are violet\n").result()))
        {
            Ok(p) => assert_eq!(p, ()),
            Err(e) => panic!("Unexpected error sending: {}", e),
        }

        match do_command(
            Record
            {
                targets: vec!["poem.txt".to_string()],
                source_indices: vec![],
                ticket: TicketFactory::new().result(),
                command: vec![
                    "error".to_string(),
                    "poem is already correct".to_string(),
                    "this command should not run".to_string(),
                    "the end".to_string()],
            },
            vec![(0, sender_c)],
            vec![receiver_a, receiver_b],
            Station::new(file_system.clone(), rule_history),
            FakeExecutor::new(file_system.clone()),
            FakeMetadataGetter::new())
        {
            Ok(result) =>
            {
                assert_eq!(result.out, "");
                assert_eq!(result.err, "");
                assert_eq!(result.code, Some(0));
                assert_eq!(result.success, true);

                match receiver_c.recv()
                {
                    Ok(packet) =>
                    {
                        match packet.get_ticket()
                        {
                            Ok(ticket) =>
                            {
                                assert_eq!(
                                    ticket,
                                    TicketFactory::from_str("Roses are red\nViolets are violet\n").result());
                            },
                            Err(message) =>
                            {
                                panic!("Failed to receive ticket: {}", message);
                            },
                        }
                    }
                    Err(_) => panic!("Unexpected fail to receive"),
                }
            },
            Err(err) => panic!("Command failed: {}", err),
        }
    }

    #[test]
    fn file_not_there()
    {
        let file_system = FakeFileSystem::new();

        match file_system.write_file(Path::new(&"some-other-file.txt"), "Arbitrary content\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match do_command(
            Record
            {
                targets: vec!["verse1.txt".to_string()],
                source_indices: vec![],
                ticket: TicketFactory::new().result(),
                command: vec![],
            },
            vec![],
            vec![],
            Station::new(file_system.clone(), RuleHistory::new()),
            FakeExecutor::new(file_system.clone()),
            FakeMetadataGetter::new())
        {
            Ok(_) =>
            {
                panic!("Expected failure when file not present")
            },
            Err(err) =>
            {
                assert_eq!(err, "File not found: verse1.txt");
            },
        }
    }

    fn spawn_command<FSType: FileSystem + Send + 'static>(
        record: Record,
        senders : Vec<(usize, Sender<Packet>)>,
        receivers : Vec<Receiver<Packet>>,
        station : Station<FSType>,
        executor: FakeExecutor,
        metadata_getter: FakeMetadataGetter)
        -> JoinHandle<Result<CommandLineOutput, String>>
    {
        thread::spawn(
            move || -> Result<CommandLineOutput, String>
            {
                do_command(
                    record,
                    senders,
                    receivers,
                    station,
                    executor,
                    metadata_getter)
            }
        )
    }

    #[test]
    fn one_dependence()
    {
        let file_system = FakeFileSystem::new();

        match file_system.write_file(Path::new(&"verse1.txt"), "I wish I were a windowsill")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        let (sender, receiver) = mpsc::channel();

        let handle1 = spawn_command(
            Record
            {
                targets: vec!["stanza1.txt".to_string()],
                source_indices: vec![],
                ticket: TicketFactory::new().result(),
                command: vec![
                    "mycat".to_string(),
                    "verse1.txt".to_string(),
                    "stanza1.txt".to_string()
                ],
            },
            vec![(0, sender)],
            vec![],
            Station::new(file_system.clone(), RuleHistory::new()),
            FakeExecutor::new(file_system.clone()),
            FakeMetadataGetter::new()
        );

        let handle2 = spawn_command(
            Record
            {
                targets: vec!["poem.txt".to_string()],
                source_indices: vec![],
                ticket: TicketFactory::new().result(),
                command: vec![
                    "mycat".to_string(),
                    "stanza1.txt".to_string(),
                    "poem.txt".to_string()
                ],
            },
            vec![],
            vec![receiver],
            Station::new(file_system.clone(), RuleHistory::new()),
            FakeExecutor::new(file_system.clone()),
            FakeMetadataGetter::new()
        );

        match handle1.join()
        {
            Ok(_) =>
            {
                assert_eq!(from_utf8(&file_system.read_file("verse1.txt").unwrap()).unwrap(), "I wish I were a windowsill");
            },
            Err(_) => panic!("First thread failed"),
        }

        match handle2.join()
        {
            Ok(_) =>
            {
                assert_eq!(from_utf8(&file_system.read_file("verse1.txt").unwrap()).unwrap(), "I wish I were a windowsill");
            },
            Err(_) => panic!("Second thread failed"),
        }
    }


    #[test]
    fn one_dependence_with_error()
    {
        let file_system = FakeFileSystem::new();

        match file_system.write_file(Path::new(&"some-other-file.txt"), "Arbitrary content\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match file_system.write_file(Path::new(&"stanza1.txt"), "Arbitrary content\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        let (sender, receiver) = mpsc::channel();

        let handle1 = spawn_command(
            Record
            {
                targets: vec!["stanza1.txt".to_string()],
                source_indices: vec![],
                ticket: TicketFactory::new().result(),
                command: vec![
                    "mycat".to_string(),
                    "verse1.txt".to_string(),
                    "stanza1.txt".to_string()
                ],
            },
            vec![(0, sender)],
            vec![],
            Station::new(file_system.clone(), RuleHistory::new()),
            FakeExecutor::new(file_system.clone()),
            FakeMetadataGetter::new()
        );

        let handle2 = spawn_command(
            Record
            {
                targets: vec!["poem.txt".to_string()],
                source_indices: vec![],
                ticket: TicketFactory::new().result(),
                command: vec![
                    "mycat".to_string(),
                    "stanza1.txt".to_string(),
                    "poem.txt".to_string()
                ],
            },
            vec![],
            vec![receiver],
            Station::new(file_system.clone(), RuleHistory::new()),
            FakeExecutor::new(file_system.clone()),
            FakeMetadataGetter::new()
        );

        match handle1.join()
        {
            Ok(thread_result) =>
            {
                match thread_result
                {
                    Ok(_) => panic!("First thread failed to error"),
                    Err(_) => {},
                }
            }
            Err(_) => panic!("First thread join failed"),
        }

        match handle2.join()
        {
            Ok(thread_result) =>
            {
                match thread_result
                {
                    Ok(_) => panic!("Second thread failed to error"),
                    Err(_) => {},
                }
            },
            Err(_) => panic!("Second thread join failed"),
        }
    }
}
