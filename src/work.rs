extern crate filesystem;

use crate::packet::Packet;
use crate::ticket::TicketFactory;
use crate::station::{Station, get_file_ticket};
use crate::executor::{CommandLineOutput, Executor};
use crate::metadata::MetadataGetter;

use filesystem::FileSystem;
use std::process::Command;
use std::sync::mpsc::{Sender, Receiver};
use std::collections::VecDeque;

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

pub fn do_command<FileSystemType: FileSystem, ExecType: Executor, MetadataGetterType: MetadataGetter>(
    station : Station<FileSystemType, MetadataGetterType>,
    senders : Vec<(usize, Sender<Packet>)>,
    receivers : Vec<Receiver<Packet>>,
    executor : ExecType)
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
    for target_info in station.target_infos.iter()
    {
        match get_file_ticket(
            &station.file_system,
            &station.metadata_getter,
            target_info)
        {
            Ok(ticket) =>
            {
                target_tickets.push(ticket);
            },
            Err(why) => return Err(format!("TICKET ALIGNMENT ERROR {}", why)),
        }
    }

    let remembered_target_tickets = station.rule_history.remember_target_tickets(&factory.result());

    let result = if target_tickets != remembered_target_tickets
    {
        match executor.execute_command(station.command)
        {
            Ok(command_result) => Ok(command_result),
            Err(why) =>
            {
                return Err(format!("Error executing command: {}", why))
            },
        }
    }
    else
    {
        Ok(CommandLineOutput::new())
    };

    // Make sure all the targets exist at this point.
    for target_info in station.target_infos.iter()
    {
        if !station.file_system.is_file(&target_info.path) && !station.file_system.is_dir(&target_info.path)
        {
            return Err(format!("File not found: {}", target_info.path));
        }
    }

    for (sub_index, sender) in senders
    {
        // Potential optimization: avoid calling get_file_ticket twice if
        // the command didn't run.
        match get_file_ticket(
            &station.file_system,
            &station.metadata_getter,
            &station.target_infos[sub_index])
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
    use crate::station::{Station, TargetFileInfo};
    use crate::work::do_command;
    use crate::ticket::TicketFactory;
    use crate::memory::{RuleHistory, TargetHistory};
    use crate::executor::{Executor, CommandLineOutput};
    use crate::packet::Packet;
    use crate::metadata::{MetadataGetter, FakeMetadataGetter};

    use filesystem::{FileSystem, FakeFileSystem};
    use std::path::Path;
    use std::sync::mpsc::{self, Sender, Receiver};
    use std::str::from_utf8;
    use std::thread::{self, JoinHandle};
 
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

                    "rm" =>
                    {
                        for file in command_list[1..n].iter()
                        {
                            match self.file_system.remove_file(file)
                            {
                                Ok(()) => {}
                                Err(_) =>
                                {
                                    return Err(format!("File failed to delete: {}", file));
                                }
                            }
                        }

                        Ok(CommandLineOutput::new())
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

    fn to_info(mut targets : Vec<String>) -> Vec<TargetFileInfo>
    {
        let mut result = Vec::new();

        for target_path in targets.drain(..)
        {
            result.push(
                TargetFileInfo
                {
                    path : target_path,
                    history : TargetHistory::new(
                        TicketFactory::new().result(),
                        0,
                    ),
                }
            );
        }

        result
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
            Station::new(
                to_info(vec!["A".to_string()]),
                vec!["noop".to_string()],
                RuleHistory::new(),
                file_system.clone(),
                FakeMetadataGetter::new()),
            Vec::new(),
            Vec::new(),
            FakeExecutor::new(file_system.clone()))
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
            Station::new(
                to_info(vec!["A.txt".to_string()]),
                vec!["noop".to_string()],
                RuleHistory::new(),
                file_system.clone(),
                FakeMetadataGetter::new()),
            vec![(0, sender_c)],
            vec![receiver_a, receiver_b],
            FakeExecutor::new(file_system.clone()))
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
            Station::new(
                to_info(vec!["poem.txt".to_string()]),
                vec![
                    "mycat".to_string(),
                    "verse1.txt".to_string(),
                    "verse2.txt".to_string(),
                    "poem.txt".to_string()
                ],
                RuleHistory::new(),
                file_system.clone(),
                FakeMetadataGetter::new()),
            vec![(0, sender_c)],
            vec![receiver_a, receiver_b],
            FakeExecutor::new(file_system.clone()))
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
            Station::new(
                to_info(vec!["poem.txt".to_string()]),
                vec![
                    "error".to_string(),
                    "poem is already correct".to_string(),
                    "this command should not run".to_string(),
                    "the end".to_string()],
                rule_history,
                file_system.clone(),
                FakeMetadataGetter::new(),
            ),
            vec![(0, sender_c)],
            vec![receiver_a, receiver_b],
            FakeExecutor::new(file_system.clone()))
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
            Station::new(
                to_info(vec!["verse1.txt".to_string()]),
                vec![],
                RuleHistory::new(),
                file_system.clone(),
                FakeMetadataGetter::new()
            ),
            vec![],
            vec![],
            FakeExecutor::new(file_system.clone()))
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

    #[test]
    fn target_removed_by_command()
    {
        let file_system = FakeFileSystem::new();

        match file_system.write_file(Path::new(&"verse1.txt"), "Arbitrary content\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match do_command(
            Station::new(
                to_info(vec!["verse1.txt".to_string()]),
                vec!["rm".to_string(), "verse1.txt".to_string()],
                RuleHistory::new(),
                file_system.clone(),
                FakeMetadataGetter::new()
            ),
            vec![],
            vec![],
            FakeExecutor::new(file_system.clone()))
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

    fn spawn_command<
        FileSystemType: FileSystem + Send + 'static,
        MetadataGetterType: MetadataGetter + Send + 'static
        > (
            station : Station<FileSystemType, MetadataGetterType>,
            senders : Vec<(usize, Sender<Packet>)>,
            receivers : Vec<Receiver<Packet>>,
            executor: FakeExecutor
        ) -> JoinHandle<Result<CommandLineOutput, String>>
    {
        thread::spawn(
            move || -> Result<CommandLineOutput, String>
            {
                do_command(station, senders, receivers, executor)
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
            Station::new(
                to_info(vec!["stanza1.txt".to_string()]),
                vec![
                    "mycat".to_string(),
                    "verse1.txt".to_string(),
                    "stanza1.txt".to_string()
                ],
                RuleHistory::new(),
                file_system.clone(),
                FakeMetadataGetter::new(),
            ),
            vec![(0, sender)],
            vec![],
            FakeExecutor::new(file_system.clone())
        );

        let handle2 = spawn_command(
            Station::new(
                to_info(vec!["poem.txt".to_string()]),
                vec![
                    "mycat".to_string(),
                    "stanza1.txt".to_string(),
                    "poem.txt".to_string()
                ],
                RuleHistory::new(),
                file_system.clone(),
                FakeMetadataGetter::new(),
            ),
            vec![],
            vec![receiver],
            FakeExecutor::new(file_system.clone())
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
                assert_eq!(from_utf8(&file_system.read_file("poem.txt").unwrap()).unwrap(), "I wish I were a windowsill");
            },
            Err(_) => panic!("Second thread failed"),
        }
    }


    #[test]
    fn one_target_already_correct()
    {
        let file_system = FakeFileSystem::new();
        let metadata_getter1 = FakeMetadataGetter::new();
        let metadata_getter2 = FakeMetadataGetter::new();
        let mut rule_history2 = RuleHistory::new();

        match file_system.write_file(Path::new(&"verse1.txt"), "I wish I were a windowsill")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match file_system.write_file(Path::new(&"poem.txt"), "I wish I were a windowsill")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        let mut factory = TicketFactory::new();
        factory.input_ticket(
            TicketFactory::from_str("I wish I were a windowsill").result()
        );

        rule_history2.insert(
            factory.result(),
            vec![TicketFactory::from_str("I wish I were a windowsill").result()]);

        let (sender, receiver) = mpsc::channel();

        let handle1 = spawn_command(
            Station::new(
                to_info(vec!["verse1.txt".to_string()]),
                vec![],
                RuleHistory::new(),
                file_system.clone(),
                metadata_getter1,
            ),
            vec![(0, sender)],
            vec![],
            FakeExecutor::new(file_system.clone())
        );

        let handle2 = spawn_command(
            Station::new(
                to_info(vec!["poem.txt".to_string()]),
                vec![
                    "error".to_string(),
                    "this file should".to_string(),
                    "already be correct".to_string()
                ],
                rule_history2,
                file_system.clone(),
                metadata_getter2,
            ),
            vec![],
            vec![receiver],
            FakeExecutor::new(file_system.clone())
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
                assert_eq!(from_utf8(&file_system.read_file("poem.txt").unwrap()).unwrap(), "I wish I were a windowsill");
            },
            Err(_) => panic!("Second thread failed"),
        }
    }


    #[test]
    fn one_target_not_there_error_in_command()
    {
        let file_system = FakeFileSystem::new();
        let metadata_getter1 = FakeMetadataGetter::new();
        let metadata_getter2 = FakeMetadataGetter::new();
        let mut rule_history2 = RuleHistory::new();

        match file_system.write_file(Path::new(&"verse1.txt"), "I wish I were a windowsill")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        let mut factory = TicketFactory::new();
        factory.input_ticket(
            TicketFactory::from_str("I wish I were a windowsill").result()
        );

        rule_history2.insert(
            factory.result(),
            vec![TicketFactory::from_str("I wish I were a windowsill").result()]);

        let (sender, receiver) = mpsc::channel();

        let handle1 = spawn_command(
            Station::new(
                to_info(vec!["verse1.txt".to_string()]),
                vec![],
                RuleHistory::new(),
                file_system.clone(),
                metadata_getter1,
            ),
            vec![(0, sender)],
            vec![],
            FakeExecutor::new(file_system.clone())
        );

        let handle2 = spawn_command(
            Station::new(
                to_info(vec!["poem.txt".to_string()]),
                vec![
                    "nonsense".to_string(),
                    "should".to_string(),
                    "error".to_string(),
                ],
                rule_history2,
                file_system.clone(),
                metadata_getter2,
            ),
            vec![],
            vec![receiver],
            FakeExecutor::new(file_system.clone())
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
            Ok(thread_result) =>
            {
                match thread_result
                {
                    Ok(_) => panic!("Second thread failed to error"),
                    Err(_) =>
                    {
                        assert!(!file_system.is_file("poem.txt") && !file_system.is_dir("poem.txt"));
                    },
                }
            }
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
            Station::new(
                to_info(vec!["stanza1.txt".to_string()]),
                vec![
                    "mycat".to_string(),
                    "verse1.txt".to_string(),
                    "stanza1.txt".to_string()
                ],
                RuleHistory::new(),
                file_system.clone(),
                FakeMetadataGetter::new(),
            ),
            vec![(0, sender)],
            vec![],
            FakeExecutor::new(file_system.clone())
        );

        let handle2 = spawn_command(
            Station::new(
                to_info(vec!["poem.txt".to_string()]),
                vec![
                    "mycat".to_string(),
                    "stanza1.txt".to_string(),
                    "poem.txt".to_string()
                ],
                RuleHistory::new(),
                file_system.clone(),
                FakeMetadataGetter::new(),
            ),
            vec![],
            vec![receiver],
            FakeExecutor::new(file_system.clone())
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


    #[test]
    fn one_target_already_correct_according_to_timestamp()
    {
        let file_system = FakeFileSystem::new();
        let metadata_getter1 = FakeMetadataGetter::new();
        let mut metadata_getter2 = FakeMetadataGetter::new();
        let mut rule_history2 = RuleHistory::new();

        match file_system.write_file(Path::new(&"verse1.txt"), "I wish I were a windowsill")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match file_system.write_file(Path::new(&"poem.txt"), "Content actually wrong")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        metadata_getter2.insert_timestamp("poem.txt", 17);

        let mut factory = TicketFactory::new();
        factory.input_ticket(
            TicketFactory::from_str("I wish I were a windowsill").result()
        );

        rule_history2.insert(
            factory.result(),
            vec![TicketFactory::from_str("I wish I were a windowsill").result()]);

        let (sender, receiver) = mpsc::channel();

        let handle1 = spawn_command(
            Station::new(
                to_info(vec!["verse1.txt".to_string()]),
                vec![],
                RuleHistory::new(),
                file_system.clone(),
                metadata_getter1,
            ),
            vec![(0, sender)],
            vec![],
            FakeExecutor::new(file_system.clone())
        );

        let handle2 = spawn_command(
            Station::new(
                vec![
                    TargetFileInfo
                    {
                        path : "poem.txt".to_string(),
                        history : TargetHistory::new(
                            TicketFactory::from_str("I wish I were a windowsill").result(),
                            17,
                        ),
                    }
                ],
                vec![
                    "error".to_string(),
                    "this file should".to_string(),
                    "already be correct".to_string()
                ],
                rule_history2,
                file_system.clone(),
                metadata_getter2,
            ),
            vec![],
            vec![receiver],
            FakeExecutor::new(file_system.clone())
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
                assert_eq!(from_utf8(&file_system.read_file("poem.txt").unwrap()).unwrap(), "Content actually wrong");
            },
            Err(_) => panic!("Second thread failed"),
        }
    }


    #[test]
    fn one_target_correct_hash_incorrect_timestamp()
    {
        let file_system = FakeFileSystem::new();
        let metadata_getter1 = FakeMetadataGetter::new();
        let mut metadata_getter2 = FakeMetadataGetter::new();
        let mut rule_history2 = RuleHistory::new();

        match file_system.write_file(Path::new(&"verse1.txt"), "I wish I were a windowsill")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match file_system.write_file(Path::new(&"poem.txt"), "Content wrong at first")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        metadata_getter2.insert_timestamp("poem.txt", 18);

        let mut factory = TicketFactory::new();
        factory.input_ticket(
            TicketFactory::from_str("I wish I were a windowsill").result()
        );

        rule_history2.insert(
            factory.result(),
            vec![TicketFactory::from_str("I wish I were a windowsill").result()]);

        let (sender, receiver) = mpsc::channel();

        let handle1 = spawn_command(
            Station::new(
                to_info(vec!["verse1.txt".to_string()]),
                vec![],
                RuleHistory::new(),
                file_system.clone(),
                metadata_getter1,
            ),
            vec![(0, sender)],
            vec![],
            FakeExecutor::new(file_system.clone())
        );

        let handle2 = spawn_command(
            Station::new(
                vec![
                    TargetFileInfo
                    {
                        path : "poem.txt".to_string(),
                        history : TargetHistory::new(
                            TicketFactory::from_str("I wish I were a windowsill").result(),
                            17,
                        ),
                    }
                ],
                vec![
                    "mycat".to_string(),
                    "verse1.txt".to_string(),
                    "poem.txt".to_string()
                ],
                rule_history2,
                file_system.clone(),
                metadata_getter2,
            ),
            vec![],
            vec![receiver],
            FakeExecutor::new(file_system.clone())
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
                assert_eq!(from_utf8(&file_system.read_file("poem.txt").unwrap()).unwrap(), "I wish I were a windowsill");
            },
            Err(_) => panic!("Second thread failed"),
        }
    }
}
