extern crate bincode;
extern crate serde;

use crypto::sha2::Sha512;
use base64::encode;
use crypto::digest::Digest;
use std::hash::{Hash, Hasher};
use serde::{Serialize, Deserialize};
use filesystem::FileSystem;
use std::fmt;

pub struct TicketFactory
{
    dig : Sha512
}

impl TicketFactory
{
    pub fn new() -> TicketFactory
    {
        TicketFactory{ dig : Sha512::new() }
    }

    pub fn does_not_exist() -> Ticket
    {
        Ticket
        {
            sha : Vec::new()
        }
    }

    pub fn from_str(first_input: &str) -> TicketFactory
    {
        let mut d = Sha512::new();
        d.input(first_input.as_bytes());
        TicketFactory{ dig : d }
    }

    pub fn input_ticket(&mut self, input: Ticket)
    {
        self.dig.input(&input.sha);
    }

    pub fn input_str(&mut self, input: &str)
    {
        self.dig.input(input.as_bytes());
    }

    pub fn result(&mut self) -> Ticket
    {
        let mut out_sha = vec![0u8; 64];
        self.dig.result(&mut out_sha);
        Ticket
        {
            sha : out_sha
        }
    }


    pub fn from_file<FSType: FileSystem>(
        file_system: &FSType,
        path : &str)
        -> Result<TicketFactory, std::io::Error>
    {
        let mut dig = Sha512::new();
        let mut buffer = Vec::new();

        match file_system.read_file_into(path, &mut buffer)
        {
            Ok(_) =>
            {
                dig.input(&buffer);
                Ok(TicketFactory{dig : dig})
            },
            Err(why) => return Err(why),
        }

    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Ticket
{
    sha: Vec<u8>,
}

impl Ticket
{
    pub fn base64(&self) -> String
    {
        format!("{}", encode(&self.sha))
    }

    pub fn from_strings(
        targets: &Vec<String>,
        sources: &Vec<String>,
        command: &Vec<String>) -> Ticket
    {
        let mut factory = TicketFactory::new();

        for target in targets.iter()
        {
            factory.input_str(target);
            factory.input_str("\n");
        }

        factory.input_str("\n:\n");

        for source in sources.iter()
        {
            factory.input_str(source);
            factory.input_str("\n");
        }

        factory.input_str("\n:\n");

        for line in command.iter()
        {
            factory.input_str(line);
            factory.input_str("\n");
        }

        factory.input_str("\n:\n");
        factory.result()
    }
}

impl fmt::Display for Ticket
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "{}", self.base64())
    }
}

impl Eq for Ticket {}

impl Hash for Ticket
{
    fn hash<H: Hasher>(&self, state: &mut H)
    {
        self.sha[..8].hash(state);
    }
}


#[cfg(test)]
mod test
{
    use crate::ticket::{Ticket, TicketFactory};
    use filesystem::{FileSystem, FakeFileSystem};

    #[test]
    fn ticket_factory_string()
    {
        let ticket = TicketFactory::from_str("b").result();
        assert_eq!(ticket.sha.len(), 64);
        assert_eq!(ticket.base64(),
            "Umd2iCLuYk1I/OFexcp5y9YCy39MIVelFlVpkfIu+Me173sY0f9BxZNw77CFhlHUSpNsEbexRMSP4E3zxqPo2g==");
    }

    #[test]
    fn ticket_from_string2()
    {
        let ticket = TicketFactory::from_str("Time wounds all heels.\n").result();
        assert_eq!(ticket.base64(),
            "PRemaMHXvOuGAx87EOGZY1/cGUv4udBiqVmgP8nwVX93njjGOdE41zf4rV9PAbiJp/i6ucukKrvFp3zldP42wA==");
    }

    #[test]
    fn ticket_from_string_from_new()
    {
        let mut factory = TicketFactory::new();
        factory.input_str("Time ");
        factory.input_str("wounds ");
        factory.input_str("all ");
        factory.input_str("heels.\n");
        let ticket = factory.result();
        assert_eq!(ticket.base64(),
            "PRemaMHXvOuGAx87EOGZY1/cGUv4udBiqVmgP8nwVX93njjGOdE41zf4rV9PAbiJp/i6ucukKrvFp3zldP42wA==");
    }

    #[test]
    fn ticket_factory_file()
    {
        let file_system = FakeFileSystem::new();
        match file_system.write_file("time0.txt", "Time wounds all heels.\n")
        {
            Ok(_) => {},
            Err(why) => panic!("Failed to create temp file: {}", why),
        }

        match TicketFactory::from_file(&file_system, "time0.txt")
        {
            Ok(mut factory) =>
            {
                assert_eq!(factory.result().base64(),
                    "PRemaMHXvOuGAx87EOGZY1/cGUv4udBiqVmgP8nwVX93njjGOdE41zf4rV9PAbiJp/i6ucukKrvFp3zldP42wA==");
            },
            Err(why) => panic!("Failed to open test file time.txt: {}", why),
        }
    }

    #[test]
    fn ticket_factory_hashes()
    {
        let file_system = FakeFileSystem::new();
        match file_system.write_file("time1.txt", "Time wounds all heels.\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match TicketFactory::from_file(&file_system, "time1.txt")
        {
            Ok(mut factory) =>
            {
                let mut new_factory = TicketFactory::from_str("time1.txt\n:\n:\n:\n");
                new_factory.input_ticket(factory.result());
                assert_eq!(new_factory.result().base64(),
                    "CcCQWumbtg7N3xkZEAv+GlmKKe5XRJGzz+0fbdeG5poAnHSTVTjM2jCo5xu7/9r4GiYcevWaG2mesTMoB6NK6g==");
            },
            Err(why) => panic!("Failed to open test file time1.txt: {}", why),
        }
    }

    #[test]
    fn ticket_serialize_round_trip()
    {
        let ticket = TicketFactory::from_str("apples").result();
        let encoded: Vec<u8> = bincode::serialize(&ticket).unwrap();
        let decoded: Ticket = bincode::deserialize(&encoded[..]).unwrap();
        assert_eq!(ticket, decoded);
    }
}