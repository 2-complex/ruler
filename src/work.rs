extern crate filesystem;

use crate::packet::Packet;
use crate::ticket::{Ticket, TicketFactory};
use crate::station::{Station, get_file_ticket, TargetFileInfo};
use crate::executor::{CommandLineOutput, Executor};
use crate::metadata::MetadataGetter;
use crate::memory::{RuleHistory, RuleHistoryError};
use crate::cache::{LocalCache, RestoreResult};
use crate::internet::{upload, download, UploadError};

use filesystem::FileSystem;
use std::sync::mpsc::{Sender, Receiver, RecvError};
use std::fmt;

pub enum FileResolution
{
    AlreadyCorrect,
    Recovered,
    Downloaded,
    NeedsRebuild,
}

pub enum WorkOption
{
    SourceOnly,
    AllAlreadyCorrect,
    TargetResolutions(Vec<FileResolution>),
    CommandExecuted(CommandLineOutput),
}

pub struct WorkResult
{
    pub target_infos : Vec<TargetFileInfo>,
    pub work_option : WorkOption,
    pub rule_history : Option<RuleHistory>,
}

pub enum WorkError
{
    ReceivedErrorFromSource(String),
    ReceiverError(RecvError),
    SenderError,
    TicketAlignmentError(std::io::Error),
    FileNotFound(String),
    FileNotAvailableToCache(String),
    FileIoError(String, std::io::Error),
    CommandExecutedButErrored(String),
    CommandFailedToExecute(String),
    Contradiction(Vec<String>),
    CacheDirectoryMissing,
    CacheMalfunction(std::io::Error),
    CommandWithNoRuleHistory,
    UploadError(UploadError),
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

            WorkError::FileNotAvailableToCache(path) =>
                write!(formatter, "File failed to cache: {}", path),

            WorkError::FileIoError(path, error) =>
                write!(formatter, "Error reading file: {}: {}", path, error),

            WorkError::CommandExecutedButErrored(message) =>
                write!(formatter, "Command executed but errored: {}", message),

            WorkError::CommandFailedToExecute(error) =>
                write!(formatter, "Failed to execute command with message: {}", error),

            WorkError::UploadError(error) =>
                write!(formatter, "File Failed to upload: {}", error),

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

            WorkError::CommandWithNoRuleHistory =>
                write!(formatter, "Command provided but no rule history, that should be impossible"),

            WorkError::Weird =>
                write!(formatter, "Weird! How did you do that!"),
        }
    }
}

fn attempt_download
<
    FileSystemType : FileSystem
>
(
    file_system : &FileSystemType,
    download_urls : &Vec<String>,
    remembered_ticket : &Ticket,
    target_path : &str
)
->
FileResolution
{
    for url in download_urls
    {
        match download(
            &format!("{}{}", url, remembered_ticket),
            file_system,
            target_path)
        {
            Ok(_) =>
            {
                // TODO: Might want to check that the ticket matches
                return FileResolution::Downloaded
            },
            Err(_) =>
            {
            }
        }
    }

    FileResolution::NeedsRebuild
}

/*  Given a target-info and a remembered ticket for that target file, check the current
    ticket, and if it matches, return AlreadyCorrect.  If it doesn't match, back up the current
    file, and then attempt to restore the remembered file from cache, if the cache doesn't have it
    attempt to download.  If no recovery or download works, shrug and return NeedsRebuild */
fn resolve_single_target
<
    FileSystemType : FileSystem,
    MetadataGetterType : MetadataGetter,
>
(
    file_system : &FileSystemType,
    metadata_getter : &MetadataGetterType,
    cache : &LocalCache,
    download_urls : &Vec<String>,
    remembered_ticket : &Ticket,
    target_info : &TargetFileInfo
)
->
Result<FileResolution, WorkError>
{
    match get_file_ticket(file_system, metadata_getter, target_info)
    {
        Ok(Some(current_target_ticket)) =>
        {
            if *remembered_ticket == current_target_ticket
            {
                return Ok(FileResolution::AlreadyCorrect);
            }

            match cache.back_up_file_with_ticket(
                file_system,
                &current_target_ticket,
                &target_info.path)
            {
                Ok(_) => {},
                Err(_error) => return Err(
                    WorkError::FileNotAvailableToCache(
                        target_info.path.clone())),
            }

            match cache.restore_file(
                file_system,
                &remembered_ticket,
                &target_info.path)
            {
                RestoreResult::Done =>
                    Ok(FileResolution::Recovered),

                RestoreResult::NotThere =>
                    Ok(attempt_download(file_system, download_urls, remembered_ticket, &target_info.path)),

                RestoreResult::CacheDirectoryMissing =>
                    Err(WorkError::CacheDirectoryMissing),

                RestoreResult::FileSystemError(error) =>
                    Err(WorkError::CacheMalfunction(error)),
            }
        },

        // None means the file is not there, in which case, we just try to restore/download, and then go home.
        Ok(None) =>
        {
            match cache.restore_file(
                file_system,
                &remembered_ticket,
                &target_info.path)
            {
                RestoreResult::Done =>
                    Ok(FileResolution::Recovered),

                RestoreResult::NotThere =>
                    Ok(attempt_download(file_system, download_urls, remembered_ticket, &target_info.path)),

                RestoreResult::CacheDirectoryMissing =>
                    Err(WorkError::CacheDirectoryMissing),

                RestoreResult::FileSystemError(error) =>
                    Err(WorkError::CacheMalfunction(error)),
            }
        },
        Err(error) =>
            Err(WorkError::TicketAlignmentError(error)),
    }
}

fn resolve_with_cache
<
    FileSystemType : FileSystem,
    MetadataGetterType : MetadataGetter
>
(
    file_system : &FileSystemType,
    metadata_getter : &MetadataGetterType,
    cache : &LocalCache,
    download_urls : &Vec<String>,
    rule_history : &RuleHistory,
    sources_ticket : &Ticket,
    target_infos : &Vec<TargetFileInfo>,
)
->
Result<Vec<FileResolution>, WorkError>
{
    let mut resolutions = Vec::new();

    match rule_history.get_target_tickets(sources_ticket)
    {
        Some(remembered_target_tickets) =>
        {
            for (i, target_info) in target_infos.iter().enumerate()
            {
                let remembered_ticket =
                match remembered_target_tickets.get(i)
                {
                    Some(ticket) => ticket,
                    None => return Err(WorkError::Weird),
                };

                match resolve_single_target(
                    file_system,
                    metadata_getter,
                    cache,
                    download_urls,
                    remembered_ticket,
                    target_info)
                {
                    Ok(resolution) => resolutions.push(resolution),
                    Err(error) => return Err(error),
                }
            }
        },

        None =>
        {
            for target_info in target_infos.iter()
            {
                match get_file_ticket(file_system, metadata_getter, target_info)
                {
                    Ok(Some(current_target_ticket)) =>
                    {
                        match cache.back_up_file_with_ticket(
                            file_system,
                            &current_target_ticket,
                            &target_info.path)
                        {
                            Ok(_) =>
                            {
                                // TODO: Maybe encode whether it was cached in the FileResoluton
                                resolutions.push(FileResolution::NeedsRebuild);
                            },
                            Err(_error) =>
                            {
                                return Err(
                                    WorkError::FileNotAvailableToCache(
                                        target_info.path.clone()));
                            }
                        }
                    },

                    Ok(None) => resolutions.push(FileResolution::NeedsRebuild),

                    Err(error) =>
                        return Err(WorkError::TicketAlignmentError(error)),
                }
            }
        }
    }

    Ok(resolutions)
}


fn get_current_target_tickets
<
    FileSystemType: FileSystem,
    MetadataGetterType: MetadataGetter
>
(
    file_system : &FileSystemType,
    metadata_getter : &MetadataGetterType,
    target_infos : &Vec<TargetFileInfo>,
)
-> Result<Vec<Ticket>, WorkError>
{
    let mut target_tickets = Vec::new();
    for target_info in target_infos.iter()
    {
        match get_file_ticket(file_system, metadata_getter, target_info)
        {
            Ok(ticket_opt) =>
            {
                match ticket_opt
                {
                    Some(ticket) => target_tickets.push(ticket),
                    None => return Err(WorkError::FileNotFound(target_info.path.clone())),
                }
            },
            Err(error) => return Err(WorkError::FileIoError(target_info.path.clone(), error)),
        }
    }

    Ok(target_tickets)
}

/*  A rule_history of None means that the node in question doesn't represent a rule.
    So, error if there is a command specified, and at the end return SourceOnly. */
fn handle_source_only_node
<
    FileSystemType: FileSystem,
    MetadataGetterType: MetadataGetter
>
(
    station : Station<FileSystemType, MetadataGetterType>,
    senders : Vec<(usize, Sender<Packet>)>
)
->
Result<WorkResult, WorkError>
{
    if station.command.len() != 0
    {
        return Err(WorkError::CommandWithNoRuleHistory)
    }

    let current_target_tickets = match get_current_target_tickets(
        &station.file_system,
        &station.metadata_getter,
        &station.target_infos)
    {
        Ok(target_tickets) => target_tickets,
        Err(error) => return Err(error),
    };

    for (sub_index, sender) in senders
    {
        match sender.send(Packet::from_ticket(
            current_target_tickets[sub_index].clone()))
        {
            Ok(_) => {},
            Err(_error) => return Err(WorkError::SenderError),
        }
    }

    Ok(
        WorkResult
        {
            target_infos : station.target_infos,
            work_option : WorkOption::SourceOnly,
            rule_history : None
        }
    )
}

/*  Takes a vector of receivers, and waits for them all to receive, so it can
    hash together all their results into one Ticket obejct.  Returns an error
    if the receivers error or if the packet produces an error when it tries to
    get the ticket from it. */
fn wait_for_sources_ticket
(
    receivers : Vec<Receiver<Packet>>
)
->
Result<Ticket, WorkError>
{
    let mut factory = TicketFactory::new();

    for receiver in receivers.iter()
    {
        match receiver.recv()
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

    Ok(factory.result())
}

/*  Takes a vector of resolutions, and returns true if any of them are NeedsRebuild*/
fn needs_rebuild(resolutions : &Vec<FileResolution>) -> bool
{
    for resolution in resolutions
    {
        match resolution
        {
            FileResolution::NeedsRebuild =>
            {
                return true
            },
            _ => {},
        }
    }

    false
}

fn rebuild_node
<
    FileSystemType : FileSystem,
    ExecType : Executor,
    MetadataGetterType : MetadataGetter
>
(
    file_system : &FileSystemType,
    metadata_getter : &MetadataGetterType,
    executor : &ExecType,
    mut rule_history : RuleHistory,
    sources_ticket : Ticket,
    command : Vec<String>,
    senders : Vec<(usize, Sender<Packet>)>,
    target_infos : Vec<TargetFileInfo>
)
->
Result<WorkResult, WorkError>
{
    match executor.execute_command(command)
    {
        Ok(command_result) =>
        {
            if ! command_result.success
            {
                return Err(WorkError::CommandExecutedButErrored(command_result.err));
            }

            let mut post_command_target_tickets = Vec::new();
            for target_info in target_infos.iter()
            {
                match get_file_ticket(
                    file_system,
                    metadata_getter,
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
                                contradicting_target_paths.push(target_infos[index].path.clone())
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
                    target_infos : target_infos,
                    work_option : WorkOption::CommandExecuted(command_result),
                    rule_history : Some(rule_history),
                }
            )
        },
        Err(error) =>
        {
            return Err(WorkError::CommandFailedToExecute(error))
        },
    }
}

pub fn handle_node
<
    FileSystemType: FileSystem,
    ExecType: Executor,
    MetadataGetterType: MetadataGetter
>
(
    station : Station<FileSystemType, MetadataGetterType>,
    senders : Vec<(usize, Sender<Packet>)>,
    receivers : Vec<Receiver<Packet>>,
    executor : ExecType,
    cache : LocalCache,
    download_urls : Vec<String>
)
->
Result<WorkResult, WorkError>
{
    let sources_ticket = match wait_for_sources_ticket(receivers)
    {
        Ok(ticket) => ticket,
        Err(error) => return Err(error),
    };

    match station.rule_history
    {
        None => handle_source_only_node(station, senders),

        Some(rule_history) =>
        {
            match resolve_with_cache(
                &station.file_system,
                &station.metadata_getter,
                &cache,
                &download_urls,
                &rule_history,
                &sources_ticket,
                &station.target_infos)
            {
                Ok(resolutions) =>
                {
                    if needs_rebuild(&resolutions)
                    {
                        rebuild_node(
                            &station.file_system,
                            &station.metadata_getter,
                            &executor,
                            rule_history,
                            sources_ticket,
                            station.command,
                            senders,
                            station.target_infos
                        )
                    }
                    else
                    {
                        let target_tickets = match get_current_target_tickets(
                            &station.file_system,
                            &station.metadata_getter,
                            &station.target_infos)
                        {
                            Ok(target_tickets) => target_tickets,
                            Err(error) => return Err(error),
                        };

                        for (sub_index, sender) in senders
                        {
                            match sender.send(
                                Packet::from_ticket(target_tickets[sub_index].clone()))
                            {
                                Ok(_) => {},
                                Err(_error) => return Err(WorkError::SenderError),
                            }
                        }

                        Ok(
                            WorkResult
                            {
                                target_infos : station.target_infos,
                                work_option : WorkOption::AllAlreadyCorrect,
                                rule_history : Some(rule_history),
                            }
                        )
                    }
                },

                Err(error) => Err(error),
            }
        },
    }
}


pub fn clean_targets<
    FileSystemType: FileSystem,
    MetadataGetterType: MetadataGetter>
(
    target_infos : Vec<TargetFileInfo>,
    file_system : &FileSystemType,
    metadata_getter : &MetadataGetterType,
    cache : &LocalCache
)
-> Result<(), WorkError>
{
    for target_info in target_infos
    {
        if file_system.is_file(&target_info.path)
        {
            match get_file_ticket(file_system, metadata_getter, &target_info)
            {
                Ok(Some(current_target_ticket)) =>
                {
                    {
                        match cache.back_up_file_with_ticket(
                            file_system,
                            &current_target_ticket,
                            &target_info.path)
                        {
                            Ok(_) => {},
                            Err(_error) =>
                                return Err(WorkError::FileNotAvailableToCache(
                                    target_info.path.clone())),
                        }
                    }
                },
                Ok(None)=>
                {
                    match cache.back_up_file(
                        file_system,
                        &target_info.path)
                    {
                        Ok(_) => {},
                        Err(_error) =>
                            return Err(WorkError::FileNotAvailableToCache(
                                target_info.path.clone())),
                    }
                },
                Err(error) => return Err(WorkError::TicketAlignmentError(error)),
            }
        }
    }

    Ok(())
}


pub fn upload_targets<FileSystemType: FileSystem>
(
    file_system : &FileSystemType,
    target_paths : Vec<String>,
    server_url : String,
)
-> Result<(), WorkError>
{
    for path in target_paths
    {
        if file_system.is_file(&path)
        {
            match upload(&server_url, file_system, &path)
            {
                Ok(_) => {},
                Err(error) => return Err(WorkError::UploadError(error)),
            }
        }
    }

    Ok(())
}



#[cfg(test)]
mod test
{
    use crate::station::{Station, TargetFileInfo};
    use crate::work::
    {
        handle_node,
        WorkResult,
        WorkOption,
        WorkError
    };
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

        match handle_node(
            Station::new(
                to_info(vec!["A".to_string()]),
                vec![],
                None,
                file_system.clone(),
                FakeMetadataGetter::new()),
            Vec::new(),
            Vec::new(),
            FakeExecutor::new(file_system.clone()),
            LocalCache::new(".ruler-cache"),
            vec![])
        {
            Ok(result) =>
            {
                match result.work_option
                {
                    WorkOption::SourceOnly => {},
                    _ => panic!("Wrong kind of WorkOption in result when command empty"),
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

        match file_system.create_dir(".ruler-cache")
        {
            Ok(_) => {},
            Err(_) => panic!("Failed to make cache directory"),
        }

        match file_system.write_file("A-source.txt", "")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match file_system.write_file("A.txt", "")
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

        match handle_node(
            Station::new(
                to_info(vec!["A.txt".to_string()]),
                vec!["mycat".to_string(), "A-source.txt".to_string(), "A.txt".to_string()],
                Some(RuleHistory::new()),
                file_system.clone(),
                FakeMetadataGetter::new()),
            vec![(0, sender_c)],
            vec![receiver_a, receiver_b],
            FakeExecutor::new(file_system.clone()),
            LocalCache::new(".ruler-cache"),
            vec![])
        {
            Ok(result) =>
            {
                match result.work_option
                {
                    WorkOption::CommandExecuted(output) =>
                    {
                        assert_eq!(output.out, "");
                        assert_eq!(output.err, "");
                        assert_eq!(output.code, Some(0));
                        assert_eq!(output.success, true);
                    },
                    _ => panic!("Wrong type of work option.  Command was supposed to execute."),
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
    fn command_errors()
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

        match handle_node(
            Station::new(
                to_info(vec!["poem.txt".to_string()]),
                vec!["error".to_string()],
                Some(RuleHistory::new()),
                file_system.clone(),
                FakeMetadataGetter::new()),
            vec![(0, sender_c)],
            vec![receiver_a, receiver_b],
            FakeExecutor::new(file_system.clone()),
            LocalCache::new(".ruler-cache"),
            vec![])
        {
            Ok(_) => panic!("Unexpected command success"),
            Err(WorkError::CommandExecutedButErrored(message)) =>
            {
                match receiver_c.recv()
                {
                    Ok(_) => panic!("Unexpected successful receive"),
                    Err(_) => {}
                }

                assert_eq!(message, "Failed");
            },
            Err(error) => panic!("Wrong kind of error when command errors: {}", error),
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

        match handle_node(
            Station::new(
                to_info(vec!["poem.txt".to_string()]),
                vec![
                    "mycat".to_string(),
                    "verse1.txt".to_string(),
                    "verse2.txt".to_string(),
                    "poem.txt".to_string()
                ],
                Some(RuleHistory::new()),
                file_system.clone(),
                FakeMetadataGetter::new()),
            vec![(0, sender_c)],
            vec![receiver_a, receiver_b],
            FakeExecutor::new(file_system.clone()),
            LocalCache::new(".ruler-cache"),
            vec![])
        {
            Ok(result) =>
            {
                match result.work_option
                {
                    WorkOption::CommandExecuted(output)=>
                    {
                        assert_eq!(output.out, "");
                        assert_eq!(output.err, "");
                        assert_eq!(output.code, Some(0));
                        assert_eq!(output.success, true);
                    },
                    _ => panic!("Comand was supposed to execute.  Output wasn't there."),
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

        match handle_node(
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
            LocalCache::new(".ruler-cache"),
            vec![])
        {
            Ok(result) =>
            {
                match result.work_option
                {
                    WorkOption::AllAlreadyCorrect => {},
                    _ => panic!("Expected poem to already be correct, was some other work option {}"),
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

        match handle_node(
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
            LocalCache::new(".ruler-cache"),
            vec![])
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

        match handle_node(
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
            LocalCache::new(".ruler-cache"),
            vec![])
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

        match handle_node(
            Station::new(
                to_info(vec!["verse1.txt".to_string()]),
                vec!["rm".to_string(), "verse1.txt".to_string()],
                Some(RuleHistory::new()),
                file_system.clone(),
                FakeMetadataGetter::new()
            ),
            vec![],
            vec![],
            FakeExecutor::new(file_system.clone()),
            LocalCache::new(".rule-cache"),
            vec![])
        {
            Ok(_) =>
            {
                panic!("Expected failure when file not present")
            },
            Err(error) =>
            {
                match error
                {
                    WorkError::FileNotAvailableToCache(path) => assert_eq!(path, "verse1.txt"),
                    _ => panic!("Wrong kind of error!  Incorrect error: {}", error),
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
                handle_node(station, senders, receivers, executor, LocalCache::new(".ruler-cache"), vec![])
            }
        )
    }

    #[test]
    fn one_dependence_only()
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
                Some(RuleHistory::new()),
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
                Some(RuleHistory::new()),
                file_system.clone(),
                FakeMetadataGetter::new(),
            ),
            vec![],
            vec![receiver],
            FakeExecutor::new(file_system.clone())
        );

        match handle1.join()
        {
            Ok(result) =>
            {
                match result
                {
                    Ok(work_result) =>
                    {
                        assert_eq!(work_result.target_infos[0].path, "stanza1.txt");

                        match work_result.rule_history
                        {
                            Some(_) => {},
                            None => panic!("Thread left with rule-history, came back with nothing."),
                        }

                        match work_result.work_option
                        {
                            WorkOption::CommandExecuted(output) =>
                            {
                                assert_eq!(output.out, "");
                            },
                            _ => panic!("Thread was supposed to execute command, did something else, got wrong work-option."),
                        }

                        assert_eq!(from_utf8(&file_system.read_file("stanza1.txt").unwrap()).unwrap(), "I wish I were a windowsill");
                    },
                    Err(_) => panic!("Thread inside failed"),
                }
            },


            Err(_) => panic!("Thread mechanics failed"),
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
    fn two_targets_both_not_there()
    {
        let file_system = FakeFileSystem::new();

        match file_system.write_file(Path::new(&"verse1.txt"), "I wish I were a windowsill")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        let (sender, _receiver) = mpsc::channel();

        let handle1 = spawn_command(
            Station::new(
                to_info(vec!["stanza1.txt".to_string()]),
                vec![
                    "mycat2".to_string(),
                    "verse1.txt".to_string(),
                    "stanza1.txt".to_string(),
                    "stanza2.txt".to_string(),
                ],
                Some(RuleHistory::new()),
                file_system.clone(),
                FakeMetadataGetter::new(),
            ),
            vec![(0, sender)],
            vec![],
            FakeExecutor::new(file_system.clone())
        );

        match handle1.join()
        {
            Ok(join_result) =>
            {
                match join_result
                {
                    Ok(work_result) =>
                    {
                        assert_eq!(work_result.target_infos[0].path, "stanza1.txt");

                        match work_result.rule_history
                        {
                            Some(_) => {},
                            None => panic!("Thread left with rule-history, came back with nothing."),
                        }

                        match work_result.work_option
                        {
                            WorkOption::CommandExecuted(output) =>
                            {
                                assert_eq!(output.out, "");
                            },
                            _ => panic!("Thread was supposed to execute command, did something else, got wrong work-option."),
                        }

                        assert_eq!(from_utf8(&file_system.read_file("stanza1.txt").unwrap()).unwrap(), "I wish I were a windowsill");
                        assert_eq!(from_utf8(&file_system.read_file("stanza2.txt").unwrap()).unwrap(), "I wish I were a windowsill");
                    },
                    Err(error) => panic!("Thread inside failed {}", error),
                }
            },

            Err(_) => panic!("Thread execution failed"),
        }
    }


    #[test]
    fn one_target_already_correct_only()
    {
        let file_system = FakeFileSystem::new();
        let metadata_getter1 = FakeMetadataGetter::new();
        let metadata_getter2 = FakeMetadataGetter::new();
        let mut rule_history = RuleHistory::new();

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

        match rule_history.insert(
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
                Some(rule_history),
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
