extern crate bincode;
extern crate serde;

use crypto::sha2::Sha512;
use base64::encode;
use crypto::digest::Digest;
use std::hash::{Hash, Hasher};
use serde::{Serialize, Deserialize};
use std::fs::File;
use std::io::Read;

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

    pub fn from_file(path : &str) -> Result<TicketFactory, std::io::Error>
    {
        match File::open(path)
        {
            Ok(mut file) =>
            {
                let mut dig = Sha512::new();
                let mut buf = [0u8; 256];

                loop
                {
                    match file.read(&mut buf)
                    {
                        Ok(0) => break,
                        Ok(packet_size) => dig.input(&buf[..packet_size]),
                        Err(why) => return Err(why),
                    }
                }

                Ok(TicketFactory{dig : dig})
            },
            Err(why) => Err(why),
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
    #[allow(dead_code)]
    pub fn base64(&self) -> String
    {
        format!("{}", encode(&self.sha))
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
    use std::fs::File;
    use std::io::prelude::*;

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
        match File::create("time0.txt")
        {
            Ok(mut file) =>
            {
                match file.write_all("Time wounds all heels.\n".as_bytes())
                {
                    Ok(p) => assert_eq!(p, ()),
                    Err(_) => panic!("Could not write to test file"),
                }
            },
            Err(err) => panic!("Could not open file for writing {}", err),
        }

        match TicketFactory::from_file("time0.txt")
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
        match File::create("time1.txt")
        {
            Ok(mut file) =>
            {
                match file.write_all("Time wounds all heels.\n".as_bytes())
                {
                    Ok(p) => assert_eq!(p, ()),
                    Err(_) => panic!("Could not write to test file"),
                }
            },
            Err(err) => panic!("Could not open file for writing {}", err),
        }

        match TicketFactory::from_file("time1.txt")
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
