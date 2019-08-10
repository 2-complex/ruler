extern crate crypto;

use crypto::sha2::Sha512;
use base64::{encode};
use crypto::digest::Digest;
use std::fs::File;
use std::io::Read;

pub fn base64_sha(sha: &[u8]) -> String
{
    format!("{}", encode(&sha))
}

pub struct Hash
{
    sha : [u8; 64]
}

impl Hash
{
    pub fn base64(&self) -> String
    {
        base64_sha(&self.sha)
    }
}

pub struct HashFactory
{
    dig : Sha512
}

impl HashFactory
{
    pub fn new_from_str(first_input: &str) -> HashFactory
    {
        let mut d = Sha512::new();
        d.input(first_input.as_bytes());
        HashFactory{ dig : d }
    }

    pub fn input_hash(&mut self, input: Hash)
    {
        self.dig.input(&input.sha);
    }

    pub fn result(&mut self) -> Hash
    {
        let mut out_sha : [u8; 64] = [0; 64];
        self.dig.result(&mut out_sha);
        Hash
        {
            sha : out_sha
        }
    }

    pub fn new_from_filepath(path : &str) -> Result<HashFactory, std::io::Error>
    {
        match File::open(path)
        {
            Ok(mut file) =>
            {
                let mut d = Sha512::new();
                let mut buf = [0u8; 256];
                let mut done = false;

                loop
                {
                    match file.read(&mut buf)
                    {
                        Ok(0) => break,
                        Ok(packet_size) =>
                        {
                            if packet_size == 0
                            {
                                done = true;
                            }
                            else
                            {
                                d.input(&buf[..packet_size]);
                            }
                        },
                        Err(why) => return Err(why),
                    }
                }

                Ok(HashFactory{dig : d})
            },
            Err(why) => Err(why),
        }
    }
}
