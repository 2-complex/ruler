extern crate filesystem;

use crate::ticket::{Ticket, TicketFactory};
use crate::rule::Record;
use crate::station::Station;

use filesystem::FileSystem;
use std::process::{Output, Command};
use std::sync::mpsc::{Sender, Receiver};
use std::str::from_utf8;
use std::collections::VecDeque;

pub struct CommandResult
{
    pub out : String,
    pub err : String,
    pub code : Option<i32>,
    pub success : bool,
}

impl CommandResult
{
    fn from_output(output: Output) -> CommandResult
    {
        CommandResult
        {
            out : match from_utf8(&output.stdout)
            {
                Ok(text) => text,
                Err(_) => "<non-utf8 data>",
            }.to_string(),

            err : match from_utf8(&output.stderr)
            {
                Ok(text) => text,
                Err(_) => "<non-utf8 data>",
            }.to_string(),

            code : output.status.code(),
            success : output.status.success(),
        }
    }

    fn new() -> CommandResult
    {
        CommandResult
        {
            out : "".to_string(),
            err : "".to_string(),
            code : Some(0),
            success : true,
        }
    }
}

pub fn do_command<FSType: FileSystem>(
    record: Record,
    senders : Vec<(usize, Sender<Ticket>)>,
    receivers : Vec<Receiver<Ticket>>,
    station : Station<FSType> )
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
        match station.get_target_ticket(target_path)
        {
            Ok(ticket) =>
            {
                target_tickets.push(ticket);
            },
            Err(why) => return Err(format!("TICKET ALIGNMENT ERROR {}", why)),
        }
    }

    let result =
    if station.remember_target_tickets(&factory.result()) != target_tickets
    {
        let mut command_queue = VecDeque::from(record.command.clone());
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
                    Err(why)=>
                    {
                        return Err(format!("Error in command to build: {}\n{}", record.targets.join(" "), why))
                    },
                }
            },
            None => Ok(CommandResult::new()),
        }
    }
    else
    {
        Ok(CommandResult::new())
    };

    for (sub_index, sender) in senders
    {
        match station.get_target_ticket(&record.targets[sub_index])
        {
            Ok(mut ticket) =>
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

    use filesystem::{FileSystem, FakeFileSystem};
    use std::path::Path;
    use std::sync::mpsc;

    #[test]
    fn do_empty_command()
    {
        let mut file_system = FakeFileSystem::new();
        file_system.write_file("A", "A-content");

        match do_command(
            Record
            {
                targets: vec!["A".to_string()],
                source_indices: vec![],
                ticket: TicketFactory::new().result(),
                command: vec![],
            },
            Vec::new(),
            Vec::new(),
            Station::new(file_system, RuleHistory::new()))
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

        file_system.write_file(Path::new(&"A.txt"), "");

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
                command: vec![],
            },
            vec![(0, sender_c)],
            vec![receiver_a, receiver_b],
            Station::new(file_system, RuleHistory::new()))
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
}
