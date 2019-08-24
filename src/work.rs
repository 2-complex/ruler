use crate::ticket::{Ticket, TicketFactory};
use crate::rule::Record;

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

pub struct Station
{
}

impl Station
{
    fn remember_target_tickets(&self, _source_ticket : &Ticket) -> Vec<Ticket>
    {
        vec![TicketFactory::new().result()]
    }

    fn get_target_ticket(&self, _target_path : &str) -> Ticket
    {
        TicketFactory::from_str("abc").result()
    }
}

pub fn do_command(
    record: Record,
    senders : Vec<(usize, Sender<Ticket>)>,
    receivers : Vec<Receiver<Ticket>>,
    station : Station )
    -> Result<CommandResult, String>
{
    let mut factory = TicketFactory::new();

    for rcv in receivers
    {
        match rcv.recv()
        {
            Ok(ticket) => 
            {
                println!("{}", ticket.base64());
                factory.input_ticket(ticket);
            },
            Err(why) => return Err(format!("ERROR {}", why)),
        }
    }

    let mut target_tickets = Vec::new();
    for target_path in record.targets.iter()
    {
        target_tickets.push(station.get_target_ticket(target_path));
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
        match TicketFactory::from_file(&record.targets[sub_index])
        {
            Ok(mut hash) =>
            {
                match sender.send(hash.result())
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
    use crate::ticket::{TicketFactory};

    use std::fs::File;
    use std::io::prelude::*;
    use std::sync::mpsc;

    #[test]
    fn do_empty_command()
    {
        match do_command(
            Record
            {
                targets: vec!["A".to_string()],
                source_indices: vec![],
                ticket: TicketFactory::new().result(),
                command: vec![],
            },
            vec![],
            vec![],
            Station{})
        {
            Ok(result) =>
            {
                assert_eq!(result.out, "");
                assert_eq!(result.err, "");
                assert_eq!(result.code, Some(0));
                assert_eq!(result.success, true);
            },
            Err(_) => panic!("Command failed"),
        }
    }

    #[test]
    fn wait_for_channels()
    {
        let (sender_a, receiver_a) = mpsc::channel();
        let (sender_b, receiver_b) = mpsc::channel();
        let (sender_c, receiver_c) = mpsc::channel();

        match File::create("A.txt")
        {
            Ok(mut file) =>
            {
                match file.write_all(b"")
                {
                    Ok(p) => assert_eq!(p, ()),
                    Err(_) => panic!("Could not write to test file"),
                }
            },
            Err(err) => panic!("Could not open file for writing {}", err),
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
                command: vec![],
            },
            vec![(0, sender_c)],
            vec![receiver_a, receiver_b],
            Station{})
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
