extern crate bincode;
extern crate serde;

use crypto::
{
    sha2::Sha256,
    digest::Digest,
};
use std::hash::
{
    Hash,
    Hasher
};
use serde::{Serialize, Deserialize};
use crate::system::
{
    System,
    ReadWriteError,
    SystemError,
};
use std::fmt;
use std::io::Read;

use num_bigint::
{
    BigUint
};

use num_traits::
{
    ToPrimitive,
    identities::{Zero, One}
};

#[derive(Debug, PartialEq)]
pub enum FromHumanReadableError
{
    InvalidLength,
    Overflow,
    InvalidCharacter(char),
}

impl fmt::Display for FromHumanReadableError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            FromHumanReadableError::InvalidLength =>
                write!(formatter, "Invalid length, must be 43"),

            FromHumanReadableError::Overflow =>
                write!(formatter, "Encodes a value too big to fit in a 256-bit unsigned integer"),

            FromHumanReadableError::InvalidCharacter(character) =>
                write!(formatter, "Invalid character: {} must be 0-9 a-z A-Z", character),
        }
    }
}

fn decode62(tag : &str) -> Result<[u8; 32], FromHumanReadableError>
{
    if tag.len() != 43
    {
        return Err(FromHumanReadableError::InvalidLength);
    }

    let mut n = BigUint::zero();
    let mut d = BigUint::one();
    for c in tag.chars()
    {
        n += &d *
        match c
        {
            '0' => 0u32, '1' => 1, '2' => 2, '3' => 3, '4' => 4, '5' => 5, '6' => 6, '7' => 7, '8' => 8, '9' => 9, 'a' => 10,
            'b' => 11, 'c' => 12, 'd' => 13, 'e' => 14, 'f' => 15, 'g' => 16, 'h' => 17, 'i' => 18, 'j' => 19, 'k' => 20, 'l' => 21, 'm' => 22, 'n' => 23, 'o' => 24, 'p' => 25, 'q' => 26, 'r' => 27, 's' => 28, 't' => 29, 'u' => 30, 'v' => 31, 'w' => 32, 'x' => 33, 'y' => 34, 'z' => 35,
            'A' => 36, 'B' => 37, 'C' => 38, 'D' => 39, 'E' => 40, 'F' => 41, 'G' => 42, 'H' => 43, 'I' => 44, 'J' => 45, 'K' => 46, 'L' => 47, 'M' => 48, 'N' => 49, 'O' => 50, 'P' => 51, 'Q' => 52, 'R' => 53, 'S' => 54, 'T' => 55, 'U' => 56, 'V' => 57, 'W' => 58, 'X' => 59, 'Y' => 60, 'Z' => 61,
            _ =>
            {
                return Err(FromHumanReadableError::InvalidCharacter(c));
            },
        };
        d *= 62u32;
    }

    let v = n.to_bytes_le();
    if v.len() > 32
    {
        return Err(FromHumanReadableError::Overflow);
    }

    let mut result = [0u8; 32];
    let mut i = 0;
    for x in v
    {
        result[i] = x;
        i+=1;
    }

    return Ok(result)
}

fn encode62(bytes: &[u8; 32]) -> String
{
    let mut n = BigUint::from_bytes_le(bytes);

    // 0-9, a-z, A-Z
    let alphabet : [u8; 62] = [
        48, 49, 50, 51, 52, 53, 54, 55, 56, 57,
        97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122,
        65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90
    ];

    let mut buffer = [48u8; 43];
    let mut i = 0;
    while n > BigUint::zero()
    {
        buffer[i] = alphabet[
            (&n % 62u32).to_u32().unwrap() as usize];
        i+=1;
        n /= 62u32;
    }

    std::str::from_utf8(&buffer).unwrap().to_string()
}

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
        let mut out_sha = [0u8; 32];
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
        system: &FSType,
        path : &str
    )
    ->
    Result<TicketFactory, ReadWriteError>
    {

        let path_list =
        match system.list_dir(path)
        {
            Ok(path_list) => path_list,
            Err(_error) => return Err(ReadWriteError::SystemError(SystemError::NotFound)),
        };

        let mut factory = TicketFactory::from_str(&path_list.join("\n"));
        for path in path_list
        {
            if system.is_dir(&path)
            {
                let mut sub_factory =
                match TicketFactory::from_directory(system, &path)
                {
                    Ok(fact) => fact,
                    Err(error) => return Err(error),
                };
                factory.input_ticket(sub_factory.result());
            }
            else if system.is_file(&path)
            {
                let mut sub_factory =
                match TicketFactory::from_file(system, &path)
                {
                    Ok(fact) => fact,
                    Err(error) => return Err(error),
                };
                factory.input_ticket(sub_factory.result());
            }
            else
            {
                return Err(ReadWriteError::SystemError(SystemError::NotFound));
            }
        }

        Ok(factory)
    }
}

/*  Ticket represents a hash of a file or a rule */
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Eq)]
pub struct Ticket
{
    sha: [u8; 32],
}

impl Hash for Ticket
{
    fn hash<H: Hasher>(&self, state: &mut H)
    {
        /*  If we're hashing the ticket... for the puproses of putting it in a hash map
            or a HashSet, there isn't much point in digesting the entire 32 bytes of already
            hashed data.  8 will do. */
        self.sha[..8].hash(state);
    }
}

impl Ticket
{
    /*  Returns a string URL-safe human-readable hash string */
    pub fn human_readable(&self) -> String
    {
        encode62(&self.sha)
    }

    /*  Takes a url-safe human-readable hash string and returns a ticket objcet
        or an error about why the hash string was invalid. */
    pub fn from_human_readable(human_readable_str: &str) ->
        Result<Ticket, FromHumanReadableError>
    {
        Ok(Ticket{sha:decode62(human_readable_str)?})
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

#[cfg(test)]
use std::collections::HashMap;

/*  Takes a string, computes a map of character to character-count */
#[cfg(test)]
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
#[cfg(test)]
pub fn hash_heuristic(hash_str : &str) -> bool
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

#[cfg(test)]
mod test
{
    use crate::ticket::
    {
        Ticket,
        TicketFactory,
        FromHumanReadableError,
        hash_heuristic,
        encode62,
        decode62,
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
    use std::collections::HashSet;
    use rand::prelude::*;

    #[test]
    fn ticket_factory_passes_heuristic()
    {
        for n in 0..10000
        {
            let content = format!("{} is a very interesting number.", n);
            let ticket = TicketFactory::from_str(&content).result();
            assert!(hash_heuristic(&ticket.human_readable()));
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
    fn encode_flat_byte_arrays()
    {
        assert_eq!("0000000000000000000000000000000000000000000", encode62(&[0u8; 32]));
        assert_eq!("92DWrWRE9D5pbrqNyzR7wOBASXgV2j8dfuSWxfx6Le0", encode62(&[1u8; 32]));
        assert_eq!("i4gTTSJjjgbOmSQA79Jf2DdbLVxQ5CgquYKT5v4dwt0", encode62(&[2u8; 32]));
        assert_eq!("1Px8WoR5J2acUNJh7gll8MwzwhMy1la1zo6aDWKSJHY", encode62(&[255u8; 32]));
    }

    #[test]
    fn decode_flat_byte_arrays()
    {
        assert_eq!(decode62("0000000000000000000000000000000000000000000").unwrap(), [0u8; 32]);
        assert_eq!(decode62("92DWrWRE9D5pbrqNyzR7wOBASXgV2j8dfuSWxfx6Le0").unwrap(), [1u8; 32]);
        assert_eq!(decode62("i4gTTSJjjgbOmSQA79Jf2DdbLVxQ5CgquYKT5v4dwt0").unwrap(), [2u8; 32]);
        assert_eq!(decode62("1Px8WoR5J2acUNJh7gll8MwzwhMy1la1zo6aDWKSJHY").unwrap(), [255u8; 32]);
    }

    #[test]
    fn decode_invalid_length()
    {
        assert_eq!(
            decode62("92DWrWRE9D5pbrqNyzR7wOBASXgV2j8dfuSWxfx6Le00"),
            Err(FromHumanReadableError::InvalidLength));

        assert_eq!(
            decode62("92DWrWRE9D5pbrqNyzR7wOBASXgV2j8dfuSWxfx6Le"),
            Err(FromHumanReadableError::InvalidLength));

        assert_eq!(
            decode62(""),
            Err(FromHumanReadableError::InvalidLength));
    }

    #[test]
    fn decode_invalid_character()
    {
        assert_eq!(
            decode62("92DWrWRE9D5pbrqNyzR7wO-ASXgV2j8dfuSWxfx6Le0"),
            Err(FromHumanReadableError::InvalidCharacter('-')));
    }

    #[test]
    fn decode_overflow()
    {
        // Add 1 to the human-readable string representing all-ones, expect it to overflow
        assert_eq!(
            decode62("2Px8WoR5J2acUNJh7gll8MwzwhMy1la1zo6aDWKSJHY"),
            Err(FromHumanReadableError::Overflow));
    }

    #[test]
    fn encode_random_bytes()
    {
        for _ in 0..6000
        {
            let mut bytes = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut bytes);
            let hash_string = encode62(&bytes);
            assert_eq!(hash_string.len(), 43);
            assert_eq!(decode62(&hash_string).unwrap(), bytes);
        }
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
        assert!(hash_heuristic(&ticket.human_readable()));
    }

    /*  Uses a TicketFactory to construct a Ticket based on a single string with more than one character,
        compares with exemplar. */
    #[test]
    fn ticket_from_string_more()
    {
        let ticket = TicketFactory::from_str("Time wounds all heels.\n").result();
        assert!(hash_heuristic(&ticket.human_readable()));
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
        hash_heuristic(&TicketFactory::from_file(&system, "time0.txt").unwrap().result().human_readable());
    }

    /*  Using a fake file-system, create two files with different content, use TicketFactory::from_file
        to get a hash from each, and compare to each other.  */
    #[test]
    fn ticket_factory_two_files_different()
    {
        let mut system = FakeSystem::new(10);
        write_str_to_file(&mut system, "time0.txt", "Time wounds all heels.\n").unwrap();
        write_str_to_file(&mut system, "time1.txt", "Time: March is on.\n").unwrap();

        let ticket0 = TicketFactory::from_file(&system, "time0.txt").unwrap().result();
        let ticket1 = TicketFactory::from_file(&system, "time1.txt").unwrap().result();

        hash_heuristic(&ticket0.human_readable());
        hash_heuristic(&ticket1.human_readable());

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
        assert!(&hash_heuristic(&ticket.human_readable()));
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

        let ticket0 = TicketFactory::from_directory(&system, "time-files-0").unwrap().result();
        let ticket1 = TicketFactory::from_directory(&system, "time-files-1").unwrap().result();

        assert!(hash_heuristic(&ticket0.human_readable()));
        assert!(hash_heuristic(&ticket1.human_readable()));

        assert_ne!(ticket0, ticket1)
    }

    /*  Using a fake file-system, create two directories, populate with some known text, use TicketFactory::from_file
        to get a hash and compare with an exemplar.  */
    #[test]
    fn ticket_factory_two_directories_different_names()
    {
        let mut system = FakeSystem::new(10);
        system.create_dir("time-files-0").unwrap();
        system.create_dir("time-files-1").unwrap();
        let content = "Time wounds all heels.\n";
        write_str_to_file(&mut system, "time-files-0/time0.txt", content).unwrap();
        write_str_to_file(&mut system, "time-files-1/time1.txt", content).unwrap();

        let ticket0 = TicketFactory::from_directory(&system, "time-files-0").unwrap().result();
        let ticket1 = TicketFactory::from_directory(&system, "time-files-1").unwrap().result();

        assert!(hash_heuristic(&ticket0.human_readable()));
        assert!(hash_heuristic(&ticket1.human_readable()));

        assert_ne!(ticket0, ticket1)
    }

    /*  Using a fake file-system, create a file, populate it with with known text, then use TicketFactory::from_str
        and input_ticket to simulate making a ticket with that file as a target.  Compare the hash with an exemplar.*/
    #[test]
    fn ticket_factory_hashes()
    {
        let mut file_system = FakeSystem::new(10);
        write_str_to_file(&mut file_system, "time1.txt", "Time wounds all heels.\n").unwrap();
        let mut factory_from_file = TicketFactory::from_file(&file_system, "time1.txt").unwrap();
        let mut new_factory = TicketFactory::from_str("time1.txt\n:\n:\n:\n");
        new_factory.input_ticket(factory_from_file.result());
        assert!(hash_heuristic(&new_factory.result().human_readable()));
    }

    /*  Obtain lorem ipsum, and write it to a file in a fake filesystem.  Then use TicketFactory::from_file
        to generate a ticket.  Compare hash with an exemplar. */
    #[test]
    fn ticket_factory_hashes_bigger_file()
    {
        let mut system = FakeSystem::new(10);
        write_str_to_file(&mut system, "good_and_evil.txt", LOREM_IPSUM).unwrap();
        assert!(hash_heuristic(&TicketFactory::from_file(
            &system, "good_and_evil.txt").unwrap().result().human_readable()));
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
    fn ticket_from_human_readable_round_trip()
    {
        let hash_str = "0123456789abcdefghij0123456789ABCDEFGHIJ012";
        assert_eq!(
            Ticket::from_human_readable(hash_str).unwrap().human_readable(),
            hash_str.to_string());
    }

    /*  Attempt to decode the empty string as base-64,
        check that it fails. */
    #[test]
    fn ticket_from_human_readable_empty()
    {
        assert_eq!(
            Ticket::from_human_readable(""),
            Err(FromHumanReadableError::InvalidLength));
    }

    /*  Attempt to decode a string as base-64 that is too short to represent a ticket,
        check that it fails with ShaInvalidLength. */
    #[test]
    fn ticket_from_human_readable_short()
    {
        assert_eq!(
            Ticket::from_human_readable("abcdefg="),
            Err(FromHumanReadableError::InvalidLength));
    }

    /*  Attempt to decode a string as base-64 that has an ampersand in it,
        check that it fails with DecodeInvalidByte. */
    #[test]
    fn ticket_from_human_readable_invalid_character()
    {
        assert_eq!(
            Ticket::from_human_readable("0123456789012345678&01234567890123456789012"),
            Err(FromHumanReadableError::InvalidCharacter('&'))
        );
    }

    /*  Attempt to decode a string as base-64 that has an ampersand in it,
        check that it fails with DecodeInvalidByte. */
    #[test]
    fn ticket_from_human_readable_invalid_length()
    {
        assert_eq!(
            Ticket::from_human_readable("0abcdef"),
            Err(FromHumanReadableError::InvalidLength)
        );
    }
}
