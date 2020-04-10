extern crate bincode;
extern crate serde;

use crypto::sha2::Sha512;
use base64::encode_config;
use crypto::digest::Digest;
use std::hash::{Hash, Hasher};
use serde::{Serialize, Deserialize};
use filesystem::FileSystem;
use std::fmt;
use std::io::Read;

/*  Ticket is a struct representing a hash of a file or a rule.  To construct a ticket,
    you first make a TiketFactory, and you can feed the factory data bit by bit for it to
    hash, using functions that start with "input_" then get the ticket using result(). */
pub struct TicketFactory
{
    dig : Sha512
}

impl TicketFactory
{
    /*  Create an empty TicketFactory initialized with no bytes. */
    pub fn new() -> TicketFactory
    {
        TicketFactory{ dig : Sha512::new() }
    }

    /*  Construct a TicketFactory immediately reading in
        the bytes of the given string as input. */
    #[cfg(test)]
    pub fn from_str(first_input: &str) -> TicketFactory
    {
        let mut d = Sha512::new();
        d.input(first_input.as_bytes());
        TicketFactory{ dig : d }
    }

    /*  Read in a Ticket, convert (the hash therein) to bytes,
        and incorporate those bytes into the currently building ticket. */
    pub fn input_ticket(&mut self, input: Ticket)
    {
        self.dig.input(&input.sha);
    }

    /*  Read in a str, convert to bytes, and incorporate those bytes
        into the currently building ticket. */
    pub fn input_str(&mut self, input: &str)
    {
        self.dig.input(input.as_bytes());
    }

    /*  Create a ticket from the bytes incorporated so far. */
    pub fn result(&mut self) -> Ticket
    {
        let mut out_sha = vec![0u8; 64];
        self.dig.result(&mut out_sha);
        Ticket
        {
            sha : out_sha
        }
    }

    /*  Construct a TicketFactory, initialized with the contents of a file
        from a FileSystem */
    pub fn from_file<FSType: FileSystem>(
        file_system: &FSType,
        path : &str)
        -> Result<TicketFactory, std::io::Error>
    {
        match file_system.open(path)
        {
            Ok(mut reader) =>
            {
                let mut buffer = [0u8; 256];
                let mut dig = Sha512::new();
                loop
                {
                    match reader.read(&mut buffer)
                    {
                        Ok(0) =>
                        {
                            return Ok(TicketFactory{dig : dig});
                        }
                        Ok(size) =>
                        {
                            dig.input(&buffer[..size]);
                        },
                        Err(why) => return Err(why),
                    }
                }
            },
            Err(why) => return Err(why),
        }
    }
}

/*  Ticket represents a hash of a file or a rule */
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Ticket
{
    sha: Vec<u8>,
}

impl Ticket
{
    /*  Returns a string URL-safe base64-encoded hash */
    pub fn base64(&self) -> String
    {
        format!("{}", encode_config(&self.sha, base64::URL_SAFE))
    }

    /*  Use this function to create a ticket based on the targets, sources and command
        of a rule. */
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
    use lipsum::{LOREM_IPSUM};

    /*  Uses a TicketFactory to construct a Ticket based on a single string with one character,
        compares with exemplar. */
    #[test]
    fn ticket_factory_string()
    {
        let ticket = TicketFactory::from_str("b").result();
        assert_eq!(ticket.sha.len(), 64);
        assert_eq!(ticket.base64(),
            "Umd2iCLuYk1I_OFexcp5y9YCy39MIVelFlVpkfIu-Me173sY0f9BxZNw77CFhlHUSpNsEbexRMSP4E3zxqPo2g==");
    }

    /*  Uses a TicketFactory to construct a Ticket based on a single string with more than one character,
        compares with exemplar. */
    #[test]
    fn ticket_from_string_more()
    {
        let ticket = TicketFactory::from_str("Time wounds all heels.\n").result();
        assert_eq!(ticket.base64(),
            "PRemaMHXvOuGAx87EOGZY1_cGUv4udBiqVmgP8nwVX93njjGOdE41zf4rV9PAbiJp_i6ucukKrvFp3zldP42wA==");
    }

    /*  Constructs two tickets for the same string, A: by calling input_str with pieces of the string,
        and B: by taking the whole string at once in from_str().  Checks that the tickets are the same,
        and equal to an exemplar. */
    #[test]
    fn ticket_from_string_from_new()
    {
        let mut factory = TicketFactory::new();
        factory.input_str("Time ");
        factory.input_str("wounds ");
        factory.input_str("all ");
        factory.input_str("heels.\n");
        let ticketA = factory.result();
        let ticketB = TicketFactory::from_str("Time wounds all heels.\n").result();

        assert_eq!(ticketA.base64(),
            "PRemaMHXvOuGAx87EOGZY1_cGUv4udBiqVmgP8nwVX93njjGOdE41zf4rV9PAbiJp_i6ucukKrvFp3zldP42wA==");

        assert_eq!(ticketA.base64(), ticketB.base64());
    }

    /*  Using a fake file system, create a file, populate with some known text, use TicketFactory::from_file
        to get a hash and compare with an exemplar.  */
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
                    "PRemaMHXvOuGAx87EOGZY1_cGUv4udBiqVmgP8nwVX93njjGOdE41zf4rV9PAbiJp_i6ucukKrvFp3zldP42wA==");
            },
            Err(why) => panic!("Failed to open test file time.txt: {}", why),
        }
    }

    /*  Using a fake filesystem, create a file, populate it with with known text, then use TicketFactory::from_str
        and input_ticket to simulate making a ticket with that file as a target.  Compare the hash with an exemplar.*/
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
                    "CcCQWumbtg7N3xkZEAv-GlmKKe5XRJGzz-0fbdeG5poAnHSTVTjM2jCo5xu7_9r4GiYcevWaG2mesTMoB6NK6g==");
            },
            Err(why) => panic!("Failed to open test file time1.txt: {}", why),
        }
    }

    /*  Obtain lorem ipsum, and write it to a file in a fake filesystem.  Then use TicketFactory::from_file
        to generate a ticket.  Compare hash with an exemplar. */
    #[test]
    fn ticket_factory_hashes_bigger_file()
    {
        let file_system = FakeFileSystem::new();

        println!("{} {}\n", LOREM_IPSUM.len(), LOREM_IPSUM);

        match file_system.write_file("good_and_evil.txt", LOREM_IPSUM)
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match TicketFactory::from_file(&file_system, "good_and_evil.txt")
        {
            Ok(mut factory) =>
            {
                assert_eq!(factory.result().base64(),
                    "UUIguBwMxofHdUKdfRVzVpLkqPRwg5IISF49Wc2jVd6-pmF9lxunRtP26JDPNlAgX3MoUrJEfrQ9nVKFJly8Og==");
            },
            Err(why) => panic!("Failed to open test file good_and_evil.txt: {}", why),
        }
    }

    /*  Make a ticket, serialize to a vector of bytes, then deserialize, and check that
        the deserialized ticket matches the original. */
    #[test]
    fn ticket_serialize_round_trip()
    {
        let ticket = TicketFactory::from_str("apples").result();
        let encoded: Vec<u8> = bincode::serialize(&ticket).unwrap();
        let decoded: Ticket = bincode::deserialize(&encoded[..]).unwrap();
        assert_eq!(ticket, decoded);
        assert_eq!(ticket.base64(), decoded.base64());
    }
}
