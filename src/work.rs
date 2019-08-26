extern crate filesystem;

use crate::ticket::{Ticket, TicketFactory};
use crate::rule::Record;
use crate::station::Station;
use crate::executor::{CommandResult, Executor};

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
    fn execute_command(&self, command_list: Vec<String>) -> Result<CommandResult, String>
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
                    Ok(out) => Ok(CommandResult::from_output(out)),
                    Err(why) => Err(why.to_string()),
                }
            },
            None => Ok(CommandResult::new()),
        }
    }
}

pub fn do_command<FSType: FileSystem, ExecType: Executor>(
    record: Record,
    senders : Vec<(usize, Sender<Ticket>)>,
    receivers : Vec<Receiver<Ticket>>,
    station : Station<FSType>,
    executor: ExecType)
    -> Result<CommandResult, String>
{
    let mut factory = TicketFactory::new();

    for rcv in receivers
    {
        match rcv.recv()
        {
            Ok(ticket) => 
            {
                factory.input_ticket(ticket);
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
            Err(why) => Err(format!("Error in command to build: {}\n{}", record.targets.join(" "), why)),
        }
    }
    else
    {
        Ok(CommandResult::new())
    };

    for (sub_index, sender) in senders
    {
        match station.get_file_ticket(&record.targets[sub_index])
        {
            Ok(ticket) =>
            {
                match sender.send(ticket)
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
    use crate::executor::{Executor, CommandResult};

    use filesystem::{FileSystem, FakeFileSystem};
    use std::path::Path;
    use std::sync::mpsc;
    use std::str::from_utf8;

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
        fn execute_command(&self, command_list : Vec<String>) -> Result<CommandResult, String>
        {
            let n = command_list.len();
            let mut output = String::new();


            if n > 1
            {
                match command_list[0].as_str()
                {
                    "mycat" =>
                    {
                        println!("mycat something: {}", command_list[n-1]);
                        for f in command_list[1..(n-1)].iter()
                        {
                            match self.file_system.read_file(f)
                            {
                                Ok(content) =>
                                {
                                    match from_utf8(&content)
                                    {
                                        Ok(content_string) => output.push_str(content_string),
                                        Err(_) => return Err(format!("File contained non utf8 bytes: {}", f)),
                                    }
                                }
                                Err(_) => return Err(format!("File failed to open: {}", f)),
                            }
                        }

                        println!("about to concat into file: {}", command_list[n-1]);
                        match self.file_system.write_file(Path::new(&command_list[n-1]), output)
                        {
                            Ok(_) => Ok(CommandResult::new()),
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
                Ok(CommandResult::new())
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

        match sender_a.send(TicketFactory::from_str("apples").result())
        {
            Ok(p) => assert_eq!(p, ()),
            Err(e) => panic!("Unexpected error sending: {}", e),
        }

        match sender_b.send(TicketFactory::from_str("bananas").result())
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
                    Ok(ticket) =>
                    {
                        assert_eq!(ticket, TicketFactory::new().result());
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

        match sender_a.send(TicketFactory::from_str("Roses are red\n").result())
        {
            Ok(p) => assert_eq!(p, ()),
            Err(e) => panic!("Unexpected error sending: {}", e),
        }

        match sender_b.send(TicketFactory::from_str("Violets are violet\n").result())
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
                    Ok(ticket) =>
                    {
                        assert_eq!(
                            ticket,
                            TicketFactory::from_str("Roses are red\nViolets are violet\n").result());
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

        match sender_a.send(TicketFactory::from_str("Roses are red\n").result())
        {
            Ok(p) => assert_eq!(p, ()),
            Err(e) => panic!("Unexpected error sending: {}", e),
        }

        match sender_b.send(TicketFactory::from_str("Violets are violet\n").result())
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
                    Ok(ticket) =>
                    {
                        assert_eq!(
                            ticket,
                            TicketFactory::from_str("Roses are red\nViolets are violet\n").result());
                    }
                    Err(_) => panic!("Unexpected fail to receive"),
                }
            },
            Err(err) => panic!("Command failed: {}", err),
        }
    }
}
