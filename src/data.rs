extern crate bincode;
extern crate crypto;

use std::collections::HashMap;
use sqlite::{Connection, State};
use crypto::sha2::Sha512;
use base64::encode;
use crypto::digest::Digest;
use std::hash::{Hash, Hasher};
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

    pub fn input_hash(&mut self, input: Ticket)
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

    pub fn from_filepath(path : &str) -> Result<TicketFactory, std::io::Error>
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

use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, PartialEq)]
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


#[derive(Serialize, Deserialize, PartialEq)]
pub struct Memory
{
    to_target_hashes : HashMap<Ticket, Vec<Ticket>>,
}

impl Eq for Ticket {}

impl Hash for Ticket
{
    fn hash<H: Hasher>(&self, state: &mut H)
    {
        self.sha[..8].hash(state);
    }
}

fn open_connection()
{
    let connection = sqlite::open("history.db").unwrap();
    connection.execute(
        "CREATE TABLE IF NOT EXISTS history (source varchar(88), target varchar(88), UNIQUE(source) );"
    ).unwrap();
}

fn get_target_hash(connection : &Connection, sha_str : &str) -> String
{
    let mut statement = connection
        .prepare("SELECT * FROM history WHERE source = ?")
        .unwrap();

    statement.bind(1, sha_str).unwrap();

    match statement.next().unwrap()
    {
        State::Row => statement.read::<String>(1).unwrap(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests
{
    use std::collections::HashMap;
    use crate::data::{TicketFactory, Ticket};
    use crate::data::Memory;

    #[test]
    fn ticket_factory_string()
    {
        let ticket = TicketFactory::from_str("b").result();
        assert_eq!(ticket.sha.len(), 64);
        assert_eq!(ticket.base64(),
            "Umd2iCLuYk1I/OFexcp5y9YCy39MIVelFlVpkfIu+Me173sY0f9BxZNw77CFhlHUSpNsEbexRMSP4E3zxqPo2g==");
    }

    #[test]
    fn data_serialization()
    {
        let mut to_target_hashes : HashMap<Ticket, Vec<Ticket>> = HashMap::new();
        to_target_hashes.insert(
            TicketFactory::from_str("a").result(),
            vec![
                TicketFactory::from_str("b").result(),
                TicketFactory::from_str("c").result(),
                TicketFactory::from_str("d").result()
            ]
        );

        let memory = Memory
        {
            to_target_hashes : to_target_hashes
        };

        let bytes: Vec<u8> = bincode::serialize(&memory).unwrap();
        let decoded_memory: Memory = bincode::deserialize(&bytes[..]).unwrap();

        assert!(memory == decoded_memory);
    }
}
