extern crate bincode;
extern crate serde;

use crypto::sha2::Sha256;
use base64::
{
    Engine,
    engine::general_purpose,
    DecodeError,
};
use crypto::digest::Digest;
use std::hash::{Hash, Hasher};
use serde::{Serialize, Deserialize};
use crate::system::
{
    System,
    ReadWriteError,
};
use std::fmt;
use std::io::Read;

/*  Ticket is a struct representing a hash of a file or a rule.  To construct a ticket,
    you first make a TiketFactory, and you can feed the factory data bit by bit for it to
    hash, using functions that start with "input_" then get the ticket using result(). */
pub struct TicketFactory
{
    dig : Sha256
}

impl TicketFactory
{
    /*  Create an empty TicketFactory initialized with no bytes. */
    pub fn new() -> TicketFactory
    {
        TicketFactory{ dig : Sha256::new() }
    }

    /*  Construct a TicketFactory immediately reading in
        the bytes of the given string as input. */
    pub fn from_str(first_input: &str) -> TicketFactory
    {
        let mut d = Sha256::new();
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
        let mut out_sha = vec![0u8; 32];
        self.dig.result(&mut out_sha);
        Ticket
        {
            sha : out_sha
        }
    }

    /*  Construct a TicketFactory, initialized with the contents of a file from a System. */
    pub fn from_file<FSType: System>
    (
        file_system: &FSType,
        path : &str
    )
    ->
    Result<TicketFactory, ReadWriteError>
    {
        match file_system.open(path)
        {
            Ok(mut reader) =>
            {
                let mut buffer = [0u8; 256];
                let mut dig = Sha256::new();
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
                        Err(error) => return Err(ReadWriteError::IOError(error)),
                    }
                }
            },
            Err(error) => return Err(ReadWriteError::SystemError(error)),
        }
    }

    /*  Construct a TicketFactory, initialized with the contents of a file from a System. */
    pub fn from_directory<FSType: System>
    (
        file_system: &FSType,
        path : &str
    )
    ->
    Result<TicketFactory, ReadWriteError>
    {
        let contents = file_system.list_dir(path);
        Ok(TicketFactory::from_str("alfseilasiefj34f4jlasdf8"))
    }
}

/*  Ticket represents a hash of a file or a rule */
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Ticket
{
    sha: Vec<u8>,
}

pub enum From64Error
{
    DecodeInvalidPadding,
    DecodeInvalidByte(usize, u8),
    DecodeInvalidLastSymbol(usize, u8),
    DecodeInvalidLength,
    ShaInvalidLength
}

impl fmt::Display for From64Error
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            From64Error::DecodeInvalidPadding =>
                write!(formatter, "Invalid padding"),

            From64Error::DecodeInvalidByte(location, byte) =>
                write!(formatter, "Invalid byte: {} at location {}", byte, location),

            From64Error::DecodeInvalidLastSymbol(location, byte) =>
                write!(formatter, "Invalid last symbol: {} at location {}", byte, location),

            From64Error::DecodeInvalidLength =>
                write!(formatter, "Failed to decode base-64: invalid length"),

            From64Error::ShaInvalidLength =>
                write!(formatter, "Successful base-64 conversion to data of wrong size"),
        }
    }
}

impl Ticket
{
    /*  Returns a string URL-safe human-readable hash string */
    pub fn human_readable(&self) -> String
    {
        format!("{}", general_purpose::URL_SAFE.encode(&self.sha))
    }

    /*  Takes a url-safe human-readable hash string and returns a ticket objcet
        or an error about why the hash string was invalid. */
    pub fn from_human_readable(human_readable_str: &str) ->
        Result<Ticket, From64Error>
    {
        match general_purpose::URL_SAFE.decode(human_readable_str)
        {
            Ok(sha) => 
            {
                if sha.len() == 32
                {
                    Ok(Ticket{sha:sha})
                }
                else
                {
                    Err(From64Error::ShaInvalidLength)
                }
            },
            Err(DecodeError::InvalidPadding) => Err(From64Error::DecodeInvalidPadding),
            Err(DecodeError::InvalidByte(location, byte)) => Err(From64Error::DecodeInvalidByte(location, byte)),
            Err(DecodeError::InvalidLastSymbol(location, byte)) => Err(From64Error::DecodeInvalidLastSymbol(location, byte)),
            Err(DecodeError::InvalidLength) => Err(From64Error::DecodeInvalidLength),
        }
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
        write!(f, "{}", self.human_readable())
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
    use crate::ticket::
    {
        Ticket,
        TicketFactory,
        From64Error
    };
    use crate::system::util::
    {
        write_str_to_file
    };
    use crate::system::fake::
    {
        FakeSystem
    };
    use crate::system::System;
    use lipsum::{LOREM_IPSUM};
    use std::collections::HashMap;
    use std::collections::HashSet;

    /*  Takes a string, computes a map of character to character-count */
    fn get_counts(hash_str : &str) -> HashMap<char, i32>
    {
        let mut result = HashMap::new();
        for c in hash_str.chars()
        {
            result.insert(
                c, match result.get(&c)
                {
                    Some(count) => count + 1,
                    None => 1
                }
            );
        }
        result
    }

    /*  Returns true if the given string is:
            - sufficiently long,
            - comprised of ascii characters you can type
            - random-ish. */
    fn hash_heuristic(hash_str : &str) -> bool
    {
        if hash_str.len() < 20
        {
            return false;
        }

        for c in hash_str.chars()
        {
            if !(c as i32 >= 0x20 && c as i32 <= 0x7e)
            {
                return false;
            }
        }

        let counts = get_counts(hash_str);
        let x = hash_str.len() as i32;
        let y = counts.len() as i32;

        return (x-y).abs() < x / 2;
    }

    #[test]
    fn ticket_factory_passes_heuristic()
    {
        for n in 0..10000
        {
            let content = format!("{} is a very interesting number.", n);
            let ticket = TicketFactory::from_str(&content).result();
            println!("{} {}", content, ticket.base64());
            assert!(hash_heuristic(&ticket.base64()));
        }
    }

    #[test]
    fn short_hashes_fail_heuristic()
    {
        assert!(!hash_heuristic(""));
        assert!(!hash_heuristic("1"));
        assert!(!hash_heuristic("12345"));
    }

    #[test]
    fn not_typable_hashes_fail_heuristic()
    {
        assert!(!hash_heuristic("\0"));
        assert!(hash_heuristic("PiPoFgA5WUoziU9lZOGxNIu9egCI1CxKy3PurtWcAJ0"));
        assert!(!hash_heuristic("PiPoFgA5WUoziU9lZOGxNIu9egCI1CxKy3PurtWcAJ0å"));
        assert!(!hash_heuristic("PiPoFgA5WUoziU9lZOGxNIu9egCI1CxKy3PurtWcAJ0🍌"));
    }

    #[test]
    fn not_randomly_distributed_hashes_fail_heuristic()
    {
        assert!(!hash_heuristic("appleappleappleappleappleappleappleapple"));
        assert!(!hash_heuristic("0000000000000000000000000000000000000000"));
        assert!(!hash_heuristic("abcdefghijklmnopqrstabcdefghijklmnopqrst"));
    }

    #[test]
    fn ticket_factory_generates_unique_tickets()
    {
        let mut tickets = HashSet::new();
        let k = 1000;
        for n in 0..k
        {
            let content = format!("{} is a very interesting number, isn't it Mr. {}", n, n+1);
            let ticket = TicketFactory::from_str(&content).result();
            tickets.insert(ticket);
        }
        assert!(tickets.len()==k)
    }

    /*  Uses a TicketFactory to construct a Ticket based on a single string with one character,
        compares with exemplar. */
    #[test]
    fn ticket_factory_string()
    {
        let ticket = TicketFactory::from_str("b").result();
        assert_eq!(ticket.sha.len(), 32);
        assert!(hash_heuristic(&ticket.base64()));
    }

    /*  Uses a TicketFactory to construct a Ticket based on a single string with more than one character,
        compares with exemplar. */
    #[test]
    fn ticket_from_string_more()
    {
        let ticket = TicketFactory::from_str("Time wounds all heels.\n").result();
        assert!(hash_heuristic(&ticket.base64()));
    }

    /*  Constructs two tickets for the same string, A: by calling input_str with pieces of the string,
        and B: by taking the whole string at once in from_str().  Checks that the tickets are equal. */
    #[test]
    fn ticket_from_string_from_new()
    {
        let mut factory = TicketFactory::new();
        factory.input_str("Time ");
        factory.input_str("wounds ");
        factory.input_str("all ");
        factory.input_str("heels.\n");
        let ticket_a = factory.result();
        let ticket_b = TicketFactory::from_str("Time wounds all heels.\n").result();

        assert_eq!(ticket_a.human_readable(), ticket_b.human_readable());
    }

    /*  Using a fake file-system, create a file, populate with some known text, use TicketFactory::from_file
        to get a hash and compare with an exemplar.  */
    #[test]
    fn ticket_factory_file()
    {
        let mut system = FakeSystem::new(10);
        write_str_to_file(&mut system, "time0.txt", "Time wounds all heels.\n").unwrap();
        hash_heuristic(&TicketFactory::from_file(&system, "time0.txt").unwrap().result().base64());
    }

    /*  Using a fake file-system, create a file, populate with some known text, use TicketFactory::from_file
        to get a hash and compare with an exemplar.  */
    #[test]
    fn ticket_factory_two_files_different()
    {
        let mut system = FakeSystem::new(10);
        write_str_to_file(&mut system, "time0.txt", "Time wounds all heels.\n").unwrap();
        write_str_to_file(&mut system, "time1.txt", "Time: March is on.\n").unwrap();

        let ticket0 = TicketFactory::from_file(&system, "time0.txt").unwrap().result();
        let ticket1 = TicketFactory::from_file(&system, "time0.txt").unwrap().result();

        hash_heuristic(&ticket0.base64());
        hash_heuristic(&ticket0.base64());

        assert_ne!(ticket0, ticket1);
    }

    /*  Using a fake file-system, create a file, populate with some known text, use TicketFactory::from_file
        to get a hash and compare with an exemplar.  */
    #[test]
    fn ticket_factory_directory()
    {
        let mut system = FakeSystem::new(10);
        system.create_dir("time-files").unwrap();
        write_str_to_file(&mut system, "time-files/time0.txt", "Time wounds all heels.\n").unwrap();

        let ticket = TicketFactory::from_directory(&system, "time-files").unwrap().result();
        assert!(&hash_heuristic(&ticket.base64()));
    }

    /*  Using a fake file-system, create two directories, populate with some known text, use TicketFactory::from_file
        to get a hash and compare with an exemplar.  */
    #[test]
    fn ticket_factory_two_directories_different()
    {
        let mut system = FakeSystem::new(10);
        system.create_dir("time-files-0").unwrap();
        system.create_dir("time-files-1").unwrap();
        write_str_to_file(&mut system, "time-files-0/time0.txt", "Time wounds all heels.\n").unwrap();
        write_str_to_file(&mut system, "time-files-1/time1.txt", "Time: March is on.\n").unwrap();

        let ticket0 = TicketFactory::from_directory(&system, "time-files").unwrap().result();
        let ticket1 = TicketFactory::from_directory(&system, "time-files").unwrap().result();

        assert!(hash_heuristic(&ticket0.base64()));
        assert!(hash_heuristic(&ticket1.base64()));

        assert_ne!(ticket0, ticket1)
    }

    /*  Using a fake file-system, create a file, populate it with with known text, then use TicketFactory::from_str
        and input_ticket to simulate making a ticket with that file as a target.  Compare the hash with an exemplar.*/
    #[test]
    fn ticket_factory_hashes()
    {
        let mut file_system = FakeSystem::new(10);
        match write_str_to_file(&mut file_system, "time1.txt", "Time wounds all heels.\n")
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
                assert_eq!(new_factory.result().human_readable(),
                    "k4eqylqAAMXeFu_O8Gigms5bM9n9iFwFznBDRojDO8o=");
            },
            Err(why) => panic!("Failed to open test file time1.txt: {}", why),
        }
    }

    /*  Obtain lorem ipsum, and write it to a file in a fake filesystem.  Then use TicketFactory::from_file
        to generate a ticket.  Compare hash with an exemplar. */
    #[test]
    fn ticket_factory_hashes_bigger_file()
    {
        let mut file_system = FakeSystem::new(10);

        println!("{} {}\n", LOREM_IPSUM.len(), LOREM_IPSUM);

        match write_str_to_file(&mut file_system, "good_and_evil.txt", LOREM_IPSUM)
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match TicketFactory::from_file(&file_system, "good_and_evil.txt")
        {
            Ok(mut factory) =>
            {
                assert_eq!(factory.result().human_readable(),
                    "1-TbmtqWEoNv0OQQLb3OkYE2-f1LUOIH0SU71FP7Qo0=");
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
        assert_eq!(ticket.human_readable(), decoded.human_readable());
    }

    /*  Decode a valid ticket as human_readable, and do a round-trip check.*/
    #[test]
    fn ticket_from_human_readable()
    {
        match Ticket::from_human_readable("1-TbmtqWEoNv0OQQLb3OkYE2-f1LUOIH0SU71FP7Qo0=")
        {
            Ok(ticket) => 
            {
                assert_eq!(ticket.human_readable(), "1-TbmtqWEoNv0OQQLb3OkYE2-f1LUOIH0SU71FP7Qo0=");
            },
            Err(error) =>
            {
                panic!("Unexpected error getting ticket from human_readable: {}", error)
            }
        }
    }

    /*  Attempt to decode the empty string as base-64,
        check that it fails with ShaInvalidLength. */
    #[test]
    fn ticket_from_human_readable_empty()
    {
        match Ticket::from_human_readable("")
        {
            Ok(_ticket) =>
            {
                panic!("Unexpected success getting ticket from empty string as human-readable")
            },
            Err(From64Error::ShaInvalidLength) =>
            {
            },
            Err(error) =>
            {
                panic!("Unexpected error getting ticket from empty string as human-readable: {}", error)
            }
        }
    }

    /*  Attempt to decode a string as base-64 that is too short to represent a ticket,
        check that it fails with ShaInvalidLength. */
    #[test]
    fn ticket_from_human_readable_short()
    {
        match Ticket::from_human_readable("abcdefg=")
        {
            Ok(_ticket) =>
            {
                panic!("Unexpected success getting ticket from short string as human-readable")
            },
            Err(From64Error::ShaInvalidLength) =>
            {
            },
            Err(error) =>
            {
                panic!("Unexpected error getting ticket from short string as human-readable: {}", error)
            }
        }
    }

    /*  Attempt to decode a string as base-64 that has an ampersand in it,
        check that it fails with DecodeInvalidByte. */
    #[test]
    fn ticket_from_human_readable_invalid_byte()
    {
        match Ticket::from_human_readable("abcde&ghijk=")
        {
            Ok(_ticket) =>
            {
                panic!("Unexpected success getting ticket from string with invalid character as human_readable")
            },
            Err(From64Error::DecodeInvalidByte(location, byte)) =>
            {
                assert_eq!(location, 5);
                assert_eq!(byte, 38);
            },
            Err(error) =>
            {
                panic!("Unexpected error getting ticket from string with invalid character as human-readable: {}", error)
            }
        }
    }

    /*  Attempt to decode a string as base-64 that has an ampersand in it,
        check that it fails with DecodeInvalidByte. */
    #[test]
    fn ticket_from_human_readable_invalid_length()
    {
        match Ticket::from_human_readable("0abcdef==")
        {
            Ok(_ticket) =>
            {
                panic!("Unexpected success getting ticket from string as human-readable")
            },
            Err(From64Error::DecodeInvalidLength) =>
            {
            },
            Err(error) =>
            {
                panic!("Unexpected error getting ticket from empty string as human-readable: {}", error)
            }
        }
    }
}
