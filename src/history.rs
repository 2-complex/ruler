use crate::ticket::
{
    Ticket
};
use crate::system::System;
use crate::blob::
{
    TargetTickets,
    BlobError,
};
use std::collections::HashMap;
use serde::
{
    Serialize,
    Deserialize
};
use std::fmt;
use std::io::
{
    Read,
    Write,
};

/*  Recall that a Rule is three things: sources, targets and command.  For each particular rule, a RuleHistory stores
    the Tickets of target files witnessed by the program when the command built with a given rule-ticket.

    This is what Ruler uses to determine if targets are up-to-date.  It creates a ticket based on the current
    state of the rule, and indexes by that ticket into a RuleHistory to get target-tickets.  If the target
    tickets match, then the targets are up-to-date. */
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct RuleHistory
{
    /*  Each rule history consists of a map
            key = source-ticket
            value = a target ticket for each target */
    source_to_targets : HashMap<Ticket, TargetTickets>,
}

/*  Inserting target tickets in a RuleHistory can go wrong in a couple ways.
    Either there's already something there, which suggests user error, or the number
    of target tickets is wrong, which suggests a logical error in the code. */
pub enum RuleHistoryInsertError
{
    Contradiction(Vec<usize>),
    TargetSizesDifferWeird,
}

impl fmt::Display for RuleHistoryInsertError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            RuleHistoryInsertError::Contradiction(indices) =>
                write!(formatter, "Rule history insert contradicts what is already there: {:?}", indices),

            RuleHistoryInsertError::TargetSizesDifferWeird =>
                write!(formatter, "Rule history TargetTicket length differs.  That's weird."),
        }
    }
}


impl RuleHistory
{
    /*  Create a new rule history with empty map. */
    pub fn new() -> RuleHistory
    {
        RuleHistory
        {
            source_to_targets : HashMap::new()
        }
    }

    /*  With the given source_ticket, add the given target_tickets to the history.
        If there's a contradiction, constructs a RuleHistoryInsertError::Contradiction
        with a vector of indices. */
    pub fn insert(
        &mut self,
        source_ticket: Ticket,
        target_tickets: TargetTickets)
    -> Result<(), RuleHistoryInsertError>
    {
        match self.source_to_targets.get(&source_ticket)
        {
            Some(existing_tickets) =>
            {
                match existing_tickets.compare(target_tickets)
                {
                    Err(BlobError::Contradiction(v)) => Err(RuleHistoryInsertError::Contradiction(v)),
                    Err(BlobError::TargetSizesDifferWeird) => Err(RuleHistoryInsertError::TargetSizesDifferWeird),
                    Ok(_) => Ok(()),
                }
            },
            None =>
            {
                self.source_to_targets.insert(source_ticket, target_tickets);
                Ok(())
            }
        }
    }

    pub fn get_target_tickets(&self, source_ticket: &Ticket) -> Option<&TargetTickets>
    {
        self.source_to_targets.get(source_ticket)
    }
}

impl fmt::Display for RuleHistory
{
    /*  Displaying the rule history shows the source tickets' hashes and the target hashe
        with indentation showing which is which. */
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        let mut out = String::new();

        for (source_ticket, target_tickets) in self.source_to_targets.iter()
        {
            out.push_str("  ");
            out.push_str(&source_ticket.base64());
            out.push_str("\n");

            out.push_str(&target_tickets.base64())
        }

        write!(formatter, "{}", out)
    }
}

/*  History includes both the rule-histories and target-histories.  Recall that:
    rule_histories: For a given rule-hash stores the previously witnessed hashes of the targets built by that rule.
    target_histories: For a given target (file path) stores the most recently observed hash of that target along
        with the modified timestamp for the file at that time. */
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct History<SystemType : System>
{
    system_box : Box<SystemType>,
    path : String,
}

/*  When accessing History, a few things can go wrong.  History is stored in a file, so that file could be unreadable or
    corrupt.  These would mean that user has tried to modify files that ruler depends on to to work.  Serialization
    of an empty history could fail, which would indicate a logical error in this source code. */
pub enum HistoryError
{
    CannotReadRuleHistoryFile(String),
    CannotInterpretRuleHistoryFile(String),
    CannotSerializeRuleHistory(String),
    CannotWriteRuleHistoryFile(String),
}

/*  Display a HistoryError by printing a reasonable error message.  Of course, during everyday Ruler use, these
    will not likely display. */
impl fmt::Display for HistoryError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            HistoryError::CannotReadRuleHistoryFile(path) =>
                write!(formatter, "Cannot read rule history file: {}", path),

            HistoryError::CannotInterpretRuleHistoryFile(path) =>
                write!(formatter, "Cannot interpret rule history file: {}", path),

            HistoryError::CannotSerializeRuleHistory(path) =>
                write!(formatter, "Cannot serialize rule history: {}", path),

            HistoryError::CannotWriteRuleHistoryFile(path) =>
                write!(formatter, "Cannot record rule history file: {}", path),
        }
    }
}

impl<SystemType : System> History<SystemType>
{
    /*  Create a new History from a filepath in the filesystem. */
    pub fn new(system: SystemType, path : &str)
    -> History<SystemType>
    {
        History
        {
            system_box : Box::new(system),
            path : path.to_string(),
        }
    }

    /*  Insert a RuleHistory for a given rule. */
    pub fn write_rule_history(&mut self, rule_ticket: Ticket, rule_history: RuleHistory)
    -> Result<(), HistoryError>
    {
        let system = &mut (*self.system_box);
        let rule_history_file_path = format!("{}/{}", self.path, rule_ticket);

        let content =
        match bincode::serialize(&rule_history)
        {
            Ok(rule_history_bytes) => rule_history_bytes,
            Err(_) => return Err(HistoryError::CannotSerializeRuleHistory(rule_history_file_path)),
        };

        let mut file =
        match system.create_file(&rule_history_file_path)
        {
            Ok(file) => file,
            Err(_error) => return Err(HistoryError::CannotWriteRuleHistoryFile(rule_history_file_path)),
        };

        match file.write_all(&content)
        {
            Ok(_) => Ok(()),
            Err(_error) => Err(HistoryError::CannotWriteRuleHistoryFile(rule_history_file_path)),
        }
    }

    /*  Retrive a RuleHisotry for a given rule.  If it can't openthe file, it just makes a new RuleHistory */
    pub fn read_rule_history(&mut self, rule_ticket: &Ticket) -> Result<RuleHistory, HistoryError>
    {
        let system = &mut (*self.system_box);
        let rule_history_file_path = format!("{}/{}", self.path, rule_ticket);

        let mut file = 
        match system.open(&rule_history_file_path)
        {
            Ok(file) => file,
            Err(_) => return Ok(RuleHistory::new()),
        };

        let mut content = Vec::new();
        match file.read_to_end(&mut content)
        {
            Ok(_size) => {},
            Err(_) => return Err(HistoryError::CannotReadRuleHistoryFile(rule_history_file_path)),
        }

        match bincode::deserialize(&content)
        {
            Ok(rule_history) => Ok(rule_history),
            Err(_) => Err(HistoryError::CannotInterpretRuleHistoryFile(rule_history_file_path)),
        }
    }
}

#[cfg(test)]
mod test
{
    use crate::history::
    {
        RuleHistory,
        History,
    };
    use crate::blob::
    {
        TargetTickets,
    };
    use crate::ticket::{TicketFactory};
    use crate::system::
    {
        System,
        fake::FakeSystem
    };

    /*  Create a History, get a RuleHistory from it, insert source/target tickets, then write it back to the filesystem,
        read back to create a new History, get back the same RuleHistory and check that its contents are the same */
    #[test]
    fn round_trip_history_through_file_to_from()
    {
        let rule_ticket = TicketFactory::from_str("rule").result();
        let source_ticket = TicketFactory::from_str("source").result();
        let target_tickets = TargetTickets::from_vec(vec![
            TicketFactory::from_str("target1").result(),
            TicketFactory::from_str("target2").result(),
            TicketFactory::from_str("target3").result(),
        ]);

        let mut system = FakeSystem::new(10);
        match system.create_dir("history")
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to initialize file situation: {}", error),
        }
        let mut history = History::new(system.clone(), "history");

        let mut rule_history =
        match history.read_rule_history(&rule_ticket)
        {
            Ok(rule_history) => rule_history,
            Err(error) => panic!("History failed to create RuleHistory: {}", error),
        };

        assert_eq!(rule_history, RuleHistory::new());
        match rule_history.insert(source_ticket.clone(), target_tickets.clone())
        {
            Ok(()) => {},
            Err(error) => panic!("RuleHisotry failed to insert source / target-ticket pair: {}", error),
        }
        match history.write_rule_history(rule_ticket.clone(), rule_history.clone())
        {
            Ok(()) => {},
            Err(error) => panic!("Failed to write rule history: {}", error),
        }
        drop(history);

        let mut history2 = History::new(system, "history");
        let rule_history2 =
        match history2.read_rule_history(&rule_ticket)
        {
            Ok(rule_history) => rule_history,
            Err(error) => panic!("History failed to retrieve RuleHistory: {}", error),
        };

        assert_eq!(rule_history, rule_history2);
        let target_tickets2 = match rule_history.get_target_tickets(&source_ticket)
        {
            Some(target_tickets) => target_tickets,
            None => panic!("RuleHistory retrieved from History failed to produce expected TargetTicket"),
        };

        assert_eq!(target_tickets, *target_tickets2);
    }
}
