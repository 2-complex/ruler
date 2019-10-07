extern crate filesystem;

use crate::packet::Packet;
use crate::ticket::TicketFactory;
use crate::station::{Station, get_file_ticket, TargetFileInfo};
use crate::executor::{CommandLineOutput, Executor};
use crate::metadata::MetadataGetter;
use crate::memory::{RuleHistory, RuleHistoryError};
use crate::cache::{LocalCache, RestoreResult};

use filesystem::FileSystem;
use std::process::Command;
use std::sync::mpsc::{Sender, Receiver, RecvError};
use std::collections::VecDeque;
use std::fmt;

#[derive(Clone)]
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
        let mut command_queue = VecDeque::from(command_list);
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

pub struct WorkResult
{
    pub target_infos : Vec<TargetFileInfo>,
    pub command_line_output : Option<CommandLineOutput>,
    pub rule_history : Option<RuleHistory>,
}

pub enum WorkError
{
    ReceivedErrorFromSource(String),
    ReceiverError(RecvError),
    SenderError,
    TicketAlignmentError(std::io::Error),
    FileNotFound(String),
    FileIoError(String, std::io::Error),
    CommandErrorWhileExecuting(String),
    Contradiction(Vec<String>),
    CacheDirectoryMissing,
    CacheMalfunction(std::io::Error),
    Weird,
}

impl fmt::Display for WorkError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            WorkError::ReceivedErrorFromSource(error) =>
                write!(formatter, "Received error from source: {}", error),

            WorkError::ReceiverError(error) =>
                write!(formatter, "Failed to recieve anything from source: {}", error),

            WorkError::SenderError =>
                write!(formatter, "Failed to send to dependent"),

            WorkError::TicketAlignmentError(error) =>
                write!(formatter, "File IO error when attempting to get hash of sources: {}", error),

            WorkError::FileNotFound(path) =>
                write!(formatter, "File not found: {}", path),

            WorkError::FileIoError(path, error) =>
                write!(formatter, "Error reading file: {}: {}", path, error),

            WorkError::CommandErrorWhileExecuting(message) =>
                write!(formatter, "Failed to execute command with message: {}", message),

            WorkError::Contradiction(contradicting_target_paths) =>
            {
                let mut message = "The following targets failed to record into history because they contradict an existing target history:\n".to_string();
                for path in contradicting_target_paths
                {
                    message.push_str(path);
                    message.push_str("\n");
                }
                message.push_str("This likely means a real dependence is not reflected in the rule.\n");
                write!(formatter, "{}", message)
            },

            WorkError::CacheDirectoryMissing =>
                write!(formatter, "Cache directory missing"),

            WorkError::CacheMalfunction(error) =>
                write!(formatter, "Cache file i/o failed: {}", error),

            WorkError::Weird =>
                write!(formatter, "Weird! How did you do that!"),
        }
    }
}

pub fn do_command<
    FileSystemType: FileSystem,
    ExecType: Executor,
    MetadataGetterType: MetadataGetter>
(
    station : Station<FileSystemType, MetadataGetterType>,
    senders : Vec<(usize, Sender<Packet>)>,
    receivers : Vec<Receiver<Packet>>,
    executor : ExecType,
    cache : LocalCache
)
-> Result<WorkResult, WorkError>
{
    let mut factory = TicketFactory::new();

    for rcv in receivers.iter()
    {
        match rcv.recv()
        {
            Ok(packet) => 
            {
                match packet.get_ticket()
                {
                    Ok(ticket) => factory.input_ticket(ticket),
                    Err(error) => return Err(WorkError::ReceivedErrorFromSource(error)),
                }
            },
            Err(error) => return Err(WorkError::ReceiverError(error)),
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
            Ok(ticket_opt) =>
            {
                match ticket_opt
                {
                    Some(ticket) => target_tickets.push(ticket),
                    None => target_tickets.push(TicketFactory::does_not_exist()),
                }
            },
            Err(error) => return Err(WorkError::TicketAlignmentError(error)),
        }
    }
    let sources_ticket = factory.result();

    match station.rule_history
    {
        None =>
        {
            // TODO: test this flow.
            match executor.execute_command(station.command)
            {
                Ok(command_result) =>
                {
                    let mut post_command_target_tickets = Vec::new();
                    for target_info in station.target_infos.iter()
                    {
                        match get_file_ticket(
                            &station.file_system,
                            &station.metadata_getter,
                            &target_info)
                        {
                            Ok(ticket_opt) =>
                            {
                                match ticket_opt
                                {
                                    Some(ticket) => post_command_target_tickets.push(ticket),
                                    None => return Err(WorkError::FileNotFound(target_info.path.clone())),
                                }
                            },
                            Err(error) => return Err(WorkError::FileIoError(target_info.path.clone(), error)),
                        }
                    }

                    for (sub_index, sender) in senders
                    {
                        match sender.send(Packet::from_ticket(
                            post_command_target_tickets[sub_index].clone()))
                        {
                            Ok(_) => {},
                            Err(_error) => return Err(WorkError::SenderError),
                        }
                    }

                    Ok(
                        WorkResult
                        {
                            target_infos : station.target_infos,
                            command_line_output : Some(command_result),
                            rule_history : None
                        }
                    )
                },
                Err(_error) => return Err(WorkError::SenderError),
            }
        },
        Some(mut rule_history) =>
        {
            if ! match rule_history.get_target_tickets(&sources_ticket)
            {
                Some(remembered_target_tickets) =>
                {
                    let mut all_same = true;
                    for (i, remembered_ticket) in remembered_target_tickets.iter().enumerate()
                    {
                        if target_tickets[i] != *remembered_ticket
                        {
                            match cache.back_up_file_with_ticket(
                                &station.file_system,
                                &target_tickets[i],
                                &station.target_infos[i].path)
                            {
                                Ok(()) => {},
                                Err(error) =>
                                    return Err(WorkError::CacheMalfunction(error)),
                            }

                            all_same = all_same &&
                            match cache.restore_file(
                                &station.file_system,
                                remembered_ticket,
                                &station.target_infos[i].path)
                            {
                                RestoreResult::Done => true,
                                RestoreResult::NotThere => false,

                                RestoreResult::CacheDirectoryMissing =>
                                    return Err(WorkError::CacheDirectoryMissing),

                                RestoreResult::FileSystemError(error) =>
                                    return Err(WorkError::CacheMalfunction(error)),
                            };
                        }
                    }

                    all_same
                },
                None => false,
            }
            {
                match executor.execute_command(station.command)
                {
                    Ok(command_result) =>
                    {
                        let mut post_command_target_tickets = Vec::new();
                        for target_info in station.target_infos.iter()
                        {
                            match get_file_ticket(
                                &station.file_system,
                                &station.metadata_getter,
                                &target_info)
                            {
                                Ok(ticket_opt) =>
                                {
                                    match ticket_opt
                                    {
                                        Some(ticket) => post_command_target_tickets.push(ticket),
                                        None => return Err(WorkError::FileNotFound(target_info.path.clone())),
                                    }
                                },
                                Err(error) => return Err(WorkError::FileIoError(target_info.path.clone(), error)),
                            }
                        }

                        for (sub_index, sender) in senders
                        {
                            match sender.send(Packet::from_ticket(
                                post_command_target_tickets[sub_index].clone()))
                            {
                                Ok(_) => {},
                                Err(_error) => return Err(WorkError::SenderError),
                            }
                        }

                        match rule_history.insert(sources_ticket, post_command_target_tickets)
                        {
                            Ok(_) => {},
                            Err(error) =>
                            {
                                match error
                                {
                                    RuleHistoryError::Contradiction(contradicting_indices) =>
                                    {
                                        let mut contradicting_target_paths = Vec::new();
                                        for index in contradicting_indices
                                        {
                                            contradicting_target_paths.push(station.target_infos[index].path.clone())
                                        }
                                        return Err(WorkError::Contradiction(contradicting_target_paths));
                                    }

                                    RuleHistoryError::TargetSizesDifferWeird =>
                                        return Err(WorkError::Weird),
                                }
                            },
                        }

                        Ok(
                            WorkResult
                            {
                                target_infos : station.target_infos,
                                command_line_output : Some(command_result),
                                rule_history : Some(rule_history),
                            }
                        )
                    },
                    Err(error) =>
                    {
                        return Err(WorkError::CommandErrorWhileExecuting(error))
                    },
                }
            }
            else
            {
                for (sub_index, sender) in senders
                {
                    match sender.send(Packet::from_ticket(
                        target_tickets[sub_index].clone()))
                    {
                        Ok(_) => {},
                        Err(_error) => return Err(WorkError::SenderError),
                    }
                }

                Ok(
                    WorkResult
                    {
                        target_infos : station.target_infos,
                        command_line_output : None,
                        rule_history : Some(rule_history),
                    }
                )
            }
        },
    }
}

#[cfg(test)]
mod test
{
    use crate::station::{Station, TargetFileInfo};
    use crate::work::{do_command, WorkResult, WorkError};
    use crate::ticket::TicketFactory;
    use crate::memory::{RuleHistory, TargetHistory};
    use crate::executor::FakeExecutor;
    use crate::packet::Packet;
    use crate::metadata::{MetadataGetter, FakeMetadataGetter};
    use crate::cache::LocalCache;

    use filesystem::{FileSystem, FakeFileSystem};
    use std::path::Path;
    use std::sync::mpsc::{self, Sender, Receiver};
    use std::str::from_utf8;
    use std::thread::{self, JoinHandle};

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
                None,
                file_system.clone(),
                FakeMetadataGetter::new()),
            Vec::new(),
            Vec::new(),
            FakeExecutor::new(file_system.clone()),
            LocalCache::new(".ruler-cache"))
        {
            Ok(result) =>
            {
                match result.command_line_output
                {
                    Some(output) =>
                    {
                        assert_eq!(output.out, "");
                        assert_eq!(output.err, "");
                        assert_eq!(output.code, Some(0));
                        assert_eq!(output.success, true);
                    }
                    None => panic!("Ouptut errored"),
                }
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
                None,
                file_system.clone(),
                FakeMetadataGetter::new()),
            vec![(0, sender_c)],
            vec![receiver_a, receiver_b],
            FakeExecutor::new(file_system.clone()),
            LocalCache::new(".ruler-cache"))
        {
            Ok(result) =>
            {
                match result.command_line_output
                {
                    Some(output) =>
                    {
                        assert_eq!(output.out, "");
                        assert_eq!(output.err, "");
                        assert_eq!(output.code, Some(0));
                        assert_eq!(output.success, true);
                    },
                    None => panic!("Output wasn't there"),
                }

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
                None,
                file_system.clone(),
                FakeMetadataGetter::new()),
            vec![(0, sender_c)],
            vec![receiver_a, receiver_b],
            FakeExecutor::new(file_system.clone()),
            LocalCache::new(".ruler-cache"))
        {
            Ok(result) =>
            {
                match result.command_line_output
                {
                    Some(output)=>
                    {
                        assert_eq!(output.out, "");
                        assert_eq!(output.err, "");
                        assert_eq!(output.code, Some(0));
                        assert_eq!(output.success, true);
                    },
                    None => panic!("Output wasn't there."),
                }

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

        match rule_history.insert(
            factory.result(),
            vec![
                TicketFactory::from_str("Roses are red\nViolets are violet\n").result()
            ]
        )
        {
            Ok(_) => {},
            Err(_) => panic!("Rule history failed to insert"),
        }

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
                Some(rule_history),
                file_system.clone(),
                FakeMetadataGetter::new(),
            ),
            vec![(0, sender_c)],
            vec![receiver_a, receiver_b],
            FakeExecutor::new(file_system.clone()),
            LocalCache::new(".ruler-cache"))
        {
            Ok(result) =>
            {
                match result.command_line_output
                {
                    Some(_output) => panic!("Output present when none was expected."),
                    None => {},
                }

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
    fn poem_contradicts_history()
    {
        let (sender_a, receiver_a) = mpsc::channel();
        let (sender_b, receiver_b) = mpsc::channel();
        let (sender_c, _receiver_c) = mpsc::channel();

        let mut rule_history = RuleHistory::new();

        let mut factory = TicketFactory::new();
        factory.input_ticket(TicketFactory::from_str("Roses are red\n").result());
        factory.input_ticket(TicketFactory::from_str("Violets are violet\n").result());

        match rule_history.insert(
            factory.result(),
            vec![
                TicketFactory::from_str("Roses are red\nViolets are blue\n").result()
            ]
        )
        {
            Ok(_) => {},
            Err(_) => panic!("Rule history failed to insert"),
        }

        let file_system = FakeFileSystem::new();

        match file_system.create_dir(".ruler-cache")
        {
            Ok(_) => {},
            Err(_) => panic!("Failed to create directory"),
        }


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

        match file_system.write_file(Path::new(&"poem.txt"), "Arbitrary content")
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
                Some(rule_history),
                file_system.clone(),
                FakeMetadataGetter::new(),
            ),
            vec![(0, sender_c)],
            vec![receiver_a, receiver_b],
            FakeExecutor::new(file_system.clone()),
            LocalCache::new(".ruler-cache"))
        {
            Ok(_result) =>
            {
                panic!("Build was success when it should have been a contradition")
            },
            Err(error) =>
            {
                match error
                {
                    WorkError::Contradiction(paths) => 
                    {
                        assert_eq!(paths.len(), 1);
                    },
                    _ => panic!("Wrong error: {}", error),
                }
            }
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
                None,
                file_system.clone(),
                FakeMetadataGetter::new()
            ),
            vec![],
            vec![],
            FakeExecutor::new(file_system.clone()),
            LocalCache::new(".ruler-cache"))
        {
            Ok(_) =>
            {
                panic!("Expected failure when file not present")
            },
            Err(error) =>
            {
                match error
                {
                    WorkError::FileNotFound(path) => assert_eq!(path, "verse1.txt"),
                    _=> panic!("Wrong kind of error"),
                }
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
                None,
                file_system.clone(),
                FakeMetadataGetter::new()
            ),
            vec![],
            vec![],
            FakeExecutor::new(file_system.clone()),
            LocalCache::new(".rule-cache"))
        {
            Ok(_) =>
            {
                panic!("Expected failure when file not present")
            },
            Err(error) =>
            {
                match error
                {
                    WorkError::FileNotFound(path) => assert_eq!(path, "verse1.txt"),
                    _ => panic!("Wrong kind of error"),
                }
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
        ) -> JoinHandle<Result<WorkResult, WorkError>>
    {
        thread::spawn(
            move || -> Result<WorkResult, WorkError>
            {
                do_command(station, senders, receivers, executor, LocalCache::new(".ruler-cache"))
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
                None,
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
                None,
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

        match rule_history2.insert(
            factory.result(),
            vec![TicketFactory::from_str("I wish I were a windowsill").result()])
        {
            Ok(_) => {},
            Err(_) => panic!("Rule history failed to insert"),
        }

        let (sender, receiver) = mpsc::channel();

        let handle1 = spawn_command(
            Station::new(
                to_info(vec!["verse1.txt".to_string()]),
                vec![],
                None,
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
                Some(rule_history2),
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

        match rule_history2.insert(
            factory.result(),
            vec![TicketFactory::from_str("I wish I were a windowsill").result()])
        {
            Ok(_) => {},
            Err(_) => panic!("Rule history failed to insert"),
        }

        let (sender, receiver) = mpsc::channel();

        let handle1 = spawn_command(
            Station::new(
                to_info(vec!["verse1.txt".to_string()]),
                vec![],
                None,
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
                Some(rule_history2),
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
                None,
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
                None,
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

        match rule_history2.insert(
            factory.result(),
            vec![TicketFactory::from_str("I wish I were a windowsill").result()])
        {
            Ok(_) => {},
            Err(_) => panic!("Rule history failed to insert"),
        }

        let (sender, receiver) = mpsc::channel();

        let handle1 = spawn_command(
            Station::new(
                to_info(vec!["verse1.txt".to_string()]),
                vec![],
                None,
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
                Some(rule_history2),
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

        match file_system.create_dir(".ruler-cache")
        {
            Ok(_) => {},
            Err(_) => panic!("Failed to create directory"),
        }

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

        match rule_history2.insert(
            factory.result(),
            vec![TicketFactory::from_str("I wish I were a windowsill").result()])
        {
            Ok(_) => {},
            Err(_) => panic!("Rule history failed to insert"),
        }

        let (sender, receiver) = mpsc::channel();

        let handle1 = spawn_command(
            Station::new(
                to_info(vec!["verse1.txt".to_string()]),
                vec![],
                None,
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
                Some(rule_history2),
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
