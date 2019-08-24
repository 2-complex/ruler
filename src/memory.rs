use crate::ticket::{Ticket};

use std::collections::HashMap;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct RuleHistory
{
    source_to_targets : HashMap<Ticket, Vec<Ticket>>
}

impl RuleHistory
{
    fn new() -> RuleHistory
    {
        RuleHistory
        {
            source_to_targets : HashMap::new()
        }
    }

    fn insert(&mut self, source_ticket: Ticket, target_tickets: Vec<Ticket>)
    {
        self.source_to_targets.insert(source_ticket, target_tickets);
    }

    fn get(&self, source_ticket: &Ticket) -> Option<&Vec<Ticket>>
    {
        self.source_to_targets.get(source_ticket)
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Memory
{
    rule_histories : HashMap<Ticket, RuleHistory>
}

impl Memory
{
    fn new() -> Memory
    {
        Memory
        {
            rule_histories: HashMap::new(),
        }
    }

    fn insert(&mut self, rule_ticket: Ticket, source_ticket: Ticket, target_tickets: Vec<Ticket>)
    {
        let rule_history = self.rule_histories.entry(rule_ticket).or_insert(
            RuleHistory
            {
                source_to_targets: HashMap::new(),
            }
        );

        rule_history.insert(source_ticket, target_tickets);
    }

    fn remove(&mut self, rule_ticket: &Ticket) -> Option<RuleHistory>
    {
        self.rule_histories.remove(rule_ticket)
    }
}

#[cfg(test)]
mod test
{
    use crate::memory::{RuleHistory, Memory};
    use crate::ticket::{TicketFactory};

    #[test]
    fn round_trip()
    {
        let mut mem = Memory::new();
        mem.insert(
            TicketFactory::from_str("rule").result(),
            TicketFactory::from_str("source").result(),
            [
                TicketFactory::from_str("target1").result(),
                TicketFactory::from_str("target2").result(),
                TicketFactory::from_str("target3").result(),
            ].to_vec()
        );

        let encoded: Vec<u8> = bincode::serialize(&mem).unwrap();
        let decoded: Memory = bincode::deserialize(&encoded[..]).unwrap();
        assert_eq!(mem, decoded);
    }

    #[test]
    fn round_trip_history()
    {
        let mut history = RuleHistory::new();
        history.insert(TicketFactory::from_str("source").result(),
            [
                TicketFactory::from_str("target1").result(),
                TicketFactory::from_str("target2").result(),
                TicketFactory::from_str("target3").result(),
            ].to_vec());

        let encoded: Vec<u8> = bincode::serialize(&history).unwrap();
        let decoded: RuleHistory = bincode::deserialize(&encoded[..]).unwrap();
        assert_eq!(history, decoded);

        match history.get(&TicketFactory::from_str("source").result())
        {
            Some(target_tickets) =>
            {
                assert_eq!(target_tickets.len(), 3);

                assert_eq!(
                    target_tickets[0],
                    TicketFactory::from_str("target1").result()
                );

                assert_eq!(
                    target_tickets[1],
                    TicketFactory::from_str("target2").result()
                );

                assert_eq!(
                    target_tickets[2],
                    TicketFactory::from_str("target3").result()
                );
            },
            None => panic!("Targets not found"),
        }
    }

    #[test]
    fn add_remove_rules()
    {
        let mut history_a = RuleHistory::new();
        history_a.insert(TicketFactory::from_str("sourceA").result(),
            [
                TicketFactory::from_str("target1A").result(),
                TicketFactory::from_str("target2A").result(),
                TicketFactory::from_str("target3A").result(),
            ].to_vec());

        let mut history_b = RuleHistory::new();
        history_b.insert(TicketFactory::from_str("sourceB").result(),
            [
                TicketFactory::from_str("target1B").result(),
                TicketFactory::from_str("target2B").result(),
                TicketFactory::from_str("target3B").result(),
            ].to_vec());

        let mut memory = Memory::new();

        memory.insert(TicketFactory::from_str("ruleA").result(),
            TicketFactory::from_str("sourceA").result(),
            [
                TicketFactory::from_str("target1A").result(),
                TicketFactory::from_str("target2A").result(),
                TicketFactory::from_str("target3A").result(),
            ].to_vec());

        memory.insert(TicketFactory::from_str("ruleB").result(),
            TicketFactory::from_str("sourceB").result(),
            [
                TicketFactory::from_str("target1B").result(),
                TicketFactory::from_str("target2B").result(),
                TicketFactory::from_str("target3B").result(),
            ].to_vec());

        match memory.remove(&TicketFactory::from_str("ruleA").result())
        {
            Some(history) =>
            {
                assert_eq!(history, history_a);
                match history.get(&TicketFactory::from_str("sourceA").result())
                {
                    Some(target_tickets) =>
                    {
                        assert_eq!(target_tickets.len(), 3);
                    },
                    None => panic!("Important event missing from hisotry"),
                }

                match history.get(&TicketFactory::from_str("sourceB").result())
                {
                    Some(_target_tickets) => panic!("Important event missing from hisotry"),
                    None => {},
                }
            },
            None=> panic!("Rule added to memory mysteriously gone"),
        }

        match memory.remove(&TicketFactory::from_str("ruleA").result())
        {
            Some(_history) => panic!("Removed rule still there"),
            None => {},
        }

        match memory.remove(&TicketFactory::from_str("ruleB").result())
        {
            Some(history) =>
            {
                assert_eq!(history, history_b);
                match history.get(&TicketFactory::from_str("sourceB").result())
                {
                    Some(target_tickets) =>
                    {
                        assert_eq!(target_tickets.len(), 3);
                    },
                    None => panic!("Important event missing from hisotry"),
                }
            },
            None=> panic!("Rule added to memory mysteriously gone"),
        }
    }
}
