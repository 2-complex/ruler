
use crate::ticket::Ticket;
use crate::system::
{
    CommandLineOutput,
    ReadWriteError,
    System,
    SystemError,
};
use crate::history::
{
    RuleHistory,
    DownloaderRuleHistory,
    RuleHistoryInsertError,
};
use crate::blob::
{
    Blob,
    GetFileStateError,
    FileStateVec,
    FileResolution,
    ResolutionError,
    GetCurrentFileInfoError,
    get_file_ticket,
};
use crate::cache::
{
    SysCache,
    DownloaderCache,
};

use std::fmt;

#[derive(Debug)]
pub enum WorkOption
{
    SourceOnly,
    Resolutions(Vec<FileResolution>),
    CommandExecuted(CommandLineOutput),
}

#[derive(Debug)]
pub struct WorkResult
{
    pub file_state_vec : FileStateVec,
    pub blob : Blob,
    pub work_option : WorkOption,
    pub rule_history : Option<RuleHistory>,
}

#[derive(Debug)]
pub enum WorkError
{
    TicketAlignmentError(ReadWriteError),
    FileNotFound(String),
    TargetFileNotGenerated(String),
    FileNotAvailableToCache(String, ReadWriteError),
    ReadWriteError(String, ReadWriteError),
    ResolutionError(ResolutionError),
    GetCurrentFileInfoError(GetCurrentFileInfoError),
    CommandExecutedButErrored,
    CommandFailedToExecute(SystemError),
    Contradiction(Vec<String>),
    Weird,
}

impl fmt::Display for WorkError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            WorkError::TicketAlignmentError(error) =>
                write!(formatter, "File IO error when attempting to get hash of sources: {}", error),

            WorkError::FileNotFound(path) =>
                write!(formatter, "File not found: {}", path),

            WorkError::TargetFileNotGenerated(path) =>
                write!(formatter, "Target file missing after running build command: {}", path),

            WorkError::FileNotAvailableToCache(path, error) =>
                write!(formatter, "File not available to be cached: {} : {}", path, error),

            WorkError::ReadWriteError(path, error) =>
                write!(formatter, "Error reading file: {}: {}", path, error),

            WorkError::ResolutionError(error) =>
                write!(formatter, "Error resolving rule: {}", error),

            WorkError::GetCurrentFileInfoError(error) =>
                write!(formatter, "Error getting ticket and timestamp: {}", error),

            WorkError::CommandExecutedButErrored =>
                write!(formatter, "Command executed but errored"),

            WorkError::CommandFailedToExecute(error) =>
                write!(formatter, "Failed to execute command: {}", error),

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

            WorkError::Weird =>
                write!(formatter, "Weird! How did you do that!"),
        }
    }
}

pub fn handle_source_only_node<SystemType: System>
(
    system : SystemType,
    blob : Blob
)
->
Result<WorkResult, WorkError>
{
    let current_file_state_vec =
    match blob.get_current_file_state_vec(&system)
    {
        Ok(tickets) => tickets,
        Err(GetFileStateError::FileNotFound(path)) => return Err(WorkError::FileNotFound(path)),
        Err(GetFileStateError::ReadWriteError(path, error)) => return Err(WorkError::ReadWriteError(path, error)),
    };

    Ok(
        WorkResult
        {
            file_state_vec : current_file_state_vec,
            blob : blob,
            work_option : WorkOption::SourceOnly,
            rule_history : None
        }
    )
}

/*  Takes a vector of resolutions, and returns true if any of them are NeedsRebuild */
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

/*  Handles the case where at least one target is irrecoverable and therefore the command
    needs to execute to rebuild the node.  When successful, returns a WorkResult with option
    indicating that the command executed (WorkResult contains the commandline result) */
fn rebuild_node<SystemType : System>
(
    system : &mut SystemType,
    mut rule_history : RuleHistory,
    sources_ticket : Ticket,
    command : Vec<String>,
    mut blob : Blob
)
->
Result<WorkResult, WorkError>
{
    let command_result =
    match system.execute_command(command)
    {
        Ok(command_result) => command_result,
        Err(error) =>
        {
            return Err(WorkError::CommandFailedToExecute(error));
        },
    };

    if ! command_result.success
    {
        return Err(WorkError::CommandExecutedButErrored);
    }

    let file_state_vec =
    match blob.update_to_match_system_file_state(system)
    {
        Ok(file_state_vec) => file_state_vec,
        Err(GetCurrentFileInfoError::TargetFileNotFound(path, _system_error)) => return Err(WorkError::TargetFileNotGenerated(path)),
        Err(error) => return Err(WorkError::GetCurrentFileInfoError(error)),
    };

    match rule_history.insert(sources_ticket, file_state_vec.clone())
    {
        Ok(_) => {},
        Err(error) =>
        {
            match error
            {
                RuleHistoryInsertError::Contradiction(contradicting_indices) =>
                {
                    let mut contradicting_target_paths = Vec::new();
                    let paths = blob.get_paths();
                    for index in contradicting_indices
                    {
                        contradicting_target_paths.push(paths[index].clone());
                    }
                    return Err(WorkError::Contradiction(contradicting_target_paths));
                }

                RuleHistoryInsertError::TargetSizesDifferWeird =>
                    return Err(WorkError::Weird),
            }
        },
    }

    Ok(
        WorkResult
        {
            file_state_vec : file_state_vec,
            blob : blob,
            work_option : WorkOption::CommandExecuted(command_result),
            rule_history : Some(rule_history),
        }
    )
}

/*  Takes a target blob and attempts to resolve the targets using cache or download-urls.

    If there are target tickets in the rule-history, then this function appeals to resolve_single_target
    to try to retrieve a backup copy either from the local cache or from the internet (backing up the current copy
    of each target as it goes)

    If there are no remembered tickets, then this function goes through each target, backs up the current version
    if it's there, and returns a vector full of NeedsRebuild */
fn resolve_with_cache<SystemType : System>
(
    system : &mut SystemType,
    cache : &mut SysCache<SystemType>,
    downloader_cache_opt : &Option<DownloaderCache>,
    rule_history : &RuleHistory,
    downloader_rule_history_opt : &Option<DownloaderRuleHistory>,
    sources_ticket : &Ticket,
    blob : &Blob,
)
->
Result<Vec<FileResolution>, WorkError>
{
    match rule_history.get_file_state_vec(sources_ticket)
    {
        Some(remembered_file_state_vec) =>
        {
            return match blob.resolve_remembered_file_state_vec(
                system, cache, downloader_cache_opt, remembered_file_state_vec)
            {
                Ok(file_resolution) => Ok(file_resolution),
                Err(resolution_error) => Err(WorkError::ResolutionError(resolution_error)),
            };
        },

        None => {},
    }

    match downloader_rule_history_opt
    {
        Some(downloader_rule_history) =>
        {
            match downloader_rule_history.get_file_state_vec(sources_ticket)
            {
                Some(file_state_vec) =>
                {
                    return match blob.resolve_remembered_file_state_vec(
                        system, cache, downloader_cache_opt, &file_state_vec)
                    {
                        Ok(file_resolution) => Ok(file_resolution),
                        Err(resolution_error) => Err(WorkError::ResolutionError(resolution_error)),
                    };
                },

                None => {},
            }
        },

        None => {},
    }

    match blob.resolve_with_no_current_file_states(system, cache)
    {
        Ok(resolutions) => Ok(resolutions),
        Err(resolution_error) => Err(WorkError::ResolutionError(resolution_error)),
    }
}

pub struct RuleExt<SystemType: System>
{
    pub sources_ticket : Ticket,
    pub command : Vec<String>,
    pub rule_history : RuleHistory,
    pub cache : SysCache<SystemType>,
    pub downloader_cache_opt : Option<DownloaderCache>,
    pub downloader_rule_history_opt : Option<DownloaderRuleHistory>,
}

impl<SystemType: System> RuleExt<SystemType>
{
    #[cfg(test)]
    fn new(cache : SysCache<SystemType>, sources_ticket : Ticket) -> RuleExt<SystemType>
    {
        return RuleExt
        {
            cache : cache,
            sources_ticket : sources_ticket,
            command : Vec::new(),
            rule_history : RuleHistory::new(),
            downloader_cache_opt : None,
            downloader_rule_history_opt : None,
        };
    }
}

pub struct HandleNodeInfo<SystemType: System>
{
    pub system : SystemType,
    pub blob : Blob,
}

impl<SystemType: System> HandleNodeInfo<SystemType>
{
    pub fn new(system : SystemType) -> HandleNodeInfo<SystemType>
    {
        HandleNodeInfo
        {
            system : system,
            blob : Blob::empty(),
        }
    }
}

/*  This is a central, public function for handling a node in the depednece graph.
    It is meant to be called by a dedicated thread, and as such, it eats all its arguments.

    The RuleHistory gets modified when appropriate, and gets returned as part of the result.

    The possible parameters to this function are so many that they warrant a dedicated struct:
    HandleNodeInfo.
*/
pub fn handle_rule_node<SystemType: System>
(
    mut info : HandleNodeInfo<SystemType>,
    mut rule_ext : RuleExt<SystemType>,
)
->
Result<WorkResult, WorkError>
{
    match resolve_with_cache(
        &mut info.system,
        &mut rule_ext.cache,
        & rule_ext.downloader_cache_opt,
        & rule_ext.rule_history,
        & rule_ext.downloader_rule_history_opt,
        & rule_ext.sources_ticket,
        & info.blob)
    {
        Ok(resolutions) =>
        {
            if needs_rebuild(&resolutions)
            {
                rebuild_node(
                    &mut info.system,
                    rule_ext.rule_history,
                    rule_ext.sources_ticket,
                    rule_ext.command,
                    info.blob)
            }
            else
            {
                let file_state_vec = match info.blob.get_current_file_state_vec(&info.system)
                {
                    Ok(file_state_vec) => file_state_vec,
                    Err(GetFileStateError::FileNotFound(path)) => return Err(WorkError::FileNotFound(path)),
                    Err(GetFileStateError::ReadWriteError(path, error)) => return Err(WorkError::ReadWriteError(path, error)),
                };

                Ok(
                    WorkResult
                    {
                        file_state_vec : file_state_vec,
                        blob : info.blob,
                        work_option : WorkOption::Resolutions(resolutions),
                        rule_history : Some(rule_ext.rule_history),
                    }
                )
            }
        },

        Err(error) => Err(error),
    }
}

pub fn clean_targets<SystemType: System>
(
    blob : Blob,
    system : &mut SystemType,
    cache : &mut SysCache<SystemType>
)
-> Result<(), WorkError>
{
    for target_info in blob.get_file_infos()
    {
        if system.is_file(&target_info.path)
        {
            match get_file_ticket(system, &target_info.path, &target_info.file_state)
            {
                Ok(Some(current_target_ticket)) =>
                {
                    {
                        match cache.back_up_file_with_ticket(
                            &current_target_ticket,
                            &target_info.path)
                        {
                            Ok(_) => {},
                            Err(error) =>
                                return Err(WorkError::FileNotAvailableToCache(
                                    target_info.path.clone(), error)),
                        }
                    }
                },
                Ok(None)=>
                {
                    match cache.back_up_file(
                        &target_info.path)
                    {
                        Ok(_) => {},
                        Err(error) =>
                            return Err(WorkError::FileNotAvailableToCache(
                                target_info.path.clone(), error)),
                    }
                },
                Err(error) => return Err(WorkError::TicketAlignmentError(error)),
            }
        }
    }

    Ok(())
}


#[cfg(test)]
mod test
{
    use crate::work::
    {
        FileResolution,
        WorkOption,
        WorkError,
        HandleNodeInfo,
        RuleExt,
        handle_source_only_node,
        handle_rule_node,
    };
    use crate::ticket::
    {
        TicketFactory,
        Ticket,
    };
    use crate::history::
    {
        RuleHistory,
    };
    use crate::blob::
    {
        Blob,
        FileState,
        FileStateVec,
        ResolutionError,
        get_file_ticket
    };
    use crate::cache::
    {
        SysCache,
    };
    use crate::system::util::
    {
        write_str_to_file,
        read_file_to_string,
    };
    use crate::system::
    {
        System,
        fake::FakeSystem,
    };

    /*  For testing, it's useful to be able to check the ticket of a list of source files. */
    fn current_sources_ticket
    <
        SystemType : System,
    >
    (
        system : &SystemType,
        paths : Vec<&str>
    )
    -> Result<Ticket, WorkError>
    {
        let mut factory = TicketFactory::new();

        for path in paths
        {
            factory.input_ticket(
                match TicketFactory::from_file(system, path)
                {
                    Ok(mut file_factory) => file_factory.result(),
                    Err(error) => return Err(WorkError::ReadWriteError(path.to_string(), error)),
                });
        }
        Ok(factory.result())
    }

    /*  Create a rule-history and populate it simulating a game having been built from a
        single C++ source file.  Get the file-ticket for the source, use that ticket to
        get a target ticket from the rule-history, and check it is what's expected. */
    #[test]
    fn work_get_tickets_from_history()
    {
        let mut rule_history = RuleHistory::new();
        let mut system = FakeSystem::new(10);

        let source_content = "int main(){printf(\"my game\"); return 0;}";
        let target_content = "machine code for my game";

        let mut source_factory = TicketFactory::new();
        source_factory.input_ticket(TicketFactory::from_str(source_content).result());

        // Make rule history remembering that the source c++ code built
        // to the target executable.
        rule_history.insert(
            source_factory.result(),
            FileStateVec::from_ticket_vec(vec![TicketFactory::from_str(target_content).result()])).unwrap();

        // Meanwhile, in the filesystem put some rubbish in game.cpp
        match write_str_to_file(&mut system, "game.cpp", source_content)
        {
            Ok(_) => {},
            Err(why) => panic!("Failed to make fake file: {}", why),
        }

        // Then get the file-ticket for the current source file:
        match get_file_ticket(
            &system,
            "game.cpp",
            &FileState::new_with_ticket(TicketFactory::new().result()))
        {
            Ok(ticket_opt) =>
            {
                match ticket_opt
                {
                    Some(ticket) =>
                    {
                        // Make sure it matches the content of the file that we wrote
                        assert_eq!(ticket, TicketFactory::from_str(source_content).result());

                        // Then create a source ticket for all (one) sources
                        let mut source_factory = TicketFactory::new();
                        source_factory.input_ticket(ticket);
                        let source_ticket = source_factory.result();

                        // Remember what the target tickets were when built with that source before:
                        let file_state_vec =
                        match rule_history.get_file_state_vec(&source_ticket)
                        {
                            Some(file_state_vec) => file_state_vec,
                            None => panic!("Tickets not in history as expected"),
                        };

                        // Check that the target tickets in the history match the ones for the target
                        assert_eq!(
                            *file_state_vec,
                            FileStateVec::from_ticket_vec(vec![
                                TicketFactory::from_str(target_content).result()
                            ])
                        );
                    },
                    None => panic!("No ticket found where expected"),
                }
            }
            Err(err) => panic!("Could not get ticket: {}", err),
        }
    }

    /*  Helper function to make a HandleNodeInfo with default FileStates given a system and
        a list of paths. */
    fn make_handle_node_info(system : FakeSystem, paths : Vec<String>) -> HandleNodeInfo<FakeSystem>
    {
        let mut info = HandleNodeInfo::new(system);
        info.blob = Blob::from_paths(paths, |_path|{FileState::empty()});
        info
    }

    /*  Call handle_rule_node with minimal connections and the empty list as a command. */
    #[test]
    fn do_empty_command()
    {
        let mut system = FakeSystem::new(10);
        write_str_to_file(&mut system, "A", "A-content").unwrap();
        system.create_dir(".ruler-cache").unwrap();

        let mut ticket_factory = TicketFactory::new();
        ticket_factory.input_ticket(TicketFactory::from_str("A-content").result());

        let mut rule_ext = RuleExt::new(SysCache::new(system.clone(), ".ruler-cache"), ticket_factory.result());
        rule_ext.command = vec![];

        match handle_rule_node(make_handle_node_info(system.clone(), vec![]), rule_ext)
        {
            Ok(result) =>
            {
                match result.work_option
                {
                    WorkOption::Resolutions(resolutions) => assert_eq!(resolutions.len(), 0),
                    _ => panic!("Wrong kind of WorkOption in result when command empty"),
                }
            },
            Err(error) => panic!("Command failed: {}", error),
        }
    }

    #[test]
    fn work_handle_rule_node_command_executed()
    {
        let mut system = FakeSystem::new(10);

        system.create_dir(".ruler-cache").unwrap();
        write_str_to_file(&mut system, "A-source.txt", "").unwrap();
        write_str_to_file(&mut system, "A.txt", "").unwrap();

        let mut ticket_factory = TicketFactory::new();
        ticket_factory.input_ticket(TicketFactory::from_str("apples").result());
        ticket_factory.input_ticket(TicketFactory::from_str("bananas").result());

        let mut rule_ext = RuleExt::new(SysCache::new(system.clone(), ".ruler-cache"), ticket_factory.result());
        rule_ext.command = vec!["mycat".to_string(), "A-source.txt".to_string(), "A.txt".to_string()];

        match handle_rule_node(make_handle_node_info(system.clone(), vec!["A.txt".to_string()]), rule_ext)
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
            },
            Err(err) => panic!("Command failed: {}", err),
        }
    }


    #[test]
    fn work_command_errors()
    {
        let system = FakeSystem::new(10);

        let mut ticket_factory = TicketFactory::new();
        ticket_factory.input_ticket(TicketFactory::from_str("Roses are red\n").result());
        ticket_factory.input_ticket(TicketFactory::from_str("Violets are violet\n").result());

        let mut rule_ext = RuleExt::new(SysCache::new(system.clone(), ".ruler-cache"), ticket_factory.result());
        rule_ext.command = vec!["error".to_string()];

        match handle_rule_node(make_handle_node_info(system.clone(), vec!["poem.txt".to_string()]), rule_ext)
        {
            Ok(_) => panic!("Unexpected command success"),
            Err(WorkError::CommandExecutedButErrored) => {},
            Err(error) => panic!("Wrong kind of error when command errors: {}", error),
        }
    }


    #[test]
    fn command_fails_to_generate_target()
    {
        let mut system = FakeSystem::new(10);

        write_str_to_file(&mut system, "verse1.txt", "Roses are red\n").unwrap();
        write_str_to_file(&mut system, "verse2.txt", "Violets are violet\n").unwrap();

        let mut ticket_factory = TicketFactory::new();
        ticket_factory.input_ticket(TicketFactory::from_str("Roses are red\n").result());
        ticket_factory.input_ticket(TicketFactory::from_str("Violets are violet\n").result());

        let mut rule_ext = RuleExt::new(SysCache::new(system.clone(), ".ruler-cache"), ticket_factory.result());
        rule_ext.command = vec!["mycat".to_string(),"verse1.txt".to_string(),"verse2.txt".to_string(),"wrong.txt".to_string()];

        match handle_rule_node(make_handle_node_info(system.clone(), vec!["poem.txt".to_string()]), rule_ext)
        {
            Ok(_) => panic!("Unexpected command success"),
            Err(WorkError::TargetFileNotGenerated(path)) =>
            {
                assert_eq!(path, "poem.txt");
            },
            Err(error) => panic!("Wrong kind of error when command errors: {}", error),
        }
    }

    #[test]
    fn poem_concatination()
    {
        let mut system = FakeSystem::new(10);
        write_str_to_file(&mut system, "verse1.txt", "Roses are red\n").unwrap();
        write_str_to_file(&mut system, "verse2.txt", "Violets are violet\n").unwrap();

        let mut ticket_factory = TicketFactory::new();
        ticket_factory.input_ticket(TicketFactory::from_str("Roses are red\n").result());
        ticket_factory.input_ticket(TicketFactory::from_str("Violets are violet\n").result());

        let mut rule_ext = RuleExt::new(SysCache::new(system.clone(), ".ruler-cache"), ticket_factory.result());
        rule_ext.command = vec!["mycat".to_string(),"verse1.txt".to_string(),"verse2.txt".to_string(),"poem.txt".to_string()];

        match handle_rule_node(make_handle_node_info(system.clone(), vec!["poem.txt".to_string()]), rule_ext)
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

                        let content = read_file_to_string(&system, "poem.txt").unwrap();
                        assert_eq!(content, "Roses are red\nViolets are violet\n");
                    },
                    _ => panic!("Comand was supposed to execute.  Output wasn't there."),
                }
            },
            Err(err) => panic!("Command failed: {}", err),
        }
    }


    /*  Create sourcefiles with the two lines in a two-line poem.  Fabricate a rule history
        that recalls the two lines concatinated as the result.  In the fake file system, make
        the source files and create poem with already correct content.  Then handle the node.
        Check that handle_rule_node behaves as if the poem is already correct.
    */
    #[test]
    fn poem_already_correct()
    {
        let mut rule_history = RuleHistory::new();

        let mut factory = TicketFactory::new();
        factory.input_ticket(TicketFactory::from_str("Roses are red\n").result());
        factory.input_ticket(TicketFactory::from_str("Violets are violet\n").result());
        let sources_ticket = factory.result();

        match rule_history.insert(
            sources_ticket.clone(),
            FileStateVec::from_ticket_vec(vec![
                TicketFactory::from_str("Roses are red\nViolets are violet\n").result()
            ])
        )
        {
            Ok(_) => {},
            Err(_) => panic!("Rule history failed to insert"),
        }

        let mut system = FakeSystem::new(10);

        system.create_dir(".ruler-cache").unwrap();
        write_str_to_file(&mut system, "verse1.txt", "Roses are red\n").unwrap();
        write_str_to_file(&mut system, "verse2.txt", "Violets are violet\n").unwrap();
        write_str_to_file(&mut system, "poem.txt", "Roses are red\nViolets are violet\n").unwrap();

        let mut rule_ext = RuleExt::new(SysCache::new(system.clone(), ".ruler-cache"), sources_ticket);
        rule_ext.command = vec!["mycat".to_string(), "verse1.txt".to_string(), "verse2.txt".to_string(), "poem.txt".to_string()];
        rule_ext.rule_history = rule_history;

        match handle_rule_node(make_handle_node_info(system.clone(), vec!["poem.txt".to_string()]), rule_ext)
        {
            Ok(result) =>
            {
                match result.work_option
                {
                    WorkOption::Resolutions(resolutions) =>
                    {
                        assert_eq!(resolutions.len(), 1);

                        match resolutions[0]
                        {
                            FileResolution::AlreadyCorrect => {},
                            _ => panic!("Expected poem to already be correct, was some other work option"),
                        }
                    },
                    _ => panic!("Expected poem to already be resolved, was some other work option"),
                }
            },
            Err(err) => panic!("Command failed: {}", err),
        }
    }


    /*  Create source files for a two-line poem.  Fabricate a hisotry which describes
        a different result when the two source files are compiled.

        Populate poem.txt with arbitrary content, so that it mush rebuild.  Send the
        tickets for the file-system contents of the source files through the channels.

        Check that handle_rule_node produces the correct contradiction error.
    */
    #[test]
    fn poem_contradicts_history()
    {
        let mut factory = TicketFactory::new();
        factory.input_ticket(TicketFactory::from_str("Roses are red\n").result());
        factory.input_ticket(TicketFactory::from_str("Violets are violet\n").result());
        let sources_ticket = factory.result();

        let mut rule_history = RuleHistory::new();
        match rule_history.insert(
            sources_ticket.clone(),
            FileStateVec::from_ticket_vec(
                vec![TicketFactory::from_str("Roses are red\nViolets are blue\n").result()]
            ))
        {
            Ok(_) => {},
            Err(_) => panic!("Rule history failed to insert"),
        }

        let mut system = FakeSystem::new(10);

        system.create_dir(".ruler-cache").unwrap();
        write_str_to_file(&mut system, "verse1.txt", "Roses are red\n").unwrap();
        write_str_to_file(&mut system, "verse2.txt", "Violets are violet\n").unwrap();
        write_str_to_file(&mut system, "poem.txt", "Arbitrary content").unwrap();

        let mut rule_ext = RuleExt::new(SysCache::new(system.clone(), ".ruler-cache"), sources_ticket);
        rule_ext.command = vec!["mycat".to_string(), "verse1.txt".to_string(), "verse2.txt".to_string(), "poem.txt".to_string()];
        rule_ext.rule_history = rule_history;

        match handle_rule_node(make_handle_node_info(system.clone(), vec!["poem.txt".to_string()]), rule_ext)
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

    /*  Build a poem by concatinating two verses.  When the build succeeds (panic if it does not)
        check that the rule history has a new pair in it with the source-ticket and target ticket according
        to what was built. */
    #[test]
    fn poem_work_populates_rule_history()
    {
        let mut system = FakeSystem::new(10);

        system.create_dir(".ruler-cache").unwrap();
        write_str_to_file(&mut system, "verse1.txt", "Roses are red\n").unwrap();
        write_str_to_file(&mut system, "verse2.txt", "Violets are violet\n").unwrap();
        write_str_to_file(&mut system, "poem.txt", "Arbitrary content").unwrap();

        let mut factory = TicketFactory::new();
        factory.input_ticket(TicketFactory::from_str("Roses are red\n").result());
        factory.input_ticket(TicketFactory::from_str("Violets are violet\n").result());
        let sources_ticket = factory.result();

        let mut rule_ext = RuleExt::new(SysCache::new(system.clone(), ".ruler-cache"), sources_ticket);
        rule_ext.command = vec!["mycat".to_string(), "verse1.txt".to_string(), "verse2.txt".to_string(), "poem.txt".to_string()];
        rule_ext.rule_history = RuleHistory::new();

        match handle_rule_node(make_handle_node_info(system.clone(), vec!["poem.txt".to_string()]), rule_ext)
        {
            Ok(result) =>
            {
                match result.work_option
                {
                    WorkOption::CommandExecuted(_command_result) =>
                    {
                    },
                    _ => panic!("Wrong kind of WorkOption"),
                }

                let source_ticket = 
                match current_sources_ticket(
                    &system,
                    vec!["verse1.txt", "verse2.txt"])
                {
                    Ok(ticket) => ticket,
                    Err(error) => panic!("Expected ticket, got error: {}", error),
                };

                match result.rule_history
                {
                    Some(rule_history) => 
                    {
                        let file_state_vec = rule_history.get_file_state_vec(&source_ticket).unwrap();
                        assert_eq!(
                            *file_state_vec,
                            FileStateVec::from_ticket_vec(vec![
                                TicketFactory::from_str("Roses are red\nViolets are violet\n").result()
                            ])
                        );
                    },
                    None => panic!("Expected RuleHistory, got none"),
                }

            },
            Err(error) =>  panic!("Unexpected error: {}", error),
        }
    }


    /*  Make a source-only node describing a source file that does not exist in the filesystem.
        Check for a file-not-found error. */
    #[test]
    fn source_only_file_not_there()
    {
        let mut system = FakeSystem::new(10);

        match write_str_to_file(&mut system, "some-other-file.txt", "Arbitrary content\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match handle_source_only_node(system, Blob::from_paths(
            vec!["verse1.txt".to_string()],
            |_path|{FileState::empty()}
        ))
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


    /*  Contruct a rule with one target, except instead of building that target, the rule
        contains a commandline invocation that deletes it.  Check this produces an appropriate error. */
    #[test]
    fn target_removed_by_command()
    {
        let mut system = FakeSystem::new(10);

        write_str_to_file(&mut system, "verse1.txt", "Arbitrary content\n").unwrap();

        let mut rule_ext = RuleExt::new(SysCache::new(system.clone(), ".rule-cache"), TicketFactory::new().result());
        rule_ext.command = vec!["rm".to_string(), "verse1.txt".to_string()];

        match handle_rule_node(make_handle_node_info(system.clone(), vec!["verse1.txt".to_string()]), rule_ext)
        {
            Ok(_) =>
            {
                panic!("Expected failure when file not present")
            },
            Err(error) =>
            {
                match error
                {
                    WorkError::ResolutionError(ResolutionError::FileNotAvailableToCache(path, _error)) => assert_eq!(path, "verse1.txt"),
                    _ => panic!("Wrong kind of error!  Incorrect error: {}", error),
                }
            },
        }
    }

    /*  Use the fake command mycat2 to generate a poem and a copy of that poem.  Put one poem in place, with incorrect
        content.  Handle the node.  Check for the presence of both poems and check the command logs  */
    #[test]
    fn two_targets_one_already_present()
    {
        let mut system = FakeSystem::new(10);

        system.create_dir(".ruler-cache").unwrap();
        write_str_to_file(&mut system, "verse1.txt", "Roses are red\n").unwrap();
        write_str_to_file(&mut system, "verse2.txt", "Violets are blue\n").unwrap();
        write_str_to_file(&mut system, "poem_copy.txt", "Arbitrary content").unwrap();

        let mut factory = TicketFactory::new();
        factory.input_ticket(TicketFactory::from_str("Roses are red\n").result());
        factory.input_ticket(TicketFactory::from_str("Violets are blue\n").result());
        let sources_ticket = factory.result();

        let mut rule_ext = RuleExt::new(SysCache::new(system.clone(), ".ruler-cache"), sources_ticket);
        rule_ext.command = vec![
            "mycat2".to_string(),
            "verse1.txt".to_string(),
            "verse2.txt".to_string(),
            "poem.txt".to_string(),
            "poem_copy.txt".to_string()];
        rule_ext.rule_history = RuleHistory::new();

        match handle_rule_node(make_handle_node_info(system.clone(),
            vec!["poem.txt".to_string(), "poem_copy.txt".to_string()]), rule_ext)
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
            },
            Err(err) => panic!("Command failed: {}", err),
        }

        assert_eq!(read_file_to_string(&system, "poem.txt").unwrap(),
            "Roses are red\nViolets are blue\n");
        assert_eq!(read_file_to_string(&system, "poem_copy.txt").unwrap(),
            "Roses are red\nViolets are blue\n");

        let command_log = system.get_command_log();
        assert_eq!(command_log.len(), 1);
        assert_eq!(command_log[0], "mycat2 verse1.txt verse2.txt poem.txt poem_copy.txt");
    }


    #[test]
    fn one_target_already_correct_only()
    {
        let mut system = FakeSystem::new(10);

        system.create_dir(".ruler-cache").unwrap();
        write_str_to_file(&mut system, "verse1.txt", "Roses are red\n").unwrap();
        write_str_to_file(&mut system, "verse2.txt", "Violets are blue\n").unwrap();
        write_str_to_file(&mut system, "poem.txt", "Roses are red\nViolets are blue\n").unwrap();

        let mut factory = TicketFactory::new();
        factory.input_ticket(TicketFactory::from_str("Roses are red\n").result());
        factory.input_ticket(TicketFactory::from_str("Violets are blue\n").result());
        let sources_ticket = factory.result();

        let mut rule_ext = RuleExt::new(SysCache::new(system.clone(), ".ruler-cache"), sources_ticket);
        rule_ext.command = vec!["mycat".to_string(), "verse1.txt".to_string(), "verse2.txt".to_string(), "poem.txt".to_string()];
        rule_ext.rule_history = RuleHistory::new();

        match handle_rule_node(make_handle_node_info(system.clone(), vec![
            "poem.txt".to_string()
        ]), rule_ext)
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
            },
            Err(err) => panic!("Command failed: {}", err),
        }

        assert_eq!(read_file_to_string(&system, "poem.txt").unwrap(),
            "Roses are red\nViolets are blue\n");
    }


    /*  One file depends on one file, but the command errors.  Check that the target does not
        appear after the build step. */
    #[test]
    fn one_dependence_with_error()
    {
        let mut system = FakeSystem::new(10);

        system.create_dir(".ruler-cache").unwrap();
        write_str_to_file(&mut system, "verse1.txt", "Roses are red\n").unwrap();
        write_str_to_file(&mut system, "poem.txt", "Roses are red\n").unwrap();

        let mut factory = TicketFactory::new();
        factory.input_ticket(TicketFactory::from_str("Roses are red\n").result());
        let sources_ticket = factory.result();

        assert_eq!(system.is_file("poem.txt"), true);

        let mut rule_ext = RuleExt::new(SysCache::new(system.clone(), ".ruler-cache"), sources_ticket);
        rule_ext.command = vec!["error".to_string()];

        match handle_rule_node(make_handle_node_info(system.clone(), vec![
            "poem.txt".to_string()
        ]), rule_ext)
        {
            Ok(result) =>
            {
                match result.work_option
                {
                    WorkOption::CommandExecuted(_output) => panic!("Unexpected success"),
                    _ => panic!("Wrong type of work option.  Command was supposed to execute."),
                }
            },
            Err(WorkError::CommandExecutedButErrored) => {},
            Err(err) => panic!("Error of wrong type: {}", err),
        }

        /*  The files we tried to build should not be there. */
        assert_eq!(system.is_file("poem.txt"), false);
    }

    /*  Poem with two target files, but there is a mistake in the command, and it produces an error
        instead of building the targets.  Run the build command and check the error.  Also confirm
        that the target already present got moved into the cache. */
    #[test]
    fn one_target_not_there_error_in_command()
    {
        let mut system = FakeSystem::new(10);

        system.create_dir(".ruler-cache").unwrap();
        write_str_to_file(&mut system, "verse1.txt", "Roses are red\n").unwrap();
        write_str_to_file(&mut system, "verse2.txt", "Violets are blue\n").unwrap();
        write_str_to_file(&mut system, "poem.txt", "Roses are red\nViolets are blue\n").unwrap();

        let mut factory = TicketFactory::new();
        factory.input_ticket(TicketFactory::from_str("Roses are red\n").result());
        factory.input_ticket(TicketFactory::from_str("Violets are blue\n").result());
        let sources_ticket = factory.result();

        assert_eq!(system.is_file("poem.txt"), true);
        assert_eq!(system.is_file("poem_copy.txt"), false);

        let cache = SysCache::new(system.clone(), ".ruler-cache");

        let mut rule_ext = RuleExt::new(cache.clone(), sources_ticket);
        rule_ext.command = vec!["error".to_string()];

        match handle_rule_node(make_handle_node_info(system.clone(), vec![
            "poem.txt".to_string(),
            "poem_copy.txt".to_string()
        ]), rule_ext)
        {
            Ok(result) =>
            {
                match result.work_option
                {
                    WorkOption::CommandExecuted(_output) => panic!("Unexpected success"),
                    _ => panic!("Wrong type of work option.  Command was supposed to execute."),
                }
            },
            Err(WorkError::CommandExecutedButErrored) => {},
            Err(err) => panic!("Error of wrong type: {}", err),
        }

        /*  The files we tried to build should not be there. */
        assert_eq!(system.is_file("poem.txt"), false);
        assert_eq!(system.is_file("poem_copy.txt"), false);

        /*  The file that was there should move into the cache. */
        cache.open(&TicketFactory::from_str("Roses are red\nViolets are blue\n").result()).unwrap();
    }


    #[test]
    fn one_target_already_correct_according_to_timestamp()
    {
        let mut rule_history = RuleHistory::new();

        let mut factory = TicketFactory::new();
        factory.input_ticket(TicketFactory::from_str("Roses are red\n").result());
        factory.input_ticket(TicketFactory::from_str("Violets are violet\n").result());
        let sources_ticket = factory.result();

        rule_history.insert(
            sources_ticket.clone(),
            FileStateVec::from_ticket_vec(vec![
                TicketFactory::from_str("Roses are red\nViolets are violet\n").result()
            ])
        ).unwrap();

        let mut system = FakeSystem::new(19);

        system.create_dir(".ruler-cache").unwrap();
        write_str_to_file(&mut system, "verse1.txt", "Roses are red\n").unwrap();
        write_str_to_file(&mut system, "verse2.txt", "Violets are violet\n").unwrap();
        write_str_to_file(&mut system, "poem.txt", "Content wrong\n").unwrap();

        system.time_passes(1);

        let mut rule_ext = RuleExt::new(SysCache::new(system.clone(), ".ruler-cache"), sources_ticket);
        rule_ext.command = vec!["mycat".to_string(), "verse1.txt".to_string(), "verse2.txt".to_string(), "poem.txt".to_string()];
        rule_ext.rule_history = rule_history;

        let mut info = HandleNodeInfo::new(system.clone());
        info.blob = Blob::from_paths(
            vec!["poem.txt".to_string()], |_path|
            {
                FileState::new(
                    TicketFactory::from_str("Roses are red\nViolets are violet\n").result(),
                    19,
                )
            });

        match handle_rule_node(info, rule_ext)
        {
            Ok(result) =>
            {
                match result.work_option
                {
                    WorkOption::Resolutions(resolutions) =>
                    {
                        assert_eq!(resolutions.len(), 1);

                        match resolutions[0]
                        {
                            FileResolution::AlreadyCorrect => {},
                            _ => panic!("Expected poem to already be correct, was some other work option"),
                        }
                    },
                    _ => panic!("Expected poem to already be resolved, was: {:?}", result.work_option),
                }
            },
            Err(err) => panic!("Command failed: {}", err),
        }
    }

    #[test]
    fn one_target_correct_hash_incorrect_timestamp()
    {
    }
}
